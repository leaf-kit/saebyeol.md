//! Abbreviation (약어 속기) expansion engine.
//!
//! The engine observes the stream of committed text (raw conjoining
//! jamo or NFC syllables) and, when a registered [`Trigger`] matches
//! the tail of that stream, fires an [`AbbrEvent::Expand`] telling the
//! host to rollback the matched characters and insert the replacement
//! body.
//!
//! # Supported triggers (MVP)
//!
//! * [`Trigger::ChoSeq`] — a run of conjoining initial-consonant code
//!   points (U+1100..=U+1112). Designed to exploit the fact that a
//!   sequence of pure Cho jamo at the commit tail is a strong, rarely
//!   false-positive signal in Sebeolsik typing.
//! * [`Trigger::Literal`] — any literal string match, including NFC
//!   syllables or Latin text.
//!
//! # Triggering
//!
//! Matches only *fire* (produce an expansion) when the engine is
//! notified of a matching [`TriggerEvent`]. A match that doesn't fire
//! yet stays as "pending" so the UI can preview it.

pub mod dict;
pub mod engine;
#[cfg(feature = "toml-layout")]
pub mod loader;
pub mod model;

pub use dict::starter_dict;
pub use engine::{AbbrEvent, AbbreviationEngine, Suggestion};
#[cfg(feature = "toml-layout")]
pub use loader::{load_from_file as load_user_abbrs, parse_abbr_toml, LoadError, SAMPLE_FILE};
pub use model::{Abbreviation, Trigger, TriggerEvent};
