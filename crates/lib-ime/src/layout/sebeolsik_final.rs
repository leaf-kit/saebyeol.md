//! 공병우 세벌식 3-91 (Sebeolsik Final, 1991).
//!
//! Ported verbatim from the authoritative chart used by libhangul
//! (`data/keyboards/hangul-keyboard-3f.xml.template`). Differs from
//! 3-90 primarily in the Shift layer, which puts commonly-needed
//! **compound finals** (겹받침 ᆪ ᆬ ᆰ ᆱ ᆲ ᆳ ᆴ ᆵ ᆶ ᇁ ᇀ) on
//! Shift + letter shortcuts.
//!
//! Base layer is identical to 3-90; only the Shift layer and a handful
//! of punctuation positions differ.

use super::key::{KeyCode, KeyEvent};
use super::{Layout, LayoutKind, LayoutOutput};
use crate::hangul::jamo::JamoInput;

/// 공병우 세벌식 3-91 (Sebeolsik Final).
#[derive(Copy, Clone, Debug, Default)]
pub struct SebeolsikFinal;

impl Layout for SebeolsikFinal {
    fn id(&self) -> &'static str {
        "sebeolsik-final"
    }
    fn name(&self) -> &'static str {
        "세벌식 최종"
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
            // ────────── Base layer (unshifted) — same as 3-90 ──────────
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

            (KeyCode::KeyZ, false) => jong(0x11B7), // ᆷ ㅁ
            (KeyCode::KeyX, false) => jong(0x11A8), // ᆨ ㄱ
            (KeyCode::KeyC, false) => jung(0x1166), // ᅦ ㅔ
            (KeyCode::KeyV, false) => jung(0x1169), // ᅩ ㅗ
            (KeyCode::KeyB, false) => jung(0x116E), // ᅮ ㅜ (alt)
            (KeyCode::KeyN, false) => cho(0x1109),  // ᄉ ㅅ
            (KeyCode::KeyM, false) => cho(0x1112),  // ᄒ ㅎ
            (KeyCode::Slash, false) => jung(0x1169), // ᅩ ㅗ (alt)

            // ────────── Shift layer — 3-91 specific ──────────
            // Digit row Shift → compound / simple Jongs.
            (KeyCode::Digit1, true) => jong(0x11A9), // Shift+1 → ᆩ ㄲ
            (KeyCode::Digit2, true) => jong(0x11B0), // Shift+2 → ᆰ ㄺ
            (KeyCode::Digit3, true) => jong(0x11BD), // Shift+3 → ᆽ ㅈ
            (KeyCode::Digit4, true) => jong(0x11B5), // Shift+4 → ᆵ ㄿ
            (KeyCode::Digit5, true) => jong(0x11B4), // Shift+5 → ᆴ ㄾ

            // Letter Shift layer.
            (KeyCode::KeyA, true) => jong(0x11AE), // ᆮ ㄷ
            (KeyCode::KeyC, true) => jong(0x11BF), // ᆿ ㅋ
            (KeyCode::KeyD, true) => jong(0x11B2), // ᆲ ㄼ
            (KeyCode::KeyE, true) => jong(0x11AC), // ᆬ ㄵ
            (KeyCode::KeyF, true) => jong(0x11B1), // ᆱ ㄻ
            (KeyCode::KeyG, true) => jung(0x1164), // ᅤ ㅒ
            (KeyCode::KeyQ, true) => jong(0x11C1), // ᇁ ㅍ
            (KeyCode::KeyR, true) => jong(0x11B6), // ᆶ ㅀ
            (KeyCode::KeyS, true) => jong(0x11AD), // ᆭ ㄶ
            (KeyCode::KeyT, true) => jong(0x11B3), // ᆳ ㄽ
            (KeyCode::KeyV, true) => jong(0x11AA), // ᆪ ㄳ
            (KeyCode::KeyW, true) => jong(0x11C0), // ᇀ ㅌ
            (KeyCode::KeyX, true) => jong(0x11B9), // ᆹ ㅄ
            (KeyCode::KeyZ, true) => jong(0x11BE), // ᆾ ㅊ

            // Digit aliases — the base digit row is Hangul, so 3-91
            // exposes the digits on Shift + the right-hand letters.
            (KeyCode::KeyY, true) => LayoutOutput::Char('5'),
            (KeyCode::KeyU, true) => LayoutOutput::Char('6'),
            (KeyCode::KeyI, true) => LayoutOutput::Char('7'),
            (KeyCode::KeyO, true) => LayoutOutput::Char('8'),
            (KeyCode::KeyP, true) => LayoutOutput::Char('9'),
            (KeyCode::KeyH, true) => LayoutOutput::Char('0'),
            (KeyCode::KeyJ, true) => LayoutOutput::Char('1'),
            (KeyCode::KeyK, true) => LayoutOutput::Char('2'),
            (KeyCode::KeyL, true) => LayoutOutput::Char('3'),

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
    use crate::{to_nfc_syllable, ComposeMode, HangulFsm, LayoutOutput as LO};

    fn typeout_with(mode: ComposeMode, events: &[(KeyCode, bool)]) -> String {
        let layout = SebeolsikFinal;
        let mut fsm = HangulFsm::with_mode(mode);
        let mut out = String::new();
        for &(code, shift) in events {
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

    fn typeout(events: &[(KeyCode, bool)]) -> String {
        typeout_with(ComposeMode::Sequential, events)
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
    fn hakgyo() {
        assert_eq!(
            typeout(&[
                (KeyCode::KeyM, false), (KeyCode::KeyF, false), (KeyCode::KeyX, false),
                (KeyCode::KeyK, false), (KeyCode::Digit4, false),
            ]),
            "학교"
        );
    }

    #[test]
    fn meokda() {
        // 먹 = i(ᄆ) t(ᅥ) x(ᆨ)
        // 다 = u(ᄃ) f(ᅡ)
        assert_eq!(
            typeout(&[
                (KeyCode::KeyI, false), (KeyCode::KeyT, false), (KeyCode::KeyX, false),
                (KeyCode::KeyU, false), (KeyCode::KeyF, false),
            ]),
            "먹다"
        );
    }

    #[test]
    fn compound_jong_via_base_and_shift() {
        // 밝 base-layer: ; f w x (ᄇ ᅡ ᆯ ᆨ  →  ㅂ+ㅏ+ㄺ)
        assert_eq!(
            typeout(&[
                (KeyCode::Semicolon, false), (KeyCode::KeyF, false),
                (KeyCode::KeyW, false), (KeyCode::KeyX, false),
            ]),
            "밝"
        );
        // 밝 shift shortcut: ; f Shift+2 (ᆰ)
        assert_eq!(
            typeout(&[
                (KeyCode::Semicolon, false), (KeyCode::KeyF, false),
                (KeyCode::Digit2, true),
            ]),
            "밝"
        );
    }

    /// User presses a simple Jong (ᆯ via `KeyW`) then a direct-compound
    /// Shift key (ᆲ via `Shift+D`, ᆰ via `Shift+2`, etc.) that starts
    /// with that simple. The compound must absorb the existing Jong
    /// rather than starting a new syllable.
    #[test]
    fn simple_then_direct_compound_absorbs() {
        // 갋 = k(ᄀ) f(ᅡ) w(ᆯ) Shift+D(ᆲ)
        assert_eq!(
            typeout(&[
                (KeyCode::KeyK, false), (KeyCode::KeyF, false),
                (KeyCode::KeyW, false), (KeyCode::KeyD, true),
            ]),
            "갋"
        );
        // 갉 = k(ᄀ) f(ᅡ) w(ᆯ) Shift+2(ᆰ)
        assert_eq!(
            typeout(&[
                (KeyCode::KeyK, false), (KeyCode::KeyF, false),
                (KeyCode::KeyW, false), (KeyCode::Digit2, true),
            ]),
            "갉"
        );
        // 값 = k(ᄀ) f(ᅡ) 3(ᆸ) Shift+X(ᆹ)
        assert_eq!(
            typeout(&[
                (KeyCode::KeyK, false), (KeyCode::KeyF, false),
                (KeyCode::Digit3, false), (KeyCode::KeyX, true),
            ]),
            "값"
        );
        // Same cases in Moachigi mode.
        assert_eq!(
            typeout_with(ComposeMode::Moachigi, &[
                (KeyCode::KeyK, false), (KeyCode::KeyF, false),
                (KeyCode::KeyW, false), (KeyCode::KeyD, true),
            ]),
            "갋"
        );
    }

    #[test]
    fn reverse_order_works_in_moachigi() {
        // 안 typed in reverse: s(ᆫ) → f(ᅡ) → j(ᄋ)
        assert_eq!(
            typeout_with(ComposeMode::Moachigi, &[
                (KeyCode::KeyS, false),
                (KeyCode::KeyF, false),
                (KeyCode::KeyJ, false),
            ]),
            "안"
        );
    }

    #[test]
    fn reverse_whole_annyeonghaseyo_in_moachigi() {
        // Each syllable typed Jong → Jung → Cho.
        // 안 = s f j, 녕 = a e h, 하 = f m, 세 = c n, 요 = 4 j
        assert_eq!(
            typeout_with(ComposeMode::Moachigi, &[
                (KeyCode::KeyS, false), (KeyCode::KeyF, false), (KeyCode::KeyJ, false),
                (KeyCode::KeyA, false), (KeyCode::KeyE, false), (KeyCode::KeyH, false),
                (KeyCode::KeyF, false), (KeyCode::KeyM, false),
                (KeyCode::KeyC, false), (KeyCode::KeyN, false),
                (KeyCode::Digit4, false), (KeyCode::KeyJ, false),
            ]),
            "안녕하세요"
        );
    }

    /// End-to-end sweep: every compound-Jong syllable reachable via
    /// Sebeolsik Final keystrokes, through three paths (direct Shift
    /// key, component compose, simple-absorbed-by-compound), in both
    /// Moachigi and Sequential modes. Regression for
    /// "종성 결합이 잘 안되는 문제".
    #[test]
    #[allow(clippy::type_complexity)]
    fn all_compound_jongs_via_sebeolsik_final_keystrokes() {
        // (label, direct, compose, absorb, expected)
        // `direct` = the Shift key that emits the compound Jong directly.
        // `compose` = two Jong keys whose codepoints feed compose_jong.
        // `absorb` = simple component first, then direct compound key.
        // Prefix `k f` (ᄀ+ᅡ) is prepended to every case.
        let cases: &[(
            &str,
            &[(KeyCode, bool)],
            &[(KeyCode, bool)],
            &[(KeyCode, bool)],
            &str,
        )] = &[
            ("ᆩ", &[(KeyCode::Digit1, true)],
             &[(KeyCode::KeyX, false), (KeyCode::KeyX, false)],
             &[(KeyCode::KeyX, false), (KeyCode::Digit1, true)], "갂"),
            ("ᆪ", &[(KeyCode::KeyV, true)],
             &[(KeyCode::KeyX, false), (KeyCode::KeyQ, false)],
             &[(KeyCode::KeyX, false), (KeyCode::KeyV, true)], "갃"),
            ("ᆬ", &[(KeyCode::KeyE, true)],
             &[(KeyCode::KeyS, false), (KeyCode::Digit3, true)],
             &[(KeyCode::KeyS, false), (KeyCode::KeyE, true)], "갅"),
            ("ᆭ", &[(KeyCode::KeyS, true)],
             &[(KeyCode::KeyS, false), (KeyCode::Digit1, false)],
             &[(KeyCode::KeyS, false), (KeyCode::KeyS, true)], "갆"),
            ("ᆰ", &[(KeyCode::Digit2, true)],
             &[(KeyCode::KeyW, false), (KeyCode::KeyX, false)],
             &[(KeyCode::KeyW, false), (KeyCode::Digit2, true)], "갉"),
            ("ᆱ", &[(KeyCode::KeyF, true)],
             &[(KeyCode::KeyW, false), (KeyCode::KeyZ, false)],
             &[(KeyCode::KeyW, false), (KeyCode::KeyF, true)], "갊"),
            ("ᆲ", &[(KeyCode::KeyD, true)],
             &[(KeyCode::KeyW, false), (KeyCode::Digit3, false)],
             &[(KeyCode::KeyW, false), (KeyCode::KeyD, true)], "갋"),
            ("ᆳ", &[(KeyCode::KeyT, true)],
             &[(KeyCode::KeyW, false), (KeyCode::KeyQ, false)],
             &[(KeyCode::KeyW, false), (KeyCode::KeyT, true)], "갌"),
            ("ᆴ", &[(KeyCode::Digit5, true)],
             &[(KeyCode::KeyW, false), (KeyCode::KeyW, true)],
             &[(KeyCode::KeyW, false), (KeyCode::Digit5, true)], "갍"),
            ("ᆵ", &[(KeyCode::Digit4, true)],
             &[(KeyCode::KeyW, false), (KeyCode::KeyQ, true)],
             &[(KeyCode::KeyW, false), (KeyCode::Digit4, true)], "갎"),
            ("ᆶ", &[(KeyCode::KeyR, true)],
             &[(KeyCode::KeyW, false), (KeyCode::Digit1, false)],
             &[(KeyCode::KeyW, false), (KeyCode::KeyR, true)], "갏"),
            ("ᆹ", &[(KeyCode::KeyX, true)],
             &[(KeyCode::Digit3, false), (KeyCode::KeyQ, false)],
             &[(KeyCode::Digit3, false), (KeyCode::KeyX, true)], "값"),
            ("ᆻ", &[(KeyCode::Digit2, false)],
             &[(KeyCode::KeyQ, false), (KeyCode::KeyQ, false)],
             &[(KeyCode::KeyQ, false), (KeyCode::Digit2, false)], "갔"),
        ];
        let prefix = [(KeyCode::KeyK, false), (KeyCode::KeyF, false)];
        let join = |tail: &[(KeyCode, bool)]| -> Vec<(KeyCode, bool)> {
            let mut v = prefix.to_vec();
            v.extend_from_slice(tail);
            v
        };
        for &(label, direct, compose, absorb, expected) in cases {
            for (mode, mode_name) in [
                (ComposeMode::Moachigi, "moachigi"),
                (ComposeMode::Sequential, "sequential"),
            ] {
                assert_eq!(
                    typeout_with(mode, &join(direct)), expected,
                    "{label} direct ({mode_name})",
                );
                assert_eq!(
                    typeout_with(mode, &join(compose)), expected,
                    "{label} compose ({mode_name})",
                );
                assert_eq!(
                    typeout_with(mode, &join(absorb)), expected,
                    "{label} absorb ({mode_name})",
                );
            }
        }
    }

    #[test]
    fn moachigi_accepts_sequential_input_too() {
        // Moachigi is a superset: normal order still works.
        assert_eq!(
            typeout_with(ComposeMode::Moachigi, &[
                (KeyCode::KeyJ, false), (KeyCode::KeyF, false), (KeyCode::KeyS, false),
            ]),
            "안"
        );
    }
}
