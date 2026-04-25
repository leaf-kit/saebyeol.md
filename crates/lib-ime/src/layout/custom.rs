//! User-defined layout loaded from a TOML file.
//!
//! The schema mirrors the §3.3 example from the design spec:
//!
//! ```toml
//! [meta]
//! id = "my-custom-3set"
//! name = "나만의 세벌식"
//! kind = "sebeolsik"     # dubeolsik | sebeolsik | latin | custom
//! version = "1.0.0"
//! author = "user"
//!
//! [options]
//! moachigi = true
//!
//! [keys]
//! KeyR = { base = "U+1100", shift = "U+1101" }  # ㄱ / ㄲ
//! KeyK = "U+1161"                               # ㅏ (no shift)
//! ```
//!
//! Each key's role (Cho / Jung / Jong) is inferred from the conjoining
//! code-point range. For `kind = "dubeolsik"`, Cho code points are
//! automatically paired with their Jong counterpart so a single base
//! value produces a dual-role consonant.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::key::KeyCode;
use super::{Layout, LayoutKind, LayoutOutput};
use crate::hangul::compose::cho_to_jong;
use crate::hangul::jamo::{Cho, JamoInput, Jong, Jung};
use crate::KeyEvent;

// ────────────────────────── On-disk schema ─────────────────────────────

#[derive(Debug, Deserialize)]
struct CustomLayoutFile {
    meta: CustomLayoutMeta,
    #[serde(default)]
    options: CustomLayoutOptions,
    keys: HashMap<String, RawBinding>,
}

#[derive(Debug, Deserialize)]
struct CustomLayoutMeta {
    id: String,
    name: String,
    kind: String,
    #[serde(default)]
    _version: Option<String>,
    #[serde(default)]
    _author: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct CustomLayoutOptions {
    #[serde(default)]
    moachigi: bool,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawBinding {
    /// Single code point bound to the base (unshifted) layer.
    Short(String),
    /// Full spec with optional shift layer.
    Full { base: String, #[serde(default)] shift: Option<String> },
}

// ────────────────────────────── Errors ─────────────────────────────────

/// Error returned by [`load_custom_layout`] and friends.
#[derive(Debug)]
pub enum LoadError {
    /// Underlying IO failure (file missing, permission denied, ...).
    Io(std::io::Error),
    /// TOML parse error.
    Toml(toml::de::Error),
    /// Semantic validation error (unknown kind, unknown key name,
    /// malformed code point, etc.).
    Validation(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Toml(e) => write!(f, "toml error: {e}"),
            Self::Validation(m) => write!(f, "validation error: {m}"),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Toml(e) => Some(e),
            Self::Validation(_) => None,
        }
    }
}

impl From<std::io::Error> for LoadError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<toml::de::Error> for LoadError {
    fn from(e: toml::de::Error) -> Self {
        Self::Toml(e)
    }
}

// ───────────────────── Resolved in-memory layout ───────────────────────

/// A user-defined layout resolved from a TOML file.
///
/// Implements [`Layout`] and can be passed to the FSM alongside the
/// built-in layouts.
#[derive(Debug, Clone)]
pub struct CustomLayout {
    id: String,
    name: String,
    kind: LayoutKind,
    moachigi: bool,
    bindings: HashMap<KeyCode, LayerBinding>,
}

#[derive(Debug, Clone, Copy)]
struct LayerBinding {
    base: JamoInput,
    shift: Option<JamoInput>,
}

impl CustomLayout {
    /// The layout's stable identifier (from `meta.id`).
    pub fn id_str(&self) -> &str {
        &self.id
    }
    /// The layout's human-readable name (from `meta.name`).
    pub fn name_str(&self) -> &str {
        &self.name
    }
}

impl Layout for CustomLayout {
    fn id(&self) -> &'static str {
        // Custom layouts don't have a 'static id. Surface a stable
        // placeholder; UIs should call `id_str()` for the real value.
        "custom"
    }
    fn name(&self) -> &'static str {
        "Custom"
    }
    fn kind(&self) -> LayoutKind {
        self.kind
    }

    fn map(&self, ev: &KeyEvent) -> LayoutOutput {
        if !ev.mods.is_ime_eligible() {
            return LayoutOutput::Passthrough;
        }
        match self.bindings.get(&ev.code) {
            Some(bind) => {
                let slot = if ev.mods.shift {
                    bind.shift.unwrap_or(bind.base)
                } else {
                    bind.base
                };
                LayoutOutput::Jamo(slot)
            }
            None => LayoutOutput::Passthrough,
        }
    }

    fn supports_moachigi(&self) -> bool {
        self.moachigi
    }
}

// ───────────────────────────── Public API ──────────────────────────────

/// Load a custom layout from a TOML file on disk.
pub fn load_custom_layout(path: impl AsRef<Path>) -> Result<CustomLayout, LoadError> {
    let text = fs::read_to_string(path)?;
    parse_custom_layout(&text)
}

/// Parse a custom layout from a TOML string (useful for tests and
/// embedding layouts in binaries).
pub fn parse_custom_layout(toml_text: &str) -> Result<CustomLayout, LoadError> {
    let raw: CustomLayoutFile = toml::from_str(toml_text)?;
    build_layout(raw)
}

fn build_layout(raw: CustomLayoutFile) -> Result<CustomLayout, LoadError> {
    let kind = parse_kind(&raw.meta.kind)?;
    let mut bindings = HashMap::with_capacity(raw.keys.len());

    for (key_name, binding) in raw.keys {
        let code = parse_keycode(&key_name)
            .ok_or_else(|| LoadError::Validation(format!("unknown physical key: {key_name}")))?;
        let (base_str, shift_str) = match binding {
            RawBinding::Short(s) => (s, None),
            RawBinding::Full { base, shift } => (base, shift),
        };
        let base = parse_binding(&base_str, kind)?;
        let shift = shift_str
            .map(|s| parse_binding(&s, kind))
            .transpose()?;
        bindings.insert(code, LayerBinding { base, shift });
    }

    Ok(CustomLayout {
        id: raw.meta.id,
        name: raw.meta.name,
        kind,
        moachigi: raw.options.moachigi,
        bindings,
    })
}

fn parse_kind(s: &str) -> Result<LayoutKind, LoadError> {
    match s {
        "dubeolsik" => Ok(LayoutKind::Dubeolsik),
        "sebeolsik" => Ok(LayoutKind::Sebeolsik),
        "latin" => Ok(LayoutKind::Latin),
        "steno" => Ok(LayoutKind::Steno),
        "custom" => Ok(LayoutKind::Custom),
        other => Err(LoadError::Validation(format!("unknown layout kind: {other}"))),
    }
}

fn parse_binding(value: &str, kind: LayoutKind) -> Result<JamoInput, LoadError> {
    let cp = parse_codepoint(value)?;

    if Jung::from_codepoint(cp).is_some() {
        return Ok(JamoInput::vowel(cp));
    }
    if let Some(cho) = Cho::from_codepoint(cp) {
        return Ok(cho_binding(cho.codepoint(), kind));
    }
    if Jong::from_codepoint(cp).is_some() {
        return Ok(JamoInput::jong_only(cp));
    }
    Err(LoadError::Validation(format!(
        "code point U+{cp:04X} is not a conjoining Hangul Jamo"
    )))
}

fn cho_binding(cho: u32, kind: LayoutKind) -> JamoInput {
    match kind {
        LayoutKind::Dubeolsik => match cho_to_jong(cho) {
            Some(jong) => JamoInput::cho_dual(cho, jong),
            None => JamoInput::cho_only(cho),
        },
        _ => JamoInput::cho_only(cho),
    }
}

fn parse_codepoint(s: &str) -> Result<u32, LoadError> {
    // Accept both "U+1100" / "u+1100" forms.
    if let Some(h) = s.strip_prefix("U+").or_else(|| s.strip_prefix("u+")) {
        return u32::from_str_radix(h, 16)
            .map_err(|_| LoadError::Validation(format!("invalid code point: {s}")));
    }
    // Allow a direct single character as shorthand: "ㄱ".
    let mut chars = s.chars();
    let first = chars
        .next()
        .ok_or_else(|| LoadError::Validation("empty code point".into()))?;
    if chars.next().is_some() {
        return Err(LoadError::Validation(format!(
            "expected single character or \"U+XXXX\", got {s:?}"
        )));
    }
    Ok(first as u32)
}

/// Parse a `KeyCode` from its canonical string name (as used in the
/// TOML `keys` table). Names follow the `KeyboardEvent.code` convention.
#[allow(clippy::too_many_lines)]
pub fn parse_keycode(name: &str) -> Option<KeyCode> {
    Some(match name {
        "KeyA" => KeyCode::KeyA, "KeyB" => KeyCode::KeyB, "KeyC" => KeyCode::KeyC,
        "KeyD" => KeyCode::KeyD, "KeyE" => KeyCode::KeyE, "KeyF" => KeyCode::KeyF,
        "KeyG" => KeyCode::KeyG, "KeyH" => KeyCode::KeyH, "KeyI" => KeyCode::KeyI,
        "KeyJ" => KeyCode::KeyJ, "KeyK" => KeyCode::KeyK, "KeyL" => KeyCode::KeyL,
        "KeyM" => KeyCode::KeyM, "KeyN" => KeyCode::KeyN, "KeyO" => KeyCode::KeyO,
        "KeyP" => KeyCode::KeyP, "KeyQ" => KeyCode::KeyQ, "KeyR" => KeyCode::KeyR,
        "KeyS" => KeyCode::KeyS, "KeyT" => KeyCode::KeyT, "KeyU" => KeyCode::KeyU,
        "KeyV" => KeyCode::KeyV, "KeyW" => KeyCode::KeyW, "KeyX" => KeyCode::KeyX,
        "KeyY" => KeyCode::KeyY, "KeyZ" => KeyCode::KeyZ,
        "Digit0" => KeyCode::Digit0, "Digit1" => KeyCode::Digit1,
        "Digit2" => KeyCode::Digit2, "Digit3" => KeyCode::Digit3,
        "Digit4" => KeyCode::Digit4, "Digit5" => KeyCode::Digit5,
        "Digit6" => KeyCode::Digit6, "Digit7" => KeyCode::Digit7,
        "Digit8" => KeyCode::Digit8, "Digit9" => KeyCode::Digit9,
        "Space" => KeyCode::Space, "Enter" => KeyCode::Enter, "Tab" => KeyCode::Tab,
        "Backspace" => KeyCode::Backspace, "Escape" => KeyCode::Escape,
        "Minus" => KeyCode::Minus, "Equal" => KeyCode::Equal,
        "BracketLeft" => KeyCode::BracketLeft, "BracketRight" => KeyCode::BracketRight,
        "Backslash" => KeyCode::Backslash, "Semicolon" => KeyCode::Semicolon,
        "Quote" => KeyCode::Quote, "Comma" => KeyCode::Comma,
        "Period" => KeyCode::Period, "Slash" => KeyCode::Slash,
        "Backquote" => KeyCode::Backquote,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Modifiers;

    const SAMPLE: &str = r#"
        [meta]
        id = "mini-dubeolsik"
        name = "Mini Dubeolsik"
        kind = "dubeolsik"

        [options]
        moachigi = false

        [keys]
        KeyR = { base = "U+1100", shift = "U+1101" }
        KeyK = "U+1161"
    "#;

    #[test]
    fn parses_minimal_dubeolsik_layout() {
        let layout = parse_custom_layout(SAMPLE).expect("parse ok");
        assert_eq!(layout.id_str(), "mini-dubeolsik");
        assert_eq!(layout.name_str(), "Mini Dubeolsik");
        assert_eq!(layout.kind(), LayoutKind::Dubeolsik);
        assert!(!layout.supports_moachigi());

        // Cho key in Dubeolsik kind should have both cho and jong.
        let r = layout.map(&KeyEvent::plain(KeyCode::KeyR));
        assert_eq!(r, LayoutOutput::Jamo(JamoInput::cho_dual(0x1100, 0x11A8)));

        // Shift layer uses the explicit override.
        let shift_r = layout.map(&KeyEvent::shift(KeyCode::KeyR));
        assert_eq!(shift_r, LayoutOutput::Jamo(JamoInput::cho_dual(0x1101, 0x11A9)));

        // Vowel key.
        let k = layout.map(&KeyEvent::plain(KeyCode::KeyK));
        assert_eq!(k, LayoutOutput::Jamo(JamoInput::vowel(0x1161)));

        // Unbound key passes through.
        let z = layout.map(&KeyEvent::plain(KeyCode::KeyZ));
        assert_eq!(z, LayoutOutput::Passthrough);
    }

    #[test]
    fn shift_falls_back_to_base_when_unset() {
        let layout = parse_custom_layout(SAMPLE).expect("parse ok");
        // KeyK has no explicit shift: Shift+k should still yield ㅏ.
        let shift_k = layout.map(&KeyEvent::shift(KeyCode::KeyK));
        assert_eq!(shift_k, LayoutOutput::Jamo(JamoInput::vowel(0x1161)));
    }

    #[test]
    fn sebeolsik_cho_stays_cho_only() {
        let src = r#"
            [meta]
            id = "x"
            name = "x"
            kind = "sebeolsik"
            [keys]
            KeyB = "U+1100"
        "#;
        let layout = parse_custom_layout(src).unwrap();
        let out = layout.map(&KeyEvent::plain(KeyCode::KeyB));
        assert_eq!(out, LayoutOutput::Jamo(JamoInput::cho_only(0x1100)));
    }

    #[test]
    fn direct_character_shorthand() {
        // "ㄱ" conjoining form = U+1100
        let src = r#"
            [meta]
            id = "x"
            name = "x"
            kind = "dubeolsik"
            [keys]
            KeyR = "\u1100"
        "#;
        let layout = parse_custom_layout(src).unwrap();
        let out = layout.map(&KeyEvent::plain(KeyCode::KeyR));
        assert_eq!(out, LayoutOutput::Jamo(JamoInput::cho_dual(0x1100, 0x11A8)));
    }

    #[test]
    fn rejects_unknown_kind() {
        let src = r#"
            [meta]
            id = "x"
            name = "x"
            kind = "blargh"
            [keys]
        "#;
        let err = parse_custom_layout(src).unwrap_err();
        matches!(err, LoadError::Validation(_));
    }

    #[test]
    fn rejects_unknown_keycode() {
        let src = r#"
            [meta]
            id = "x"
            name = "x"
            kind = "dubeolsik"
            [keys]
            NotARealKey = "U+1100"
        "#;
        let err = parse_custom_layout(src).unwrap_err();
        match err {
            LoadError::Validation(m) => assert!(m.contains("NotARealKey")),
            other => panic!("expected validation error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_out_of_range_codepoint() {
        let src = r#"
            [meta]
            id = "x"
            name = "x"
            kind = "dubeolsik"
            [keys]
            KeyA = "U+0041"
        "#;
        let err = parse_custom_layout(src).unwrap_err();
        match err {
            LoadError::Validation(m) => assert!(m.contains("U+0041")),
            other => panic!("expected validation error, got {other:?}"),
        }
    }

    #[test]
    fn ime_eligibility_is_respected() {
        let layout = parse_custom_layout(SAMPLE).unwrap();
        let ev = KeyEvent {
            code: KeyCode::KeyR,
            mods: Modifiers { ctrl: true, ..Modifiers::NONE },
            repeat: false,
        };
        assert_eq!(layout.map(&ev), LayoutOutput::Passthrough);
    }
}
