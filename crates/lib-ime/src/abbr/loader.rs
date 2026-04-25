//! External TOML abbreviation loader.
//!
//! Users can drop an `abbreviations.toml` into the app's config
//! directory to add or override entries in the built-in starter
//! dictionary. The file is plain TOML; each abbreviation is a
//! `[[abbr]]` array entry.
//!
//! # Schema
//!
//! ```toml
//! [[abbr]]
//! trigger = "ㄱㅅ"           # compat jamo ok; converted to conjoining
//! kind    = "cho_seq"        # "cho_seq" | "literal" | "ending"
//! body    = "감사합니다."
//! priority   = 100           # optional, default 100
//! trigger_on = "space"       # optional
//!                            # "immediate" | "space" | "enter" |
//!                            # "punctuation" | "jong_completion" |
//!                            # "explicit"
//! id      = "my-greet"       # optional, auto-generated if omitted
//! ```

use std::path::Path;

use serde::Deserialize;

use super::model::{Abbreviation, Trigger, TriggerEvent};

/// Error returned by [`load_from_file`] / [`parse_abbr_toml`].
#[derive(Debug)]
pub enum LoadError {
    /// I/O failure reading the file.
    Io(std::io::Error),
    /// TOML parse error.
    Parse(toml::de::Error),
    /// Semantic validation error — unknown kind, bad code point, etc.
    Validation(String),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Parse(e) => write!(f, "toml error: {e}"),
            Self::Validation(m) => write!(f, "validation: {m}"),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<std::io::Error> for LoadError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<toml::de::Error> for LoadError {
    fn from(e: toml::de::Error) -> Self {
        Self::Parse(e)
    }
}

#[derive(Debug, Deserialize)]
struct FileDoc {
    #[serde(default)]
    abbr: Vec<AbbrEntry>,
}

#[derive(Debug, Deserialize)]
struct AbbrEntry {
    trigger: String,
    kind: String,
    body: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    priority: Option<u32>,
    #[serde(default)]
    trigger_on: Option<String>,
}

/// Load and parse a TOML abbreviation file. Returns the parsed
/// [`Abbreviation`]s.
pub fn load_from_file(path: impl AsRef<Path>) -> Result<Vec<Abbreviation>, LoadError> {
    let text = std::fs::read_to_string(path)?;
    parse_abbr_toml(&text)
}

/// Parse TOML from a string.
pub fn parse_abbr_toml(text: &str) -> Result<Vec<Abbreviation>, LoadError> {
    let doc: FileDoc = toml::from_str(text)?;
    let mut out = Vec::with_capacity(doc.abbr.len());
    for (i, e) in doc.abbr.into_iter().enumerate() {
        out.push(build_abbr(e, i)?);
    }
    Ok(out)
}

fn build_abbr(e: AbbrEntry, idx: usize) -> Result<Abbreviation, LoadError> {
    let trigger = match e.kind.as_str() {
        "cho_seq" => Trigger::ChoSeq(parse_cho_seq(&e.trigger)?),
        "literal" => {
            if e.trigger.is_empty() {
                return Err(LoadError::Validation("literal trigger empty".into()));
            }
            Trigger::Literal(e.trigger.clone())
        }
        "ending" => {
            if e.trigger.is_empty() {
                return Err(LoadError::Validation("ending trigger empty".into()));
            }
            Trigger::Ending(e.trigger.clone())
        }
        other => return Err(LoadError::Validation(format!("unknown kind: {other}"))),
    };
    let trigger_on = match e.trigger_on.as_deref() {
        Some("immediate") => TriggerEvent::Immediate,
        Some("space") => TriggerEvent::Space,
        Some("enter") => TriggerEvent::Enter,
        Some("punctuation") => TriggerEvent::Punctuation,
        Some("jong_completion") => TriggerEvent::JongCompletion,
        Some("explicit") | None => TriggerEvent::Explicit,
        Some(other) => return Err(LoadError::Validation(format!("unknown trigger_on: {other}"))),
    };
    let id = e
        .id
        .unwrap_or_else(|| format!("user:{idx}:{}", &e.trigger));
    Ok(Abbreviation {
        id,
        trigger,
        body: e.body,
        trigger_on,
        priority: e.priority.unwrap_or(100),
    })
}

/// Convert a user-facing trigger string (may contain compat jamo like
/// `ㄱㅅ` or conjoining `ᄀᄉ`) into an array of conjoining initial
/// consonant code points.
fn parse_cho_seq(s: &str) -> Result<Vec<u32>, LoadError> {
    if s.is_empty() {
        return Err(LoadError::Validation("cho_seq trigger empty".into()));
    }
    let mut out = Vec::with_capacity(s.chars().count());
    for ch in s.chars() {
        let cp = ch as u32;
        let conjoining = if (0x1100..=0x1112).contains(&cp) {
            cp
        } else if let Some(c) = compat_cho_to_conjoining(cp) {
            c
        } else {
            return Err(LoadError::Validation(format!(
                "'{ch}' (U+{cp:04X}) is not an initial consonant",
            )));
        };
        out.push(conjoining);
    }
    Ok(out)
}

/// Hangul Compatibility Jamo (U+3131 range) → conjoining Cho (U+1100 range).
fn compat_cho_to_conjoining(cp: u32) -> Option<u32> {
    Some(match cp {
        0x3131 => 0x1100, // ㄱ
        0x3132 => 0x1101, // ㄲ
        0x3134 => 0x1102, // ㄴ
        0x3137 => 0x1103, // ㄷ
        0x3138 => 0x1104, // ㄸ
        0x3139 => 0x1105, // ㄹ
        0x3141 => 0x1106, // ㅁ
        0x3142 => 0x1107, // ㅂ
        0x3143 => 0x1108, // ㅃ
        0x3145 => 0x1109, // ㅅ
        0x3146 => 0x110A, // ㅆ
        0x3147 => 0x110B, // ㅇ
        0x3148 => 0x110C, // ㅈ
        0x3149 => 0x110D, // ㅉ
        0x314A => 0x110E, // ㅊ
        0x314B => 0x110F, // ㅋ
        0x314C => 0x1110, // ㅌ
        0x314D => 0x1111, // ㅍ
        0x314E => 0x1112, // ㅎ
        _ => return None,
    })
}

/// Default sample file written to the user's config dir the first time
/// they run the app, so they have a ready-to-edit template.
pub const SAMPLE_FILE: &str = include_str!("sample_abbreviations.toml");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cho_seq_from_compat() {
        let items = parse_abbr_toml(
            r#"
            [[abbr]]
            trigger = "ㄱㅅ"
            kind = "cho_seq"
            body = "감사합니다."
            trigger_on = "space"
            priority = 90
            "#,
        )
        .expect("parse ok");
        assert_eq!(items.len(), 1);
        match &items[0].trigger {
            Trigger::ChoSeq(cs) => {
                assert_eq!(cs, &vec![0x1100, 0x1109]);
            }
            t => panic!("expected ChoSeq, got {t:?}"),
        }
        assert_eq!(items[0].body, "감사합니다.");
        assert_eq!(items[0].trigger_on, TriggerEvent::Space);
        assert_eq!(items[0].priority, 90);
    }

    #[test]
    fn parses_literal_and_ending() {
        let items = parse_abbr_toml(
            r#"
            [[abbr]]
            trigger = "메일끝"
            kind = "literal"
            body = "감사합니다.\n홍길동 드림."

            [[abbr]]
            trigger = "습니다"
            kind = "ending"
            body = "습니다."
            "#,
        )
        .unwrap();
        assert!(matches!(items[0].trigger, Trigger::Literal(ref s) if s == "메일끝"));
        assert!(matches!(items[1].trigger, Trigger::Ending(ref s) if s == "습니다"));
        assert_eq!(items[1].trigger_on, TriggerEvent::Explicit);
    }

    #[test]
    fn rejects_unknown_kind() {
        let err = parse_abbr_toml(
            r#"
            [[abbr]]
            trigger = "x"
            kind = "weird"
            body = "y"
            "#,
        )
        .unwrap_err();
        match err {
            LoadError::Validation(m) => assert!(m.contains("weird")),
            other => panic!("expected validation error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_bad_cho_codepoint() {
        let err = parse_abbr_toml(
            r#"
            [[abbr]]
            trigger = "abc"
            kind = "cho_seq"
            body = "nope"
            "#,
        )
        .unwrap_err();
        assert!(matches!(err, LoadError::Validation(_)));
    }

    #[test]
    fn sample_file_parses() {
        // The bundled sample must itself be valid TOML (so users starting
        // from it won't immediately hit errors).
        let _ = parse_abbr_toml(SAMPLE_FILE).expect("sample file is valid");
    }
}
