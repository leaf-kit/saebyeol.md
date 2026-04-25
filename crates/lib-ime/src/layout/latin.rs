//! Latin (English) keyboard layouts.
//!
//! These layouts translate a physical [`KeyCode`] to a single Unicode
//! character via [`LayoutOutput::Char`], bypassing the Hangul FSM
//! entirely. They are used when the user toggles the input mode to
//! English.
//!
//! Both layouts derive their output from the **physical key position**
//! (`KeyboardEvent.code`), so they produce a consistent result
//! regardless of the host OS's configured keyboard.

use super::key::{KeyCode, KeyEvent};
use super::{Layout, LayoutKind, LayoutOutput};

/// US QWERTY.
#[derive(Copy, Clone, Debug, Default)]
pub struct Qwerty;

impl Layout for Qwerty {
    fn id(&self) -> &'static str {
        "qwerty-us"
    }
    fn name(&self) -> &'static str {
        "QWERTY (US)"
    }
    fn kind(&self) -> LayoutKind {
        LayoutKind::Latin
    }
    fn map(&self, ev: &KeyEvent) -> LayoutOutput {
        if !ev.mods.is_ime_eligible() {
            return LayoutOutput::Passthrough;
        }
        match qwerty_char(ev.code, ev.mods.shift) {
            Some(ch) => LayoutOutput::Char(ch),
            None => LayoutOutput::Passthrough,
        }
    }
}

/// Classic Dvorak Simplified Keyboard.
#[derive(Copy, Clone, Debug, Default)]
pub struct Dvorak;

impl Layout for Dvorak {
    fn id(&self) -> &'static str {
        "dvorak"
    }
    fn name(&self) -> &'static str {
        "Dvorak"
    }
    fn kind(&self) -> LayoutKind {
        LayoutKind::Latin
    }
    fn map(&self, ev: &KeyEvent) -> LayoutOutput {
        if !ev.mods.is_ime_eligible() {
            return LayoutOutput::Passthrough;
        }
        match dvorak_char(ev.code, ev.mods.shift) {
            Some(ch) => LayoutOutput::Char(ch),
            None => LayoutOutput::Passthrough,
        }
    }
}

// ─────────────────────── Key → char tables ─────────────────────────

#[allow(clippy::too_many_lines)]
fn qwerty_char(code: KeyCode, shift: bool) -> Option<char> {
    let (lo, hi) = match code {
        // Number row
        KeyCode::Backquote => ('`', '~'),
        KeyCode::Digit1 => ('1', '!'),
        KeyCode::Digit2 => ('2', '@'),
        KeyCode::Digit3 => ('3', '#'),
        KeyCode::Digit4 => ('4', '$'),
        KeyCode::Digit5 => ('5', '%'),
        KeyCode::Digit6 => ('6', '^'),
        KeyCode::Digit7 => ('7', '&'),
        KeyCode::Digit8 => ('8', '*'),
        KeyCode::Digit9 => ('9', '('),
        KeyCode::Digit0 => ('0', ')'),
        KeyCode::Minus => ('-', '_'),
        KeyCode::Equal => ('=', '+'),
        // Letters
        KeyCode::KeyA => ('a', 'A'),
        KeyCode::KeyB => ('b', 'B'),
        KeyCode::KeyC => ('c', 'C'),
        KeyCode::KeyD => ('d', 'D'),
        KeyCode::KeyE => ('e', 'E'),
        KeyCode::KeyF => ('f', 'F'),
        KeyCode::KeyG => ('g', 'G'),
        KeyCode::KeyH => ('h', 'H'),
        KeyCode::KeyI => ('i', 'I'),
        KeyCode::KeyJ => ('j', 'J'),
        KeyCode::KeyK => ('k', 'K'),
        KeyCode::KeyL => ('l', 'L'),
        KeyCode::KeyM => ('m', 'M'),
        KeyCode::KeyN => ('n', 'N'),
        KeyCode::KeyO => ('o', 'O'),
        KeyCode::KeyP => ('p', 'P'),
        KeyCode::KeyQ => ('q', 'Q'),
        KeyCode::KeyR => ('r', 'R'),
        KeyCode::KeyS => ('s', 'S'),
        KeyCode::KeyT => ('t', 'T'),
        KeyCode::KeyU => ('u', 'U'),
        KeyCode::KeyV => ('v', 'V'),
        KeyCode::KeyW => ('w', 'W'),
        KeyCode::KeyX => ('x', 'X'),
        KeyCode::KeyY => ('y', 'Y'),
        KeyCode::KeyZ => ('z', 'Z'),
        // Punctuation
        KeyCode::BracketLeft => ('[', '{'),
        KeyCode::BracketRight => (']', '}'),
        KeyCode::Backslash => ('\\', '|'),
        KeyCode::Semicolon => (';', ':'),
        KeyCode::Quote => ('\'', '"'),
        KeyCode::Comma => (',', '<'),
        KeyCode::Period => ('.', '>'),
        KeyCode::Slash => ('/', '?'),
        _ => return None,
    };
    Some(if shift { hi } else { lo })
}

#[allow(clippy::too_many_lines)]
fn dvorak_char(code: KeyCode, shift: bool) -> Option<char> {
    // Number row stays identical to QWERTY.
    if let KeyCode::Backquote
    | KeyCode::Digit0
    | KeyCode::Digit1
    | KeyCode::Digit2
    | KeyCode::Digit3
    | KeyCode::Digit4
    | KeyCode::Digit5
    | KeyCode::Digit6
    | KeyCode::Digit7
    | KeyCode::Digit8
    | KeyCode::Digit9 = code
    {
        return qwerty_char(code, shift);
    }

    let (lo, hi) = match code {
        // `- [` and `= ]` (in Dvorak, brackets move from top-right to
        // right of P)
        KeyCode::Minus => ('[', '{'),
        KeyCode::Equal => (']', '}'),

        // Top row Q-P → ',.pyfgcrl/='   with ' and , in first two.
        KeyCode::KeyQ => ('\'', '"'),
        KeyCode::KeyW => (',', '<'),
        KeyCode::KeyE => ('.', '>'),
        KeyCode::KeyR => ('p', 'P'),
        KeyCode::KeyT => ('y', 'Y'),
        KeyCode::KeyY => ('f', 'F'),
        KeyCode::KeyU => ('g', 'G'),
        KeyCode::KeyI => ('c', 'C'),
        KeyCode::KeyO => ('r', 'R'),
        KeyCode::KeyP => ('l', 'L'),
        KeyCode::BracketLeft => ('/', '?'),
        KeyCode::BracketRight => ('=', '+'),

        // Home row A-' → 'aoeuidhtns-'
        KeyCode::KeyA => ('a', 'A'),
        KeyCode::KeyS => ('o', 'O'),
        KeyCode::KeyD => ('e', 'E'),
        KeyCode::KeyF => ('u', 'U'),
        KeyCode::KeyG => ('i', 'I'),
        KeyCode::KeyH => ('d', 'D'),
        KeyCode::KeyJ => ('h', 'H'),
        KeyCode::KeyK => ('t', 'T'),
        KeyCode::KeyL => ('n', 'N'),
        KeyCode::Semicolon => ('s', 'S'),
        KeyCode::Quote => ('-', '_'),

        // Bottom row Z-/ → ';qjkxbmwvz'
        KeyCode::KeyZ => (';', ':'),
        KeyCode::KeyX => ('q', 'Q'),
        KeyCode::KeyC => ('j', 'J'),
        KeyCode::KeyV => ('k', 'K'),
        KeyCode::KeyB => ('x', 'X'),
        KeyCode::KeyN => ('b', 'B'),
        KeyCode::KeyM => ('m', 'M'),
        KeyCode::Comma => ('w', 'W'),
        KeyCode::Period => ('v', 'V'),
        KeyCode::Slash => ('z', 'Z'),

        KeyCode::Backslash => ('\\', '|'),

        _ => return None,
    };
    Some(if shift { hi } else { lo })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(code: KeyCode, shift: bool) -> KeyEvent {
        if shift {
            KeyEvent::shift(code)
        } else {
            KeyEvent::plain(code)
        }
    }

    #[test]
    fn qwerty_letters() {
        assert_eq!(Qwerty.map(&k(KeyCode::KeyA, false)), LayoutOutput::Char('a'));
        assert_eq!(Qwerty.map(&k(KeyCode::KeyA, true)), LayoutOutput::Char('A'));
    }

    #[test]
    fn qwerty_digits_and_shifts() {
        assert_eq!(Qwerty.map(&k(KeyCode::Digit1, false)), LayoutOutput::Char('1'));
        assert_eq!(Qwerty.map(&k(KeyCode::Digit1, true)), LayoutOutput::Char('!'));
        assert_eq!(Qwerty.map(&k(KeyCode::Digit2, true)), LayoutOutput::Char('@'));
    }

    #[test]
    fn dvorak_letters_differ_from_qwerty() {
        // Physical "S" in Dvorak → 'o'
        assert_eq!(Dvorak.map(&k(KeyCode::KeyS, false)), LayoutOutput::Char('o'));
        assert_eq!(Dvorak.map(&k(KeyCode::KeyS, true)), LayoutOutput::Char('O'));
        // Physical "Q" in Dvorak → '''
        assert_eq!(Dvorak.map(&k(KeyCode::KeyQ, false)), LayoutOutput::Char('\''));
    }

    #[test]
    fn dvorak_number_row_matches_qwerty() {
        assert_eq!(Dvorak.map(&k(KeyCode::Digit1, false)), LayoutOutput::Char('1'));
        assert_eq!(Dvorak.map(&k(KeyCode::Digit1, true)), LayoutOutput::Char('!'));
    }

    #[test]
    fn dvorak_right_of_p_is_slash() {
        // Physical "[" in Dvorak is '/'
        assert_eq!(Dvorak.map(&k(KeyCode::BracketLeft, false)), LayoutOutput::Char('/'));
    }
}
