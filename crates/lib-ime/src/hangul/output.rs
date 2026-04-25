//! Output-form conversion.
//!
//! Internal state is always conjoining Jamo (U+1100..=U+11FF). When the
//! IME hands text off to the OS or an editor, three presentation forms
//! are typical:
//!
//! * **Conjoining Jamo** — the internal form; no transformation. Useful
//!   when the downstream renderer shapes jamo itself (e.g. `CodeMirror`
//!   with an IME preedit overlay).
//! * **NFC syllable** (U+AC00..=U+D7A3) — the canonical composed form
//!   expected by almost every app and file format.
//! * **Compatibility Jamo** (U+3130..=U+318F) — standalone jamo glyphs
//!   used when a syllable isn't formable (e.g. lone vowel input).
//!
//! This module also exposes [`to_nfc_syllable`] and [`to_compat_jamo`]
//! as free functions for one-off conversions.

use super::fsm::FsmEvent;

const SYLLABLE_BASE: u32 = 0xAC00;
const JUNG_COUNT: u32 = 21;
const JONG_SLOTS: u32 = 28; // 27 finals + "no final"
const SYLLABLE_STRIDE: u32 = JUNG_COUNT * JONG_SLOTS; // 588

/// Selects how [`FsmEvent`] payloads are rendered to final text.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum OutputForm {
    /// Raw conjoining Jamo. No transformation.
    JamoConjoining,
    /// NFC syllables where possible; unpaired jamo pass through.
    #[default]
    NfcSyllable,
    /// Hangul Compatibility Jamo (U+3130..=U+318F).
    JamoCompat,
}

impl OutputForm {
    /// Render a conjoining-Jamo string in this output form.
    pub fn render(self, s: &str) -> String {
        match self {
            Self::JamoConjoining => s.to_owned(),
            Self::NfcSyllable => to_nfc_syllable(s),
            Self::JamoCompat => to_compat_jamo(s),
        }
    }

    /// Render the committed portion of an FSM event in this output form.
    /// For events without a commit ([`FsmEvent::Nothing`],
    /// [`FsmEvent::Preedit`]), returns an empty string.
    pub fn render_event(self, ev: &FsmEvent) -> String {
        match ev.commit_str() {
            Some(s) => self.render(s),
            None => String::new(),
        }
    }
}

/// Display-friendly rendering: NFC syllables where possible, otherwise
/// Hangul Compatibility Jamo for any unpaired conjoining jamo left over.
///
/// This is what most UIs want for preedit text. A fully composed
/// syllable renders as a single crisp glyph (U+AC00-range), while a
/// lone Cho or Jung falls back to the standalone glyph fonts know how
/// to draw (U+3131-range) instead of the conjoining form that many
/// fonts render poorly in isolation.
pub fn to_display_text(s: &str) -> String {
    to_compat_jamo(&to_nfc_syllable(s))
}

/// Convert a conjoining-Jamo string to NFC syllables.
///
/// The scanner greedily collapses runs of `Cho [Jung [Jong]]` into
/// U+AC00-range syllables; any jamo that can't participate in a
/// syllable (e.g. a lone final, an isolated vowel) passes through
/// unchanged as a conjoining code point. Non-Hangul code points are
/// preserved verbatim.
pub fn to_nfc_syllable(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let cp = chars[i] as u32;
        if let Some(cho_idx) = cho_index(cp) {
            if let Some(&next) = chars.get(i + 1) {
                if let Some(jung_idx) = jung_index(next as u32) {
                    let mut idx = cho_idx * SYLLABLE_STRIDE + jung_idx * JONG_SLOTS;
                    let mut consumed = 2;
                    if let Some(&third) = chars.get(i + 2) {
                        if let Some(jong_comp) = jong_composition_index(third as u32) {
                            idx += jong_comp;
                            consumed = 3;
                        }
                    }
                    if let Some(ch) = char::from_u32(SYLLABLE_BASE + idx) {
                        out.push(ch);
                        i += consumed;
                        continue;
                    }
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Convert a conjoining-Jamo string to Hangul Compatibility Jamo.
///
/// Conjoining initials and finals that share the same letter (e.g. Cho
/// ㄱ U+1100 and Jong ㄱ U+11A8) both map to the same compatibility
/// code point (ㄱ U+3131). Non-Hangul characters pass through.
pub fn to_compat_jamo(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        let cp = ch as u32;
        let mapped = conjoining_to_compat(cp).unwrap_or(cp);
        if let Some(m) = char::from_u32(mapped) {
            out.push(m);
        }
    }
    out
}

fn cho_index(cp: u32) -> Option<u32> {
    (0x1100..=0x1112).contains(&cp).then(|| cp - 0x1100)
}

fn jung_index(cp: u32) -> Option<u32> {
    (0x1161..=0x1175).contains(&cp).then(|| cp - 0x1161)
}

/// NFC composition index for a Jong (1..=27).
fn jong_composition_index(cp: u32) -> Option<u32> {
    (0x11A8..=0x11C2).contains(&cp).then(|| cp - 0x11A8 + 1)
}

/// Map a conjoining-Jamo code point to its Hangul Compatibility Jamo
/// counterpart. Returns `None` for non-Hangul or the small number of
/// conjoining code points that have no compatibility equivalent.
///
/// The `match_same_arms` allow is intentional: Cho and Jong forms of
/// the same letter (e.g. Cho ㄱ U+1100 and Jong ㄱ U+11A8) share a
/// single compatibility code point, and keeping them as separate arms
/// — grouped by role — keeps the table readable against the Unicode
/// chart.
#[allow(clippy::too_many_lines, clippy::match_same_arms)]
fn conjoining_to_compat(cp: u32) -> Option<u32> {
    match cp {
        // Initial consonants (Cho).
        0x1100 => Some(0x3131), // ㄱ
        0x1101 => Some(0x3132), // ㄲ
        0x1102 => Some(0x3134), // ㄴ
        0x1103 => Some(0x3137), // ㄷ
        0x1104 => Some(0x3138), // ㄸ
        0x1105 => Some(0x3139), // ㄹ
        0x1106 => Some(0x3141), // ㅁ
        0x1107 => Some(0x3142), // ㅂ
        0x1108 => Some(0x3143), // ㅃ
        0x1109 => Some(0x3145), // ㅅ
        0x110A => Some(0x3146), // ㅆ
        0x110B => Some(0x3147), // ㅇ
        0x110C => Some(0x3148), // ㅈ
        0x110D => Some(0x3149), // ㅉ
        0x110E => Some(0x314A), // ㅊ
        0x110F => Some(0x314B), // ㅋ
        0x1110 => Some(0x314C), // ㅌ
        0x1111 => Some(0x314D), // ㅍ
        0x1112 => Some(0x314E), // ㅎ

        // Medial vowels (Jung).
        0x1161 => Some(0x314F), // ㅏ
        0x1162 => Some(0x3150), // ㅐ
        0x1163 => Some(0x3151), // ㅑ
        0x1164 => Some(0x3152), // ㅒ
        0x1165 => Some(0x3153), // ㅓ
        0x1166 => Some(0x3154), // ㅔ
        0x1167 => Some(0x3155), // ㅕ
        0x1168 => Some(0x3156), // ㅖ
        0x1169 => Some(0x3157), // ㅗ
        0x116A => Some(0x3158), // ㅘ
        0x116B => Some(0x3159), // ㅙ
        0x116C => Some(0x315A), // ㅚ
        0x116D => Some(0x315B), // ㅛ
        0x116E => Some(0x315C), // ㅜ
        0x116F => Some(0x315D), // ㅝ
        0x1170 => Some(0x315E), // ㅞ
        0x1171 => Some(0x315F), // ㅟ
        0x1172 => Some(0x3160), // ㅠ
        0x1173 => Some(0x3161), // ㅡ
        0x1174 => Some(0x3162), // ㅢ
        0x1175 => Some(0x3163), // ㅣ

        // Final consonants (Jong) — many share a compat glyph with the
        // initial form of the same letter.
        0x11A8 => Some(0x3131), // ㄱ
        0x11A9 => Some(0x3132), // ㄲ
        0x11AA => Some(0x3133), // ㄳ
        0x11AB => Some(0x3134), // ㄴ
        0x11AC => Some(0x3135), // ㄵ
        0x11AD => Some(0x3136), // ㄶ
        0x11AE => Some(0x3137), // ㄷ
        0x11AF => Some(0x3139), // ㄹ
        0x11B0 => Some(0x313A), // ㄺ
        0x11B1 => Some(0x313B), // ㄻ
        0x11B2 => Some(0x313C), // ㄼ
        0x11B3 => Some(0x313D), // ㄽ
        0x11B4 => Some(0x313E), // ㄾ
        0x11B5 => Some(0x313F), // ㄿ
        0x11B6 => Some(0x3140), // ㅀ
        0x11B7 => Some(0x3141), // ㅁ
        0x11B8 => Some(0x3142), // ㅂ
        0x11B9 => Some(0x3144), // ㅄ
        0x11BA => Some(0x3145), // ㅅ
        0x11BB => Some(0x3146), // ㅆ
        0x11BC => Some(0x3147), // ㅇ
        0x11BD => Some(0x3148), // ㅈ
        0x11BE => Some(0x314A), // ㅊ
        0x11BF => Some(0x314B), // ㅋ
        0x11C0 => Some(0x314C), // ㅌ
        0x11C1 => Some(0x314D), // ㅍ
        0x11C2 => Some(0x314E), // ㅎ
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nfc_composes_simple_syllable() {
        // ㄱ + ㅏ + ㄴ → 간
        let s = "\u{1100}\u{1161}\u{11AB}";
        assert_eq!(to_nfc_syllable(s), "간");
    }

    #[test]
    fn nfc_composes_without_jong() {
        // ㄱ + ㅏ → 가
        assert_eq!(to_nfc_syllable("\u{1100}\u{1161}"), "가");
    }

    #[test]
    fn nfc_passes_unpaired_jamo_through() {
        // Lone Cho (no following Jung) stays as conjoining.
        assert_eq!(to_nfc_syllable("\u{1100}"), "\u{1100}");
    }

    #[test]
    fn nfc_handles_nonhangul_verbatim() {
        assert_eq!(to_nfc_syllable("\u{1100}\u{1161}!"), "가!");
    }

    #[test]
    fn nfc_composes_multi_syllable() {
        // 한글 = ㅎㅏㄴ + ㄱㅡㄹ
        let s = "\u{1112}\u{1161}\u{11AB}\u{1100}\u{1173}\u{11AF}";
        assert_eq!(to_nfc_syllable(s), "한글");
    }

    #[test]
    fn compat_maps_cho_and_jong_to_same_letter() {
        let cho = to_compat_jamo("\u{1100}"); // Cho ㄱ
        let jong = to_compat_jamo("\u{11A8}"); // Jong ㄱ
        assert_eq!(cho, jong);
        assert_eq!(cho, "\u{3131}");
    }

    #[test]
    fn render_event_returns_empty_for_preedit_only() {
        let ev = FsmEvent::Preedit("\u{1100}".into());
        assert_eq!(OutputForm::NfcSyllable.render_event(&ev), "");
    }

    #[test]
    fn render_event_converts_commit() {
        let ev = FsmEvent::Commit("\u{1100}\u{1161}".into());
        assert_eq!(OutputForm::NfcSyllable.render_event(&ev), "가");
    }

    #[test]
    fn display_text_uses_nfc_then_compat_fallback() {
        // Lone Cho U+1100 (conjoining) → compat ㄱ U+3131.
        assert_eq!(to_display_text("\u{1100}"), "\u{3131}");
        // ㄱ + ㅏ → NFC 가 (passes through compat layer unchanged).
        assert_eq!(to_display_text("\u{1100}\u{1161}"), "가");
        // Full syllable followed by lone Cho: syllable stays NFC, Cho → compat.
        assert_eq!(to_display_text("\u{1100}\u{1161}\u{1100}"), "가\u{3131}");
    }
}
