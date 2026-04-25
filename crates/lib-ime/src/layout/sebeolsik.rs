//! 공병우 세벌식 3-90 (Sebeolsik 390).
//!
//! Ported verbatim from the authoritative chart used by libhangul
//! (`data/keyboards/hangul-keyboard-39.xml.template`). Layer semantics:
//!
//! * **Unshifted letters/digits** → base-layer Hangul (mostly Cho /
//!   Jung / Jong assignments).
//! * **Shift + letter** → additional Jong consonants (including
//!   compound 겹받침 ᆰ/ᆱ/ᆭ/ᆶ/ᆹ/ᆾ/ᇀ/…).
//! * **Shift + digit / punctuation** → small set of Jong shortcuts
//!   and raw ASCII.
//!
//! The chart treats the unused ASCII positions as literal characters
//! (`,` stays `,`, `!` stays `!` unless a Hangul mapping overrides it).
//! This implementation only returns a Jamo for the mappings that the
//! libhangul XML defines; everything else passes through.

use super::key::{KeyCode, KeyEvent};
use super::{Layout, LayoutKind, LayoutOutput};
use crate::hangul::jamo::JamoInput;

/// 공병우 세벌식 3-90 (Sebeolsik 390).
#[derive(Copy, Clone, Debug, Default)]
pub struct Sebeolsik390;

impl Layout for Sebeolsik390 {
    fn id(&self) -> &'static str {
        "sebeolsik-390"
    }
    fn name(&self) -> &'static str {
        "세벌식 390"
    }
    fn kind(&self) -> LayoutKind {
        LayoutKind::Sebeolsik
    }

    #[allow(
        clippy::too_many_lines,
        clippy::match_same_arms, // alt positions sharing a jamo are intentional
    )]
    fn map(&self, ev: &KeyEvent) -> LayoutOutput {
        if !ev.mods.is_ime_eligible() {
            return LayoutOutput::Passthrough;
        }
        match (ev.code, ev.mods.shift) {
            // ────────── Base layer (unshifted) ──────────
            // Digit row
            (KeyCode::Digit1, false) => jong(0x11C2), // ᇂ ㅎ
            (KeyCode::Digit2, false) => jong(0x11BB), // ᆻ ㅆ
            (KeyCode::Digit3, false) => jong(0x11B8), // ᆸ ㅂ
            (KeyCode::Digit4, false) => jung(0x116D), // ᅭ ㅛ
            (KeyCode::Digit5, false) => jung(0x1172), // ᅲ ㅠ
            (KeyCode::Digit6, false) => jung(0x1163), // ᅣ ㅑ
            (KeyCode::Digit7, false) => jung(0x1168), // ᅨ ㅖ
            (KeyCode::Digit8, false) => jung(0x1174), // ᅴ ㅢ
            (KeyCode::Digit9, false) => jung(0x116E), // ᅮ ㅜ
            (KeyCode::Digit0, false) => cho(0x110F),  // ᄏ ㅋ

            // Top row (Jong on the left, Jung in the middle, Cho on the right)
            (KeyCode::KeyQ, false) => jong(0x11BA), // ᆺ ㅅ
            (KeyCode::KeyW, false) => jong(0x11AF), // ᆯ ㄹ
            (KeyCode::KeyE, false) => jung(0x1167), // ᅧ ㅕ
            (KeyCode::KeyR, false) => jung(0x1162), // ᅢ ㅐ
            (KeyCode::KeyT, false) => jung(0x1165), // ᅥ ㅓ
            (KeyCode::KeyY, false) => cho(0x1105),  // ᄅ ㄹ
            (KeyCode::KeyU, false) => cho(0x1103),  // ᄃ ㄷ
            (KeyCode::KeyI, false) => cho(0x1106),  // ᄆ ㅁ
            (KeyCode::KeyO, false) => cho(0x110E),  // ᄎ ㅊ
            (KeyCode::KeyP, false) => cho(0x1111),  // ᄑ ㅍ

            // Home row
            (KeyCode::KeyA, false) => jong(0x11BC), // ᆼ ㅇ
            (KeyCode::KeyS, false) => jong(0x11AB), // ᆫ ㄴ
            (KeyCode::KeyD, false) => jung(0x1175), // ᅵ ㅣ
            (KeyCode::KeyF, false) => jung(0x1161), // ᅡ ㅏ
            (KeyCode::KeyG, false) => jung(0x1173), // ᅳ ㅡ
            (KeyCode::KeyH, false) => cho(0x1102),  // ᄂ ㄴ
            (KeyCode::KeyJ, false) => cho(0x110B),  // ᄋ ㅇ
            (KeyCode::KeyK, false) => cho(0x1100),  // ᄀ ㄱ
            (KeyCode::KeyL, false) => cho(0x110C),  // ᄌ ㅈ
            (KeyCode::Semicolon, false) => cho(0x1107), // ᄇ ㅂ
            (KeyCode::Quote, false) => cho(0x1110),     // ᄐ ㅌ

            // Bottom row
            (KeyCode::KeyZ, false) => jong(0x11B7), // ᆷ ㅁ
            (KeyCode::KeyX, false) => jong(0x11A8), // ᆨ ㄱ
            (KeyCode::KeyC, false) => jung(0x1166), // ᅦ ㅔ
            (KeyCode::KeyV, false) => jung(0x1169), // ᅩ ㅗ
            (KeyCode::KeyB, false) => jung(0x116E), // ᅮ ㅜ (alt)
            (KeyCode::KeyN, false) => cho(0x1109),  // ᄉ ㅅ
            (KeyCode::KeyM, false) => cho(0x1112),  // ᄒ ㅎ
            (KeyCode::Slash, false) => jung(0x1169), // ᅩ ㅗ (alt)

            // ────────── Shift layer ──────────
            // Digit row: Shift+digit adds a Jong shortcut where libhangul defines it.
            (KeyCode::Digit1, true) => jong(0x11BD), // Shift+1 → ᆽ ㅈ

            // Letters: Shift+letter exposes additional Jong
            (KeyCode::KeyA, true) => jong(0x11AE), // ᆮ ㄷ
            (KeyCode::KeyC, true) => jong(0x11B1), // ᆱ ㄻ
            (KeyCode::KeyD, true) => jong(0x11B0), // ᆰ ㄺ
            (KeyCode::KeyE, true) => jong(0x11BF), // ᆿ ㅋ
            (KeyCode::KeyF, true) => jong(0x11A9), // ᆩ ㄲ
            (KeyCode::KeyQ, true) => jong(0x11C1), // ᇁ ㅍ
            (KeyCode::KeyR, true) => jung(0x1164), // ᅤ ㅒ
            (KeyCode::KeyS, true) => jong(0x11AD), // ᆭ ㄶ
            (KeyCode::KeyV, true) => jong(0x11B6), // ᆶ ㅀ
            (KeyCode::KeyW, true) => jong(0x11C0), // ᇀ ㅌ
            (KeyCode::KeyX, true) => jong(0x11B9), // ᆹ ㅄ
            (KeyCode::KeyZ, true) => jong(0x11BE), // ᆾ ㅊ

            _ => LayoutOutput::Passthrough,
        }
    }

    fn supports_moachigi(&self) -> bool {
        true
    }
}

#[inline]
fn cho(cp: u32) -> LayoutOutput {
    LayoutOutput::Jamo(JamoInput::cho_only(cp))
}
#[inline]
fn jung(cp: u32) -> LayoutOutput {
    LayoutOutput::Jamo(JamoInput::vowel(cp))
}
#[inline]
fn jong(cp: u32) -> LayoutOutput {
    LayoutOutput::Jamo(JamoInput::jong_only(cp))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{to_nfc_syllable, HangulFsm, LayoutOutput as LO};

    fn typeout(keys: &[(KeyCode, bool)]) -> String {
        let layout = Sebeolsik390;
        let mut fsm = HangulFsm::new();
        let mut out = String::new();
        for &(code, shift) in keys {
            let ev = if shift { KeyEvent::shift(code) } else { KeyEvent::plain(code) };
            if let LO::Jamo(j) = layout.map(&ev) {
                let fe = fsm.feed(j);
                if let Some(s) = fe.commit_str() {
                    out.push_str(s);
                }
            }
        }
        out.push_str(&fsm.preedit_string());
        to_nfc_syllable(&out)
    }

    #[test]
    fn annyeong_haseyo() {
        // 안 = j(ᄋ) f(ᅡ) s(ᆫ)
        // 녕 = h(ᄂ) e(ᅧ) a(ᆼ)
        // 하 = m(ᄒ) f(ᅡ)
        // 세 = n(ᄉ) c(ᅦ)
        // 요 = j(ᄋ) 4(ᅭ)
        assert_eq!(
            typeout(&[
                (KeyCode::KeyJ, false), (KeyCode::KeyF, false), (KeyCode::KeyS, false),
                (KeyCode::KeyH, false), (KeyCode::KeyE, false), (KeyCode::KeyA, false),
                (KeyCode::KeyM, false), (KeyCode::KeyF, false),
                (KeyCode::KeyN, false), (KeyCode::KeyC, false),
                (KeyCode::KeyJ, false), (KeyCode::Digit4, false),
            ]),
            "안녕하세요"
        );
    }

    #[test]
    fn hakgyo_via_390() {
        // 학 = m(ᄒ) f(ᅡ) x(ᆨ)
        // 교 = k(ᄀ) 4(ᅭ)
        assert_eq!(
            typeout(&[
                (KeyCode::KeyM, false), (KeyCode::KeyF, false), (KeyCode::KeyX, false),
                (KeyCode::KeyK, false), (KeyCode::Digit4, false),
            ]),
            "학교"
        );
    }

    #[test]
    fn zones_match_libhangul() {
        assert_eq!(
            Sebeolsik390.map(&KeyEvent::plain(KeyCode::KeyK)),
            LayoutOutput::Jamo(JamoInput::cho_only(0x1100))
        );
        assert_eq!(
            Sebeolsik390.map(&KeyEvent::plain(KeyCode::KeyS)),
            LayoutOutput::Jamo(JamoInput::jong_only(0x11AB))
        );
        assert_eq!(
            Sebeolsik390.map(&KeyEvent::plain(KeyCode::KeyF)),
            LayoutOutput::Jamo(JamoInput::vowel(0x1161))
        );
        assert_eq!(
            Sebeolsik390.map(&KeyEvent::plain(KeyCode::Digit4)),
            LayoutOutput::Jamo(JamoInput::vowel(0x116D))
        );
    }
}
