//! Integration tests covering spec §2.4 (Hangul FSM must-pass cases).
//!
//! The tests drive [`lib_ime::HangulFsm`] directly using the Dubeolsik
//! layout to translate physical keys. Results are collapsed to NFC so
//! assertions read naturally.

use lib_ime::{
    to_nfc_syllable, Dubeolsik, FsmEvent, HangulFsm, JamoInput, KeyCode, KeyEvent, Layout,
    LayoutOutput, Modifiers, OutputForm,
};

/// Drive the FSM through a sequence of `(KeyCode, shift)` pairs and
/// return `(commit_so_far, final_preedit)` in NFC form.
fn type_through(keys: &[(KeyCode, bool)]) -> (String, String) {
    let layout = Dubeolsik;
    let mut fsm = HangulFsm::new();
    let mut commit = String::new();
    for &(code, shift) in keys {
        let ev = KeyEvent {
            code,
            mods: if shift { Modifiers::SHIFT } else { Modifiers::NONE },
            repeat: false,
        };
        if let LayoutOutput::Jamo(j) = layout.map(&ev) {
            let ev = fsm.feed(j);
            commit.push_str(OutputForm::JamoConjoining.render_event(&ev).as_str());
        }
    }
    let preedit_raw = fsm.preedit_string();
    (to_nfc_syllable(&commit), to_nfc_syllable(&preedit_raw))
}

/// Feed raw Jamo inputs directly (bypasses the layout), returning committed + preedit in NFC.
fn feed_jamos(inputs: &[JamoInput]) -> (String, String) {
    let mut fsm = HangulFsm::new();
    let mut commit = String::new();
    for &input in inputs {
        let ev = fsm.feed(input);
        commit.push_str(OutputForm::JamoConjoining.render_event(&ev).as_str());
    }
    let preedit = fsm.preedit_string();
    (to_nfc_syllable(&commit), to_nfc_syllable(&preedit))
}

// ─────────────────────────── Spec §2.4 cases ───────────────────────────

#[test]
fn annyeong_haseyo_roundtrip() {
    // "안녕하세요" = ㅇㅏㄴ + ㄴㅕㅇ + ㅎㅏ + ㅅㅔ + ㅇㅛ
    // Dubeolsik keys: d k s / s u d / g k / t p / d y
    let keys = &[
        (KeyCode::KeyD, false), (KeyCode::KeyK, false), (KeyCode::KeyS, false), // 안
        (KeyCode::KeyS, false), (KeyCode::KeyU, false), (KeyCode::KeyD, false), // 녕
        (KeyCode::KeyG, false), (KeyCode::KeyK, false),                         // 하
        (KeyCode::KeyT, false), (KeyCode::KeyP, false),                         // 세
        (KeyCode::KeyD, false), (KeyCode::KeyY, false),                         // 요
    ];
    let (commit, preedit) = type_through(keys);
    // The final syllable "요" remains in the preedit until flushed.
    assert_eq!(format!("{commit}{preedit}"), "안녕하세요");
}

#[test]
fn compound_jong_moves_on_vowel() {
    // Spec §2.4: "갃" + "ㅏ" → commit "각", preedit "사"
    // Keys: r k r t k  (ㄱ ㅏ ㄱ ㅅ ㅏ)
    let keys = &[
        (KeyCode::KeyR, false),
        (KeyCode::KeyK, false),
        (KeyCode::KeyR, false),
        (KeyCode::KeyT, false),
        (KeyCode::KeyK, false),
    ];
    let (commit, preedit) = type_through(keys);
    assert_eq!(commit, "각");
    assert_eq!(preedit, "사");
}

#[test]
fn single_jong_moves_whole_consonant() {
    // Spec §2.4: "갔" + "ㅏ". ㅆ is a single jong, so the whole
    // consonant moves to the new syllable's Cho position.
    //
    // Real-world Hangul IMEs produce "가싸" (ㅆ moves intact as ㅆ);
    // the original spec note "(ㅆ은 겹받침 아님, 단일)" with example
    // "사" appears to be a typo. We assert the correct "싸" behavior.
    let keys = &[
        (KeyCode::KeyR, false),           // ㄱ
        (KeyCode::KeyK, false),           // ㅏ
        (KeyCode::KeyT, true),            // ㅆ (Shift+t)
        (KeyCode::KeyK, false),           // ㅏ
    ];
    let (commit, preedit) = type_through(keys);
    assert_eq!(commit, "가");
    assert_eq!(preedit, "싸");
}

#[test]
fn compound_jong_forms_on_second_consonant() {
    // Spec §2.4: "갈" + "ㄱ" → preedit "갉"
    let keys = &[
        (KeyCode::KeyR, false), // ㄱ
        (KeyCode::KeyK, false), // ㅏ
        (KeyCode::KeyF, false), // ㄹ
        (KeyCode::KeyR, false), // ㄱ
    ];
    let (commit, preedit) = type_through(keys);
    assert!(commit.is_empty());
    assert_eq!(preedit, "갉");
}

#[test]
fn compound_vowel_forms_in_preedit() {
    // Spec §2.4: "뷁" — ㅂ + ㅜ + ㅔ (→ㅞ) + ㄹ + ㄱ (→ㄺ)
    let inputs = &[
        JamoInput::cho_dual(0x1107, 0x11B8), // ㅂ
        JamoInput::Jung(0x116E),             // ㅜ
        JamoInput::Jung(0x1166),             // ㅔ → ㅞ
        JamoInput::cho_dual(0x1105, 0x11AF), // ㄹ
        JamoInput::cho_dual(0x1100, 0x11A8), // ㄱ → ㄺ
    ];
    let (commit, preedit) = feed_jamos(inputs);
    assert!(commit.is_empty());
    assert_eq!(preedit, "뷁");
}

#[test]
fn escape_cancels_without_commit() {
    let mut fsm = HangulFsm::new();
    let _ = fsm.feed(JamoInput::cho_dual(0x1100, 0x11A8));
    let _ = fsm.feed(JamoInput::Jung(0x1161));
    assert!(fsm.is_composing());
    let ev = fsm.cancel();
    assert_eq!(ev, FsmEvent::Preedit(String::new()));
    assert!(!fsm.is_composing());
}

#[test]
fn backspace_decomposes_compound_vowel_before_removing() {
    // Build "과" = ChoJung(ㄱ, ㅘ). Backspace should first collapse ㅘ→ㅗ.
    let mut fsm = HangulFsm::new();
    let _ = fsm.feed(JamoInput::cho_dual(0x1100, 0x11A8)); // ㄱ
    let _ = fsm.feed(JamoInput::Jung(0x1169));             // ㅗ
    let _ = fsm.feed(JamoInput::Jung(0x1161));             // ㅏ → ㅘ
    assert_eq!(to_nfc_syllable(&fsm.preedit_string()), "과");

    let ev = fsm.backspace();
    assert_eq!(to_nfc_syllable(ev.preedit_str().unwrap_or("")), "고");

    let ev = fsm.backspace();
    assert_eq!(ev.preedit_str(), Some("\u{1100}")); // ㄱ alone

    let ev = fsm.backspace();
    assert_eq!(ev, FsmEvent::Preedit(String::new()));
    assert!(!fsm.is_composing());
}

#[test]
fn backspace_decomposes_compound_jong() {
    // "갉" = ChoJungJong(ㄱ, ㅏ, ㄺ). Backspace → "갈", then "가".
    let mut fsm = HangulFsm::new();
    let _ = fsm.feed(JamoInput::cho_dual(0x1100, 0x11A8));
    let _ = fsm.feed(JamoInput::Jung(0x1161));
    let _ = fsm.feed(JamoInput::cho_dual(0x1105, 0x11AF));
    let _ = fsm.feed(JamoInput::cho_dual(0x1100, 0x11A8));
    assert_eq!(to_nfc_syllable(&fsm.preedit_string()), "갉");

    let ev = fsm.backspace();
    assert_eq!(to_nfc_syllable(ev.preedit_str().unwrap_or("")), "갈");
    let ev = fsm.backspace();
    assert_eq!(to_nfc_syllable(ev.preedit_str().unwrap_or("")), "가");
}

// ─────────────────── Additional commit-then-preedit cases ───────────────

#[test]
fn consecutive_consonants_commit_and_start_new() {
    // ㄱ + ㄴ (no vowel between) → commit "ㄱ", start "ㄴ".
    let mut fsm = HangulFsm::new();
    let _ = fsm.feed(JamoInput::cho_dual(0x1100, 0x11A8));
    let ev = fsm.feed(JamoInput::cho_dual(0x1102, 0x11AB));
    match ev {
        FsmEvent::CommitThenPreedit { commit, preedit } => {
            assert_eq!(commit, "\u{1100}");
            assert_eq!(preedit, "\u{1102}");
        }
        other => panic!("expected CommitThenPreedit, got {other:?}"),
    }
}

#[test]
fn flush_commits_final_preedit() {
    let mut fsm = HangulFsm::new();
    let _ = fsm.feed(JamoInput::cho_dual(0x1100, 0x11A8));
    let _ = fsm.feed(JamoInput::Jung(0x1161));
    let ev = fsm.flush();
    assert_eq!(ev, FsmEvent::Commit("\u{1100}\u{1161}".into()));
    assert!(!fsm.is_composing());
}

#[test]
fn state_is_inspectable() {
    let mut fsm = HangulFsm::new();
    assert!(fsm.state().is_empty());
    let _ = fsm.feed(JamoInput::cho_dual(0x1100, 0x11A8));
    let s = fsm.state();
    assert!(s.cho.is_some());
    assert!(s.jung.is_none());
    assert!(s.jong.is_none());
}
