//! Persisted user preferences.
//!
//! Stored as TOML at `<app-config-dir>/settings.toml`. On macOS that's
//! `~/Library/Application Support/dev.leaf.sbmd/settings.toml`.
//!
//! A corrupt or missing file silently falls back to defaults so a bad
//! config never prevents the app from starting.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// File name under the app config dir.
pub const FILE_NAME: &str = "settings.toml";

/// User settings persisted across launches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// `Layout::id()` of the active **Hangul** layout.
    pub hangul_layout_id: String,
    /// `Layout::id()` of the active **Latin** layout (QWERTY / Dvorak).
    pub latin_layout_id: String,
    /// Active input mode (`hangul` / `english`).
    pub input_mode: String,
    /// Serialized [`lib_ime::OutputForm`] (`nfc` / `conjoining` / `compat`).
    pub output_form: String,
    /// Serialized [`lib_ime::ComposeMode`] (`sequential` / `moachigi`).
    pub compose_mode: String,
    /// Whether the autocomplete suggestion popup is shown. Auto-expansion
    /// (`ChoSeq` greetings on Space) is unaffected by this flag.
    #[serde(default = "default_suggestions_enabled")]
    pub suggestions_enabled: bool,
    /// How Backspace deletes already-committed text.
    /// `"syllable"` — whole syllable per press (default).
    /// `"jamo"`     — decompose the trailing syllable and drop one jamo
    /// at a time (compound finals shed one component first).
    #[serde(default = "default_backspace_mode")]
    pub backspace_mode: String,
    /// Whether user-supplied abbreviation dicts (the hand-authored
    /// `abbreviations.toml` and the directory-learned
    /// `learned_ngrams.toml`) are merged with the built-in starter
    /// dictionary. `true` = merge (default); `false` = built-in only.
    /// The files on disk are untouched either way — toggling this just
    /// changes what the running engine sees.
    #[serde(default = "default_use_user_abbrs")]
    pub use_user_abbrs: bool,

    /// Legacy field name from earlier builds. If present it seeds
    /// `hangul_layout_id` on read so user settings survive the rename.
    /// Never written back. Kept `pub` only so consumers can build the
    /// struct literally with `..Settings::default()`.
    #[serde(default, skip_serializing)]
    #[doc(hidden)]
    pub layout_id: Option<String>,
}

fn default_suggestions_enabled() -> bool {
    true
}

fn default_backspace_mode() -> String {
    "syllable".into()
}

fn default_use_user_abbrs() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hangul_layout_id: "sebeolsik-final".into(),
            latin_layout_id: "qwerty-us".into(),
            input_mode: "hangul".into(),
            output_form: "nfc".into(),
            compose_mode: "moachigi".into(),
            suggestions_enabled: true,
            backspace_mode: "syllable".into(),
            use_user_abbrs: true,
            layout_id: None,
        }
    }
}

impl Settings {
    /// Load settings from `dir/settings.toml`. Any failure (missing
    /// file, parse error) is swallowed and defaults are returned.
    pub fn load(dir: &Path) -> Self {
        let path = dir.join(FILE_NAME);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        let mut s: Self = toml::from_str(&text).unwrap_or_default();
        // Migrate the pre-multi-layout field name.
        if let Some(legacy) = s.layout_id.take() {
            if s.hangul_layout_id.is_empty() || s.hangul_layout_id == "sebeolsik-final" {
                s.hangul_layout_id = legacy;
            }
        }
        s
    }

    /// Persist settings to `dir/settings.toml`, creating the directory
    /// if needed. Errors are logged and returned but not fatal —
    /// callers typically `.ok()` the result.
    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        if !dir.exists() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
        })?;
        let path = dir.join(FILE_NAME);
        std::fs::write(path, text)
    }
}

/// State-managed wrapper for the settings directory. Kept as a
/// separate type so Tauri commands can `State<'_, SettingsPath>`.
#[derive(Debug, Clone)]
pub struct SettingsPath(pub PathBuf);

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    #[test]
    fn default_settings_point_to_sebeolsik_final() {
        let s = Settings::default();
        assert_eq!(s.hangul_layout_id, "sebeolsik-final");
        assert_eq!(s.latin_layout_id, "qwerty-us");
        assert_eq!(s.input_mode, "hangul");
        assert_eq!(s.output_form, "nfc");
        assert_eq!(s.compose_mode, "moachigi");
    }

    #[test]
    fn roundtrips_through_toml() {
        let dir = temp_dir().join("sbmd-settings-test");
        let _ = std::fs::remove_dir_all(&dir);
        let original = Settings {
            hangul_layout_id: "sebeolsik-390".into(),
            latin_layout_id: "dvorak".into(),
            input_mode: "english".into(),
            output_form: "compat".into(),
            compose_mode: "sequential".into(),
            suggestions_enabled: false,
            backspace_mode: "jamo".into(),
            layout_id: None,
            use_user_abbrs: true,
        };
        original.save(&dir).expect("save");
        let loaded = Settings::load(&dir);
        assert_eq!(loaded, original);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_layout_id_migrates_to_hangul_layout_id() {
        let dir = temp_dir().join("sbmd-legacy-migration-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(FILE_NAME),
            "layout_id = \"sebeolsik-390\"\nhangul_layout_id = \"sebeolsik-final\"\n",
        )
        .unwrap();
        let loaded = Settings::load(&dir);
        // Legacy value overrides the default-ish hangul_layout_id.
        assert_eq!(loaded.hangul_layout_id, "sebeolsik-390");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_returns_defaults() {
        let dir = temp_dir().join("sbmd-missing-dir-test");
        let _ = std::fs::remove_dir_all(&dir);
        let loaded = Settings::load(&dir);
        assert_eq!(loaded, Settings::default());
    }

    #[test]
    fn corrupt_file_returns_defaults() {
        let dir = temp_dir().join("sbmd-corrupt-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE_NAME), "this :: is [[[ not toml").unwrap();
        let loaded = Settings::load(&dir);
        assert_eq!(loaded, Settings::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
