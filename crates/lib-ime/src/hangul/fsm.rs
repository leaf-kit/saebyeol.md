//! Hangul composition finite-state machine.
//!
//! The FSM consumes [`JamoInput`] events (produced by a
//! [`crate::Layout`]) and emits [`FsmEvent`]s that describe how the
//! in-progress composition (*preedit*) and finalized text (*commit*)
//! evolve.
//!
//! Internally the state is a struct of three independently-fillable
//! slots — Cho, Jung, Jong — so the same machine can run in two modes:
//!
//! * [`ComposeMode::Sequential`] — the traditional 두벌식/세벌식 flow:
//!   Cho must arrive first, then Jung, then Jong, and any out-of-order
//!   input commits the current syllable early.
//! * [`ComposeMode::Moachigi`] — order-independent input: the user may
//!   type Jong → Jung → Cho (or any permutation) within one syllable
//!   and the FSM packs each jamo into its slot. A new syllable starts
//!   only when a slot would be overwritten.
//!
//! State is always stored as conjoining Jamo. Render through
//! [`crate::OutputForm`] for NFC / compat output.

use super::compose::{
    compose_cho_double, compose_jong, compose_jung, decompose_jong, decompose_jung,
    split_jong,
};
use super::jamo::{Cho, JamoInput, Jong, Jung};

/// Composition slots. Each can independently hold a value or be empty.
///
/// A complete Hangul syllable fills all three slots. Transient states
/// (e.g. `{jong: Some(_), cho: None, jung: None}` after a lone final
/// in Moachigi mode) are valid and simply don't compose to an NFC
/// syllable until more slots are populated.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct CompositionState {
    /// Initial consonant slot.
    pub cho: Option<Cho>,
    /// Medial vowel slot.
    pub jung: Option<Jung>,
    /// Final consonant slot.
    pub jong: Option<Jong>,
}

impl CompositionState {
    /// Whether no slot is filled.
    pub fn is_empty(self) -> bool {
        self.cho.is_none() && self.jung.is_none() && self.jong.is_none()
    }

    /// Render the state as a conjoining-Jamo string in canonical
    /// Cho → Jung → Jong order.
    fn render(self) -> String {
        let mut s = String::with_capacity(9);
        if let Some(c) = self.cho {
            push_cp(&mut s, c.codepoint());
        }
        if let Some(j) = self.jung {
            push_cp(&mut s, j.codepoint());
        }
        if let Some(t) = self.jong {
            push_cp(&mut s, t.codepoint());
        }
        s
    }
}

fn push_cp(s: &mut String, cp: u32) {
    if let Some(ch) = char::from_u32(cp) {
        s.push(ch);
    }
}

/// Runtime hints for a single [`HangulFsm::feed_with`] call.
///
/// The FSM stays time-agnostic; host layers that know real keyboard
/// timing (e.g. the Tauri session) pass [`Self::within_chord`] so
/// near-simultaneous keypresses of a 4-jamo moachigi syllable converge
/// even when the OS delivers events in a non-canonical order.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct FeedOptions {
    /// `true` when this keystroke arrived within the host-defined
    /// "chord window" of the previous keystroke. Only consulted in
    /// [`ComposeMode::Moachigi`].
    pub within_chord: bool,
}

/// Which composition ordering rules the FSM enforces.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum ComposeMode {
    /// Strict Cho → Jung → Jong order. Any out-of-order input commits
    /// the current syllable and starts a new one.
    #[default]
    Sequential,
    /// Order-independent ("모아치기"). Each jamo fills its slot; the
    /// FSM commits only when a slot would be overwritten.
    Moachigi,
}

/// Output produced by a single call to [`HangulFsm::feed`] and related
/// methods.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FsmEvent {
    /// Nothing changed.
    Nothing,
    /// Preedit updated; no commit.
    Preedit(String),
    /// A commit happened; no preedit remains.
    Commit(String),
    /// A commit happened *and* a fresh preedit is now in progress.
    CommitThenPreedit {
        /// Finalized text.
        commit: String,
        /// New in-progress composition.
        preedit: String,
    },
}

impl FsmEvent {
    /// Extract the commit payload, if any.
    pub fn commit_str(&self) -> Option<&str> {
        match self {
            Self::Commit(s) | Self::CommitThenPreedit { commit: s, .. } => Some(s.as_str()),
            _ => None,
        }
    }

    /// Extract the preedit payload, if any.
    pub fn preedit_str(&self) -> Option<&str> {
        match self {
            Self::Preedit(s) | Self::CommitThenPreedit { preedit: s, .. } => Some(s.as_str()),
            _ => None,
        }
    }
}

/// Which composition slot a given input filled. Used by the FSM to
/// implement the optional "자소 덮어쓰기" typo-correction rule: when the
/// *previous* keystroke filled the Cho slot and the *current* keystroke
/// would also target Cho, replace the old value instead of doubling or
/// starting a new syllable. Currently only the Cho slot participates;
/// the enum is a placeholder for future Jung/Jong extensions.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum SlotRole {
    Cho,
}

/// The Hangul composition FSM.
#[derive(Clone, Debug, Default)]
pub struct HangulFsm {
    state: CompositionState,
    mode: ComposeMode,
    /// Whether the current Jong slot was set (or last updated) by a
    /// dual-role key — i.e. a 두벌식 consonant that could equally have
    /// started the next syllable as a Cho. Used to gate 받침 이동 in
    /// Moachigi mode: a dual-role jong followed by a vowel should move
    /// to the next syllable's Cho (matching Sequential mode), while a
    /// pure-jong key (세벌식) should commit the syllable and leave the
    /// naked vowel as the start of the next one.
    jong_from_dual: bool,
    /// When `true`, two consecutive inputs that would fill the same
    /// slot replace the old value instead of compounding (복모음/
    /// 겹받침/쌍자음) or committing. Opt-in typo-correction aid.
    same_slot_overwrite: bool,
    /// Which slot the previous accepted input filled. Reset on commit,
    /// cancel, flush, backspace, and mode changes so that "연속" means
    /// "the very last keystroke", not "anywhere in this syllable".
    last_slot: Option<SlotRole>,
}

impl HangulFsm {
    /// Construct an empty FSM in [`ComposeMode::Sequential`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Construct an empty FSM in the given mode.
    pub fn with_mode(mode: ComposeMode) -> Self {
        Self {
            state: CompositionState::default(),
            mode,
            jong_from_dual: false,
            same_slot_overwrite: false,
            last_slot: None,
        }
    }

    /// Active compose mode.
    pub fn mode(&self) -> ComposeMode {
        self.mode
    }

    /// Switch compose mode. Safe to call mid-composition.
    pub fn set_mode(&mut self, mode: ComposeMode) {
        self.mode = mode;
        // Switching mode resets the "previous input slot" tracker: any
        // consecutive-slot relationship across the switch is spurious.
        self.last_slot = None;
    }

    /// Whether the optional 자소 덮어쓰기 typo-correction rule is on.
    pub fn same_slot_overwrite(&self) -> bool {
        self.same_slot_overwrite
    }

    /// Enable or disable 자소 덮어쓰기. Safe to toggle mid-composition.
    pub fn set_same_slot_overwrite(&mut self, on: bool) {
        self.same_slot_overwrite = on;
        self.last_slot = None;
    }

    /// Current state.
    pub fn state(&self) -> CompositionState {
        self.state
    }

    /// Whether a syllable is currently in progress.
    pub fn is_composing(&self) -> bool {
        !self.state.is_empty()
    }

    /// Render the current preedit as conjoining Jamo.
    pub fn preedit_string(&self) -> String {
        self.state.render()
    }

    /// Feed one [`JamoInput`] and advance the FSM according to the
    /// active [`ComposeMode`].
    ///
    /// # Panics
    ///
    /// Panics only if a [`Layout`](crate::Layout) emits a code point
    /// outside the valid conjoining-Jamo ranges. Built-in layouts
    /// never do this; user layouts are validated at load time.
    pub fn feed(&mut self, input: JamoInput) -> FsmEvent {
        self.feed_with(input, FeedOptions::default())
    }

    /// Like [`Self::feed`] but accepts runtime hints.
    ///
    /// In [`ComposeMode::Moachigi`], callers that track real keyboard
    /// timing should set [`FeedOptions::within_chord`] when the key
    /// arrives within a "chord window" (e.g. 50 ms) of the previous
    /// key. This relaxes the slot-boundary guard so the four jamo of a
    /// syllable like 원 (ᄋ + ᅮ + ᅥ + ᆫ) gather into one syllable
    /// regardless of the OS-delivered event order — e.g. J → 9 → S
    /// → T, where the 받침 arrives between the two compound-vowel
    /// components.
    ///
    /// [`ComposeMode::Sequential`] ignores `opts` — sequential input is
    /// order-sensitive by design.
    pub fn feed_with(&mut self, input: JamoInput, opts: FeedOptions) -> FsmEvent {
        match self.mode {
            ComposeMode::Sequential => self.feed_sequential(input),
            ComposeMode::Moachigi => self.feed_moachigi(input, opts),
        }
    }

    /// Commit the current preedit (if any) and return an event describing it.
    pub fn flush(&mut self) -> FsmEvent {
        if self.is_composing() {
            let out = self.preedit_string();
            self.state = CompositionState::default();
            self.jong_from_dual = false;
            self.last_slot = None;
            FsmEvent::Commit(out)
        } else {
            self.last_slot = None;
            FsmEvent::Nothing
        }
    }

    /// Like [`Self::flush`] but returns the committed string directly.
    pub fn flush_string(&mut self) -> String {
        match self.flush() {
            FsmEvent::Commit(s) => s,
            _ => String::new(),
        }
    }

    /// Discard the current composition without committing.
    pub fn cancel(&mut self) -> FsmEvent {
        self.jong_from_dual = false;
        self.last_slot = None;
        if self.is_composing() {
            self.state = CompositionState::default();
            FsmEvent::Preedit(String::new())
        } else {
            FsmEvent::Nothing
        }
    }

    /// Remove one Jamo piece from the current composition.
    ///
    /// Compound vowels / compound jongs decompose by one component
    /// before the slot empties entirely. Returns [`FsmEvent::Nothing`]
    /// if nothing was composing.
    ///
    /// # Panics
    ///
    /// Panics only if the FSM reached an internally inconsistent
    /// state with an out-of-range decomposition result; unreachable
    /// via the public API.
    pub fn backspace(&mut self) -> FsmEvent {
        // Backspace breaks any consecutive-same-slot pairing and resets
        // the dual-role Jong marker if we just cleared the Jong slot.
        self.last_slot = None;
        if let Some(t) = self.state.jong {
            self.state.jong = decompose_jong(t.codepoint()).map(|cp| {
                Jong::from_codepoint(cp).expect("decompose_jong produces a valid Jong")
            });
            if self.state.jong.is_none() {
                self.jong_from_dual = false;
            }
            return FsmEvent::Preedit(self.preedit_string());
        }
        if let Some(j) = self.state.jung {
            self.state.jung = decompose_jung(j.codepoint()).map(|cp| {
                Jung::from_codepoint(cp).expect("decompose_jung produces a valid Jung")
            });
            return FsmEvent::Preedit(self.preedit_string());
        }
        if self.state.cho.is_some() {
            self.state.cho = None;
            return FsmEvent::Preedit(self.preedit_string());
        }
        FsmEvent::Nothing
    }

    // ────────────────────── Sequential mode ──────────────────────

    #[allow(
        clippy::too_many_lines,
        clippy::match_same_arms,
        clippy::similar_names, // (cho, jung, jong) are the three Hangul slot names — intentional
    )]
    fn feed_sequential(&mut self, input: JamoInput) -> FsmEvent {
        let (cho, jung, jong) = (self.state.cho, self.state.jung, self.state.jong);

        match input {
            JamoInput::Cons { cho: cho_in, jong: jong_in } => {
                // Nothing provided: ignore.
                if cho_in.is_none() && jong_in.is_none() {
                    return FsmEvent::Nothing;
                }

                match (cho, jung, jong) {
                    (None, _, _) => {
                        // Empty: start a new syllable if we have a Cho.
                        if let Some(c) = cho_in {
                            self.state.cho = Some(
                                Cho::from_codepoint(c).expect("layout emitted invalid Cho"),
                            );
                            FsmEvent::Preedit(self.preedit_string())
                        } else if let Some(j) = jong_in {
                            // Lone Jong before any syllable: commit standalone.
                            single_char_commit(j)
                        } else {
                            FsmEvent::Nothing
                        }
                    }
                    (Some(existing_cho), None, None) => {
                        // Just Cho: a repeated doubleable consonant merges
                        // in place (ㄱㄱ→ㄲ, ㄷㄷ→ㄸ, ㅂㅂ→ㅃ, ㅅㅅ→ㅆ, ㅈㅈ→ㅉ);
                        // otherwise commit current Cho and start a new syllable.
                        if let Some(c) = cho_in {
                            if let Some(doubled) =
                                compose_cho_double(existing_cho.codepoint(), c)
                            {
                                self.state.cho = Some(
                                    Cho::from_codepoint(doubled)
                                        .expect("compose_cho_double is in Cho range"),
                                );
                                FsmEvent::Preedit(self.preedit_string())
                            } else {
                                self.commit_and_start_cho(c)
                            }
                        } else {
                            FsmEvent::Nothing
                        }
                    }
                    (Some(_), Some(_), None) => {
                        // ChoJung: try to attach a Jong first, else new Cho.
                        if let Some(t) = jong_in {
                            self.state.jong = Some(
                                Jong::from_codepoint(t).expect("layout emitted invalid Jong"),
                            );
                            FsmEvent::Preedit(self.preedit_string())
                        } else if let Some(c) = cho_in {
                            self.commit_and_start_cho(c)
                        } else {
                            FsmEvent::Nothing
                        }
                    }
                    (Some(_), Some(_), Some(t0)) => {
                        // `ChoJungJong`: try compound Jong first; otherwise
                        // commit the syllable. If the incoming input is a
                        // **Jong-only** key that doesn't compose (e.g. 각
                        // followed by an unrelated final), commit the
                        // current syllable and start a new state that
                        // carries the orphan Jong instead of silently
                        // dropping it — the user can still build something
                        // with it on the next keystroke.
                        if let Some(t) = jong_in {
                            if let Some(combined) = compose_jong(t0.codepoint(), t) {
                                self.state.jong = Some(
                                    Jong::from_codepoint(combined)
                                        .expect("compound is in range"),
                                );
                                return FsmEvent::Preedit(self.preedit_string());
                            }
                            // Absorb: existing simple Jong is the first
                            // component of the incoming direct-compound
                            // Jong (e.g. state=ᆯ + Shift+D=ᆲ → ᆲ).
                            if decompose_jong(t) == Some(t0.codepoint()) {
                                self.state.jong = Some(
                                    Jong::from_codepoint(t)
                                        .expect("layout emitted invalid Jong"),
                                );
                                return FsmEvent::Preedit(self.preedit_string());
                            }
                            if cho_in.is_none() {
                                let prev = self.preedit_string();
                                self.state = CompositionState {
                                    jong: Jong::from_codepoint(t),
                                    ..CompositionState::default()
                                };
                                return FsmEvent::CommitThenPreedit {
                                    commit: prev,
                                    preedit: self.preedit_string(),
                                };
                            }
                        }
                        if let Some(c) = cho_in {
                            self.commit_and_start_cho(c)
                        } else {
                            FsmEvent::Nothing
                        }
                    }
                    _ => FsmEvent::Nothing,
                }
            }
            JamoInput::Jung(v) => {
                match (cho, jung, jong) {
                    (None, _, _) => {
                        // Lone vowel without Cho: commit standalone.
                        single_char_commit(v)
                    }
                    (Some(_), None, None) => {
                        self.state.jung = Some(
                            Jung::from_codepoint(v).expect("layout emitted invalid Jung"),
                        );
                        FsmEvent::Preedit(self.preedit_string())
                    }
                    (Some(_), Some(j0), None) => {
                        if let Some(combined) = compose_jung(j0.codepoint(), v) {
                            self.state.jung = Some(
                                Jung::from_codepoint(combined)
                                    .expect("compound is in range"),
                            );
                            FsmEvent::Preedit(self.preedit_string())
                        } else {
                            // Can't compound: commit the syllable + the new
                            // vowel together.
                            let prev = self.preedit_string();
                            self.state = CompositionState::default();
                            let mut commit = prev;
                            push_cp(&mut commit, v);
                            FsmEvent::Commit(commit)
                        }
                    }
                    (Some(c0), Some(j0), Some(t0)) => {
                        // 받침 이동.
                        let (keep, move_cho) = split_jong(t0.codepoint());
                        let committed = CompositionState {
                            cho: Some(c0),
                            jung: Some(j0),
                            jong: keep.map(|cp| {
                                Jong::from_codepoint(cp).expect("split keep is valid Jong")
                            }),
                        };
                        let commit = committed.render();
                        self.state = CompositionState {
                            cho: Some(
                                Cho::from_codepoint(move_cho).expect("split emits valid Cho"),
                            ),
                            jung: Some(
                                Jung::from_codepoint(v).expect("layout emitted invalid Jung"),
                            ),
                            jong: None,
                        };
                        FsmEvent::CommitThenPreedit {
                            commit,
                            preedit: self.preedit_string(),
                        }
                    }
                    _ => FsmEvent::Nothing,
                }
            }
        }
    }

    // ─────────────────────── Moachigi mode ───────────────────────

    fn feed_moachigi(&mut self, input: JamoInput, opts: FeedOptions) -> FsmEvent {
        match input {
            JamoInput::Cons { cho: cho_in, jong: jong_in } => {
                if cho_in.is_none() && jong_in.is_none() {
                    return FsmEvent::Nothing;
                }
                // Once cho + jung + jong are all filled the syllable is
                // "done" from the user's point of view — the next key
                // starts a new syllable rather than bleeding into the
                // existing one. Without this guard, 안(ᄋ+ᅡ+ᆫ) + Shift+3
                // (ᆽ) would silently compound into 앉 even though the user
                // has already moved on to the next word.
                let syllable_complete = self.state.cho.is_some()
                    && self.state.jung.is_some()
                    && self.state.jong.is_some();
                // Prefer the Jong role for dual-role keys when (a) the
                // Jong slot is empty so the key simply fills it, or
                // (b) the slot already holds a Jong that would
                // compose / be absorbed with the incoming one (e.g.
                // state=안 + dual `w`(ᄌ/ᆽ) → 앉, not 안+ᄌ). Otherwise
                // fall through to the Cho role.
                let is_dual = cho_in.is_some() && jong_in.is_some();
                // prefer_jong: dual-role 키를 jong 슬롯에 먼저 넣을지 결정.
                //   (None, Some): syllable 이 아직 미완이면 jong 으로 채운다.
                //   (Some, Some): 기존 jong 과 compose/absorb 가 가능할 때만
                //     jong 으로 간다. 이 중 syllable 이 완성된 상태의 compose
                //     는 chord hint (짧은 시간 내 연타) 가 있을 때만 허용해
                //     "찬 + ᄒ → 찮", "달 + ᄀ → 닭" 류 겹받침을 모아치기로
                //     입력할 수 있게 한다. chord 없이 또 다른 dual 이 오면
                //     새 음절을 시작한다 (안 → 앉 오조합 방지).
                let prefer_jong = self.state.cho.is_some()
                    && jong_in.is_some()
                    && match (self.state.jong, jong_in) {
                        (None, Some(_)) => !syllable_complete,
                        (Some(existing), Some(t)) => {
                            let composes = compose_jong(existing.codepoint(), t).is_some()
                                || decompose_jong(t) == Some(existing.codepoint());
                            composes && (!syllable_complete || opts.within_chord)
                        }
                        _ => false,
                    };
                if prefer_jong {
                    let t = jong_in.expect("checked above");
                    return self.place_jong(t, is_dual);
                }
                if let Some(c) = cho_in {
                    return self.place_cho(c);
                }
                if let Some(t) = jong_in {
                    return self.place_jong(t, false);
                }
                FsmEvent::Nothing
            }
            JamoInput::Jung(v) => self.place_jung(v, opts),
        }
    }

    fn place_cho(&mut self, cho_cp: u32) -> FsmEvent {
        let new_cho = Cho::from_codepoint(cho_cp).expect("layout emitted invalid Cho");
        // 자소 덮어쓰기: if the previous input was also a Cho, replace
        // the current Cho in-place (first input discarded) instead of
        // doubling or starting a new syllable.
        if self.same_slot_overwrite
            && self.last_slot == Some(SlotRole::Cho)
            && self.state.cho.is_some()
        {
            self.state.cho = Some(new_cho);
            self.last_slot = Some(SlotRole::Cho);
            return FsmEvent::Preedit(self.preedit_string());
        }
        let ev = match self.state.cho {
            None => {
                self.state.cho = Some(new_cho);
                FsmEvent::Preedit(self.preedit_string())
            }
            Some(existing) => {
                // Same doubleable consonant pressed twice and we haven't
                // moved past the Cho slot yet → merge in place into the
                // double form (ㄱㄱ→ㄲ, etc.). Otherwise the second Cho
                // starts a new syllable.
                if self.state.jung.is_none() && self.state.jong.is_none() {
                    if let Some(doubled) =
                        compose_cho_double(existing.codepoint(), cho_cp)
                    {
                        self.state.cho = Some(
                            Cho::from_codepoint(doubled).expect("compose_cho_double is valid Cho"),
                        );
                        self.last_slot = Some(SlotRole::Cho);
                        return FsmEvent::Preedit(self.preedit_string());
                    }
                }
                self.commit_with_new_state(CompositionState {
                    cho: Some(new_cho),
                    ..CompositionState::default()
                })
            }
        };
        self.last_slot = Some(SlotRole::Cho);
        ev
    }

    fn place_jong(&mut self, jong_cp: u32, from_dual: bool) -> FsmEvent {
        let new_jong = Jong::from_codepoint(jong_cp).expect("layout emitted invalid Jong");
        match self.state.jong {
            None => {
                self.state.jong = Some(new_jong);
                self.jong_from_dual = from_dual;
                FsmEvent::Preedit(self.preedit_string())
            }
            Some(existing) => {
                // Moachigi treats the 4-jamo syllable as a chord, so the
                // 겹받침 components may arrive in either order (넋 = ᄂ +
                // ᅥ + ᆨ + ᆺ or ᄂ + ᅥ + ᆺ + ᆨ). Try canonical first,
                // then reverse.
                let combined = compose_jong(existing.codepoint(), jong_cp)
                    .or_else(|| compose_jong(jong_cp, existing.codepoint()));
                if let Some(combined) = combined {
                    self.state.jong = Some(
                        Jong::from_codepoint(combined).expect("compound is valid Jong"),
                    );
                    // 진짜 겹받침이 형성됐으므로 dual-role 추정은 해제한다.
                    self.jong_from_dual = false;
                    FsmEvent::Preedit(self.preedit_string())
                } else if decompose_jong(jong_cp) == Some(existing.codepoint()) {
                    // Absorb: existing simple Jong is the first component of
                    // the incoming direct-compound Jong (state=ᆯ + ᆲ → ᆲ).
                    self.state.jong = Some(new_jong);
                    self.jong_from_dual = false;
                    FsmEvent::Preedit(self.preedit_string())
                } else {
                    let ev = self.commit_with_new_state(CompositionState {
                        jong: Some(new_jong),
                        ..CompositionState::default()
                    });
                    self.jong_from_dual = from_dual;
                    ev
                }
            }
        }
    }

    fn place_jung(&mut self, jung_cp: u32, opts: FeedOptions) -> FsmEvent {
        let new_jung = Jung::from_codepoint(jung_cp).expect("layout emitted invalid Jung");
        match self.state.jung {
            None => {
                self.state.jung = Some(new_jung);
                FsmEvent::Preedit(self.preedit_string())
            }
            Some(existing) => {
                // Only form a compound vowel (e.g. ᅩ+ᅡ → ᅪ) while the
                // syllable is still being built — if cho + jung + jong
                // are all present the syllable is "done" and the new
                // vowel belongs to the next syllable (jong-only 로 들어온
                // 곡 + ᅡ 는 곡 + 아 로 읽혀야지 곽 이 되어선 안 된다;
                // dual-role 로 들어온 jong 은 아래 받침 이동 분기에서 다시
                // 처리한다).
                //
                // Moachigi treats the 4-jamo syllable as a chord, so the
                // compound-vowel components may arrive in either order
                // (원 = ᄋ + ᅮ + ᅥ + ᆫ or ᄋ + ᅥ + ᅮ + ᆫ). Try the
                // canonical pair first, then the reverse.
                //
                // When `opts.within_chord` is set, the host has decided
                // the keystroke is part of an ongoing chord (timing-wise
                // nearly simultaneous with the previous one), so the
                // "syllable complete" guard is bypassed: a late-arriving
                // 2nd compound-vowel component still folds into the
                // current syllable (원 via J→9→S→T), instead of
                // committing "운" and starting "어".
                let syllable_complete = self.state.cho.is_some()
                    && self.state.jong.is_some();
                // jong 이 dual-role 키(두벌식 자음)로 들어와 있으면 이어지는
                // 모음은 받침 이동으로 처리해야 한다. chord window 안이어도
                // 복모음 합성을 먼저 시도하면 "핫 + ᅵ → 햇" 같은 오조합이
                // 나므로, 이 경우엔 복모음 분기를 건너뛴다.
                let try_compound =
                    (!syllable_complete || opts.within_chord) && !self.jong_from_dual;
                if try_compound {
                    let combined = compose_jung(existing.codepoint(), jung_cp)
                        .or_else(|| compose_jung(jung_cp, existing.codepoint()));
                    if let Some(combined) = combined {
                        self.state.jung = Some(
                            Jung::from_codepoint(combined).expect("compound is valid Jung"),
                        );
                        return FsmEvent::Preedit(self.preedit_string());
                    }
                }
                // 받침 이동 for dual-role jongs: 두벌식에서 자음 키는 cho/jong
                // 겸용이라 cho+jung 상태에서 들어온 dual 키를 일단 jong 으로
                // 추정(place_jong)했는데, 이어서 모음이 오면 그 자음은 사실
                // 다음 음절의 cho 였다는 신호다. 현재 음절의 jong 을 떼어
                // 새 음절의 cho 로 옮기고, 이번 모음이 그 cho 에 붙는다
                // (Sequential 과 동일 동작). 세벌식 jong-only 키로 들어온
                // jong 은 jong_from_dual=false 라 영향 없음.
                if syllable_complete && self.jong_from_dual {
                    if let (Some(existing_jong), Some(existing_cho)) =
                        (self.state.jong, self.state.cho)
                    {
                        let (keep, move_cho) = split_jong(existing_jong.codepoint());
                        let committed = CompositionState {
                            cho: Some(existing_cho),
                            jung: Some(existing),
                            jong: keep.and_then(Jong::from_codepoint),
                        };
                        let commit = committed.render();
                        self.state = CompositionState {
                            cho: Cho::from_codepoint(move_cho),
                            jung: Some(new_jung),
                            jong: None,
                        };
                        self.jong_from_dual = false;
                        self.last_slot = None;
                        return FsmEvent::CommitThenPreedit {
                            commit,
                            preedit: self.preedit_string(),
                        };
                    }
                }
                // Moachigi treats slot collision as a syllable boundary —
                // commit the current syllable and start a new one whose
                // only filled slot is this Jung.
                self.commit_with_new_state(CompositionState {
                    jung: Some(new_jung),
                    ..CompositionState::default()
                })
            }
        }
    }

    fn commit_with_new_state(&mut self, new_state: CompositionState) -> FsmEvent {
        // 새 음절을 시작하므로 dual-role 추정 플래그도 함께 초기화한다.
        // jong-bearing new_state 를 만든 호출자(place_jong) 는 이 호출 뒤에
        // 명시적으로 다시 세팅한다.
        self.jong_from_dual = false;
        if self.state.is_empty() {
            self.state = new_state;
            return FsmEvent::Preedit(self.preedit_string());
        }
        let commit = self.preedit_string();
        self.state = new_state;
        FsmEvent::CommitThenPreedit {
            commit,
            preedit: self.preedit_string(),
        }
    }

    fn commit_and_start_cho(&mut self, cho_cp: u32) -> FsmEvent {
        let new_cho = Cho::from_codepoint(cho_cp).expect("layout emitted invalid Cho");
        self.commit_with_new_state(CompositionState {
            cho: Some(new_cho),
            ..CompositionState::default()
        })
    }
}

fn single_char_commit(cp: u32) -> FsmEvent {
    match char::from_u32(cp) {
        Some(ch) => FsmEvent::Commit(ch.to_string()),
        None => FsmEvent::Nothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jung(cp: u32) -> JamoInput {
        JamoInput::Jung(cp)
    }

    fn dual(cho: u32, jong: u32) -> JamoInput {
        JamoInput::cho_dual(cho, jong)
    }

    #[test]
    fn empty_vowel_commits_standalone_in_sequential() {
        let mut f = HangulFsm::new();
        assert!(matches!(f.feed(jung(0x1161)), FsmEvent::Commit(ref s) if s == "\u{1161}"));
        assert!(!f.is_composing());
    }

    #[test]
    fn empty_cho_enters_preedit() {
        let mut f = HangulFsm::new();
        let ev = f.feed(dual(0x1100, 0x11A8));
        assert_eq!(ev.preedit_str(), Some("\u{1100}"));
    }

    #[test]
    fn sequential_cho_jung_preedit() {
        let mut f = HangulFsm::new();
        let _ = f.feed(dual(0x1100, 0x11A8));
        let ev = f.feed(jung(0x1161));
        assert_eq!(ev.preedit_str(), Some("\u{1100}\u{1161}"));
    }

    /// ime_key_input 이 매 키 뒤에 프론트엔드로 보내는 preedit 문자열과
    /// 완전히 동일한 값을 재현한다. 프론트엔드는 이 문자열을 그대로
    /// preedit span 에 박아 넣으므로, 여기서 기대 값이 나오면 "렌더 경로"
    /// 까지는 정상이고 UI 문제로 범위가 좁혀진다.
    /// 모아치기 '입' 합성 — 모든 하ㄴ글 레이아웃 + 모든 키 순서에서 '입'
    /// 한 음절이 정상 합성되어야 한다. JamoInput 수준에서 6가지 순서(3!)
    /// 를 모두 확인하고, 끝에 공백을 먹여 commit 이 실제로 일어나는지도
    /// 점검한다.
    #[test]
    fn moachigi_ip_all_orders() {
        const CHO_O: u32 = 0x110B;   // ᄋ
        const JUNG_I: u32 = 0x1175;  // ᅵ
        const JONG_B: u32 = 0x11B8;  // ᆸ
        let permutations: [[JamoInput; 3]; 6] = [
            [JamoInput::cho_only(CHO_O), jung(JUNG_I), JamoInput::jong_only(JONG_B)],
            [JamoInput::cho_only(CHO_O), JamoInput::jong_only(JONG_B), jung(JUNG_I)],
            [jung(JUNG_I), JamoInput::cho_only(CHO_O), JamoInput::jong_only(JONG_B)],
            [jung(JUNG_I), JamoInput::jong_only(JONG_B), JamoInput::cho_only(CHO_O)],
            [JamoInput::jong_only(JONG_B), JamoInput::cho_only(CHO_O), jung(JUNG_I)],
            [JamoInput::jong_only(JONG_B), jung(JUNG_I), JamoInput::cho_only(CHO_O)],
        ];
        for (idx, perm) in permutations.iter().enumerate() {
            // 1) chord 힌트 없이
            let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
            for input in perm.iter() {
                let _ = f.feed(*input);
            }
            assert_eq!(
                crate::to_nfc_syllable(&f.preedit_string()),
                "입",
                "no-chord order #{idx} ({:?}) 의 preedit 이 '입' 이 아님",
                perm,
            );
            // 2) chord 힌트 ON (초/중/종이 거의 동시 입력되는 순간)
            let mut g = HangulFsm::with_mode(ComposeMode::Moachigi);
            let chord = FeedOptions { within_chord: true };
            for input in perm.iter() {
                let _ = g.feed_with(*input, chord);
            }
            assert_eq!(
                crate::to_nfc_syllable(&g.preedit_string()),
                "입",
                "chord order #{idx} ({:?}) 의 preedit 이 '입' 이 아님",
                perm,
            );
        }
    }

    /// Dual-role('ㅂ' 이 cho+jong 둘 다 가능) 키도 포함한 두벌식-스타일
    /// 입력으로 '입' 이 제대로 합성되는지. 두벌식은 모든 자음이 dual 이라
    /// cho vs jong 선택 로직이 관여한다. 첫 키는 항상 cho 로 해석되는
    /// 게 자연스러운 타자 흐름이므로, 여기서는 **정상 순서** (ㅇ → ㅣ
    /// → ㅂ) 와 chord 힌트 유무 두 경우만 확인한다.
    #[test]
    fn moachigi_ip_dubeolsik_dual_role() {
        let seq: [JamoInput; 3] = [
            JamoInput::cho_dual(0x110B, 0x11BC),
            jung(0x1175),
            JamoInput::cho_dual(0x1107, 0x11B8),
        ];
        // 1) chord 힌트 없이
        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        for input in seq {
            let _ = f.feed(input);
        }
        assert_eq!(crate::to_nfc_syllable(&f.preedit_string()), "입");
        // 2) chord 힌트 ON
        let chord = FeedOptions { within_chord: true };
        let mut g = HangulFsm::with_mode(ComposeMode::Moachigi);
        for input in seq {
            let _ = g.feed_with(input, chord);
        }
        assert_eq!(crate::to_nfc_syllable(&g.preedit_string()), "입");
    }

    /// End-to-end '입' — 실제 키 이벤트(세벌식 최종 · 두벌식) 를 레이아웃
    /// 맵핑까지 태워서 합성한다. JamoInput 직접 주입 테스트가 못 잡는
    /// 레이아웃 버그(코드포인트 오매핑 등) 를 잡는다.
    #[test]
    fn moachigi_ip_end_to_end() {
        use crate::layout::dubeolsik::Dubeolsik;
        use crate::layout::key::{KeyCode, KeyEvent};
        use crate::layout::sebeolsik_final::SebeolsikFinal;
        use crate::{Layout, LayoutOutput};

        // (이름, 레이아웃 map 에서 ᄋ → ᅵ → ᆸ 로 이어지는 키 시퀀스)
        let cases: [(&str, Vec<KeyCode>); 2] = [
            ("세벌식 최종 (J D 3)",
             vec![KeyCode::KeyJ, KeyCode::KeyD, KeyCode::Digit3]),
            ("두벌식 (D L Q)",
             vec![KeyCode::KeyD, KeyCode::KeyL, KeyCode::KeyQ]),
        ];
        for (label, keys) in cases {
            // 각 케이스를 sebeolsik_final 이면 SebeolsikFinal 로, 두벌식
            // 이면 Dubeolsik 로 매핑해 feed 한다.
            let sebeol_final = SebeolsikFinal;
            let dubeol = Dubeolsik;
            let layout: &dyn Layout = if label.starts_with("세벌식") {
                &sebeol_final
            } else {
                &dubeol
            };
            let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
            for code in keys {
                match layout.map(&KeyEvent::plain(code)) {
                    LayoutOutput::Jamo(input) => { let _ = f.feed(input); }
                    other => panic!("{label}: unexpected layout output {other:?}"),
                }
            }
            assert_eq!(
                crate::to_nfc_syllable(&f.preedit_string()),
                "입",
                "{label}: 모아치기 '입' 실패",
            );
        }
    }

    // Bug reproduction: "입수니" 를 두벌식 + Moachigi 로 입력하면 실제
    // 사용자가 보는 결과는 "입쉬ㄴ" 이라고 한다. 이 테스트가 실패하는
    // 시점에서 어떤 시퀀스/플래그 조합이 원인인지 범위가 좁혀진다.
    #[test]
    fn moachigi_ipsuni_dubeolsik() {
        // 두벌식 키 입력을 JamoInput 레벨에서 재현:
        //   D L Q T N S L
        //   = ㅇ_dual ㅣ ㅂ_dual ㅅ_dual ㅜ ㄴ_dual ㅣ
        let inputs = [
            JamoInput::cho_dual(0x110B, 0x11BC), // ㅇ
            jung(0x1175),                          // ㅣ
            JamoInput::cho_dual(0x1107, 0x11B8), // ㅂ
            JamoInput::cho_dual(0x1109, 0x11BA), // ㅅ
            jung(0x116E),                          // ㅜ
            JamoInput::cho_dual(0x1102, 0x11AB), // ㄴ
            jung(0x1175),                          // ㅣ
        ];
        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        let mut committed = String::new();
        for (i, input) in inputs.into_iter().enumerate() {
            let ev = f.feed(input);
            match ev {
                FsmEvent::Commit(s) | FsmEvent::CommitThenPreedit { commit: s, .. } => {
                    committed.push_str(&s);
                }
                _ => {}
            }
            eprintln!(
                "step {}: committed={:?} preedit={:?}",
                i,
                crate::to_nfc_syllable(&committed),
                crate::to_nfc_syllable(&f.preedit_string()),
            );
        }
        let total = format!("{}{}", committed, f.preedit_string());
        assert_eq!(
            crate::to_nfc_syllable(&total),
            "입수니",
            "expected 입수니, got {:?}",
            crate::to_nfc_syllable(&total),
        );
    }

    #[test]
    fn moachigi_weon_preedit_display_after_each_key() {
        use crate::layout::key::{KeyCode, KeyEvent};
        use crate::layout::sebeolsik_final::SebeolsikFinal;
        use crate::to_display_text;
        use crate::Layout;
        use crate::LayoutOutput;
        let layout = SebeolsikFinal;
        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        let steps = [
            (KeyCode::KeyJ, "ㅇ"),  // ᄋ 만 → compat "ㅇ"
            (KeyCode::Digit9, "우"), // ᄋ + ᅮ → NFC "우"
            (KeyCode::KeyT, "워"),   // ᄋ + ᅯ(=ᅮ+ᅥ) → "워"
            (KeyCode::KeyS, "원"),   // + ᆫ → "원"
        ];
        for (code, expected) in steps {
            if let LayoutOutput::Jamo(input) = layout.map(&KeyEvent::plain(code)) {
                let _ = f.feed(input);
            }
            let display = to_display_text(&f.preedit_string());
            assert_eq!(
                display, expected,
                "preedit after {:?} expected {:?}, got {:?}", code, expected, display
            );
        }
    }

    #[test]
    fn sequential_four_jamo_weon_dubeolsik_end_to_end() {
        // Dubeolsik (기본 Sequential) 로 D(ᄋ) N(ᅮ) J(ᅥ) S(ᆫ) 연타 → '원'.
        use crate::layout::dubeolsik::Dubeolsik;
        use crate::layout::key::{KeyCode, KeyEvent};
        use crate::{Layout, LayoutOutput};
        let layout = Dubeolsik;
        let mut f = HangulFsm::new(); // default Sequential
        for code in [KeyCode::KeyD, KeyCode::KeyN, KeyCode::KeyJ, KeyCode::KeyS] {
            if let LayoutOutput::Jamo(input) = layout.map(&KeyEvent::plain(code)) {
                let _ = f.feed(input);
            }
        }
        assert_eq!(crate::to_nfc_syllable(&f.preedit_string()), "원");
    }

    #[test]
    fn moachigi_four_jamo_variety() {
        // 복모음 + 받침 조합 4-자모 음절을 한 번에 확인. 각 케이스는
        // (cho_cp, jung_a, jung_b, jong_cp, expected).
        let cases = [
            // 원 = ᄋ + ᅮ + ᅥ(→ᅯ) + ᆫ
            (0x110Bu32, 0x116Eu32, 0x1165u32, 0x11ABu32, "원"),
            // 왔 = ᄋ + ᅩ + ᅡ(→ᅪ) + ᆻ
            (0x110B,    0x1169,    0x1161,    0x11BB,    "왔"),
            // 쉽 = ᄉ + ᅮ + ᅵ(→ᅱ) + ᆸ
            (0x1109,    0x116E,    0x1175,    0x11B8,    "쉽"),
            // 괜 = ᄀ + ᅩ + ᅢ(→ᅫ) + ᆫ
            (0x1100,    0x1169,    0x1162,    0x11AB,    "괜"),
            // 웬 = ᄋ + ᅮ + ᅦ(→ᅰ) + ᆫ
            (0x110B,    0x116E,    0x1166,    0x11AB,    "웬"),
            // 흰 = ᄒ + ᅳ + ᅵ(→ᅴ) + ᆫ
            (0x1112,    0x1173,    0x1175,    0x11AB,    "흰"),
        ];
        for (cho_cp, a, b, jong, want) in cases {
            let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
            let _ = f.feed(JamoInput::cho_only(cho_cp));
            let _ = f.feed(jung(a));
            let _ = f.feed(jung(b));
            let _ = f.feed(JamoInput::jong_only(jong));
            assert_eq!(
                crate::to_nfc_syllable(&f.preedit_string()),
                want,
                "moachigi 4-jamo failed: cho=U+{:04X} jung={:04X}+{:04X} jong=U+{:04X}",
                cho_cp, a, b, jong
            );
        }
    }

    #[test]
    fn moachigi_four_jamo_sebeolsik_end_to_end() {
        // Sebeolsik Final 레이아웃을 통해 실제 키 이벤트 4개 (J 9 T S) 가
        // '원' 하나로 모아지는지 확인한다. FSM 단독 테스트는 통과해도
        // 레이아웃이 jamo 를 다르게 뽑으면 깨질 수 있으니 end-to-end 로.
        use crate::layout::key::{KeyCode, KeyEvent};
        use crate::layout::sebeolsik_final::SebeolsikFinal;
        use crate::Layout;
        use crate::LayoutOutput;
        let layout = SebeolsikFinal;
        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        for code in [KeyCode::KeyJ, KeyCode::Digit9, KeyCode::KeyT, KeyCode::KeyS] {
            let ev = KeyEvent::plain(code);
            match layout.map(&ev) {
                LayoutOutput::Jamo(input) => { let _ = f.feed(input); }
                other => panic!("unexpected layout output for {:?}: {:?}", code, other),
            }
        }
        assert_eq!(crate::to_nfc_syllable(&f.preedit_string()), "원");
    }

    #[test]
    fn moachigi_four_jamo_weon_cho_jung_jung_jong() {
        // '원' = ᄋ + ᅮ + ᅥ (→ compound ᅯ) + ᆫ — 4 개의 자모를 순서대로
        // 때려서 하나의 모아쓴 글자가 되어야 한다. Sebeolsik Final 에서
        // J(ᄋ) 9(ᅮ) T(ᅥ) S(ᆫ).
        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        let _ = f.feed(JamoInput::cho_only(0x110B)); // ᄋ
        let _ = f.feed(jung(0x116E));                // ᅮ
        let ev = f.feed(jung(0x1165));               // ᅥ → compose ᅯ
        assert!(
            matches!(&ev, FsmEvent::Preedit(s) if s == "\u{110B}\u{116F}"),
            "after ᅥ expected preedit 워, got {:?}", ev
        );
        let ev = f.feed(JamoInput::jong_only(0x11AB)); // ᆫ
        assert!(
            matches!(&ev, FsmEvent::Preedit(s) if s == "\u{110B}\u{116F}\u{11AB}"),
            "after ᆫ expected preedit 원, got {:?}", ev
        );
        assert_eq!(crate::to_nfc_syllable(&f.preedit_string()), "원");
    }

    #[test]
    fn moachigi_chord_hint_allows_late_compound_vowel() {
        // 물리 동시 누름에서 OS가 S(받침)를 복모음 2번째 자모 T(ᅥ) 보다
        // 먼저 전달했을 때, 타이밍 힌트(within_chord=true) 가 있으면
        // 전체가 '원' 한 음절로 모여야 한다. 힌트 없이는 "운" + "어" 로
        // 쪼개져서 실패하므로, 힌트 on/off 양쪽을 대조한다.
        //
        // 시퀀스: ᄋ + ᅮ + ᆫ + ᅥ (jong이 2번째 jung보다 먼저).
        let chord = FeedOptions { within_chord: true };

        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        let _ = f.feed_with(JamoInput::cho_only(0x110B), chord);  // ᄋ
        let _ = f.feed_with(jung(0x116E), chord);                  // ᅮ
        let _ = f.feed_with(JamoInput::jong_only(0x11AB), chord);  // ᆫ → "운"
        let _ = f.feed_with(jung(0x1165), chord);                  // ᅥ → "원"
        assert_eq!(
            crate::to_nfc_syllable(&f.preedit_string()),
            "원",
            "with chord hint, late ᅥ must fold into 원; got preedit {:?}",
            f.preedit_string()
        );

        // Baseline: without the hint, the syllable-complete gate commits
        // "운" and starts a fresh "어" — by design.
        let mut g = HangulFsm::with_mode(ComposeMode::Moachigi);
        let _ = g.feed(JamoInput::cho_only(0x110B));
        let _ = g.feed(jung(0x116E));
        let _ = g.feed(JamoInput::jong_only(0x11AB));
        let ev = g.feed(jung(0x1165));
        assert!(
            matches!(&ev, FsmEvent::CommitThenPreedit { commit, .. } if commit == "\u{110B}\u{116E}\u{11AB}"),
            "without chord hint expected CommitThenPreedit('운', ...), got {ev:?}"
        );
    }

    #[test]
    fn moachigi_four_jamo_compound_vowel_reverse_order() {
        // Moachigi 의 핵심: 4-자모 음절을 "동시에" 누른다는 개념이므로
        // 복모음 구성 자모의 입력 순서가 뒤집혀도 동일 음절이 나와야 한다.
        // 원 = ᄋ + ᅮ + ᅥ + ᆫ — canonical 과 reverse 모두 '원' 이어야 함.
        let cases: [(u32, u32, u32, u32, &str); 3] = [
            // (cho, jung_first_pressed, jung_second_pressed, jong, expected)
            (0x110B, 0x1165, 0x116E, 0x11AB, "원"), // ᄋ + ᅥ + ᅮ + ᆫ
            (0x110B, 0x1165, 0x116E, 0x11AF, "월"), // ᄋ + ᅥ + ᅮ + ᆯ
            (0x1100, 0x1161, 0x1169, 0x11AB, "관"), // ᄀ + ᅡ + ᅩ + ᆫ
        ];
        for (cho_cp, a, b, jong_cp, want) in cases {
            let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
            let _ = f.feed(JamoInput::cho_only(cho_cp));
            let _ = f.feed(jung(a));
            let _ = f.feed(jung(b));
            let _ = f.feed(JamoInput::jong_only(jong_cp));
            assert_eq!(
                crate::to_nfc_syllable(&f.preedit_string()),
                want,
                "moachigi reverse-order 4-jamo failed: cho=U+{cho_cp:04X} jung={a:04X}→{b:04X} jong=U+{jong_cp:04X}"
            );
        }
    }

    #[test]
    fn moachigi_four_jamo_compound_jong_reverse_order() {
        // 겹받침 구성 자모가 역순으로 들어와도 동일 음절이 되어야 한다.
        // 넋 = ᄂ + ᅥ + ᆨ + ᆺ — 모아치기에서는 ᆺ + ᆨ 순서여도 ᆪ 로 조합.
        let cases: [(u32, u32, u32, u32, &str); 2] = [
            // (cho, jung, jong_first_pressed, jong_second_pressed, expected)
            (0x1102, 0x1165, 0x11BA, 0x11A8, "넋"), // ᄂ + ᅥ + ᆺ + ᆨ
            (0x110B, 0x1165, 0x11BA, 0x11B8, "없"), // ᄋ + ᅥ + ᆺ + ᆸ
        ];
        for (cho_cp, jung_cp, a, b, want) in cases {
            let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
            let _ = f.feed(JamoInput::cho_only(cho_cp));
            let _ = f.feed(jung(jung_cp));
            let _ = f.feed(JamoInput::jong_only(a));
            let _ = f.feed(JamoInput::jong_only(b));
            assert_eq!(
                crate::to_nfc_syllable(&f.preedit_string()),
                want,
                "moachigi reverse-order compound jong failed: cho=U+{cho_cp:04X} jung=U+{jung_cp:04X} jong={a:04X}→{b:04X}"
            );
        }
    }

    #[test]
    fn moachigi_reverse_order_composes() {
        // Jong → Jung → Cho should still form "안".
        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        let _ = f.feed(JamoInput::jong_only(0x11AB)); // ᆫ
        let _ = f.feed(jung(0x1161));                 // ᅡ
        let _ = f.feed(JamoInput::cho_only(0x110B));  // ᄋ
        let got = f.preedit_string();
        assert_eq!(crate::to_nfc_syllable(&got), "안");
    }

    #[test]
    fn moachigi_jung_then_cho_composes() {
        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        let _ = f.feed(jung(0x1161));                 // ᅡ
        let _ = f.feed(JamoInput::cho_only(0x110B));  // ᄋ
        let got = f.preedit_string();
        assert_eq!(crate::to_nfc_syllable(&got), "아");
    }

    #[test]
    fn moachigi_starts_new_on_slot_collision() {
        // Cho + Cho → commit first Cho, start new syllable.
        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        let _ = f.feed(JamoInput::cho_only(0x110B)); // ᄋ
        let ev = f.feed(JamoInput::cho_only(0x1100)); // ᄀ
        let commit = ev.commit_str().unwrap_or("");
        assert_eq!(commit, "\u{110B}");
    }

    #[test]
    fn flush_commits_in_progress() {
        let mut f = HangulFsm::new();
        let _ = f.feed(dual(0x1100, 0x11A8));
        let _ = f.feed(jung(0x1161));
        let out = f.flush_string();
        assert_eq!(out, "\u{1100}\u{1161}");
        assert!(!f.is_composing());
    }

    #[test]
    fn backspace_strips_jong_then_jung_then_cho() {
        let mut f = HangulFsm::new();
        let _ = f.feed(dual(0x1100, 0x11A8));
        let _ = f.feed(jung(0x1161));
        let _ = f.feed(JamoInput::jong_only(0x11AB));
        assert!(f.state().jong.is_some());

        let _ = f.backspace();
        assert_eq!(f.state().jong, None);
        assert!(f.state().jung.is_some());

        let _ = f.backspace();
        assert_eq!(f.state().jung, None);

        let _ = f.backspace();
        assert!(!f.is_composing());
    }

    #[test]
    fn double_cho_merges_on_repeat_sequential() {
        // ㄱ + ㄱ (Sebeolsik-style cho-only inputs) with no intervening
        // vowel should collapse to ㄲ and stay as a single preedit.
        let mut f = HangulFsm::new();
        let _ = f.feed(JamoInput::cho_only(0x1100)); // ㄱ
        let ev = f.feed(JamoInput::cho_only(0x1100)); // ㄱ again → ㄲ
        assert!(matches!(ev, FsmEvent::Preedit(_)));
        assert_eq!(f.preedit_string(), "\u{1101}"); // ᄁ
    }

    #[test]
    fn moachigi_toil_sebeolsik_final() {
        // "토일" — 토(ᄐ+ᅩ) + 일(ᄋ+ᅵ+ᆯ). Sebeolsik Final 기준 실키 시퀀스
        // Quote(ᄐ) V(ᅩ) J(ᄋ) D(ᅵ) W(ᆯ). 세벌식이라 dual-role 없음.
        use crate::layout::key::{KeyCode, KeyEvent};
        use crate::layout::sebeolsik_final::SebeolsikFinal;
        use crate::Layout;
        use crate::LayoutOutput;
        let layout = SebeolsikFinal;
        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        let mut commit = String::new();
        for code in [
            KeyCode::Quote,
            KeyCode::KeyV,
            KeyCode::KeyJ,
            KeyCode::KeyD,
            KeyCode::KeyW,
        ] {
            if let LayoutOutput::Jamo(input) = layout.map(&KeyEvent::plain(code)) {
                let ev = f.feed(input);
                if let Some(c) = ev.commit_str() {
                    commit.push_str(c);
                }
            }
        }
        commit.push_str(&f.flush_string());
        assert_eq!(crate::to_nfc_syllable(&commit), "토일", "sebeolsik-final 토일: got {:?}", commit);
    }

    #[test]
    fn moachigi_sebeolsik_final_jong_only_does_not_chain_jongs_into_next_syllable() {
        // Sebeolsik final jong-only 키로 만든 곡(ᄀ+ᅩ+ᆨ via K V X) +
        // ᅡ(F) 는 받침 이동이 일어나면 안 된다 (사용자가 명시적으로 jong
        // 키를 눌렀으므로). 결과는 commit "곡" + preedit "ㅏ".
        use crate::layout::key::{KeyCode, KeyEvent};
        use crate::layout::sebeolsik_final::SebeolsikFinal;
        use crate::Layout;
        use crate::LayoutOutput;
        let layout = SebeolsikFinal;
        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        let mut commit = String::new();
        for code in [KeyCode::KeyK, KeyCode::KeyV, KeyCode::KeyX, KeyCode::KeyF] {
            if let LayoutOutput::Jamo(input) = layout.map(&KeyEvent::plain(code)) {
                let ev = f.feed(input);
                if let Some(c) = ev.commit_str() {
                    commit.push_str(c);
                }
            }
        }
        assert_eq!(crate::to_nfc_syllable(&commit), "곡", "곡 이 commit 되어야 함");
        // 잔여 preedit 는 모음 ᅡ 단독.
        assert_eq!(f.preedit_string(), "\u{1161}");
    }

    #[test]
    fn moachigi_sebeolsik_final_hasineun() {
        // 세벌식 final + Moachigi 로 "하시는" (M F N D H G S).
        use crate::layout::key::{KeyCode, KeyEvent};
        use crate::layout::sebeolsik_final::SebeolsikFinal;
        use crate::Layout;
        use crate::LayoutOutput;
        let layout = SebeolsikFinal;
        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        let mut commit = String::new();
        let keys = [
            KeyCode::KeyM, KeyCode::KeyF,               // 하 = ᄒ + ᅡ
            KeyCode::KeyN, KeyCode::KeyD,               // 시 = ᄉ + ᅵ
            KeyCode::KeyH, KeyCode::KeyG, KeyCode::KeyS, // 는 = ᄂ + ᅳ + ᆫ
        ];
        for code in keys {
            if let LayoutOutput::Jamo(input) = layout.map(&KeyEvent::plain(code)) {
                let ev = f.feed(input);
                if let Some(c) = ev.commit_str() {
                    commit.push_str(c);
                }
            }
        }
        commit.push_str(&f.flush_string());
        assert_eq!(crate::to_nfc_syllable(&commit), "하시는", "got {:?}", commit);
    }

    #[test]
    fn moachigi_dubeolsik_chord_forms_compound_jong_dak() {
        // 두벌식 Moachigi + chord hint 로 "닭" (ᄃ+ᅡ+ᆯ+ᆨ, keys E K F R).
        // chord 있으면 겹받침(ᆰ)을 모아치기로 합성해야 한다.
        use crate::layout::dubeolsik::Dubeolsik;
        use crate::layout::key::{KeyCode, KeyEvent};
        use crate::Layout;
        use crate::LayoutOutput;
        let layout = Dubeolsik;
        let chord = FeedOptions { within_chord: true };
        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        for (idx, code) in [KeyCode::KeyE, KeyCode::KeyK, KeyCode::KeyF, KeyCode::KeyR]
            .iter()
            .enumerate()
        {
            if let LayoutOutput::Jamo(input) = layout.map(&KeyEvent::plain(*code)) {
                let opts = if idx == 0 { FeedOptions::default() } else { chord };
                let _ = f.feed_with(input, opts);
            }
        }
        assert_eq!(
            crate::to_nfc_syllable(&f.preedit_string()),
            "닭",
            "got preedit {:?}",
            f.preedit_string()
        );
    }

    #[test]
    fn moachigi_dubeolsik_no_chord_does_not_form_anj() {
        // chord hint 없이 "안" 뒤에 dual(ᄌ,ᆽ) 가 오면 앉 이 되지 않고
        // 안 을 커밋하고 새 음절을 ᄌ 로 시작해야 한다 (이전 회귀 방지).
        use crate::layout::dubeolsik::Dubeolsik;
        use crate::layout::key::{KeyCode, KeyEvent};
        use crate::Layout;
        use crate::LayoutOutput;
        let layout = Dubeolsik;
        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        for code in [KeyCode::KeyD, KeyCode::KeyK, KeyCode::KeyS, KeyCode::KeyW] {
            if let LayoutOutput::Jamo(input) = layout.map(&KeyEvent::plain(code)) {
                let _ = f.feed(input); // plain feed: within_chord=false
            }
        }
        // 마지막 키 후: 안 커밋됐고 state = (ᄌ, N, N).
        assert_eq!(f.state().cho.map(Cho::codepoint), Some(0x110C));
        assert_eq!(f.state().jong, None);
    }

    #[test]
    fn moachigi_dubeolsik_hasineun_with_chord_hint() {
        // 빠른 타이핑 → 모든 키가 chord window 안에 들어와도 "하시는" 이
        // 나와야 한다. 두벌식 자음은 dual-role 이므로 복모음 합성이 받침
        // 이동보다 우선돼선 안 된다 (기존엔 "햇는" 으로 나왔음).
        use crate::layout::dubeolsik::Dubeolsik;
        use crate::layout::key::{KeyCode, KeyEvent};
        use crate::Layout;
        use crate::LayoutOutput;
        let layout = Dubeolsik;
        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        let chord = FeedOptions { within_chord: true };
        let mut commit = String::new();
        let keys = [
            KeyCode::KeyG, KeyCode::KeyK,
            KeyCode::KeyT, KeyCode::KeyL,
            KeyCode::KeyS, KeyCode::KeyM, KeyCode::KeyS,
        ];
        for (idx, code) in keys.iter().enumerate() {
            if let LayoutOutput::Jamo(input) = layout.map(&KeyEvent::plain(*code)) {
                // 첫 키는 직전 이벤트가 없으니 plain feed, 이후는 chord on.
                let opts = if idx == 0 { FeedOptions::default() } else { chord };
                let ev = f.feed_with(input, opts);
                if let Some(c) = ev.commit_str() {
                    commit.push_str(c);
                }
            }
        }
        commit.push_str(&f.flush_string());
        assert_eq!(
            crate::to_nfc_syllable(&commit),
            "하시는",
            "chord hint on 상태에서 got {:?}",
            commit
        );
    }

    #[test]
    fn moachigi_dubeolsik_hasineun() {
        // 두벌식 + Moachigi 로 "하시는" (G K T L S M S).
        use crate::layout::dubeolsik::Dubeolsik;
        use crate::layout::key::{KeyCode, KeyEvent};
        use crate::Layout;
        use crate::LayoutOutput;
        let layout = Dubeolsik;
        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        let mut commit = String::new();
        let keys = [
            KeyCode::KeyG, KeyCode::KeyK,               // 하
            KeyCode::KeyT, KeyCode::KeyL,               // 시
            KeyCode::KeyS, KeyCode::KeyM, KeyCode::KeyS, // 는
        ];
        for code in keys {
            if let LayoutOutput::Jamo(input) = layout.map(&KeyEvent::plain(code)) {
                let ev = f.feed(input);
                if let Some(c) = ev.commit_str() {
                    commit.push_str(c);
                }
            }
        }
        commit.push_str(&f.flush_string());
        assert_eq!(crate::to_nfc_syllable(&commit), "하시는", "got {:?}", commit);
    }

    #[test]
    fn moachigi_dubeolsik_annyeonghaseyo() {
        // 두벌식 + Moachigi 로 안녕하세요 (D F S H I J D K T P D R) 입력.
        use crate::layout::dubeolsik::Dubeolsik;
        use crate::layout::key::{KeyCode, KeyEvent};
        use crate::Layout;
        use crate::LayoutOutput;
        let layout = Dubeolsik;
        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        let mut commit = String::new();
        let keys = [
            KeyCode::KeyD, KeyCode::KeyK, KeyCode::KeyS,  // 안
            KeyCode::KeyS, KeyCode::KeyU, KeyCode::KeyD,  // 녕
            KeyCode::KeyG, KeyCode::KeyK,                  // 하
            KeyCode::KeyT, KeyCode::KeyP,                  // 세
            KeyCode::KeyD, KeyCode::KeyY,                  // 요 (D=ㅇ, Y=ㅛ)
        ];
        for code in keys {
            if let LayoutOutput::Jamo(input) = layout.map(&KeyEvent::plain(code)) {
                let ev = f.feed(input);
                if let Some(c) = ev.commit_str() {
                    commit.push_str(c);
                }
            }
        }
        commit.push_str(&f.flush_string());
        // 마지막 키 매핑 차이로 깨질 수 있으니 자모 서브셋만 확인하지 말고
        // 정확히 맞춰본다 — 실패 시 메시지로 디버깅.
        assert_eq!(
            crate::to_nfc_syllable(&commit),
            "안녕하세요",
            "dubeolsik moachigi 안녕하세요: got {:?}",
            commit
        );
    }

    #[test]
    fn moachigi_toil_dubeolsik() {
        // 두벌식 + Moachigi 에서도 토일 이 나와야 한다 (현재 버그 재현).
        // 키: X(ㅌ) H(ㅗ) D(ㅇ) L(ㅣ) F(ㄹ). 모두 dual-role.
        use crate::layout::dubeolsik::Dubeolsik;
        use crate::layout::key::{KeyCode, KeyEvent};
        use crate::Layout;
        use crate::LayoutOutput;
        let layout = Dubeolsik;
        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        let mut commit = String::new();
        for code in [
            KeyCode::KeyX,
            KeyCode::KeyH,
            KeyCode::KeyD,
            KeyCode::KeyL,
            KeyCode::KeyF,
        ] {
            if let LayoutOutput::Jamo(input) = layout.map(&KeyEvent::plain(code)) {
                let ev = f.feed(input);
                if let Some(c) = ev.commit_str() {
                    commit.push_str(c);
                }
            }
        }
        commit.push_str(&f.flush_string());
        assert_eq!(crate::to_nfc_syllable(&commit), "토일", "dubeolsik 토일: got {:?}", commit);
    }

    #[test]
    fn double_cho_merges_on_repeat_moachigi() {
        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        let _ = f.feed(JamoInput::cho_only(0x1109)); // ㅅ
        let ev = f.feed(JamoInput::cho_only(0x1109)); // ㅅ again → ㅆ
        assert!(matches!(ev, FsmEvent::Preedit(_)));
        assert_eq!(f.preedit_string(), "\u{110A}"); // ᄊ
    }

    #[test]
    fn non_doubleable_repeat_still_commits_new_cho() {
        // ㄴㄴ is not a doubled form — first ㄴ commits, new ㄴ starts.
        let mut f = HangulFsm::new();
        let _ = f.feed(JamoInput::cho_only(0x1102));
        let ev = f.feed(JamoInput::cho_only(0x1102));
        match ev {
            FsmEvent::CommitThenPreedit { commit, preedit } => {
                assert_eq!(commit, "\u{1102}");
                assert_eq!(preedit, "\u{1102}");
            }
            other => panic!("expected CommitThenPreedit, got {other:?}"),
        }
    }

    #[test]
    fn double_cho_then_vowel_forms_ssa_syllable() {
        // ㅅ + ㅅ + ㅏ → 싸
        let mut f = HangulFsm::new();
        let _ = f.feed(JamoInput::cho_only(0x1109));
        let _ = f.feed(JamoInput::cho_only(0x1109)); // → ᄊ
        let _ = f.feed(JamoInput::Jung(0x1161));     // + ᅡ
        let out = crate::to_nfc_syllable(&f.preedit_string());
        assert_eq!(out, "싸");
    }

    /// Covers every compound-Jong pair registered in `compose_jong`,
    /// verifying that two consecutive Jong inputs in **Moachigi** mode
    /// fold into the expected composite on a `ChoJungJong` syllable.
    #[test]
    fn all_compound_jongs_compose_in_moachigi() {
        let cases: &[(u32, u32, u32, &str)] = &[
            (0x11A8, 0x11A8, 0x11A9, "ᆨ+ᆨ=ᆩ (doubled)"),
            (0x11A8, 0x11BA, 0x11AA, "ᆨ+ᆺ=ᆪ"),
            (0x11AB, 0x11BD, 0x11AC, "ᆫ+ᆽ=ᆬ"),
            (0x11AB, 0x11C2, 0x11AD, "ᆫ+ᇂ=ᆭ"),
            (0x11AF, 0x11A8, 0x11B0, "ᆯ+ᆨ=ᆰ"),
            (0x11AF, 0x11B7, 0x11B1, "ᆯ+ᆷ=ᆱ"),
            (0x11AF, 0x11B8, 0x11B2, "ᆯ+ᆸ=ᆲ"),
            (0x11AF, 0x11BA, 0x11B3, "ᆯ+ᆺ=ᆳ"),
            (0x11AF, 0x11C0, 0x11B4, "ᆯ+ᇀ=ᆴ"),
            (0x11AF, 0x11C1, 0x11B5, "ᆯ+ᇁ=ᆵ"),
            (0x11AF, 0x11C2, 0x11B6, "ᆯ+ᇂ=ᆶ"),
            (0x11B8, 0x11BA, 0x11B9, "ᆸ+ᆺ=ᆹ"),
            (0x11BA, 0x11BA, 0x11BB, "ᆺ+ᆺ=ᆻ (doubled)"),
        ];
        for &(a, b, expected, label) in cases {
            let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
            let _ = f.feed(JamoInput::cho_only(0x1100)); // ᄀ
            let _ = f.feed(JamoInput::Jung(0x1161));     // ᅡ
            let _ = f.feed(JamoInput::jong_only(a));
            let _ = f.feed(JamoInput::jong_only(b));
            assert_eq!(
                f.state().jong.map(Jong::codepoint),
                Some(expected),
                "{label} — final state mismatch",
            );
            assert_eq!(f.state().cho.map(Cho::codepoint), Some(0x1100));
            assert_eq!(f.state().jung.map(Jung::codepoint), Some(0x1161));
        }
    }

    /// Same matrix in **Sequential** mode; inputs go through the
    /// `ChoJungJong` branch of `feed_sequential`.
    #[test]
    fn all_compound_jongs_compose_in_sequential() {
        let cases: &[(u32, u32, u32, &str)] = &[
            (0x11A8, 0x11BA, 0x11AA, "ᆨ+ᆺ=ᆪ"),
            (0x11AB, 0x11BD, 0x11AC, "ᆫ+ᆽ=ᆬ"),
            (0x11AB, 0x11C2, 0x11AD, "ᆫ+ᇂ=ᆭ"),
            (0x11AF, 0x11A8, 0x11B0, "ᆯ+ᆨ=ᆰ"),
            (0x11AF, 0x11B7, 0x11B1, "ᆯ+ᆷ=ᆱ"),
            (0x11AF, 0x11B8, 0x11B2, "ᆯ+ᆸ=ᆲ"),
            (0x11AF, 0x11BA, 0x11B3, "ᆯ+ᆺ=ᆳ"),
            (0x11AF, 0x11C0, 0x11B4, "ᆯ+ᇀ=ᆴ"),
            (0x11AF, 0x11C1, 0x11B5, "ᆯ+ᇁ=ᆵ"),
            (0x11AF, 0x11C2, 0x11B6, "ᆯ+ᇂ=ᆶ"),
            (0x11B8, 0x11BA, 0x11B9, "ᆸ+ᆺ=ᆹ"),
        ];
        for &(a, b, expected, label) in cases {
            let mut f = HangulFsm::new();
            let _ = f.feed(JamoInput::cho_only(0x1100));
            let _ = f.feed(JamoInput::Jung(0x1161));
            let _ = f.feed(JamoInput::jong_only(a));
            let _ = f.feed(JamoInput::jong_only(b));
            assert_eq!(
                f.state().jong.map(Jong::codepoint),
                Some(expected),
                "{label} (sequential) — final state mismatch",
            );
        }
    }

    /// Regression: in Sequential mode, typing `ChoJungJong` and then a
    /// Jong-only key that doesn't compose with the existing Jong must
    /// commit the current syllable AND seed the new state with the
    /// orphan Jong — not drop it silently.
    #[test]
    fn sequential_chojungjong_plus_noncompose_jong_keeps_input() {
        let mut f = HangulFsm::new();
        let _ = f.feed(JamoInput::cho_only(0x1100));   // ᄀ
        let _ = f.feed(JamoInput::Jung(0x1161));       // ᅡ
        let _ = f.feed(JamoInput::jong_only(0x11A8));  // ᆨ (→ 각)
        // ᆨ + ᆯ has no triple-jong composition.
        let ev = f.feed(JamoInput::jong_only(0x11AF)); // ᆯ
        match ev {
            FsmEvent::CommitThenPreedit { commit, .. } => {
                // Old syllable "각" committed (as conjoining jamo).
                assert_eq!(commit, "\u{1100}\u{1161}\u{11A8}");
            }
            other => panic!("expected CommitThenPreedit, got {other:?}"),
        }
        // The orphan Jong ᆯ lives on in the new state.
        assert_eq!(f.state().jong.map(Jong::codepoint), Some(0x11AF));
        assert!(f.state().cho.is_none());
        assert!(f.state().jung.is_none());
    }

    /// End-to-end: common compound-jong syllables from a Sebeolsik
    /// Final-style Moachigi run should collapse into the expected
    /// NFC text.
    #[test]
    fn sebeolsik_compound_jong_end_to_end_moachigi() {
        struct Case<'a> {
            label: &'a str,
            cho: u32, jung: u32, jong_a: u32, jong_b: u32,
            expected: &'a str,
        }
        let cases = [
            Case { label: "몫", cho: 0x1106, jung: 0x1169, jong_a: 0x11A8, jong_b: 0x11BA, expected: "몫" },
            Case { label: "앉", cho: 0x110B, jung: 0x1161, jong_a: 0x11AB, jong_b: 0x11BD, expected: "앉" },
            Case { label: "않", cho: 0x110B, jung: 0x1161, jong_a: 0x11AB, jong_b: 0x11C2, expected: "않" },
            Case { label: "닭", cho: 0x1103, jung: 0x1161, jong_a: 0x11AF, jong_b: 0x11A8, expected: "닭" },
            Case { label: "삶", cho: 0x1109, jung: 0x1161, jong_a: 0x11AF, jong_b: 0x11B7, expected: "삶" },
            Case { label: "밟", cho: 0x1107, jung: 0x1161, jong_a: 0x11AF, jong_b: 0x11B8, expected: "밟" },
            Case { label: "곬", cho: 0x1100, jung: 0x1169, jong_a: 0x11AF, jong_b: 0x11BA, expected: "곬" },
            Case { label: "핥", cho: 0x1112, jung: 0x1161, jong_a: 0x11AF, jong_b: 0x11C0, expected: "핥" },
            Case { label: "읊", cho: 0x110B, jung: 0x1173, jong_a: 0x11AF, jong_b: 0x11C1, expected: "읊" },
            Case { label: "뚫", cho: 0x1104, jung: 0x116E, jong_a: 0x11AF, jong_b: 0x11C2, expected: "뚫" },
            Case { label: "값", cho: 0x1100, jung: 0x1161, jong_a: 0x11B8, jong_b: 0x11BA, expected: "값" },
            Case { label: "밖", cho: 0x1107, jung: 0x1161, jong_a: 0x11A8, jong_b: 0x11A8, expected: "밖" },
            Case { label: "갔", cho: 0x1100, jung: 0x1161, jong_a: 0x11BA, jong_b: 0x11BA, expected: "갔" },
        ];
        for c in &cases {
            let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
            let _ = f.feed(JamoInput::cho_only(c.cho));
            let _ = f.feed(JamoInput::Jung(c.jung));
            let _ = f.feed(JamoInput::jong_only(c.jong_a));
            let _ = f.feed(JamoInput::jong_only(c.jong_b));
            let out = crate::to_nfc_syllable(&f.preedit_string());
            assert_eq!(out, c.expected, "{}", c.label);
        }
    }

    /// Regression: state already holds a simple Jong, and the user presses
    /// a direct-compound-Jong key (as on Sebeolsik Final) whose first
    /// component matches. The compound must **absorb** the simple instead
    /// of committing the syllable and orphaning the new key.
    #[test]
    fn simple_jong_absorbed_by_matching_compound_moachigi() {
        // (simple_jong, compound_jong, label)
        let cases: &[(u32, u32, &str)] = &[
            (0x11A8, 0x11A9, "ᆨ absorbed by ᆩ"),
            (0x11A8, 0x11AA, "ᆨ absorbed by ᆪ"),
            (0x11AB, 0x11AC, "ᆫ absorbed by ᆬ"),
            (0x11AB, 0x11AD, "ᆫ absorbed by ᆭ"),
            (0x11AF, 0x11B0, "ᆯ absorbed by ᆰ"),
            (0x11AF, 0x11B1, "ᆯ absorbed by ᆱ"),
            (0x11AF, 0x11B2, "ᆯ absorbed by ᆲ"),
            (0x11AF, 0x11B3, "ᆯ absorbed by ᆳ"),
            (0x11AF, 0x11B4, "ᆯ absorbed by ᆴ"),
            (0x11AF, 0x11B5, "ᆯ absorbed by ᆵ"),
            (0x11AF, 0x11B6, "ᆯ absorbed by ᆶ"),
            (0x11B8, 0x11B9, "ᆸ absorbed by ᆹ"),
            (0x11BA, 0x11BB, "ᆺ absorbed by ᆻ"),
        ];
        for &(simple, compound, label) in cases {
            let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
            let _ = f.feed(JamoInput::cho_only(0x1100)); // ᄀ
            let _ = f.feed(JamoInput::Jung(0x1161));     // ᅡ
            let _ = f.feed(JamoInput::jong_only(simple));
            let ev = f.feed(JamoInput::jong_only(compound));
            assert!(
                matches!(ev, FsmEvent::Preedit(_)),
                "{label} — expected Preedit, got {ev:?}",
            );
            assert_eq!(
                f.state().jong.map(Jong::codepoint),
                Some(compound),
                "{label} — final Jong mismatch",
            );
            assert_eq!(f.state().cho.map(Cho::codepoint), Some(0x1100));
            assert_eq!(f.state().jung.map(Jung::codepoint), Some(0x1161));
        }
    }

    /// Same absorb rule in Sequential mode (the `ChoJungJong` branch).
    #[test]
    fn simple_jong_absorbed_by_matching_compound_sequential() {
        let cases: &[(u32, u32, &str)] = &[
            (0x11A8, 0x11AA, "ᆨ absorbed by ᆪ"),
            (0x11AB, 0x11AC, "ᆫ absorbed by ᆬ"),
            (0x11AF, 0x11B0, "ᆯ absorbed by ᆰ"),
            (0x11AF, 0x11B2, "ᆯ absorbed by ᆲ"),
            (0x11AF, 0x11B6, "ᆯ absorbed by ᆶ"),
            (0x11B8, 0x11B9, "ᆸ absorbed by ᆹ"),
        ];
        for &(simple, compound, label) in cases {
            let mut f = HangulFsm::new();
            let _ = f.feed(JamoInput::cho_only(0x1100));
            let _ = f.feed(JamoInput::Jung(0x1161));
            let _ = f.feed(JamoInput::jong_only(simple));
            let ev = f.feed(JamoInput::jong_only(compound));
            assert!(
                matches!(ev, FsmEvent::Preedit(_)),
                "{label} (sequential) — expected Preedit, got {ev:?}",
            );
            assert_eq!(
                f.state().jong.map(Jong::codepoint),
                Some(compound),
                "{label} (sequential) — final Jong mismatch",
            );
        }
    }

    /// Moachigi + dual-role consonant after a complete syllable:
    /// the syllable is "done" from the user's POV, so the next key —
    /// even a dual-role one whose Jong part would compose — must
    /// commit and start a new syllable with the Cho role. Users
    /// repeatedly bumped into 안 silently turning into 앉 when they
    /// had already moved on to the next word.
    #[test]
    fn moachigi_dual_role_after_full_syllable_starts_new() {
        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        let _ = f.feed(JamoInput::cho_only(0x110B));           // ᄋ
        let _ = f.feed(JamoInput::Jung(0x1161));               // ᅡ
        let _ = f.feed(JamoInput::jong_only(0x11AB));          // ᆫ → 안 (complete)
        let ev = f.feed(JamoInput::cho_dual(0x110C, 0x11BD));  // dual (ᄌ, ᆽ)
        match ev {
            FsmEvent::CommitThenPreedit { commit, preedit } => {
                // Commit 안 unchanged; new syllable starts with cho ᄌ.
                assert_eq!(commit, "\u{110B}\u{1161}\u{11AB}");
                assert_eq!(preedit, "\u{110C}");
            }
            other => panic!("expected CommitThenPreedit, got {other:?}"),
        }
        assert_eq!(f.state().cho.map(Cho::codepoint), Some(0x110C));
        assert_eq!(f.state().jong, None);
        // Absorb-style case also commits + starts fresh.
        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        let _ = f.feed(JamoInput::cho_only(0x110B));
        let _ = f.feed(JamoInput::Jung(0x1161));
        let _ = f.feed(JamoInput::jong_only(0x11AF));          // 알 (complete)
        let ev = f.feed(JamoInput::cho_dual(0x1105, 0x11AF));  // dual ㄹ
        match ev {
            FsmEvent::CommitThenPreedit { commit, .. } => {
                assert_eq!(commit, "\u{110B}\u{1161}\u{11AF}");
            }
            other => panic!("expected CommitThenPreedit, got {other:?}"),
        }
    }

    /// Within a still-incomplete syllable a dual-role key should
    /// still prefer the Jong role when that would compose — this
    /// is what enables reverse-order moachigi (jong-first) to compound.
    #[test]
    fn moachigi_dual_role_prefers_jong_while_incomplete() {
        // state = ᄋ + ᆫ (no jung yet), incoming = dual(ᄌ, ᆽ) → compose ᆫ+ᆽ = ᆬ
        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        let _ = f.feed(JamoInput::cho_only(0x110B));
        let _ = f.feed(JamoInput::jong_only(0x11AB));
        let ev = f.feed(JamoInput::cho_dual(0x110C, 0x11BD));
        assert!(matches!(ev, FsmEvent::Preedit(_)), "got {ev:?}");
        assert_eq!(f.state().jong.map(Jong::codepoint), Some(0x11AC));
    }

    /// Non-matching simple + compound must still commit: the existing Jong
    /// isn't a prefix of the incoming compound (e.g. ᆨ then ᆲ).
    #[test]
    fn non_matching_compound_still_commits() {
        let mut f = HangulFsm::with_mode(ComposeMode::Moachigi);
        let _ = f.feed(JamoInput::cho_only(0x1100));  // ᄀ
        let _ = f.feed(JamoInput::Jung(0x1161));      // ᅡ
        let _ = f.feed(JamoInput::jong_only(0x11A8)); // ᆨ → 각
        let ev = f.feed(JamoInput::jong_only(0x11B2)); // ᆲ (starts with ᆯ, not ᆨ)
        match ev {
            FsmEvent::CommitThenPreedit { commit, .. } => {
                assert_eq!(commit, "\u{1100}\u{1161}\u{11A8}");
            }
            other => panic!("expected CommitThenPreedit, got {other:?}"),
        }
        assert_eq!(f.state().jong.map(Jong::codepoint), Some(0x11B2));
    }

    #[test]
    fn double_jong_merges_via_compose_jong() {
        // "각" + ㄱ → "갂" (doubled ㄲ in jong position).
        let mut f = HangulFsm::new();
        let _ = f.feed(JamoInput::cho_dual(0x1100, 0x11A8)); // ㄱ
        let _ = f.feed(JamoInput::Jung(0x1161));             // ㅏ
        let _ = f.feed(JamoInput::cho_dual(0x1100, 0x11A8)); // ᆨ → attach
        let _ = f.feed(JamoInput::cho_dual(0x1100, 0x11A8)); // ᆨ again → ᆩ
        let out = crate::to_nfc_syllable(&f.preedit_string());
        assert_eq!(out, "갂");
    }

    #[test]
    fn mode_switchable_at_runtime() {
        let mut f = HangulFsm::new();
        assert_eq!(f.mode(), ComposeMode::Sequential);
        f.set_mode(ComposeMode::Moachigi);
        assert_eq!(f.mode(), ComposeMode::Moachigi);
    }
}
