//! Core input engine for 새별 마크다운 에디터 (sbmd).
//!
//! Provides the Hangul composition finite-state machine, conjoining-Jamo
//! types, output-form conversion, and keyboard-layout abstractions. This
//! crate is pure Rust (no unsafe) and is intended to be shared by the
//! Tauri app, OS-level IME backends, and the CLI test harnesses.
//!
//! # Composition model
//!
//! Internally, composition state is always represented using Unicode
//! *Hangul Jamo* conjoining forms (U+1100..=U+11FF). The final
//! output form (conjoining, NFC syllable, or Compatibility Jamo) is
//! chosen at commit time via [`OutputForm`].
//!
//! # Quick start
//!
//! ```
//! use lib_ime::{Dubeolsik, HangulFsm, KeyCode, KeyEvent, Layout, LayoutOutput,
//!     Modifiers, OutputForm, to_nfc_syllable};
//!
//! let layout = Dubeolsik;
//! let mut fsm = HangulFsm::new();
//! let mut committed = String::new();
//!
//! for code in [KeyCode::KeyR, KeyCode::KeyK] {
//!     let ev = KeyEvent { code, mods: Modifiers::default(), repeat: false };
//!     if let LayoutOutput::Jamo(input) = layout.map(&ev) {
//!         let event = fsm.feed(input);
//!         committed.push_str(&OutputForm::NfcSyllable.render_event(&event));
//!     }
//! }
//! // Flush remaining preedit
//! committed.push_str(&OutputForm::NfcSyllable.render(&fsm.flush_string()));
//!
//! assert_eq!(to_nfc_syllable(&committed), "가");
//! ```

pub mod abbr;
pub mod hangul;
pub mod layout;

pub use abbr::{
    starter_dict, AbbrEvent, Abbreviation, AbbreviationEngine, Suggestion, Trigger, TriggerEvent,
};
#[cfg(feature = "toml-layout")]
pub use abbr::{load_user_abbrs, parse_abbr_toml, SAMPLE_FILE as ABBR_SAMPLE_FILE};
pub use hangul::compose;
pub use hangul::fsm::{ComposeMode, CompositionState, FeedOptions, FsmEvent, HangulFsm};
pub use hangul::jamo::{Cho, JamoInput, Jong, Jung};
pub use hangul::output::{to_compat_jamo, to_display_text, to_nfc_syllable, OutputForm};
#[cfg(feature = "toml-layout")]
pub use layout::custom::{load_custom_layout, parse_custom_layout, CustomLayout, LoadError};
pub use layout::dubeolsik::Dubeolsik;
pub use layout::key::{KeyCode, KeyEvent, Modifiers};
pub use layout::latin::{Dvorak, Qwerty};
pub use layout::sebeolsik::Sebeolsik390;
pub use layout::sebeolsik_final::SebeolsikFinal;
pub use layout::{Layout, LayoutKind, LayoutOutput};
