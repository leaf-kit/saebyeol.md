//! 두벌식 표준 (KS X 5002) layout.
//!
//! Every consonant key carries both a Cho and a Jong conjoining code
//! point; the Hangul FSM picks the role by context. Shift-modified keys
//! map to doubled (쌍자음) variants where they exist.

use super::key::{KeyCode, KeyEvent};
use super::{Layout, LayoutKind, LayoutOutput};
use crate::hangul::jamo::JamoInput;

/// Dubeolsik standard (KS X 5002).
#[derive(Copy, Clone, Debug, Default)]
pub struct Dubeolsik;

impl Layout for Dubeolsik {
    fn id(&self) -> &'static str {
        "dubeolsik-std"
    }
    fn name(&self) -> &'static str {
        "두벌식 표준 (KS X 5002)"
    }
    fn kind(&self) -> LayoutKind {
        LayoutKind::Dubeolsik
    }

    fn map(&self, ev: &KeyEvent) -> LayoutOutput {
        if !ev.mods.is_ime_eligible() {
            return LayoutOutput::Passthrough;
        }
        map_key(ev.code, ev.mods.shift)
    }
}

fn map_key(code: KeyCode, shift: bool) -> LayoutOutput {
    use JamoInput as J;
    match (code, shift) {
        // ─── Top row: consonants ─────────────────────────────────────
        (KeyCode::KeyQ, false) => LayoutOutput::Jamo(J::cho_dual(0x1107, 0x11B8)), // ㅂ
        (KeyCode::KeyQ, true)  => LayoutOutput::Jamo(J::cho_only(0x1108)),         // ㅃ (no Jong form)
        (KeyCode::KeyW, false) => LayoutOutput::Jamo(J::cho_dual(0x110C, 0x11BD)), // ㅈ
        (KeyCode::KeyW, true)  => LayoutOutput::Jamo(J::cho_only(0x110D)),         // ㅉ
        (KeyCode::KeyE, false) => LayoutOutput::Jamo(J::cho_dual(0x1103, 0x11AE)), // ㄷ
        (KeyCode::KeyE, true)  => LayoutOutput::Jamo(J::cho_only(0x1104)),         // ㄸ
        (KeyCode::KeyR, false) => LayoutOutput::Jamo(J::cho_dual(0x1100, 0x11A8)), // ㄱ
        (KeyCode::KeyR, true)  => LayoutOutput::Jamo(J::cho_dual(0x1101, 0x11A9)), // ㄲ
        (KeyCode::KeyT, false) => LayoutOutput::Jamo(J::cho_dual(0x1109, 0x11BA)), // ㅅ
        (KeyCode::KeyT, true)  => LayoutOutput::Jamo(J::cho_dual(0x110A, 0x11BB)), // ㅆ

        // ─── Top row: vowels ─────────────────────────────────────────
        (KeyCode::KeyY, _)     => LayoutOutput::Jamo(J::vowel(0x116D)),            // ㅛ
        (KeyCode::KeyU, _)     => LayoutOutput::Jamo(J::vowel(0x1167)),            // ㅕ
        (KeyCode::KeyI, _)     => LayoutOutput::Jamo(J::vowel(0x1163)),            // ㅑ
        (KeyCode::KeyO, false) => LayoutOutput::Jamo(J::vowel(0x1162)),            // ㅐ
        (KeyCode::KeyO, true)  => LayoutOutput::Jamo(J::vowel(0x1164)),            // ㅒ
        (KeyCode::KeyP, false) => LayoutOutput::Jamo(J::vowel(0x1166)),            // ㅔ
        (KeyCode::KeyP, true)  => LayoutOutput::Jamo(J::vowel(0x1168)),            // ㅖ

        // ─── Home row ────────────────────────────────────────────────
        (KeyCode::KeyA, _)     => LayoutOutput::Jamo(J::cho_dual(0x1106, 0x11B7)), // ㅁ
        (KeyCode::KeyS, _)     => LayoutOutput::Jamo(J::cho_dual(0x1102, 0x11AB)), // ㄴ
        (KeyCode::KeyD, _)     => LayoutOutput::Jamo(J::cho_dual(0x110B, 0x11BC)), // ㅇ
        (KeyCode::KeyF, _)     => LayoutOutput::Jamo(J::cho_dual(0x1105, 0x11AF)), // ㄹ
        (KeyCode::KeyG, _)     => LayoutOutput::Jamo(J::cho_dual(0x1112, 0x11C2)), // ㅎ
        (KeyCode::KeyH, _)     => LayoutOutput::Jamo(J::vowel(0x1169)),            // ㅗ
        (KeyCode::KeyJ, _)     => LayoutOutput::Jamo(J::vowel(0x1165)),            // ㅓ
        (KeyCode::KeyK, _)     => LayoutOutput::Jamo(J::vowel(0x1161)),            // ㅏ
        (KeyCode::KeyL, _)     => LayoutOutput::Jamo(J::vowel(0x1175)),            // ㅣ

        // ─── Bottom row ──────────────────────────────────────────────
        (KeyCode::KeyZ, _)     => LayoutOutput::Jamo(J::cho_dual(0x110F, 0x11BF)), // ㅋ
        (KeyCode::KeyX, _)     => LayoutOutput::Jamo(J::cho_dual(0x1110, 0x11C0)), // ㅌ
        (KeyCode::KeyC, _)     => LayoutOutput::Jamo(J::cho_dual(0x110E, 0x11BE)), // ㅊ
        (KeyCode::KeyV, _)     => LayoutOutput::Jamo(J::cho_dual(0x1111, 0x11C1)), // ㅍ
        (KeyCode::KeyB, _)     => LayoutOutput::Jamo(J::vowel(0x1172)),            // ㅠ
        (KeyCode::KeyN, _)     => LayoutOutput::Jamo(J::vowel(0x116E)),            // ㅜ
        (KeyCode::KeyM, _)     => LayoutOutput::Jamo(J::vowel(0x1173)),            // ㅡ

        _ => LayoutOutput::Passthrough,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(code: KeyCode) -> KeyEvent {
        KeyEvent::plain(code)
    }
    fn shifted(code: KeyCode) -> KeyEvent {
        KeyEvent::shift(code)
    }

    #[test]
    fn r_maps_to_giyeok() {
        let out = Dubeolsik.map(&plain(KeyCode::KeyR));
        assert_eq!(out, LayoutOutput::Jamo(JamoInput::cho_dual(0x1100, 0x11A8)));
    }

    #[test]
    fn shift_r_maps_to_ssangkiyeok() {
        let out = Dubeolsik.map(&shifted(KeyCode::KeyR));
        assert_eq!(out, LayoutOutput::Jamo(JamoInput::cho_dual(0x1101, 0x11A9)));
    }

    #[test]
    fn shift_q_is_cho_only() {
        // ㅃ has no Jong form.
        let out = Dubeolsik.map(&shifted(KeyCode::KeyQ));
        assert_eq!(out, LayoutOutput::Jamo(JamoInput::cho_only(0x1108)));
    }

    #[test]
    fn ctrl_key_is_passthrough() {
        let ev = KeyEvent {
            code: KeyCode::KeyA,
            mods: crate::Modifiers {
                ctrl: true,
                ..crate::Modifiers::NONE
            },
            repeat: false,
        };
        assert_eq!(Dubeolsik.map(&ev), LayoutOutput::Passthrough);
    }

    #[test]
    fn space_is_passthrough() {
        assert_eq!(Dubeolsik.map(&plain(KeyCode::Space)), LayoutOutput::Passthrough);
    }
}
