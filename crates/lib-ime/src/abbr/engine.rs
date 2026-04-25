//! Abbreviation matching engine.
//!
//! A tail-oriented matcher: it maintains a sliding window of recently
//! committed characters and, after each commit or trigger event, looks
//! for the longest registered trigger that matches the tail.
//!
//! Current implementation uses a linear scan over registered
//! abbreviations (O(N·K) per update, N = abbrs, K = max trigger
//! length). For <1K registered abbreviations this is fast enough; a
//! trie can replace the scan later without changing the public API.

use std::collections::VecDeque;

use crate::hangul::jamo::Cho;

use super::model::{Abbreviation, Trigger, TriggerEvent};

/// Maximum number of trailing characters the engine retains.
const MAX_TAIL: usize = 64;

/// Outcome of an engine call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AbbrEvent {
    /// No match; nothing to do.
    None,
    /// A match is waiting for its configured trigger event. UIs can
    /// render a preview tooltip.
    Pending {
        /// The matching abbreviation's id.
        abbr_id: String,
        /// Text that *would* be inserted if the user fires the trigger.
        preview: String,
    },
    /// Expand immediately: rollback the last `rollback_chars` characters
    /// from the committed buffer, then insert `insert`.
    Expand {
        /// The matching abbreviation's id.
        abbr_id: String,
        /// Characters to remove from the commit stream.
        rollback_chars: usize,
        /// Replacement body.
        insert: String,
    },
}

#[derive(Clone, Debug)]
struct MatchState {
    abbr_idx: usize,
    rollback_chars: usize,
}

/// A candidate abbreviation surfaced by the picker.
///
/// The engine searches three ways, ranked highest to lowest:
///
/// 1. **Exact** — the entire trigger is present at the tail.
/// 2. **Prefix** — what the user has typed is the start of the trigger
///    (`"습"` matches `"습니다"`).
/// 3. **Substring** — what the user has typed appears elsewhere in the
///    trigger (`"니다"` matches `"습니다"`). Requires ≥ 2 characters of
///    overlap to cut down on noise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Suggestion {
    /// The abbreviation's id.
    pub abbr_id: String,
    /// Human-readable trigger (e.g. `"ㄱㅅ"`).
    pub trigger_display: String,
    /// Expansion body (first line only — UIs may truncate).
    pub body: String,
    /// `true` when the entire trigger is present at the tail.
    pub is_exact: bool,
    /// `true` when the match begins at position 0 of the trigger.
    pub is_prefix: bool,
    /// Position in the trigger (in characters) where the match starts.
    /// `0` for prefix matches; `>0` for substring matches.
    pub match_start: usize,
    /// Characters of the commit tail that constitute the match.
    /// The host rolls these back when the user accepts the suggestion.
    pub rollback_chars: usize,
    /// Relevance score inherited from the registered abbreviation.
    /// Used as a sort tiebreaker so common forms rank above rare ones.
    pub priority: u32,
}

/// Tail-oriented abbreviation matcher.
///
/// Tracks **two** streams:
///
/// * `commit_tail` — characters already finalized by the host
///   (typically NFC-normalized Hangul + ASCII). Survives across
///   keystrokes.
/// * `preedit` — the in-progress composition held by the Hangul FSM.
///   Pushed back to the engine every keystroke via [`set_preedit`].
///   Cleared automatically when a suggestion is fired or the host
///   resets the engine.
///
/// [`candidates`](AbbreviationEngine::candidates) matches against the
/// **effective tail** (`commit_tail` + `preedit`), so the picker can
/// surface suggestions while the user is still composing a syllable.
#[derive(Clone, Debug)]
pub struct AbbreviationEngine {
    abbrs: Vec<Abbreviation>,
    commit_tail: VecDeque<char>,
    preedit: String,
    active_match: Option<MatchState>,
    enabled: bool,
}

impl AbbreviationEngine {
    /// Create an engine with a starter set of abbreviations.
    pub fn new(abbrs: Vec<Abbreviation>) -> Self {
        Self {
            abbrs,
            commit_tail: VecDeque::with_capacity(MAX_TAIL + 8),
            preedit: String::new(),
            active_match: None,
            enabled: true,
        }
    }

    /// Create an empty engine.
    pub fn empty() -> Self {
        Self::new(Vec::new())
    }

    /// Registered abbreviations (read-only view).
    pub fn abbreviations(&self) -> &[Abbreviation] {
        &self.abbrs
    }

    /// Whether matching is enabled. Host can disable temporarily
    /// (e.g., during IME-off mode or on password fields).
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enable or disable matching.
    pub fn set_enabled(&mut self, on: bool) {
        self.enabled = on;
        if !on {
            self.active_match = None;
        }
    }

    /// Replace the abbreviation list at runtime.
    pub fn set_abbreviations(&mut self, abbrs: Vec<Abbreviation>) {
        self.abbrs = abbrs;
        self.active_match = None;
    }

    /// Reset the tail buffer (e.g., on focus loss or manual cancel).
    pub fn reset(&mut self) {
        self.commit_tail.clear();
        self.preedit.clear();
        self.active_match = None;
    }

    /// Update the tentative preedit string (the FSM composition in
    /// progress). Call this after every keystroke so
    /// [`candidates`](Self::candidates) can include partially-typed
    /// syllables.
    pub fn set_preedit(&mut self, preedit: &str) {
        self.preedit.clear();
        self.preedit.push_str(preedit);
    }

    /// Returns the effective tail — committed + preedit — as a char
    /// vector. Used internally for match scoring.
    fn effective_tail(&self) -> Vec<char> {
        let mut v: Vec<char> = self.commit_tail.iter().copied().collect();
        v.extend(self.preedit.chars());
        v
    }

    /// Notify the engine that the host has committed `s` characters.
    ///
    /// Runs match detection and, if any abbreviation is configured with
    /// [`TriggerEvent::Immediate`], fires [`AbbrEvent::Expand`]
    /// immediately.
    pub fn on_commit(&mut self, s: &str) -> AbbrEvent {
        if !self.enabled {
            return AbbrEvent::None;
        }
        for ch in s.chars() {
            self.commit_tail.push_back(ch);
            while self.commit_tail.len() > MAX_TAIL {
                self.commit_tail.pop_front();
            }
        }
        self.update_match();
        self.maybe_fire(TriggerEvent::Immediate)
    }

    /// Notify the engine of a non-committing trigger event such as Space.
    pub fn on_trigger(&mut self, ev: TriggerEvent) -> AbbrEvent {
        if !self.enabled {
            return AbbrEvent::None;
        }
        self.update_match();
        self.maybe_fire(ev)
    }

    /// Notify the engine that the host removed the last character
    /// (e.g., user pressed Backspace).
    pub fn on_backspace(&mut self) {
        self.commit_tail.pop_back();
        self.active_match = None;
    }

    /// Return abbreviation candidates related to the current commit
    /// tail, ranked by relevance:
    ///
    /// 1. **Exact match** — the whole trigger is present at the tail.
    /// 2. **Prefix match** — the tail is the beginning of the trigger
    ///    (`"습"` matches `"습니다"`).
    /// 3. **Substring match** — the tail appears later inside the
    ///    trigger (`"니다"` matches `"습니다"`). Requires at least two
    ///    characters of overlap to keep single-syllable typing from
    ///    flooding the picker.
    ///
    /// `Literal` triggers still require a word boundary at their match
    /// point; `Ending` and `ChoSeq` do not.
    #[allow(clippy::too_many_lines)] // the three match-kind arms + sort key are clearer inline
    pub fn candidates(&self) -> Vec<Suggestion> {
        if !self.enabled {
            return Vec::new();
        }
        let tail = self.effective_tail();
        if tail.is_empty() {
            return Vec::new();
        }

        // Cho-only run at the tail: walk backwards while each char is a
        // conjoining initial consonant.
        let cho_run_len = tail
            .iter()
            .rev()
            .take_while(|c| Cho::from_codepoint(**c as u32).is_some())
            .count();
        let cho_run: Vec<u32> = tail[tail.len() - cho_run_len..]
            .iter()
            .map(|c| *c as u32)
            .collect();

        let mut out: Vec<Suggestion> = Vec::new();
        for abbr in &self.abbrs {
            match &abbr.trigger {
                Trigger::ChoSeq(chos) => {
                    if chos.is_empty() || cho_run.is_empty() {
                        continue;
                    }
                    if cho_run.len() > chos.len() {
                        continue;
                    }
                    if chos[..cho_run.len()] == cho_run[..] {
                        out.push(Suggestion {
                            abbr_id: abbr.id.clone(),
                            trigger_display: abbr.trigger.display(),
                            body: abbr.body.clone(),
                            is_exact: cho_run.len() == chos.len(),
                            is_prefix: true,
                            match_start: 0,
                            rollback_chars: cho_run.len(),
                            priority: abbr.priority,
                        });
                    }
                }
                Trigger::Literal(text) | Trigger::Ending(text) => {
                    let lit: Vec<char> = text.chars().collect();
                    if lit.is_empty() {
                        continue;
                    }
                    let is_literal = matches!(abbr.trigger, Trigger::Literal(_));
                    let boundary_ok_for_start = |start_idx: usize| -> bool {
                        if !is_literal {
                            return true;
                        }
                        start_idx == 0
                            || matches!(tail[start_idx - 1], ' ' | '\n' | '\t')
                    };

                    // First pass: try prefix match (position 0 in trigger).
                    let max_k = lit.len().min(tail.len());
                    let mut best_prefix_k = 0usize;
                    for k in 1..=max_k {
                        let start_idx = tail.len() - k;
                        if tail[start_idx..] == lit[..k] && boundary_ok_for_start(start_idx) {
                            best_prefix_k = k;
                        }
                    }

                    // Second pass: try substring match at positions > 0.
                    // Requires k >= 2 to keep single-syllable typing from
                    // flooding the picker.
                    let mut best_sub: Option<(usize, usize)> = None;
                    for k in (2..=max_k).rev() {
                        if lit.len() <= k {
                            continue;
                        }
                        let tail_suffix = &tail[tail.len() - k..];
                        for start in 1..=lit.len() - k {
                            if &lit[start..start + k] == tail_suffix
                                && boundary_ok_for_start(tail.len() - k)
                            {
                                best_sub = Some((start, k));
                                break;
                            }
                        }
                        if best_sub.is_some() {
                            break;
                        }
                    }

                    // Choose the reported match: prefer prefix (higher
                    // rank). If both exist, still report prefix; the
                    // substring fallback surfaces only when prefix k == 0.
                    let (match_start, k) = if best_prefix_k > 0 {
                        (0, best_prefix_k)
                    } else if let Some((ms, sk)) = best_sub {
                        (ms, sk)
                    } else {
                        continue;
                    };

                    out.push(Suggestion {
                        abbr_id: abbr.id.clone(),
                        trigger_display: abbr.trigger.display(),
                        body: abbr.body.clone(),
                        is_exact: match_start == 0 && k == lit.len(),
                        is_prefix: match_start == 0,
                        match_start,
                        rollback_chars: k,
                        priority: abbr.priority,
                    });
                }
            }
        }
        out.sort_by(|a, b| {
            // Descending: exact > prefix > substring; within the same
            // match kind, higher-priority (more common) entries first;
            // then longer rollback; shorter trigger; alphabetical.
            b.is_exact
                .cmp(&a.is_exact)
                .then(b.is_prefix.cmp(&a.is_prefix))
                .then(b.priority.cmp(&a.priority))
                .then(b.rollback_chars.cmp(&a.rollback_chars))
                .then(a.trigger_display.chars().count().cmp(&b.trigger_display.chars().count()))
                .then(a.trigger_display.cmp(&b.trigger_display))
        });
        out
    }

    /// Fire the abbreviation with the given id against the current
    /// effective tail (commit + preedit). Used when the user accepts
    /// a picker suggestion.
    ///
    /// The returned `rollback_chars` is measured **in the host's
    /// committed buffer only**: the engine has already absorbed the
    /// preedit portion of the match internally by clearing its
    /// `preedit` field. The host is expected to cancel the Hangul FSM
    /// preedit on its side so the visible composition disappears.
    pub fn fire_by_id(&mut self, id: &str) -> AbbrEvent {
        let Some(pos) = self.abbrs.iter().position(|a| a.id == id) else {
            return AbbrEvent::None;
        };
        let abbr = &self.abbrs[pos];
        let tail = self.effective_tail();

        let rollback_total = match &abbr.trigger {
            Trigger::ChoSeq(chos) => {
                let cho_run_len = tail
                    .iter()
                    .rev()
                    .take_while(|c| Cho::from_codepoint(**c as u32).is_some())
                    .count();
                let cho_run: Vec<u32> = tail[tail.len() - cho_run_len..]
                    .iter()
                    .map(|c| *c as u32)
                    .collect();
                if !cho_run.is_empty()
                    && cho_run.len() <= chos.len()
                    && chos[..cho_run.len()] == cho_run[..]
                {
                    cho_run.len()
                } else {
                    0
                }
            }
            Trigger::Literal(text) | Trigger::Ending(text) => {
                let lit: Vec<char> = text.chars().collect();
                let max_k = lit.len().min(tail.len());
                let mut best = 0;
                for k in (1..=max_k).rev() {
                    let suffix = &tail[tail.len() - k..];
                    let found = lit.len() >= k
                        && (0..=lit.len() - k).any(|start| &lit[start..start + k] == suffix);
                    if found {
                        best = k;
                        break;
                    }
                }
                best
            }
        };

        let insert = abbr.body.clone();
        let abbr_id = abbr.id.clone();

        // Split the rollback: the preedit portion is discarded by
        // clearing the engine's `preedit` field (the host also cancels
        // the FSM). The remainder rolls back actual committed chars.
        let preedit_len = self.preedit.chars().count();
        let commit_rollback = rollback_total.saturating_sub(preedit_len);
        for _ in 0..commit_rollback {
            self.commit_tail.pop_back();
        }
        self.preedit.clear();
        self.active_match = None;
        AbbrEvent::Expand {
            abbr_id,
            rollback_chars: commit_rollback,
            insert,
        }
    }

    fn update_match(&mut self) {
        self.active_match = self.find_longest_match();
    }

    fn find_longest_match(&self) -> Option<MatchState> {
        if self.commit_tail.is_empty() {
            return None;
        }
        let tail: Vec<char> = self.commit_tail.iter().copied().collect();
        let mut best: Option<MatchState> = None;
        for (idx, abbr) in self.abbrs.iter().enumerate() {
            let len = abbr.trigger.match_len();
            if len == 0 || tail.len() < len {
                continue;
            }
            let start = tail.len() - len;
            let suffix = &tail[start..];
            let hit = match &abbr.trigger {
                Trigger::ChoSeq(chos) => suffix
                    .iter()
                    .zip(chos.iter())
                    .all(|(c, cp)| *c as u32 == *cp && Cho::from_codepoint(*cp).is_some()),
                Trigger::Literal(text) | Trigger::Ending(text) => {
                    let lit: Vec<char> = text.chars().collect();
                    suffix == lit.as_slice()
                }
            };
            if hit {
                let cand = MatchState { abbr_idx: idx, rollback_chars: len };
                best = Some(match best {
                    None => cand,
                    Some(prev) => {
                        if cand.rollback_chars > prev.rollback_chars {
                            cand
                        } else {
                            prev
                        }
                    }
                });
            }
        }
        best
    }

    fn maybe_fire(&mut self, ev: TriggerEvent) -> AbbrEvent {
        let m = match &self.active_match {
            Some(m) => m.clone(),
            None => return AbbrEvent::None,
        };
        let abbr = &self.abbrs[m.abbr_idx];
        if !TriggerEvent::matches(abbr.trigger_on, ev) {
            return AbbrEvent::Pending {
                abbr_id: abbr.id.clone(),
                preview: abbr.body.clone(),
            };
        }
        let out = AbbrEvent::Expand {
            abbr_id: abbr.id.clone(),
            rollback_chars: m.rollback_chars,
            insert: abbr.body.clone(),
        };
        // Consume the matched tail so the same match can't fire twice.
        for _ in 0..m.rollback_chars {
            self.commit_tail.pop_back();
        }
        self.active_match = None;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cho_seq(cs: &[u32]) -> Trigger {
        Trigger::ChoSeq(cs.to_vec())
    }

    fn abbr_cho(id: &str, chos: &[u32], body: &str, ev: TriggerEvent) -> Abbreviation {
        Abbreviation {
            id: id.into(),
            trigger: cho_seq(chos),
            body: body.into(),
            trigger_on: ev,
            priority: 100,
        }
    }

    #[test]
    fn cho_seq_fires_on_space() {
        // ㄱㅅ → "감사합니다"
        let mut e = AbbreviationEngine::new(vec![abbr_cho(
            "gs",
            &[0x1100, 0x1109],
            "감사합니다",
            TriggerEvent::Space,
        )]);
        // Commits from FSM: first Cho then next Cho (each lone).
        assert_eq!(e.on_commit("\u{1100}"), AbbrEvent::None);
        let after_second = e.on_commit("\u{1109}");
        assert!(matches!(after_second, AbbrEvent::Pending { .. }));
        let fired = e.on_trigger(TriggerEvent::Space);
        match fired {
            AbbrEvent::Expand { rollback_chars, insert, .. } => {
                assert_eq!(rollback_chars, 2);
                assert_eq!(insert, "감사합니다");
            }
            other => panic!("expected Expand, got {other:?}"),
        }
    }

    #[test]
    fn immediate_fires_without_trigger_key() {
        let mut e = AbbreviationEngine::new(vec![abbr_cho(
            "ok",
            &[0x110B, 0x110F],
            "알겠습니다",
            TriggerEvent::Immediate,
        )]);
        let _ = e.on_commit("\u{110B}");
        let ev = e.on_commit("\u{110F}");
        match ev {
            AbbrEvent::Expand { rollback_chars, insert, .. } => {
                assert_eq!(rollback_chars, 2);
                assert_eq!(insert, "알겠습니다");
            }
            other => panic!("expected Expand, got {other:?}"),
        }
    }

    #[test]
    fn no_match_when_jung_interrupts() {
        let mut e = AbbreviationEngine::new(vec![abbr_cho(
            "gs",
            &[0x1100, 0x1109],
            "감사합니다",
            TriggerEvent::Space,
        )]);
        let _ = e.on_commit("\u{1100}");
        // 가 (NFC syllable) doesn't look like a Cho codepoint → tail break.
        let _ = e.on_commit("가");
        let _ = e.on_commit("\u{1109}");
        // Only one Cho at the tail → no match.
        assert_eq!(e.on_trigger(TriggerEvent::Space), AbbrEvent::None);
    }

    #[test]
    fn longer_match_wins() {
        let mut e = AbbreviationEngine::new(vec![
            abbr_cho("gs", &[0x1100, 0x1109], "감사합니다", TriggerEvent::Space),
            abbr_cho(
                "gsjn",
                &[0x1100, 0x1109, 0x110C, 0x1102],
                "감사합니다, 잘 부탁드려요",
                TriggerEvent::Space,
            ),
        ]);
        let _ = e.on_commit("\u{1100}\u{1109}\u{110C}\u{1102}");
        let fired = e.on_trigger(TriggerEvent::Space);
        match fired {
            AbbrEvent::Expand { abbr_id, .. } => assert_eq!(abbr_id, "gsjn"),
            other => panic!("expected long match, got {other:?}"),
        }
    }

    #[test]
    fn literal_trigger_matches_nfc_tail() {
        let mut e = AbbreviationEngine::new(vec![Abbreviation {
            id: "mail-end".into(),
            trigger: Trigger::Literal("메일끝".into()),
            body: "감사합니다.\n홍길동 드림".into(),
            trigger_on: TriggerEvent::Space,
            priority: 100,
        }]);
        let _ = e.on_commit("메일끝");
        let fired = e.on_trigger(TriggerEvent::Space);
        match fired {
            AbbrEvent::Expand { rollback_chars, insert, .. } => {
                assert_eq!(rollback_chars, 3);
                assert_eq!(insert, "감사합니다.\n홍길동 드림");
            }
            other => panic!("expected Expand, got {other:?}"),
        }
    }

    #[test]
    fn disabled_engine_never_fires() {
        let mut e = AbbreviationEngine::new(vec![abbr_cho(
            "gs",
            &[0x1100, 0x1109],
            "감사합니다",
            TriggerEvent::Immediate,
        )]);
        e.set_enabled(false);
        assert_eq!(e.on_commit("\u{1100}\u{1109}"), AbbrEvent::None);
    }

    #[test]
    fn candidates_prefix_match_on_cho() {
        let e = AbbreviationEngine::new(vec![
            abbr_cho("gs", &[0x1100, 0x1109], "감사합니다", TriggerEvent::Space),
            abbr_cho("gd", &[0x1100, 0x1103], "고생하셨습니다", TriggerEvent::Space),
            abbr_cho("ok", &[0x110B, 0x110F], "알겠습니다", TriggerEvent::Space),
        ]);
        let mut e2 = e.clone();
        let _ = e2.on_commit("\u{1100}");
        let cands = e2.candidates();
        let ids: Vec<_> = cands.iter().map(|s| s.abbr_id.as_str()).collect();
        assert!(ids.contains(&"gs"));
        assert!(ids.contains(&"gd"));
        assert!(!ids.contains(&"ok"));
        // All these are inexact (only 1 char of 2 matched)
        assert!(cands.iter().all(|s| !s.is_exact));
        assert!(cands.iter().all(|s| s.rollback_chars == 1));
    }

    #[test]
    fn candidates_rank_exact_first() {
        let e = AbbreviationEngine::new(vec![
            abbr_cho("gs", &[0x1100, 0x1109], "감사합니다", TriggerEvent::Space),
            abbr_cho("gsj", &[0x1100, 0x1109, 0x110C], "감사합니다 잘", TriggerEvent::Space),
        ]);
        let mut e2 = e.clone();
        let _ = e2.on_commit("\u{1100}\u{1109}");
        let cands = e2.candidates();
        // gs is exact, should come first.
        assert_eq!(cands.first().unwrap().abbr_id, "gs");
        assert!(cands.first().unwrap().is_exact);
    }

    #[test]
    fn preedit_participates_in_candidates() {
        let mut e = AbbreviationEngine::new(vec![Abbreviation {
            id: "end:습니다".into(),
            trigger: Trigger::Ending("습니다".into()),
            body: "습니다".into(),
            trigger_on: TriggerEvent::Explicit,
            priority: 100,
        }]);
        // Nothing committed yet; preedit is the partial Hangul syllable
        // "습" still held by the FSM.
        e.set_preedit("습");
        let cands = e.candidates();
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].abbr_id, "end:습니다");
        assert!(cands[0].is_prefix);
        assert_eq!(cands[0].rollback_chars, 1);
    }

    #[test]
    fn fire_by_id_with_preedit_only_no_commit_rollback() {
        // When the entire match lives in the preedit (nothing committed),
        // the host-visible rollback is zero — the engine internally
        // cancels the preedit, leaving the committed buffer untouched.
        let mut e = AbbreviationEngine::new(vec![Abbreviation {
            id: "end:습니다".into(),
            trigger: Trigger::Ending("습니다".into()),
            body: "습니다".into(),
            trigger_on: TriggerEvent::Explicit,
            priority: 100,
        }]);
        e.set_preedit("습");
        let ev = e.fire_by_id("end:습니다");
        match ev {
            AbbrEvent::Expand { rollback_chars, insert, .. } => {
                assert_eq!(rollback_chars, 0);
                assert_eq!(insert, "습니다");
            }
            other => panic!("expected Expand, got {other:?}"),
        }
    }

    #[test]
    fn fire_by_id_splits_commit_and_preedit_rollback() {
        // Commit "학" and preedit "습" → effective tail "학습".
        // Accepting "습니다" should rollback 0 committed chars
        // (preedit consumes all of the match).
        let mut e = AbbreviationEngine::new(vec![Abbreviation {
            id: "end:습니다".into(),
            trigger: Trigger::Ending("습니다".into()),
            body: "습니다".into(),
            trigger_on: TriggerEvent::Explicit,
            priority: 100,
        }]);
        let _ = e.on_commit("학");
        e.set_preedit("습");
        let ev = e.fire_by_id("end:습니다");
        match ev {
            AbbrEvent::Expand { rollback_chars, insert, .. } => {
                // The match is "습" (1 char). All of it lives in preedit.
                assert_eq!(rollback_chars, 0);
                assert_eq!(insert, "습니다");
            }
            other => panic!("expected Expand, got {other:?}"),
        }
    }

    #[test]
    fn fire_by_id_applies_even_with_partial_tail() {
        let mut e = AbbreviationEngine::new(vec![
            abbr_cho("gs", &[0x1100, 0x1109], "감사합니다", TriggerEvent::Space),
        ]);
        // User only typed one Cho.
        let _ = e.on_commit("\u{1100}");
        let ev = e.fire_by_id("gs");
        match ev {
            AbbrEvent::Expand { rollback_chars, insert, .. } => {
                assert_eq!(rollback_chars, 1);
                assert_eq!(insert, "감사합니다");
            }
            other => panic!("expected Expand, got {other:?}"),
        }
    }

    #[test]
    fn ending_matches_without_word_boundary() {
        // "습" after "먹었" (no preceding space) should suggest "습니다".
        let mut e = AbbreviationEngine::new(vec![Abbreviation {
            id: "end:습니다".into(),
            trigger: Trigger::Ending("습니다".into()),
            body: "습니다".into(),
            trigger_on: TriggerEvent::Explicit,
            priority: 100,
        }]);
        let _ = e.on_commit("먹었습");
        let cands = e.candidates();
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].abbr_id, "end:습니다");
        assert_eq!(cands[0].rollback_chars, 1);
        assert!(!cands[0].is_exact);
    }

    #[test]
    fn ending_fires_by_id_with_partial_tail() {
        let mut e = AbbreviationEngine::new(vec![Abbreviation {
            id: "end:습니다".into(),
            trigger: Trigger::Ending("습니다".into()),
            body: "습니다".into(),
            trigger_on: TriggerEvent::Explicit,
            priority: 100,
        }]);
        let _ = e.on_commit("가습");
        let ev = e.fire_by_id("end:습니다");
        match ev {
            AbbrEvent::Expand { rollback_chars, insert, .. } => {
                // Only "습" (1 char) at the tail matched the trigger prefix.
                assert_eq!(rollback_chars, 1);
                assert_eq!(insert, "습니다");
            }
            other => panic!("expected Expand, got {other:?}"),
        }
    }

    #[test]
    fn candidates_literal_require_word_boundary() {
        let mut e = AbbreviationEngine::new(vec![Abbreviation {
            id: "mail".into(),
            trigger: Trigger::Literal("메일".into()),
            body: "감사합니다".into(),
            trigger_on: TriggerEvent::Space,
            priority: 100,
        }]);
        // Tail: "하메" (no boundary before 메) → no candidate.
        let _ = e.on_commit("하메");
        assert!(e.candidates().is_empty());
        // Tail with a space: " 메" → candidate.
        e.reset();
        let _ = e.on_commit(" 메");
        assert!(!e.candidates().is_empty());
    }

    #[test]
    fn candidates_surface_substring_match_for_endings() {
        // Typing "니다" should suggest any ending that *contains* "니다".
        let mut e = AbbreviationEngine::new(vec![
            Abbreviation {
                id: "end:습니다".into(),
                trigger: Trigger::Ending("습니다".into()),
                body: "습니다".into(),
                trigger_on: TriggerEvent::Explicit,
                priority: 100,
            },
            Abbreviation {
                id: "end:합니다".into(),
                trigger: Trigger::Ending("합니다".into()),
                body: "합니다".into(),
                trigger_on: TriggerEvent::Explicit,
                priority: 100,
            },
        ]);
        let _ = e.on_commit("니다");
        let cands = e.candidates();
        let ids: Vec<_> = cands.iter().map(|s| s.abbr_id.as_str()).collect();
        assert!(ids.contains(&"end:습니다"));
        assert!(ids.contains(&"end:합니다"));
        // None are prefix since 니다 starts at position 1 in both.
        assert!(cands.iter().all(|s| !s.is_prefix));
        assert!(cands.iter().all(|s| s.match_start > 0));
    }

    #[test]
    fn candidates_rank_prefix_above_substring() {
        let mut e = AbbreviationEngine::new(vec![
            Abbreviation {
                id: "end:니다만".into(),
                trigger: Trigger::Ending("니다만".into()),
                body: "니다만".into(),
                trigger_on: TriggerEvent::Explicit,
                priority: 100,
            },
            Abbreviation {
                id: "end:습니다".into(),
                trigger: Trigger::Ending("습니다".into()),
                body: "습니다".into(),
                trigger_on: TriggerEvent::Explicit,
                priority: 100,
            },
        ]);
        let _ = e.on_commit("니다");
        let cands = e.candidates();
        // The prefix match ("니다" is prefix of "니다만") ranks first.
        assert_eq!(cands.first().unwrap().abbr_id, "end:니다만");
        assert!(cands.first().unwrap().is_prefix);
    }

    #[test]
    fn declaratives_rank_above_interrogatives_on_tie() {
        // Both "습니다" and "습니까" prefix-match "습". The declarative
        // (priority 100) should outrank the interrogative (priority 50).
        let mut e = AbbreviationEngine::new(vec![
            Abbreviation {
                id: "end:습니다".into(),
                trigger: Trigger::Ending("습니다".into()),
                body: "습니다.".into(),
                trigger_on: TriggerEvent::Explicit,
                priority: 100,
            },
            Abbreviation {
                id: "end:습니까".into(),
                trigger: Trigger::Ending("습니까".into()),
                body: "습니까?".into(),
                trigger_on: TriggerEvent::Explicit,
                priority: 50,
            },
        ]);
        e.set_preedit("습");
        let cands = e.candidates();
        assert!(cands.len() >= 2);
        assert_eq!(cands[0].abbr_id, "end:습니다");
        assert_eq!(cands[1].abbr_id, "end:습니까");
    }

    #[test]
    fn single_char_substring_is_not_a_candidate() {
        // A lone syllable like "니" would substring-match many endings.
        // Require k >= 2 for non-prefix matches to cut the noise.
        let mut e = AbbreviationEngine::new(vec![Abbreviation {
            id: "end:입니다".into(),
            trigger: Trigger::Ending("입니다".into()),
            body: "입니다".into(),
            trigger_on: TriggerEvent::Explicit,
            priority: 100,
        }]);
        let _ = e.on_commit("단");
        // "단" only appears in 입니다 as substring of length 0 (not really).
        // Check with real partial: "니" — single-char substring, should be
        // excluded.
        e.reset();
        let _ = e.on_commit("니");
        assert!(e.candidates().is_empty());
    }

    #[test]
    fn backspace_invalidates_pending_match() {
        let mut e = AbbreviationEngine::new(vec![abbr_cho(
            "gs",
            &[0x1100, 0x1109],
            "감사합니다",
            TriggerEvent::Space,
        )]);
        let _ = e.on_commit("\u{1100}\u{1109}");
        e.on_backspace();
        assert_eq!(e.on_trigger(TriggerEvent::Space), AbbrEvent::None);
    }
}
