//! Keyboard layout abstraction.
//!
//! A [`Layout`] translates a physical [`KeyEvent`] into a
//! [`LayoutOutput`]: a Jamo input for the Hangul FSM, a raw Unicode
//! character for Latin layouts, or a passthrough signal when the key
//! should be handled by the shortcut system or underlying editor.

#[cfg(feature = "toml-layout")]
pub mod custom;
pub mod dubeolsik;
pub mod key;
pub mod latin;
pub mod sebeolsik;
pub mod sebeolsik_final;

use crate::hangul::jamo::JamoInput;
use key::KeyEvent;

/// High-level layout family. Primarily informational; the engine
/// dispatches on [`Layout::map`]'s output, not on the kind.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LayoutKind {
    /// Two-set Hangul layout (두벌식). Consonant keys carry both Cho
    /// and Jong roles and the FSM disambiguates by context.
    Dubeolsik,
    /// Three-set Hangul layout (세벌식). Keys are single-role.
    Sebeolsik,
    /// A Latin layout (QWERTY, Dvorak, Colemak).
    Latin,
    /// A stenography (chord-based) layout.
    Steno,
    /// A user-defined layout loaded from a TOML file.
    Custom,
}

/// Result of mapping one [`KeyEvent`] through a [`Layout`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutOutput {
    /// Feed this Jamo input into the Hangul FSM.
    Jamo(JamoInput),
    /// Emit this character directly (Latin input, punctuation, etc.).
    Char(char),
    /// The layout doesn't handle this key — caller should process it
    /// normally (shortcut system, raw editor input).
    Passthrough,
    /// The layout silently absorbed the key (e.g. unbound modifier combo).
    None,
}

/// A keyboard layout.
///
/// Implementations should be stateless; all runtime composition state
/// lives in [`crate::HangulFsm`] and the shortcut/steno subsystems.
pub trait Layout: Send + Sync {
    /// Stable identifier used in configuration files.
    fn id(&self) -> &'static str;
    /// Human-readable name for UI display.
    fn name(&self) -> &'static str;
    /// Layout family for grouping in the UI.
    fn kind(&self) -> LayoutKind;
    /// Map one physical key event to its output.
    fn map(&self, ev: &KeyEvent) -> LayoutOutput;
    /// Whether this layout supports "모아치기" (simultaneous / order-
    /// independent key presses). Defaults to `false`.
    fn supports_moachigi(&self) -> bool {
        false
    }
}
