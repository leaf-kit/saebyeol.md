//! Physical-key abstraction.
//!
//! [`KeyCode`] is the physical position on the keyboard, independent of
//! the installed OS layout. This matches `KeyboardEvent.code` in the
//! browser and the Tauri keyboard event model and lets the same layout
//! file work regardless of the host's configured keyboard.

/// Physical key position, layout-independent.
///
/// Codes follow the UI Events `KeyboardEvent.code` naming for ASCII
/// letters, digits, and common punctuation. The set is intentionally
/// small — only keys that a layout is expected to map.
#[allow(missing_docs)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KeyCode {
    KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ,
    KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT,
    KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
    Digit0, Digit1, Digit2, Digit3, Digit4,
    Digit5, Digit6, Digit7, Digit8, Digit9,
    Space, Enter, Tab, Backspace, Escape,
    Minus, Equal, BracketLeft, BracketRight, Backslash,
    Semicolon, Quote, Comma, Period, Slash, Backquote,
    /// Caps Lock — used by the app as a 한/영 toggle when enabled.
    CapsLock,
}

/// Modifier keys held during a key press.
///
/// For IME purposes only `shift` is consulted by most layouts; the
/// other modifiers typically cause the layout to pass the key through
/// to the shortcut system (see [`Modifiers::is_ime_eligible`]).
#[allow(clippy::struct_excessive_bools)] // five boolean modifier flags mirror real keyboards
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    /// Shift is down.
    pub shift: bool,
    /// Control (or Command on macOS) is down.
    pub ctrl: bool,
    /// Alt (or Option on macOS) is down.
    pub alt: bool,
    /// `AltGr` is down (European layouts).
    pub altgr: bool,
    /// OS key (Windows/Command/Super) is down.
    pub meta: bool,
}

impl Modifiers {
    /// No modifiers held.
    pub const NONE: Self = Self {
        shift: false,
        ctrl: false,
        alt: false,
        altgr: false,
        meta: false,
    };

    /// Shift only.
    pub const SHIFT: Self = Self {
        shift: true,
        ..Self::NONE
    };

    /// Whether this combination should go through the IME layout
    /// (rather than being passed straight to the shortcut system).
    /// Any of Ctrl / Alt / Meta disables IME translation.
    #[inline]
    pub const fn is_ime_eligible(self) -> bool {
        !self.ctrl && !self.alt && !self.meta
    }
}

/// A single key event delivered to a [`crate::Layout`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeyEvent {
    /// Physical key.
    pub code: KeyCode,
    /// Modifier state.
    pub mods: Modifiers,
    /// Whether this is an auto-repeat event (held key).
    pub repeat: bool,
}

impl KeyEvent {
    /// Convenience constructor for a plain (no-modifier, non-repeat) event.
    pub fn plain(code: KeyCode) -> Self {
        Self {
            code,
            mods: Modifiers::NONE,
            repeat: false,
        }
    }

    /// Convenience constructor for a Shift-only event.
    pub fn shift(code: KeyCode) -> Self {
        Self {
            code,
            mods: Modifiers::SHIFT,
            repeat: false,
        }
    }
}
