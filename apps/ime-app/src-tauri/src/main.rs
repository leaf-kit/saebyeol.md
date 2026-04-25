// Don't launch a console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// Lints relaxed for the Tauri shell:
//   * `needless_pass_by_value` — Tauri `#[command]` handlers take extractors by value.
//   * `struct_excessive_bools` — `KeyInputPayload` mirrors a raw keyboard event.
//   * `too_many_lines` — the IPC dispatch is a long match by design.
#![allow(
    clippy::needless_pass_by_value,
    clippy::struct_excessive_bools,
    clippy::too_many_lines
)]

//! Tauri desktop shell for 새별 마크다운 에디터 (sbmd).
//!
//! The heavy lifting lives in the `lib-ime` core; this binary only
//! hosts a per-window [`ImeSession`] and exposes it to the frontend
//! over Tauri IPC commands.

mod ngram;
mod settings;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 프론트엔드가 저장 여부 확인을 끝낸 뒤 `quit_app` 으로 다시 들어왔을
/// 때만 실제로 프로세스를 내린다. `CloseRequested` 가 이 플래그를 보고
/// 두 번째 진입을 통과시킨다.
static FORCE_QUIT: AtomicBool = AtomicBool::new(false);

use lib_ime::{
    load_user_abbrs, starter_dict, to_compat_jamo, to_display_text, to_nfc_syllable, AbbrEvent,
    Abbreviation, AbbreviationEngine, ComposeMode, Dubeolsik, Dvorak, FeedOptions, FsmEvent,
    HangulFsm, JamoInput, KeyCode, KeyEvent, Layout, LayoutKind, LayoutOutput, Modifiers,
    OutputForm, Qwerty, Sebeolsik390, SebeolsikFinal, Suggestion, Trigger, TriggerEvent,
    ABBR_SAMPLE_FILE,
};

/// 초기 버전의 안전성을 위해 자동완성은 "초성 시퀀스" 만 허용한다.
/// 음절 단위(Literal) 나 어미(Ending) 매칭은 예기치 못한 확장을 부르기
/// 쉬워, 사전에 등록돼 있어도 엔진에서 보이지 않도록 여기서 걸러낸다.
fn cho_only(mut v: Vec<Abbreviation>) -> Vec<Abbreviation> {
    v.retain(|a| matches!(a.trigger, Trigger::ChoSeq(_)));
    v
}
use serde::{Deserialize, Serialize};
use tauri::menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{Emitter, Manager, State};

use crate::settings::{Settings, SettingsPath};

/// Whether the user is currently typing Hangul through the IME or
/// English through a plain Latin layout.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
enum InputMode {
    #[default]
    Hangul,
    English,
}

impl InputMode {
    fn toggled(self) -> Self {
        match self {
            Self::Hangul => Self::English,
            Self::English => Self::Hangul,
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Self::Hangul => "hangul",
            Self::English => "english",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        match s {
            "hangul" => Some(Self::Hangul),
            "english" => Some(Self::English),
            _ => None,
        }
    }
}

/// Chord-window: a keystroke that lands within this interval of the
/// previous keystroke is treated as part of the same moachigi chord.
/// 50 ms covers typical near-simultaneous multi-finger taps without
/// over-gluing deliberate sequential typing (humans rarely hit the
/// next intended letter inside 50 ms).
const MOACHIGI_CHORD_WINDOW: Duration = Duration::from_millis(50);

/// One IME session: a Hangul layout, a Latin layout, the FSM driving
/// Hangul composition, and the abbreviation matcher observing the
/// commit stream.
struct ImeSession {
    hangul_layout: Box<dyn Layout>,
    latin_layout: Box<dyn Layout>,
    active_mode: InputMode,
    fsm: HangulFsm,
    output_form: OutputForm,
    abbr: AbbreviationEngine,
    /// Whether the autocomplete *picker* is shown.
    suggestions_enabled: bool,
    /// Backspace deletion granularity: `"syllable"` or `"jamo"`.
    backspace_mode: String,
    /// Whether user-supplied abbreviation dicts (abbreviations.toml +
    /// learned_ngrams.toml) are merged with the built-in dictionary.
    use_user_abbrs: bool,
    /// Timestamp of the last Hangul keystroke fed to the FSM. Used to
    /// decide whether the next keystroke is part of the same moachigi
    /// chord (see [`MOACHIGI_CHORD_WINDOW`]).
    last_jamo_at: Option<Instant>,
}

impl ImeSession {
    fn new(hangul_layout: Box<dyn Layout>, latin_layout: Box<dyn Layout>) -> Self {
        Self {
            hangul_layout,
            latin_layout,
            active_mode: InputMode::Hangul,
            fsm: HangulFsm::new(),
            output_form: OutputForm::NfcSyllable,
            abbr: AbbreviationEngine::new(cho_only(starter_dict())),
            suggestions_enabled: true,
            backspace_mode: "syllable".into(),
            use_user_abbrs: true,
            last_jamo_at: None,
        }
    }

    fn active_layout(&self) -> &dyn Layout {
        match self.active_mode {
            InputMode::Hangul => &*self.hangul_layout,
            InputMode::English => &*self.latin_layout,
        }
    }
}

/// Shared, mutex-guarded session state (one per app instance).
struct ImeState(Mutex<ImeSession>);

// ─────────────────────────── IPC payloads ──────────────────────────────

#[derive(Debug, Deserialize)]
struct KeyInputPayload {
    code: String,
    shift: bool,
    ctrl: bool,
    alt: bool,
    meta: bool,
}

#[derive(Debug, Serialize)]
struct ImeResponse {
    /// Composing text in the session's configured output form.
    preedit: String,
    /// Newly committed text (may be empty).
    commit: String,
    /// `true` if the key was not handled by the IME (pass through to the editor).
    passthrough: bool,
    /// `true` if the IME wants a literal character to follow the commit
    /// (for Space, Enter, etc.).
    emitted_char: Option<String>,
    /// Characters the frontend should remove from the end of its
    /// committed buffer **before** appending `commit` / `emitted_char`.
    /// Used when an abbreviation expansion replaces the typed trigger
    /// with its body.
    rollback_chars: usize,
    /// Identifier of the abbreviation that just fired, if any.
    /// Surfaced so the UI can flash "expanded: 감사합니다" feedback.
    abbr_fired: Option<String>,
    /// Current input mode after processing — lets the frontend update
    /// the 한/EN indicator without a separate IPC roundtrip when a
    /// Shift+Space toggle flips the mode.
    input_mode: &'static str,
    /// Abbreviation prefix-match candidates for the current commit
    /// tail. The frontend renders these as a picker below the cursor.
    /// Top-ranked (exact first, then longest) up to 8 entries.
    suggestions: Vec<SuggestionDto>,
}

/// Serializable mirror of `lib_ime::Suggestion` for the IPC layer.
#[derive(Debug, Serialize, Clone)]
struct SuggestionDto {
    abbr_id: String,
    trigger: String,
    body: String,
    is_exact: bool,
    is_prefix: bool,
    match_start: usize,
    rollback_chars: usize,
}

impl From<Suggestion> for SuggestionDto {
    fn from(s: Suggestion) -> Self {
        Self {
            abbr_id: s.abbr_id,
            trigger: s.trigger_display,
            body: s.body,
            is_exact: s.is_exact,
            is_prefix: s.is_prefix,
            match_start: s.match_start,
            rollback_chars: s.rollback_chars,
        }
    }
}

/// Maximum suggestions returned to the frontend.
const MAX_SUGGESTIONS: usize = 8;

impl Default for ImeResponse {
    fn default() -> Self {
        Self {
            preedit: String::new(),
            commit: String::new(),
            passthrough: false,
            emitted_char: None,
            rollback_chars: 0,
            abbr_fired: None,
            input_mode: "hangul",
            suggestions: Vec::new(),
        }
    }
}

fn collect_suggestions(sess: &ImeSession) -> Vec<SuggestionDto> {
    if !sess.suggestions_enabled {
        return Vec::new();
    }
    sess.abbr
        .candidates()
        .into_iter()
        .take(MAX_SUGGESTIONS)
        .map(SuggestionDto::from)
        .collect()
}

#[derive(Debug, Serialize)]
struct LayoutInfo {
    /// Active (currently in use) layout id — Hangul or Latin depending on mode.
    id: String,
    /// Active layout human name.
    name: String,
    /// Serialized output form.
    output_form: &'static str,
    /// Serialized compose mode.
    compose_mode: &'static str,
    /// Whether the Hangul layout supports moachigi.
    supports_moachigi: bool,
    /// Serialized input mode (`hangul` / `english`).
    input_mode: &'static str,
    /// Hangul layout id (independent of active mode).
    hangul_layout_id: String,
    /// Latin layout id (independent of active mode).
    latin_layout_id: String,
    /// Whether the autocomplete picker is on.
    suggestions_enabled: bool,
    /// `"syllable"` or `"jamo"`.
    backspace_mode: String,
}

#[derive(Debug, Serialize)]
struct KeyHint {
    /// Physical key label (e.g. "Q", "1", "Space").
    label: &'static str,
    /// DOM `KeyboardEvent.code` for this key.
    code: &'static str,
    /// Jamo produced on the base (unshifted) layer, as a Hangul
    /// compatibility character. Empty if the key is unmapped.
    base: String,
    /// Jamo produced on the shift layer. Empty if unbound.
    shift: String,
    /// Role of the base output: `cho` / `jung` / `jong` / `char` / empty.
    role: &'static str,
}

// ────────────────────────────── Commands ───────────────────────────────

#[tauri::command]
fn current_layout(state: State<'_, ImeState>) -> LayoutInfo {
    let sess = state.0.lock().unwrap();
    layout_info(&sess)
}

#[tauri::command]
fn set_layout(
    state: State<'_, ImeState>,
    settings_path: State<'_, SettingsPath>,
    id: String,
) -> Result<LayoutInfo, String> {
    let mut sess = state.0.lock().unwrap();
    sess.fsm.cancel();
    let layout = make_hangul_layout(&id).ok_or_else(|| format!("unknown hangul layout: {id}"))?;
    sess.hangul_layout = layout;
    // 세벌식(3-set) keys are role-fixed — order-independent slot filling
    // "just works". 두벌식 consonants need Sequential disambiguation.
    let default_mode = if sess.hangul_layout.supports_moachigi() {
        ComposeMode::Moachigi
    } else {
        ComposeMode::Sequential
    };
    sess.fsm.set_mode(default_mode);
    persist_current(&sess, &settings_path.0);
    Ok(layout_info(&sess))
}

#[tauri::command]
fn set_latin_layout(
    state: State<'_, ImeState>,
    settings_path: State<'_, SettingsPath>,
    id: String,
) -> Result<LayoutInfo, String> {
    let mut sess = state.0.lock().unwrap();
    let layout = make_latin_layout(&id).ok_or_else(|| format!("unknown latin layout: {id}"))?;
    sess.latin_layout = layout;
    persist_current(&sess, &settings_path.0);
    Ok(layout_info(&sess))
}

#[tauri::command]
fn set_input_mode(
    state: State<'_, ImeState>,
    settings_path: State<'_, SettingsPath>,
    mode: String,
) -> Result<LayoutInfo, String> {
    let mut sess = state.0.lock().unwrap();
    let new_mode = InputMode::parse(&mode).ok_or_else(|| format!("unknown input mode: {mode}"))?;
    // Flush any in-progress Hangul composition before switching.
    let _ = sess.fsm.flush();
    sess.active_mode = new_mode;
    persist_current(&sess, &settings_path.0);
    Ok(layout_info(&sess))
}

#[tauri::command]
fn set_output_form(
    state: State<'_, ImeState>,
    settings_path: State<'_, SettingsPath>,
    form: String,
) -> Result<LayoutInfo, String> {
    let mut sess = state.0.lock().unwrap();
    sess.output_form = parse_output_form(&form)
        .ok_or_else(|| format!("unknown output form: {form}"))?;
    persist_current(&sess, &settings_path.0);
    Ok(layout_info(&sess))
}

#[tauri::command]
fn set_suggestions_enabled(
    state: State<'_, ImeState>,
    settings_path: State<'_, SettingsPath>,
    enabled: bool,
) -> LayoutInfo {
    let mut sess = state.0.lock().unwrap();
    sess.suggestions_enabled = enabled;
    persist_current(&sess, &settings_path.0);
    layout_info(&sess)
}

#[tauri::command]
fn set_backspace_mode(
    state: State<'_, ImeState>,
    settings_path: State<'_, SettingsPath>,
    mode: String,
) -> Result<LayoutInfo, String> {
    let mut sess = state.0.lock().unwrap();
    sess.backspace_mode = match mode.as_str() {
        "syllable" | "jamo" => mode,
        other => return Err(format!("unknown backspace mode: {other}")),
    };
    persist_current(&sess, &settings_path.0);
    Ok(layout_info(&sess))
}

#[tauri::command]
fn set_compose_mode(
    state: State<'_, ImeState>,
    settings_path: State<'_, SettingsPath>,
    mode: String,
) -> Result<LayoutInfo, String> {
    let mut sess = state.0.lock().unwrap();
    let new_mode = parse_compose_mode(&mode)
        .ok_or_else(|| format!("unknown compose mode: {mode}"))?;
    sess.fsm.set_mode(new_mode);
    persist_current(&sess, &settings_path.0);
    Ok(layout_info(&sess))
}

fn make_hangul_layout(id: &str) -> Option<Box<dyn Layout>> {
    match id {
        "dubeolsik-std" => Some(Box::new(Dubeolsik)),
        "sebeolsik-390" => Some(Box::new(Sebeolsik390)),
        "sebeolsik-final" => Some(Box::new(SebeolsikFinal)),
        "os-ime" => Some(Box::new(OsImePassthrough)),
        _ => None,
    }
}

/// Stub layout that delegates every key to the host OS IME. Selecting
/// this as the Hangul layout tells the frontend to stop intercepting
/// keystrokes and let the system input method handle them natively.
#[derive(Copy, Clone, Debug, Default)]
struct OsImePassthrough;

impl Layout for OsImePassthrough {
    fn id(&self) -> &'static str {
        "os-ime"
    }
    fn name(&self) -> &'static str {
        "OS IME (시스템)"
    }
    fn kind(&self) -> LayoutKind {
        LayoutKind::Custom
    }
    fn map(&self, _ev: &KeyEvent) -> LayoutOutput {
        LayoutOutput::Passthrough
    }
    fn supports_moachigi(&self) -> bool {
        false
    }
}

fn make_latin_layout(id: &str) -> Option<Box<dyn Layout>> {
    match id {
        "qwerty-us" => Some(Box::new(Qwerty)),
        "dvorak" => Some(Box::new(Dvorak)),
        _ => None,
    }
}

fn parse_output_form(name: &str) -> Option<OutputForm> {
    Some(match name {
        "nfc" => OutputForm::NfcSyllable,
        "conjoining" => OutputForm::JamoConjoining,
        "compat" => OutputForm::JamoCompat,
        _ => return None,
    })
}

fn parse_compose_mode(name: &str) -> Option<ComposeMode> {
    Some(match name {
        "sequential" => ComposeMode::Sequential,
        "moachigi" => ComposeMode::Moachigi,
        _ => return None,
    })
}

fn persist_current(sess: &ImeSession, dir: &std::path::Path) {
    let settings = Settings {
        hangul_layout_id: sess.hangul_layout.id().to_string(),
        latin_layout_id: sess.latin_layout.id().to_string(),
        input_mode: sess.active_mode.as_str().to_string(),
        output_form: form_name(sess.output_form).to_string(),
        compose_mode: mode_name(sess.fsm.mode()).to_string(),
        suggestions_enabled: sess.suggestions_enabled,
        backspace_mode: sess.backspace_mode.clone(),
        use_user_abbrs: sess.use_user_abbrs,
        ..Settings::default()
    };
    if let Err(err) = settings.save(dir) {
        eprintln!("sbmd: failed to persist settings to {}: {err}", dir.display());
    }
}

fn layout_info(sess: &ImeSession) -> LayoutInfo {
    LayoutInfo {
        id: sess.active_layout().id().to_string(),
        name: sess.active_layout().name().to_string(),
        output_form: form_name(sess.output_form),
        compose_mode: mode_name(sess.fsm.mode()),
        supports_moachigi: sess.hangul_layout.supports_moachigi(),
        input_mode: sess.active_mode.as_str(),
        hangul_layout_id: sess.hangul_layout.id().to_string(),
        latin_layout_id: sess.latin_layout.id().to_string(),
        suggestions_enabled: sess.suggestions_enabled,
        backspace_mode: sess.backspace_mode.clone(),
    }
}

fn mode_name(m: ComposeMode) -> &'static str {
    match m {
        ComposeMode::Sequential => "sequential",
        ComposeMode::Moachigi => "moachigi",
    }
}

#[tauri::command]
fn ime_key_input(
    state: State<'_, ImeState>,
    settings_path: State<'_, SettingsPath>,
    ev: KeyInputPayload,
) -> ImeResponse {
    let mut sess = state.0.lock().unwrap();

    let Some(code) = parse_keycode(&ev.code) else {
        // Modifier-only keydowns (Shift/Ctrl/Alt/Meta pressed alone)
        // arrive here and must not flush the preedit — otherwise the
        // next Shift+letter key finds an empty state and the jong
        // composes into a fresh syllable instead of attaching.
        if is_modifier_code(&ev.code) {
            return ImeResponse {
                preedit: preedit_display(&sess),
                input_mode: sess.active_mode.as_str(),
                suggestions: collect_suggestions(&sess),
                ..ImeResponse::default()
            };
        }
        let rendered = flush_and_feed_abbr(&mut sess);
        return finalize(&sess, rendered, true, None, AbbrEvent::None);
    };

    // ─────────── Language toggle ───────────
    // Any of: Caps Lock, Shift+Space, Right-Alt flips 한 ↔ EN.
    // Caps Lock matches the macOS system Korean IME convention;
    // Shift+Space is the common shortcut on Windows/Linux Korean IMEs.
    let is_toggle = code == KeyCode::CapsLock
        || matches!((code, ev.shift), (KeyCode::Space, true));
    if is_toggle {
        let rendered = flush_and_feed_abbr(&mut sess);
        sess.active_mode = sess.active_mode.toggled();
        persist_current(&sess, &settings_path.0);
        return ImeResponse {
            commit: rendered,
            input_mode: sess.active_mode.as_str(),
            suggestions: collect_suggestions(&sess),
            ..ImeResponse::default()
        };
    }

    // Special non-letter keys handled by the IME.
    match code {
        KeyCode::Escape => {
            sess.fsm.cancel();
            sync_abbr_preedit(&mut sess);
            return response(&sess, String::new(), false, None);
        }
        KeyCode::Backspace => {
            if sess.fsm.is_composing() {
                let _ = sess.fsm.backspace();
                sync_abbr_preedit(&mut sess);
                return response(&sess, String::new(), false, None);
            }
            sess.abbr.on_backspace();
            sync_abbr_preedit(&mut sess);
            return ImeResponse {
                passthrough: true,
                input_mode: sess.active_mode.as_str(),
                suggestions: collect_suggestions(&sess),
                ..ImeResponse::default()
            };
        }
        KeyCode::Enter => {
            let rendered = flush_and_feed_abbr(&mut sess);
            let abbr = sess.abbr.on_trigger(TriggerEvent::Enter);
            return settle_with_trigger(&sess, rendered, "\n", abbr, "\n");
        }
        KeyCode::Space => {
            let rendered = flush_and_feed_abbr(&mut sess);
            let abbr = sess.abbr.on_trigger(TriggerEvent::Space);
            return settle_with_trigger(&sess, rendered, " ", abbr, " ");
        }
        _ => {}
    }

    let mods = Modifiers {
        shift: ev.shift,
        ctrl: ev.ctrl,
        alt: ev.alt,
        altgr: false,
        meta: ev.meta,
    };
    if !mods.is_ime_eligible() {
        let rendered = flush_and_feed_abbr(&mut sess);
        return finalize(&sess, rendered, true, None, AbbrEvent::None);
    }

    let kev = KeyEvent { code, mods, repeat: false };
    let form = sess.output_form;

    match sess.active_layout().map(&kev) {
        LayoutOutput::Jamo(input) => {
            let now = Instant::now();
            let within_chord = sess
                .last_jamo_at
                .is_some_and(|t| now.duration_since(t) <= MOACHIGI_CHORD_WINDOW);
            sess.last_jamo_at = Some(now);
            let fsm_ev = sess.fsm.feed_with(input, FeedOptions { within_chord });
            let raw_commit = fsm_ev.commit_str().unwrap_or("").to_string();
            let rendered = render(&raw_commit, form);
            // Feed NFC-normalized commit so Ending/Literal triggers
            // ("습니다" etc.) match syllable-for-syllable.
            let abbr = if raw_commit.is_empty() {
                AbbrEvent::None
            } else {
                sess.abbr.on_commit(&to_nfc_syllable(&raw_commit))
            };
            // Sync the tentative preedit so the picker can show
            // matches for still-composing syllables.
            sync_abbr_preedit(&mut sess);
            finalize(&sess, rendered, false, None, abbr)
        }
        LayoutOutput::Char(c) => {
            let flushed = flush_and_feed_abbr(&mut sess);
            let abbr = sess.abbr.on_commit(&c.to_string());
            finalize(&sess, flushed, false, Some(c.to_string()), abbr)
        }
        LayoutOutput::Passthrough => {
            let flushed = flush_and_feed_abbr(&mut sess);
            finalize(&sess, flushed, true, None, AbbrEvent::None)
        }
        LayoutOutput::None => ImeResponse {
            input_mode: sess.active_mode.as_str(),
            suggestions: collect_suggestions(&sess),
            ..ImeResponse::default()
        },
    }
}

/// Flushes the FSM and pipes the committed text to the abbreviation
/// engine in NFC form so substring matching against `"습니다"` etc.
/// works regardless of the user's selected output form.
fn flush_and_feed_abbr(sess: &mut ImeSession) -> String {
    let raw = if let FsmEvent::Commit(s) = sess.fsm.flush() {
        s
    } else {
        String::new()
    };
    if !raw.is_empty() {
        let nfc = to_nfc_syllable(&raw);
        let _ = sess.abbr.on_commit(&nfc);
    }
    // The FSM preedit is now empty; mirror that on the engine side.
    sess.abbr.set_preedit("");
    render(&raw, sess.output_form)
}

/// Pushes the FSM's current preedit (normalized to NFC) into the
/// abbreviation engine so the picker can surface suggestions that
/// match partially-typed syllables.
fn sync_abbr_preedit(sess: &mut ImeSession) {
    let preedit = to_nfc_syllable(&sess.fsm.preedit_string());
    sess.abbr.set_preedit(&preedit);
}

/// Combine a normal FSM outcome with a possible abbreviation event.
/// If the abbreviation fires, the rendered commit for *this* input is
/// absorbed into the rollback, so we swap `commit` for the expansion body.
#[allow(clippy::needless_pass_by_value)]
fn finalize(
    sess: &ImeSession,
    rendered_commit: String,
    passthrough: bool,
    emitted_char: Option<String>,
    abbr: AbbrEvent,
) -> ImeResponse {
    let preedit = preedit_display(sess);
    let input_mode = sess.active_mode.as_str();
    if let AbbrEvent::Expand { abbr_id, rollback_chars, insert } = abbr {
        let this_step = rendered_commit.chars().count();
        let fe_rollback = rollback_chars.saturating_sub(this_step);
        return ImeResponse {
            preedit,
            commit: insert,
            passthrough,
            emitted_char,
            rollback_chars: fe_rollback,
            abbr_fired: Some(abbr_id),
            input_mode,
            suggestions: collect_suggestions(sess),
        };
    }
    ImeResponse {
        preedit,
        commit: rendered_commit,
        passthrough,
        emitted_char,
        rollback_chars: 0,
        abbr_fired: None,
        input_mode,
        suggestions: collect_suggestions(sess),
    }
}

/// Trigger-keyed settler for Space / Enter — the user's key doubles as
/// an abbreviation trigger event. On expansion the expansion body takes
/// the place of the flushed commit, and we still emit the trailing
/// trigger character.
#[allow(clippy::needless_pass_by_value)]
fn settle_with_trigger(
    sess: &ImeSession,
    flushed_commit: String,
    default_emit: &str,
    abbr: AbbrEvent,
    post_expand_emit: &str,
) -> ImeResponse {
    let preedit = preedit_display(sess);
    let input_mode = sess.active_mode.as_str();
    if let AbbrEvent::Expand { abbr_id, rollback_chars, insert } = abbr {
        let this_step = flushed_commit.chars().count();
        let fe_rollback = rollback_chars.saturating_sub(this_step);
        return ImeResponse {
            preedit,
            commit: insert,
            passthrough: true,
            emitted_char: Some(post_expand_emit.to_string()),
            rollback_chars: fe_rollback,
            abbr_fired: Some(abbr_id),
            input_mode,
            suggestions: collect_suggestions(sess),
        };
    }
    ImeResponse {
        preedit,
        commit: flushed_commit,
        passthrough: true,
        emitted_char: Some(default_emit.to_string()),
        rollback_chars: 0,
        abbr_fired: None,
        input_mode,
        suggestions: collect_suggestions(sess),
    }
}

fn preedit_display(sess: &ImeSession) -> String {
    let preedit_raw = sess.fsm.preedit_string();
    if preedit_raw.is_empty() {
        String::new()
    } else {
        to_display_text(&preedit_raw)
    }
}

#[tauri::command]
fn layout_map(state: State<'_, ImeState>) -> Vec<KeyHint> {
    let sess = state.0.lock().unwrap();
    let layout = sess.active_layout();
    keyboard_keys()
        .iter()
        .map(|(label, code)| {
            let base_out = layout.map(&KeyEvent {
                code: *code,
                mods: Modifiers::NONE,
                repeat: false,
            });
            let shift_out = layout.map(&KeyEvent {
                code: *code,
                mods: Modifiers::SHIFT,
                repeat: false,
            });
            let (base, base_role) = describe_output(&base_out);
            let (shift, _) = describe_output(&shift_out);
            KeyHint {
                label,
                code: code_name(*code),
                base,
                shift,
                role: base_role,
            }
        })
        .collect()
}

#[derive(Debug, Serialize)]
struct AbbrInfo {
    id: String,
    trigger: String,
    body: String,
    trigger_on: &'static str,
    kind: &'static str,
}

#[tauri::command]
fn list_abbreviations(state: State<'_, ImeState>) -> Vec<AbbrInfo> {
    let sess = state.0.lock().unwrap();
    sess.abbr
        .abbreviations()
        .iter()
        .map(|a| AbbrInfo {
            id: a.id.clone(),
            trigger: a.trigger.display(),
            body: a.body.clone(),
            trigger_on: trigger_event_name(a.trigger_on),
            kind: match &a.trigger {
                Trigger::ChoSeq(_) => "cho_seq",
                Trigger::Literal(_) => "literal",
                Trigger::Ending(_) => "ending",
            },
        })
        .collect()
}

fn trigger_event_name(ev: TriggerEvent) -> &'static str {
    match ev {
        TriggerEvent::Immediate => "immediate",
        TriggerEvent::Space => "space",
        TriggerEvent::Enter => "enter",
        TriggerEvent::Punctuation => "punctuation",
        TriggerEvent::JongCompletion => "jong_completion",
        TriggerEvent::Explicit => "explicit",
    }
}

#[tauri::command]
fn apply_abbreviation(state: State<'_, ImeState>, id: String) -> ImeResponse {
    let mut sess = state.0.lock().unwrap();
    // The preedit is part of the engine's effective tail already. Cancel
    // the FSM composition so the frontend removes the visible preedit
    // span; do not commit or flush — that would double-count chars.
    sess.fsm.cancel();
    let fired = sess.abbr.fire_by_id(&id);
    sess.abbr.set_preedit(""); // FSM is now empty.
    match fired {
        AbbrEvent::Expand { abbr_id, rollback_chars, insert } => {
            // 자동완성 후 끝에 공백을 자동으로 붙이는 것은 *명확히
            // 종결되는 표현* 일 때만 한다.
            //   1. 접속사 — id 가 "conj-" 로 시작 (그러나/하지만/따라서…)
            //   2. 본문이 문장 종결 부호로 끝나는 경우 — '.', '?', '!'
            //      (예: "감사합니다.", "습니까?", "와!")
            // 그 외의 단어형 자동완성(사람, 학교, 미래…)은 caret 을
            // 단어 끝에 그대로 두어 사용자가 조사/어미를 이어갈 수 있게
            // 한다. 본문이 이미 공백·개행으로 끝나면 그대로 둔다.
            let insert_with_space = if insert.ends_with(&[' ', '\n', '\t'][..]) {
                insert
            } else if abbr_id.starts_with("conj-")
                || insert.ends_with(['.', '?', '!'])
            {
                format!("{insert} ")
            } else {
                insert
            };
            // Keep the engine's commit tail in sync with what we just
            // emitted so follow-on matches see a consistent buffer.
            let _ = sess.abbr.on_commit(&insert_with_space);
            ImeResponse {
                preedit: String::new(),
                commit: insert_with_space,
                passthrough: false,
                emitted_char: None,
                // `rollback_chars` from the engine is the number of
                // *committed* characters to roll back.
                rollback_chars,
                abbr_fired: Some(abbr_id),
                input_mode: sess.active_mode.as_str(),
                suggestions: collect_suggestions(&sess),
            }
        }
        _ => ImeResponse {
            preedit: String::new(),
            commit: String::new(),
            passthrough: false,
            emitted_char: None,
            rollback_chars: 0,
            abbr_fired: None,
            input_mode: sess.active_mode.as_str(),
            suggestions: collect_suggestions(&sess),
        },
    }
}

/// Cancels any in-progress Hangul composition without committing it.
/// Used when the frontend deletes a selection that may include the
/// preedit span.
#[tauri::command]
fn cancel_composition(state: State<'_, ImeState>) -> ImeResponse {
    let mut sess = state.0.lock().unwrap();
    sess.fsm.cancel();
    sync_abbr_preedit(&mut sess);
    response(&sess, String::new(), false, None)
}

/// 자동완성 팝오버에서 사용자가 Backspace 로 후보 매칭을 취소하면 호출된다.
/// preedit (FSM) 와 abbr 엔진의 commit_tail 양쪽에서 입력된 초성 시퀀스를
/// 모두 제거한다. 호출자(JS) 가 `rollback_chars` 에 *committed* 쪽에서
/// 지워야 할 글자 수(전체 cho 길이 − 현재 preedit 길이)를 넘긴다.
#[tauri::command]
fn cancel_abbr_match(state: State<'_, ImeState>, rollback_chars: usize) -> ImeResponse {
    let mut sess = state.0.lock().unwrap();
    sess.fsm.cancel();
    sess.abbr.set_preedit("");
    for _ in 0..rollback_chars {
        sess.abbr.on_backspace();
    }
    ImeResponse {
        preedit: String::new(),
        commit: String::new(),
        passthrough: false,
        emitted_char: None,
        rollback_chars,
        abbr_fired: None,
        input_mode: sess.active_mode.as_str(),
        suggestions: collect_suggestions(&sess),
    }
}

#[tauri::command]
fn flush(state: State<'_, ImeState>) -> ImeResponse {
    let mut sess = state.0.lock().unwrap();
    let form = sess.output_form;
    let commit = match sess.fsm.flush() {
        FsmEvent::Commit(s) => render(&s, form),
        _ => String::new(),
    };
    response(&sess, commit, false, None)
}

// 브라우저 `window.print()` 는 Tauri WKWebView (macOS) 에서 실제
// 네이티브 프린트 패널을 띄우지 못하는 경우가 있다. Tauri 2 가 제공하는
// WebviewWindow::print() 는 wry 쪽에서 플랫폼 네이티브 프린트 다이얼로그를
// 확실히 열어 준다. JS 쪽에서 이 커맨드를 먼저 시도하고, 실패하면
// window.print() 로 폴백한다.
#[tauri::command]
fn print_webview(window: tauri::WebviewWindow) -> Result<(), String> {
    window.print().map_err(|e| e.to_string())
}

// ───────────────────────────── Helpers ─────────────────────────────────

fn response(
    sess: &ImeSession,
    commit: String,
    passthrough: bool,
    emitted_char: Option<String>,
) -> ImeResponse {
    ImeResponse {
        preedit: preedit_display(sess),
        commit,
        passthrough,
        emitted_char,
        rollback_chars: 0,
        abbr_fired: None,
        input_mode: sess.active_mode.as_str(),
        suggestions: collect_suggestions(sess),
    }
}

fn render(s: &str, form: OutputForm) -> String {
    form.render(s)
}

/// Returns every physical key the layout panel displays, in keyboard order.
fn keyboard_keys() -> &'static [(&'static str, KeyCode)] {
    &[
        ("1", KeyCode::Digit1), ("2", KeyCode::Digit2), ("3", KeyCode::Digit3),
        ("4", KeyCode::Digit4), ("5", KeyCode::Digit5), ("6", KeyCode::Digit6),
        ("7", KeyCode::Digit7), ("8", KeyCode::Digit8), ("9", KeyCode::Digit9),
        ("0", KeyCode::Digit0), ("-", KeyCode::Minus), ("=", KeyCode::Equal),
        ("Q", KeyCode::KeyQ), ("W", KeyCode::KeyW), ("E", KeyCode::KeyE),
        ("R", KeyCode::KeyR), ("T", KeyCode::KeyT), ("Y", KeyCode::KeyY),
        ("U", KeyCode::KeyU), ("I", KeyCode::KeyI), ("O", KeyCode::KeyO),
        ("P", KeyCode::KeyP), ("[", KeyCode::BracketLeft), ("]", KeyCode::BracketRight),
        ("A", KeyCode::KeyA), ("S", KeyCode::KeyS), ("D", KeyCode::KeyD),
        ("F", KeyCode::KeyF), ("G", KeyCode::KeyG), ("H", KeyCode::KeyH),
        ("J", KeyCode::KeyJ), ("K", KeyCode::KeyK), ("L", KeyCode::KeyL),
        (";", KeyCode::Semicolon), ("'", KeyCode::Quote),
        ("Z", KeyCode::KeyZ), ("X", KeyCode::KeyX), ("C", KeyCode::KeyC),
        ("V", KeyCode::KeyV), ("B", KeyCode::KeyB), ("N", KeyCode::KeyN),
        ("M", KeyCode::KeyM), (",", KeyCode::Comma), (".", KeyCode::Period),
        ("/", KeyCode::Slash),
    ]
}

fn describe_output(out: &LayoutOutput) -> (String, &'static str) {
    match out {
        LayoutOutput::Jamo(input) => describe_jamo(*input),
        LayoutOutput::Char(c) => (c.to_string(), "char"),
        _ => (String::new(), ""),
    }
}

/// Convert a Jamo input to a compat-form display string + role tag.
/// For Dubeolsik-style dual consonants, prefer the Cho form for display.
fn describe_jamo(input: JamoInput) -> (String, &'static str) {
    match input {
        JamoInput::Jung(v) => (cp_to_compat(v), "jung"),
        JamoInput::Cons { cho: Some(c), .. } => (cp_to_compat(c), "cho"),
        JamoInput::Cons { cho: None, jong: Some(j) } => (cp_to_compat(j), "jong"),
        JamoInput::Cons { cho: None, jong: None } => (String::new(), ""),
    }
}

fn cp_to_compat(cp: u32) -> String {
    let Some(ch) = char::from_u32(cp) else {
        return String::new();
    };
    to_compat_jamo(&ch.to_string())
}

fn code_name(c: KeyCode) -> &'static str {
    match c {
        KeyCode::KeyA => "KeyA", KeyCode::KeyB => "KeyB", KeyCode::KeyC => "KeyC",
        KeyCode::KeyD => "KeyD", KeyCode::KeyE => "KeyE", KeyCode::KeyF => "KeyF",
        KeyCode::KeyG => "KeyG", KeyCode::KeyH => "KeyH", KeyCode::KeyI => "KeyI",
        KeyCode::KeyJ => "KeyJ", KeyCode::KeyK => "KeyK", KeyCode::KeyL => "KeyL",
        KeyCode::KeyM => "KeyM", KeyCode::KeyN => "KeyN", KeyCode::KeyO => "KeyO",
        KeyCode::KeyP => "KeyP", KeyCode::KeyQ => "KeyQ", KeyCode::KeyR => "KeyR",
        KeyCode::KeyS => "KeyS", KeyCode::KeyT => "KeyT", KeyCode::KeyU => "KeyU",
        KeyCode::KeyV => "KeyV", KeyCode::KeyW => "KeyW", KeyCode::KeyX => "KeyX",
        KeyCode::KeyY => "KeyY", KeyCode::KeyZ => "KeyZ",
        KeyCode::Digit0 => "Digit0", KeyCode::Digit1 => "Digit1",
        KeyCode::Digit2 => "Digit2", KeyCode::Digit3 => "Digit3",
        KeyCode::Digit4 => "Digit4", KeyCode::Digit5 => "Digit5",
        KeyCode::Digit6 => "Digit6", KeyCode::Digit7 => "Digit7",
        KeyCode::Digit8 => "Digit8", KeyCode::Digit9 => "Digit9",
        KeyCode::Space => "Space", KeyCode::Enter => "Enter", KeyCode::Tab => "Tab",
        KeyCode::Backspace => "Backspace", KeyCode::Escape => "Escape",
        KeyCode::Minus => "Minus", KeyCode::Equal => "Equal",
        KeyCode::BracketLeft => "BracketLeft", KeyCode::BracketRight => "BracketRight",
        KeyCode::Backslash => "Backslash", KeyCode::Semicolon => "Semicolon",
        KeyCode::Quote => "Quote", KeyCode::Comma => "Comma",
        KeyCode::Period => "Period", KeyCode::Slash => "Slash",
        KeyCode::Backquote => "Backquote",
        KeyCode::CapsLock => "CapsLock",
    }
}

fn form_name(f: OutputForm) -> &'static str {
    match f {
        OutputForm::NfcSyllable => "nfc",
        OutputForm::JamoConjoining => "conjoining",
        OutputForm::JamoCompat => "compat",
    }
}

/// Whether a DOM `KeyboardEvent.code` names a modifier key that fires
/// on its own (without a companion letter). Presence of such an event
/// must not flush the preedit.
fn is_modifier_code(code: &str) -> bool {
    matches!(
        code,
        "ShiftLeft" | "ShiftRight"
            | "ControlLeft" | "ControlRight"
            | "AltLeft" | "AltRight"
            | "MetaLeft" | "MetaRight"
            | "OSLeft" | "OSRight"
            | "ContextMenu"
    )
}

#[allow(clippy::too_many_lines)]
fn parse_keycode(name: &str) -> Option<KeyCode> {
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
        "CapsLock" => KeyCode::CapsLock,
        "Minus" => KeyCode::Minus, "Equal" => KeyCode::Equal,
        "BracketLeft" => KeyCode::BracketLeft, "BracketRight" => KeyCode::BracketRight,
        "Backslash" => KeyCode::Backslash, "Semicolon" => KeyCode::Semicolon,
        "Quote" => KeyCode::Quote, "Comma" => KeyCode::Comma,
        "Period" => KeyCode::Period, "Slash" => KeyCode::Slash,
        "Backquote" => KeyCode::Backquote,
        _ => return None,
    })
}

// ────────────────────────────── Entrypoint ─────────────────────────────

// ─────────────────────────── File-system IPC ──────────────────────────

#[derive(Debug, Serialize)]
struct DirEntry {
    name: String,
    path: String,
    /// `"dir"` or `"file"`.
    kind: &'static str,
    size: u64,
    /// Milliseconds since Unix epoch. `u64` so serde_json happily
    /// serializes it — u128 silently aborts the whole command response
    /// because JSON numbers can't hold 128-bit precision.
    modified_ms: u64,
}

#[tauri::command]
async fn pick_directory() -> Option<String> {
    let handle = rfd::AsyncFileDialog::new()
        .set_title("열 디렉터리 선택")
        .pick_folder()
        .await?;
    handle.path().to_str().map(str::to_string)
}

#[derive(Debug, Serialize)]
struct PickedMarkdown {
    path: String,
    name: String,
    content: String,
}

#[tauri::command]
async fn pick_markdown_file() -> Result<Option<PickedMarkdown>, String> {
    let Some(handle) = rfd::AsyncFileDialog::new()
        .set_title("마크다운 파일 열기")
        .add_filter("Markdown", &["md", "markdown", "mdx"])
        .add_filter("All Files", &["*"])
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    let path = handle.path().to_path_buf();
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let path_str = path.to_str().unwrap_or("").to_string();
    Ok(Some(PickedMarkdown { path: path_str, name, content }))
}

#[tauri::command]
fn list_markdown_files(path: String) -> Result<Vec<DirEntry>, String> {
    let mut out = Vec::new();
    let read = std::fs::read_dir(&path).map_err(|e| e.to_string())?;
    for entry in read.flatten() {
        let p = entry.path();
        let name = p
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() || name.starts_with('.') {
            continue; // skip hidden entries like .git
        }
        if p.is_dir() {
            out.push(DirEntry {
                name,
                path: p.to_str().unwrap_or("").to_string(),
                kind: "dir",
                size: 0,
                modified_ms: 0,
            });
            continue;
        }
        if !p.is_file() {
            continue;
        }
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        if !matches!(ext.as_deref(), Some("md") | Some("markdown") | Some("mdx")) {
            continue;
        }
        let meta = entry.metadata().map_err(|e| e.to_string())?;
        let modified_ms = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0);
        out.push(DirEntry {
            name,
            path: p.to_str().unwrap_or("").to_string(),
            kind: "file",
            size: meta.len(),
            modified_ms,
        });
    }
    // Folders first (alphabetical), then files (alphabetical).
    out.sort_by(|a, b| {
        if a.kind != b.kind {
            return if a.kind == "dir" {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        a.name.to_lowercase().cmp(&b.name.to_lowercase())
    });
    Ok(out)
}

#[tauri::command]
fn read_markdown_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn write_markdown_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_window_title(app: tauri::AppHandle, title: String) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.set_title(&title).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    FORCE_QUIT.store(true, Ordering::SeqCst);
    app.exit(0);
}

#[tauri::command]
fn fs_create_file(path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    if p.exists() {
        return Err(format!("이미 존재합니다: {}", p.display()));
    }
    std::fs::write(p, "").map_err(|e| e.to_string())
}

#[tauri::command]
fn fs_create_dir(path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    if p.exists() {
        return Err(format!("이미 존재합니다: {}", p.display()));
    }
    std::fs::create_dir(p).map_err(|e| e.to_string())
}

#[tauri::command]
fn fs_rename(old_path: String, new_path: String) -> Result<(), String> {
    if std::path::Path::new(&new_path).exists() {
        return Err("대상 경로가 이미 존재합니다".into());
    }
    std::fs::rename(&old_path, &new_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn fs_duplicate_file(path: String) -> Result<String, String> {
    let p = std::path::PathBuf::from(&path);
    let parent = p
        .parent()
        .ok_or_else(|| "부모 폴더를 찾을 수 없습니다".to_string())?
        .to_path_buf();
    let stem = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string();
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("md")
        .to_string();
    let mut target: std::path::PathBuf;
    let mut i = 1usize;
    loop {
        let name = if i == 1 {
            format!("{stem} (복사).{ext}")
        } else {
            format!("{stem} (복사 {i}).{ext}")
        };
        target = parent.join(&name);
        if !target.exists() {
            break;
        }
        i += 1;
        if i > 999 {
            return Err("사본 이름을 찾을 수 없습니다".into());
        }
    }
    std::fs::copy(&p, &target).map_err(|e| e.to_string())?;
    Ok(target.to_str().unwrap_or("").to_string())
}

#[tauri::command]
fn fs_delete(path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    if p.is_dir() {
        std::fs::remove_dir_all(p).map_err(|e| e.to_string())
    } else {
        std::fs::remove_file(p).map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn fs_reveal(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-R", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path))
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        let parent = std::path::Path::new(&path)
            .parent()
            .unwrap_or_else(|| std::path::Path::new(&path));
        std::process::Command::new("xdg-open")
            .arg(parent)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 릴리즈 빌드에선 false, 디버그(`cargo dev`) 빌드에선 true 를 반환.
/// 프런트엔드가 이 값을 기준으로 우클릭 네이티브 컨텍스트 메뉴를 전역
/// 차단할지 결정한다 — 개발 중엔 "Inspect Element" 접근성을 유지하고,
/// 릴리즈 사용자에겐 어떤 경로로도 노출되지 않도록 한다.
#[tauri::command]
fn is_dev_build() -> bool {
    cfg!(debug_assertions)
}

/// Open a URL with the OS's default handler. Used when the user
/// Cmd/Ctrl+Clicks a markdown link inside the editor.
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    // Very small allow-list to avoid being a drive-by launcher for
    // arbitrary schemes: http(s) + mailto.
    let ok = url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("mailto:");
    if !ok {
        return Err("unsupported url scheme".into());
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &url])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Compose the engine's abbreviation dict. 초기 버전에서는 사용자·학습
/// 사전을 엔진에 올리지 않고 내장 starter 의 초성 시퀀스 항목만 쓴다.
/// 파일은 디스크에 그대로 두며 (기존 데이터 보존), `include_user` 은
/// 시그니처 유지를 위해 남겨 두되 내부적으로 무시한다.
fn build_merged_abbrs(_config_dir: &Path, _include_user: bool) -> Vec<Abbreviation> {
    cho_only(starter_dict())
}

/// Info about one abbreviation source file on disk, for the 자동완성 관리 UI.
#[derive(Debug, serde::Serialize)]
struct AbbrFileInfo {
    path: String,
    exists: bool,
    /// Last-modified as RFC 3339 string. `None` if file absent / unreadable.
    mtime: Option<String>,
    /// How many `[[abbr]]` entries parsed successfully.
    count: usize,
}

/// One built-in starter entry, for the manager's "내장 사전" listing.
#[derive(Debug, serde::Serialize)]
struct BuiltinEntry {
    id: String,
    trigger: String,
    kind: &'static str,
    body: String,
    priority: u32,
    trigger_on: &'static str,
}

/// Full state snapshot for the 자동완성 관리 modal.
#[derive(Debug, serde::Serialize)]
struct AbbrManageInfo {
    use_user_abbrs: bool,
    builtin_count: usize,
    builtin: Vec<BuiltinEntry>,
    user_dict: AbbrFileInfo,
    learned_dict: AbbrFileInfo,
}

fn file_info(path: &Path) -> AbbrFileInfo {
    let exists = path.exists();
    if !exists {
        return AbbrFileInfo {
            path: path.display().to_string(),
            exists: false,
            mtime: None,
            count: 0,
        };
    }
    let mtime = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .map(format_mtime);
    let count = load_user_abbrs(path).map(|v| v.len()).unwrap_or(0);
    AbbrFileInfo {
        path: path.display().to_string(),
        exists: true,
        mtime,
        count,
    }
}

/// Format a SystemTime as a short, locale-agnostic display string
/// `YYYY-MM-DD HH:MM:SS` based on UTC offset applied at runtime.
fn format_mtime(t: std::time::SystemTime) -> String {
    let secs = t
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    // Convert epoch to local-ish "calendar" without pulling in a date
    // crate. We expose ISO-8601 in UTC; the frontend's `toLocaleString`
    // handles the TZ display.
    let mut days = secs / 86_400;
    let mut sec_of_day = secs.rem_euclid(86_400);
    if sec_of_day < 0 {
        sec_of_day += 86_400;
        days -= 1;
    }
    let h = sec_of_day / 3600;
    let m = (sec_of_day % 3600) / 60;
    let s = sec_of_day % 60;
    // civil_from_days (Howard Hinnant algorithm).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i32 + (era as i32) * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m_num = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = y + if m_num <= 2 { 1 } else { 0 };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, m_num, d, h, m, s
    )
}

fn kind_label(t: &Trigger) -> &'static str {
    match t {
        Trigger::ChoSeq(_) => "cho_seq",
        Trigger::Literal(_) => "literal",
        Trigger::Ending(_) => "ending",
    }
}

fn trigger_event_label(ev: TriggerEvent) -> &'static str {
    match ev {
        TriggerEvent::Immediate => "immediate",
        TriggerEvent::Space => "space",
        TriggerEvent::Enter => "enter",
        TriggerEvent::Punctuation => "punctuation",
        TriggerEvent::JongCompletion => "jong_completion",
        TriggerEvent::Explicit => "explicit",
    }
}

/// Return the full snapshot powering the 자동완성 관리 modal.
#[tauri::command]
fn get_abbr_manage_info(
    state: State<'_, ImeState>,
    settings_path: State<'_, SettingsPath>,
) -> Result<AbbrManageInfo, String> {
    let config_dir = &settings_path.0;
    let user_path = config_dir.join("abbreviations.toml");
    let learned_path = ngram::dict_path(config_dir);
    let builtin = cho_only(starter_dict());
    let builtin_count = builtin.len();
    let builtin_entries = builtin
        .into_iter()
        .map(|a| BuiltinEntry {
            id: a.id.clone(),
            trigger: a.trigger.display(),
            kind: kind_label(&a.trigger),
            body: a.body,
            priority: a.priority,
            trigger_on: trigger_event_label(a.trigger_on),
        })
        .collect();
    let use_user_abbrs = state.0.lock().unwrap().use_user_abbrs;
    Ok(AbbrManageInfo {
        use_user_abbrs,
        builtin_count,
        builtin: builtin_entries,
        user_dict: file_info(&user_path),
        learned_dict: file_info(&learned_path),
    })
}

/// Toggle whether user-defined abbreviation dicts are merged with the
/// built-in starter. Persists to settings.toml and rebuilds the live
/// engine dict accordingly.
#[tauri::command]
fn set_use_user_abbrs(
    enabled: bool,
    state: State<'_, ImeState>,
    settings_path: State<'_, SettingsPath>,
) -> Result<(), String> {
    let merged = build_merged_abbrs(&settings_path.0, enabled);
    {
        let mut sess = state.0.lock().unwrap();
        sess.use_user_abbrs = enabled;
        sess.abbr.set_abbreviations(merged);
    }
    // Persist so the next launch honors it.
    let mut saved = Settings::load(&settings_path.0);
    saved.use_user_abbrs = enabled;
    if let Err(err) = saved.save(&settings_path.0) {
        eprintln!("sbmd: failed to persist use_user_abbrs: {err}");
    }
    Ok(())
}

/// Pick a user-supplied `.toml` abbreviation dictionary, parse it, and
/// replace the current abbreviation engine's list with
/// (starter_dict + user's file). Returns the number of user entries
/// loaded, or `None` if the user cancelled the picker.
#[tauri::command]
async fn pick_and_load_abbr_dict(
    state: State<'_, ImeState>,
) -> Result<Option<usize>, String> {
    let Some(handle) = rfd::AsyncFileDialog::new()
        .set_title("자동완성 사전 선택 (TOML)")
        .add_filter("TOML", &["toml"])
        .add_filter("All Files", &["*"])
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    let path = handle.path().to_path_buf();
    let user = load_user_abbrs(&path).map_err(|e| e.to_string())?;
    let user_count = user.len();
    let mut sess = state.0.lock().unwrap();
    // 초기 버전에서는 사용자 사전 항목도 엔진에는 싣지 않는다. 파일은
    // 디스크에 그대로 보존되므로 나중에 다시 켜면 로드할 수 있다.
    let _ = user; // 로드는 했으나 엔진에 반영은 하지 않음
    let merged: Vec<Abbreviation> = cho_only(starter_dict());
    sess.abbr.set_abbreviations(merged);
    Ok(Some(user_count))
}

/// Report payload for the learn-from-directory scan.
#[derive(Debug, serde::Serialize)]
struct NgramScanReport {
    /// How many `[[abbr]]` entries were written to the learned dict.
    count: usize,
    /// Number of `.txt` / `.md` / `.markdown` / `.mdx` files read.
    files: usize,
    /// Total 어절 (whitespace-split tokens) consumed across those files.
    tokens: usize,
    /// Absolute path of the scanned directory (for status messages).
    dir: String,
}

/// Pick a directory, scan every `.txt` / `.md` inside (recursively),
/// build 1/2/3-gram abbreviations keyed by 어절, persist to
/// `<config>/learned_ngrams.toml`, and merge into the live engine.
///
/// Returns `Ok(None)` if the user cancels the directory picker.
#[tauri::command]
async fn scan_and_build_ngram_dict(
    state: State<'_, ImeState>,
    settings_path: State<'_, SettingsPath>,
    start_dir: Option<String>,
) -> Result<Option<NgramScanReport>, String> {
    // Seed the picker with the workspace folder the user last opened in
    // the sidebar tree. Non-existent paths are ignored so the dialog
    // falls back to its own default.
    let mut dialog = rfd::AsyncFileDialog::new()
        .set_title("자동완성 학습 폴더 선택 (.txt · .md)");
    if let Some(s) = start_dir.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let p = PathBuf::from(s);
        if p.is_dir() {
            dialog = dialog.set_directory(&p);
        }
    }
    let Some(handle) = dialog.pick_folder().await else {
        return Ok(None);
    };
    let dir = handle.path().to_path_buf();

    let report = ngram::scan_dir(&dir).map_err(|e| e.to_string())?;
    let count = report.abbrs.len();

    // Persist the generated dict so it survives a relaunch without a
    // rescan, and so the user can inspect / hand-edit if needed.
    let out_path = ngram::dict_path(&settings_path.0);
    if !settings_path.0.exists() {
        let _ = std::fs::create_dir_all(&settings_path.0);
    }
    std::fs::write(&out_path, ngram::to_toml(&report.abbrs))
        .map_err(|e| format!("learned_ngrams.toml 쓰기 실패: {e}"))?;

    // Rebuild the engine's dict from disk (starter + user + freshly
    // written learned). Honor the use_user_abbrs flag so a user who
    // disabled their dicts can scan a corpus without the result
    // surprise-appearing until they re-enable.
    let mut sess = state.0.lock().unwrap();
    let merged = build_merged_abbrs(&settings_path.0, sess.use_user_abbrs);
    sess.abbr.set_abbreviations(merged);
    drop(sess);

    eprintln!(
        "sbmd: learned {count} n-gram(s) from {} file(s) / {} token(s) in {}",
        report.files_scanned,
        report.tokens_read,
        dir.display()
    );

    Ok(Some(NgramScanReport {
        count,
        files: report.files_scanned,
        tokens: report.tokens_read,
        dir: dir.to_string_lossy().into_owned(),
    }))
}

/// Restore the app's built-in starter abbreviations + the user file at
/// `<app-config>/abbreviations.toml` (the normal startup source).
#[tauri::command]
fn reset_abbr_dict(
    state: State<'_, ImeState>,
    settings_path: State<'_, SettingsPath>,
) -> Result<usize, String> {
    let abbr_path = settings_path.0.join("abbreviations.toml");
    let user_count = if abbr_path.exists() {
        load_user_abbrs(&abbr_path).map(|v| v.len()).unwrap_or(0)
    } else {
        0
    };
    let mut sess = state.0.lock().unwrap();
    let merged = build_merged_abbrs(&settings_path.0, sess.use_user_abbrs);
    sess.abbr.set_abbreviations(merged);
    Ok(user_count)
}

/// Reset all persisted settings + in-memory session back to defaults.
/// Deletes `settings.toml` so the next launch starts fresh, and
/// rewrites the active `ImeSession` so the current window reflects the
/// defaults immediately.
#[tauri::command]
fn reset_settings(
    state: State<'_, ImeState>,
    settings_path: State<'_, SettingsPath>,
) -> Result<LayoutInfo, String> {
    let path = settings_path.0.join(crate::settings::FILE_NAME);
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }
    let defaults = Settings::default();
    let mut sess = state.0.lock().unwrap();
    sess.fsm.cancel();
    let hangul = make_hangul_layout(&defaults.hangul_layout_id)
        .unwrap_or_else(|| Box::new(SebeolsikFinal));
    let latin = make_latin_layout(&defaults.latin_layout_id)
        .unwrap_or_else(|| Box::new(Qwerty));
    sess.hangul_layout = hangul;
    sess.latin_layout = latin;
    sess.active_mode = InputMode::Hangul;
    sess.output_form = parse_output_form(&defaults.output_form)
        .unwrap_or(OutputForm::NfcSyllable);
    let compose = parse_compose_mode(&defaults.compose_mode)
        .unwrap_or(ComposeMode::Moachigi);
    sess.fsm.set_mode(compose);
    sess.suggestions_enabled = defaults.suggestions_enabled;
    sess.backspace_mode = defaults.backspace_mode;
    sess.use_user_abbrs = defaults.use_user_abbrs;
    // Rebuild the engine dict so the flip of use_user_abbrs takes effect
    // immediately.
    let merged = build_merged_abbrs(&settings_path.0, defaults.use_user_abbrs);
    sess.abbr.set_abbreviations(merged);
    Ok(layout_info(&sess))
}

#[tauri::command]
async fn save_as_markdown(
    suggested_name: Option<String>,
    content: String,
) -> Result<Option<String>, String> {
    let mut dlg = rfd::AsyncFileDialog::new()
        .set_title("마크다운 파일로 저장")
        .add_filter("Markdown", &["md", "markdown", "mdx"])
        .add_filter("All Files", &["*"]);
    if let Some(n) = suggested_name {
        dlg = dlg.set_file_name(&n);
    }
    let Some(handle) = dlg.save_file().await else {
        return Ok(None);
    };
    let path = handle.path().to_path_buf();
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(path.to_str().map(str::to_string))
}

// 포맷별 save 다이얼로그를 띄워 사용자가 선택한 경로에 content 를 그대로
// 기록한다. PDF 는 브라우저 프린트 다이얼로그의 "PDF 로 저장" 을 쓰므로
// 여기서는 md / html 만 취급한다. 파일 I/O 는 rfd + std::fs 만으로 끝난다.
#[tauri::command]
async fn export_file(
    format: String,
    suggested_name: Option<String>,
    content: String,
) -> Result<Option<String>, String> {
    let (title, filter_name, extensions): (&str, &str, &[&str]) = match format.as_str() {
        "md" => ("마크다운으로 내보내기", "Markdown", &["md", "markdown", "mdx"]),
        "html" => ("HTML 로 내보내기", "HTML", &["html", "htm"]),
        other => return Err(format!("unsupported export format: {other}")),
    };
    let mut dlg = rfd::AsyncFileDialog::new()
        .set_title(title)
        .add_filter(filter_name, extensions)
        .add_filter("All Files", &["*"]);
    if let Some(n) = suggested_name {
        dlg = dlg.set_file_name(&n);
    }
    let Some(handle) = dlg.save_file().await else {
        return Ok(None);
    };
    let path = handle.path().to_path_buf();
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(path.to_str().map(str::to_string))
}

#[tauri::command]
fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// 업데이트 확인 결과를 프론트엔드에 전달하기 위한 직렬화 가능 DTO.
/// `tauri_plugin_updater::Update` 자체는 핸들을 들고 있어 그대로 보낼 수
/// 없으므로 사용자에게 보일 정보만 추려 보낸다.
#[derive(Serialize)]
struct UpdateInfo {
    version: String,
    current_version: String,
    body: Option<String>,
    date: Option<String>,
}

/// 원격 매니페스트(`tauri.conf.json` `plugins.updater.endpoints`) 를 조회해
/// 새 버전이 있으면 메타데이터를 돌려준다. 없으면 `Ok(None)`.
#[tauri::command]
async fn check_for_update(app: tauri::AppHandle) -> Result<Option<UpdateInfo>, String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => Ok(Some(UpdateInfo {
            version: update.version.clone(),
            current_version: update.current_version.clone(),
            body: update.body.clone(),
            date: update.date.map(|d| d.to_string()),
        })),
        Ok(None) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// 새 버전을 다운로드·서명검증·설치한 뒤 앱을 재시작한다. 실패 시 에러
/// 문자열을 반환하고 앱은 그대로 살려 둔다.
#[tauri::command]
async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        return Err("업데이트 가능한 버전이 없습니다.".into());
    };
    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    // 자동 업데이트가 끝나면 자기 자신을 새 바이너리로 교체했으므로
    // 사용자가 수동으로 다시 켤 필요 없이 곧바로 재시작한다.
    app.restart();
}

#[tauri::command]
fn set_always_on_top(window: tauri::Window, on: bool) -> Result<(), String> {
    window.set_always_on_top(on).map_err(|e| e.to_string())
}

/// Set the webview's native zoom factor (WKWebView on macOS). CSS
/// `body.style.zoom` can get clobbered by the OS-level webview layer,
/// so we also drive the native zoom here as a reliable fallback.
#[tauri::command]
fn set_webview_zoom(webview: tauri::WebviewWindow, factor: f64) -> Result<(), String> {
    webview.set_zoom(factor).map_err(|e| e.to_string())
}

/// Flip the check mark on a CheckMenuItem by id. The frontend calls
/// this whenever a view-menu state changes (focus mode on, sidebar
/// collapsed, etc.) so the native menu mirrors the UI.
#[tauri::command]
fn set_menu_check(app: tauri::AppHandle, id: String, on: bool) -> Result<(), String> {
    let menu = app.menu().ok_or_else(|| "menu not yet built".to_string())?;
    let item = menu
        .get(id.as_str())
        .ok_or_else(|| format!("menu item '{}' not found", id))?;
    let check = item
        .as_check_menuitem()
        .ok_or_else(|| format!("menu item '{}' is not a check item", id))?;
    check.set_checked(on).map_err(|e| e.to_string())
}

/// Read the current check state of a CheckMenuItem. macOS auto-toggles
/// the check when the user clicks the item, so the frontend reads the
/// post-click state instead of guessing the new value from JS state
/// (which races with the native auto-toggle and flips the wrong way).
#[tauri::command]
fn get_menu_check(app: tauri::AppHandle, id: String) -> Result<bool, String> {
    let menu = app.menu().ok_or_else(|| "menu not yet built".to_string())?;
    let item = menu
        .get(id.as_str())
        .ok_or_else(|| format!("menu item '{}' not found", id))?;
    let check = item
        .as_check_menuitem()
        .ok_or_else(|| format!("menu item '{}' is not a check item", id))?;
    check.is_checked().map_err(|e| e.to_string())
}

/// 테마 메뉴는 라디오 형태 — 현재 적용된 테마 **하나만** 체크되어야
/// 한다. JS 가 13 개 항목에 대해 set_menu_check 를 병렬로 쏘면 IPC 순서
/// 경쟁 때문에 두 개 이상 체크된 것처럼 보이는 케이스가 생기므로, 여기
/// 서 한 번의 호출로 `active` 만 true, 나머지 `theme-*` 은 전부 false 로
/// **원자적** 으로 맞춘다.
#[tauri::command]
fn set_theme_check_exclusive(app: tauri::AppHandle, active: String) -> Result<(), String> {
    let menu = app.menu().ok_or_else(|| "menu not yet built".to_string())?;
    let target_id = format!("theme-{}", active);
    // muda 의 items() 로 트리를 얕게 훑어 theme-* 패턴에 맞는 CheckMenuItem
    // 을 전부 찾아 체크 상태를 갱신한다. 상단 메뉴 → 테마 서브메뉴 안에
    // 있는 구조이므로 depth 1 까지 들여다보면 충분.
    for item in menu.items().map_err(|e| e.to_string())? {
        if let Some(sub) = item.as_submenu() {
            for inner in sub.items().map_err(|e| e.to_string())? {
                if let Some(check) = inner.as_check_menuitem() {
                    let id = check.id().0.as_str().to_string();
                    if id.starts_with("theme-") {
                        let _ = check.set_checked(id == target_id);
                    }
                }
            }
        }
    }
    Ok(())
}

#[tauri::command]
fn set_window_fullscreen(window: tauri::Window, on: bool) -> Result<(), String> {
    window.set_fullscreen(on).map_err(|e| e.to_string())
}

#[tauri::command]
fn is_window_fullscreen(window: tauri::Window) -> Result<bool, String> {
    window.is_fullscreen().map_err(|e| e.to_string())
}

// macOS 표준 동작: 제목표시줄 더블클릭 시 창을 zoom(=최대화) ↔ 원래 크기로
// 토글한다. 이미 최대화 상태면 unmaximize, 아니면 maximize.
#[tauri::command]
fn toggle_window_maximize(window: tauri::Window) -> Result<(), String> {
    let maxed = window.is_maximized().map_err(|e| e.to_string())?;
    if maxed {
        window.unmaximize().map_err(|e| e.to_string())
    } else {
        window.maximize().map_err(|e| e.to_string())
    }
}

fn build_app_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    // ── macOS "새별" app menu (short menu-bar label for 새별 마크다운 에디터) ──
    // "새별 정보" is a custom item (not the native About) so the frontend
    // can render an animated, branded About modal. macOS's built-in About
    // dialog is locked to system styling, which doesn't fit our design.
    let about_item = MenuItemBuilder::with_id("about", "새별 정보").build(app)?;
    let check_update_item =
        MenuItemBuilder::with_id("check-update", "업데이트 확인…").build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "설정…")
        .accelerator("CmdOrCtrl+,")
        .build(app)?;
    let appearance = MenuItemBuilder::with_id("appearance", "모양 설정…")
        .accelerator("Shift+CmdOrCtrl+,")
        .build(app)?;
    let reset_settings_item =
        MenuItemBuilder::with_id("reset-settings", "설정 초기화…").build(app)?;
    let app_submenu = SubmenuBuilder::new(app, "새별")
        .item(&about_item)
        .item(&check_update_item)
        .separator()
        .item(&settings)
        .item(&appearance)
        .item(&reset_settings_item)
        .separator()
        .services_with_text("서비스")
        .separator()
        .hide_with_text("새별 숨기기")
        .hide_others_with_text("기타 항목 숨기기")
        .show_all_with_text("모두 보기")
        .separator()
        .quit_with_text("새별 종료")
        .build()?;

    // ── 파일 ──
    let new_tab = MenuItemBuilder::with_id("new-tab", "새 탭")
        .accelerator("CmdOrCtrl+T")
        .build(app)?;
    let open_file = MenuItemBuilder::with_id("open-file", "파일 열기…")
        .accelerator("CmdOrCtrl+Shift+O")
        .build(app)?;
    let open_dir = MenuItemBuilder::with_id("open-dir", "폴더 열기…")
        .accelerator("CmdOrCtrl+O")
        .build(app)?;
    let save = MenuItemBuilder::with_id("save", "저장")
        .accelerator("CmdOrCtrl+S")
        .build(app)?;
    let close_tab = MenuItemBuilder::with_id("close-tab", "탭 닫기")
        .accelerator("CmdOrCtrl+W")
        .build(app)?;
    let print = MenuItemBuilder::with_id("print", "인쇄…")
        .accelerator("CmdOrCtrl+P")
        .build(app)?;
    // ── 파일 → 내보내기 ──
    // PDF 는 OS 의 프린트 다이얼로그 "PDF 로 저장" 루트를 타고, md / html 은
    // 프런트에서 직렬화한 문자열을 Rust 쪽 save 다이얼로그로 넘겨서 기록한다.
    let export_pdf = MenuItemBuilder::with_id("export-pdf", "PDF…").build(app)?;
    let export_html = MenuItemBuilder::with_id("export-html", "HTML…").build(app)?;
    let export_md = MenuItemBuilder::with_id("export-md", "Markdown…").build(app)?;
    let export_submenu = SubmenuBuilder::new(app, "내보내기")
        .item(&export_pdf)
        .item(&export_html)
        .item(&export_md)
        .build()?;
    // 초기 버전에서는 자동완성 관리 팝업(사용자·학습 사전 UI) 을 노출
    // 하지 않는다. 엔진에는 내장 starter 의 초성 시퀀스 항목만 올라간다.
    let file_submenu = SubmenuBuilder::new(app, "파일")
        .item(&new_tab)
        .item(&open_file)
        .item(&open_dir)
        .separator()
        .item(&save)
        .item(&close_tab)
        .separator()
        .item(&print)
        .item(&export_submenu)
        .build()?;

    // ── 편집 (macOS native copy/paste so the edit menu reads natural) ──
    let edit_submenu = SubmenuBuilder::new(app, "편집")
        .undo_with_text("실행 취소")
        .redo_with_text("다시 실행")
        .separator()
        .cut_with_text("잘라내기")
        .copy_with_text("복사")
        .paste_with_text("붙여넣기")
        .select_all_with_text("모두 선택")
        .build()?;

    // ── 문단 ──
    // 제목 1~6 은 기존 Cmd+Digit 단축키와 대응하고, 그 외 삽입 항목들은
    // Alt+Cmd+X 계열을 쓴다. 단축키 표기는 macOS 네이티브 가속자 문자열.
    let h1 = MenuItemBuilder::with_id("para-h1", "제목 1").accelerator("CmdOrCtrl+1").build(app)?;
    let h2 = MenuItemBuilder::with_id("para-h2", "제목 2").accelerator("CmdOrCtrl+2").build(app)?;
    let h3 = MenuItemBuilder::with_id("para-h3", "제목 3").accelerator("CmdOrCtrl+3").build(app)?;
    let h4 = MenuItemBuilder::with_id("para-h4", "제목 4").accelerator("CmdOrCtrl+4").build(app)?;
    let h5 = MenuItemBuilder::with_id("para-h5", "제목 5").accelerator("CmdOrCtrl+5").build(app)?;
    let h6 = MenuItemBuilder::with_id("para-h6", "제목 6").accelerator("CmdOrCtrl+6").build(app)?;
    let body = MenuItemBuilder::with_id("para-body", "본문").accelerator("CmdOrCtrl+0").build(app)?;
    let heading_promote = MenuItemBuilder::with_id("para-heading-promote", "제목 올리기")
        .accelerator("CmdOrCtrl+=")
        .build(app)?;
    let heading_demote = MenuItemBuilder::with_id("para-heading-demote", "제목 내리기")
        .accelerator("CmdOrCtrl+-")
        .build(app)?;

    // 표 서브메뉴 — 현재는 기본 3x3 삽입 하나.
    let table_insert = MenuItemBuilder::with_id("para-table-insert", "표 만들기 (3×3)").build(app)?;
    let table_sub = SubmenuBuilder::new(app, "표").item(&table_insert).build()?;

    let math_block = MenuItemBuilder::with_id("para-math-block", "수식")
        .accelerator("Alt+CmdOrCtrl+B")
        .build(app)?;
    let code_fence = MenuItemBuilder::with_id("para-code-fence", "코드 블록")
        .accelerator("Alt+CmdOrCtrl+C")
        .build(app)?;

    // 코드 도구 서브메뉴.
    let code_trim = MenuItemBuilder::with_id("para-code-trim", "여백 다듬기").build(app)?;
    let code_tools_sub = SubmenuBuilder::new(app, "코드 도구").item(&code_trim).build()?;

    // 강조 상자 (GFM admonition) — 한국어·영어 병기로 의미 명확.
    let alert_note = MenuItemBuilder::with_id("para-alert-note", "참고").build(app)?;
    let alert_tip = MenuItemBuilder::with_id("para-alert-tip", "팁").build(app)?;
    let alert_important = MenuItemBuilder::with_id("para-alert-important", "중요").build(app)?;
    let alert_warning = MenuItemBuilder::with_id("para-alert-warning", "주의").build(app)?;
    let alert_caution = MenuItemBuilder::with_id("para-alert-caution", "경고").build(app)?;
    let alert_sub = SubmenuBuilder::new(app, "강조 상자")
        .item(&alert_note)
        .item(&alert_tip)
        .item(&alert_important)
        .item(&alert_warning)
        .item(&alert_caution)
        .build()?;

    let quote = MenuItemBuilder::with_id("para-quote", "인용")
        .accelerator("Alt+CmdOrCtrl+Q")
        .build(app)?;
    let ol = MenuItemBuilder::with_id("para-ol", "번호 목록")
        .accelerator("Alt+CmdOrCtrl+O")
        .build(app)?;
    let ul = MenuItemBuilder::with_id("para-ul", "점 목록")
        .accelerator("Alt+CmdOrCtrl+U")
        .build(app)?;
    let task = MenuItemBuilder::with_id("para-task", "할 일 목록")
        .accelerator("Alt+CmdOrCtrl+X")
        .build(app)?;

    // 할 일 체크 상태 서브메뉴.
    let task_check = MenuItemBuilder::with_id("para-task-check", "완료").build(app)?;
    let task_uncheck = MenuItemBuilder::with_id("para-task-uncheck", "미완료").build(app)?;
    let task_toggle = MenuItemBuilder::with_id("para-task-toggle", "상태 뒤집기").build(app)?;
    let task_state_sub = SubmenuBuilder::new(app, "할 일 상태")
        .item(&task_check)
        .item(&task_uncheck)
        .item(&task_toggle)
        .build()?;

    // 들여쓰기 서브메뉴.
    let indent_in = MenuItemBuilder::with_id("para-indent-in", "들여쓰기").build(app)?;
    let indent_out = MenuItemBuilder::with_id("para-indent-out", "내어쓰기").build(app)?;
    let indent_reset = MenuItemBuilder::with_id("para-indent-reset", "초기화").build(app)?;
    let indent_sub = SubmenuBuilder::new(app, "들여쓰기")
        .item(&indent_in)
        .item(&indent_out)
        .item(&indent_reset)
        .build()?;

    let para_before = MenuItemBuilder::with_id("para-insert-before", "위에 빈 줄").build(app)?;
    let para_after = MenuItemBuilder::with_id("para-insert-after", "아래에 빈 줄").build(app)?;

    let link = MenuItemBuilder::with_id("para-link", "링크")
        .accelerator("Alt+CmdOrCtrl+L")
        .build(app)?;
    let footnote = MenuItemBuilder::with_id("para-footnote", "각주")
        .accelerator("Alt+CmdOrCtrl+R")
        .build(app)?;

    let hr = MenuItemBuilder::with_id("para-hr", "구분선")
        .accelerator("Alt+CmdOrCtrl+-")
        .build(app)?;
    let toc = MenuItemBuilder::with_id("para-toc", "목차").build(app)?;
    let yaml_front = MenuItemBuilder::with_id("para-yaml", "YAML 머리글").build(app)?;

    let paragraph_submenu = SubmenuBuilder::new(app, "문단")
        .item(&h1).item(&h2).item(&h3).item(&h4).item(&h5).item(&h6)
        .separator()
        .item(&body)
        .separator()
        .item(&heading_promote).item(&heading_demote)
        .separator()
        .item(&ol).item(&ul).item(&task)
        .item(&task_state_sub)
        .item(&indent_sub)
        .separator()
        .item(&quote)
        .item(&alert_sub)
        .separator()
        .item(&table_sub)
        .item(&code_fence)
        .item(&code_tools_sub)
        .item(&math_block)
        .separator()
        .item(&link).item(&footnote)
        .separator()
        .item(&hr).item(&toc).item(&yaml_front)
        .separator()
        .item(&para_before).item(&para_after)
        .build()?;

    // ── 보기 ──
    // 상태를 가진 항목(모드·토글·라디오)은 CheckMenuItem 으로 만들어 메뉴에
    // 체크 표시를 띄운다. 프런트가 상태를 바꾸면 `set_menu_check` 커맨드로
    // 체크 상태를 동기화한다. 실제 크기/확대/축소/검색처럼 상태가 없는
    // 실행 항목은 일반 MenuItem 으로 남긴다.
    let toggle_source = CheckMenuItemBuilder::with_id("toggle-source", "원문 보기")
        .accelerator("CmdOrCtrl+/")
        .checked(false)
        .build(app)?;
    // 사이드바는 기본이 파일 트리. 이 메뉴를 체크하면 개요 뷰로 전환되고
    // 해제하면 파일 트리로 돌아간다.
    let sidebar_outline = CheckMenuItemBuilder::with_id("sidebar-outline", "개요 보기")
        .accelerator("Control+CmdOrCtrl+1")
        .checked(false)
        .build(app)?;
    // 우측 보조 사이드바(키보드 매핑/약어) 와 하단 스타일 바는 보기 메뉴
    // 에서 직접 토글. 기존 설정 모달의 "사이드바" 섹션에서 옮겨 왔다.
    let toggle_keymap_panel =
        CheckMenuItemBuilder::with_id("toggle-keymap-panel", "키보드 매핑 패널")
            .checked(false)
            .build(app)?;
    let toggle_abbrs_panel =
        CheckMenuItemBuilder::with_id("toggle-abbrs-panel", "등록된 약어 패널")
            .checked(false)
            .build(app)?;
    let toggle_stylebar = CheckMenuItemBuilder::with_id("toggle-stylebar", "스타일 바")
        .checked(false)
        .build(app)?;
    let toggle_statusbar = CheckMenuItemBuilder::with_id("toggle-statusbar", "상태표시줄")
        .checked(true)
        .build(app)?;
    let toggle_zoom_allow = CheckMenuItemBuilder::with_id("toggle-zoom-allow", "배율 조절 허용")
        .checked(true)
        .build(app)?;
    let zoom_reset = MenuItemBuilder::with_id("zoom-reset", "100% 크기")
        .accelerator("Shift+CmdOrCtrl+0")
        .build(app)?;
    let zoom_in = MenuItemBuilder::with_id("zoom-in", "확대")
        .accelerator("Shift+CmdOrCtrl+=")
        .build(app)?;
    let zoom_out = MenuItemBuilder::with_id("zoom-out", "축소")
        .accelerator("Shift+CmdOrCtrl+-")
        .build(app)?;
    let toggle_always_on_top =
        CheckMenuItemBuilder::with_id("toggle-always-on-top", "항상 맨 위")
            .checked(false)
            .build(app)?;
    let toggle_fullscreen = CheckMenuItemBuilder::with_id("toggle-fullscreen", "전체 화면")
        .accelerator("Control+CmdOrCtrl+F")
        .checked(false)
        .build(app)?;
    let view_submenu = SubmenuBuilder::new(app, "보기")
        .item(&toggle_source)
        .separator()
        .item(&sidebar_outline)
        .item(&toggle_keymap_panel)
        .item(&toggle_abbrs_panel)
        .item(&toggle_stylebar)
        .item(&toggle_statusbar)
        .separator()
        .item(&toggle_zoom_allow)
        .item(&zoom_reset)
        .item(&zoom_in)
        .item(&zoom_out)
        .separator()
        .item(&toggle_always_on_top)
        .item(&toggle_fullscreen)
        .build()?;

    // ── 도움말 ──
    let help = MenuItemBuilder::with_id("help", "도움말")
        .accelerator("F1")
        .build(app)?;
    let doc_markdown = MenuItemBuilder::with_id("doc-markdown", "마크다운 안내")
        .build(app)?;
    let doc_autocomplete = MenuItemBuilder::with_id("doc-autocomplete", "자동완성 안내")
        .build(app)?;
    let doc_files = MenuItemBuilder::with_id("doc-files", "파일·탭 안내")
        .build(app)?;
    let doc_shortcuts = MenuItemBuilder::with_id("doc-shortcuts", "단축키 모음")
        .build(app)?;
    let doc_moachigi = MenuItemBuilder::with_id("doc-moachigi", "모아치기 안내")
        .build(app)?;
    let help_submenu = SubmenuBuilder::new(app, "도움말")
        .item(&help)
        .separator()
        .item(&doc_markdown)
        .item(&doc_autocomplete)
        .item(&doc_files)
        .item(&doc_shortcuts)
        .separator()
        .item(&doc_moachigi)
        .build()?;

    // ── 테마 ── (설정 모달 밖에서도 빠르게 전환할 수 있도록 상단 메뉴로
    //            노출한다. 각 항목은 CheckMenuItem 이라 현재 적용된 테마
    //            하나에만 ✓ 가 뜨며, JS 쪽 applyTheme 가 다른 테마 체크를
    //            풀어준다.)
    let theme_def = [
        ("theme-graphite",        "Graphite · 무채색 다크"),
        ("theme-midnight",        "Midnight · Nord"),
        ("theme-rosepine",        "Rose Pine · 파스텔 다크"),
        ("theme-dracula",         "Dracula · 퍼플 액센트"),
        ("theme-tokyonight",      "Tokyo Night · 모던 블루"),
        ("theme-solarized-dark",  "Solarized Dark · 클래식"),
        ("theme-moss",            "Moss · 따뜻한 페이퍼"),
        ("theme-sepia",           "Sepia · 리딩 페이퍼"),
        ("theme-latte",           "Latte · Catppuccin"),
        ("theme-slate",           "Slate · Material"),
        ("theme-arctic",          "Arctic · 클린 화이트"),
        ("theme-solarized-light", "Solarized Light · 웜 페이퍼"),
        ("theme-github-light",    "GitHub Light · 클린 라이트"),
    ];
    let mut theme_submenu = SubmenuBuilder::new(app, "테마");
    let mut theme_dark_separator_inserted = false;
    for (i, (id, label)) in theme_def.iter().enumerate() {
        let item = CheckMenuItemBuilder::with_id(*id, *label)
            .checked(false)
            .build(app)?;
        theme_submenu = theme_submenu.item(&item);
        // 다크(6개) 와 라이트 사이에 구분선.
        if i == 5 && !theme_dark_separator_inserted {
            theme_submenu = theme_submenu.separator();
            theme_dark_separator_inserted = true;
        }
    }
    let theme_submenu = theme_submenu.build()?;

    MenuBuilder::new(app)
        .items(&[
            &app_submenu,
            &file_submenu,
            &edit_submenu,
            &paragraph_submenu,
            &view_submenu,
            &theme_submenu,
            &help_submenu,
        ])
        .build()
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .menu(|handle| build_app_menu(handle))
        .on_menu_event(|app, event| {
            let id = event.id().0.as_str();
            eprintln!("sbmd: menu-event '{}'", id);
            if matches!(
                id,
                "new-tab"
                    | "open-file"
                    | "open-dir"
                    | "save"
                    | "close-tab"
                    | "print"
                    | "export-pdf"
                    | "export-html"
                    | "export-md"
                    | "settings"
                    | "help"
                    | "toggle-stylebar"
                    | "toggle-statusbar"
                    | "toggle-source"
                    | "sidebar-outline"
                    | "toggle-keymap-panel"
                    | "toggle-abbrs-panel"
                    | "toggle-zoom-allow"
                    | "zoom-reset"
                    | "zoom-in"
                    | "zoom-out"
                    | "toggle-always-on-top"
                    | "toggle-fullscreen"
                    | "reset-settings"
                    | "appearance"
                    | "doc-moachigi"
                    | "doc-markdown"
                    | "doc-autocomplete"
                    | "doc-files"
                    | "doc-shortcuts"
                    | "about"
                    | "check-update"
                    // 본문 메뉴
                    | "para-h1"
                    | "para-h2"
                    | "para-h3"
                    | "para-h4"
                    | "para-h5"
                    | "para-h6"
                    | "para-body"
                    | "para-heading-promote"
                    | "para-heading-demote"
                    | "para-table-insert"
                    | "para-math-block"
                    | "para-code-fence"
                    | "para-code-trim"
                    | "para-alert-note"
                    | "para-alert-tip"
                    | "para-alert-important"
                    | "para-alert-warning"
                    | "para-alert-caution"
                    | "para-quote"
                    | "para-ol"
                    | "para-ul"
                    | "para-task"
                    | "para-task-check"
                    | "para-task-uncheck"
                    | "para-task-toggle"
                    | "para-indent-in"
                    | "para-indent-out"
                    | "para-indent-reset"
                    | "para-insert-before"
                    | "para-insert-after"
                    | "para-link"
                    | "para-footnote"
                    | "para-hr"
                    | "para-toc"
                    | "para-yaml"
            ) || id.starts_with("theme-") {
                let _ = app.emit("menu-action", id.to_string());
            }
        })
        .on_window_event(|window, event| {
            // macOS는 기본적으로 창을 닫아도 프로세스가 남는다. 이 앱은
            // 창 X 버튼을 완전 종료로 취급하지만, 그 전에 저장 안 된 탭이
            // 없는지 프론트엔드에 한 번 물어본다. 첫 진입에서는 닫기를
            // 막고 `quit-requested` 이벤트만 흘려보내고, 프론트가 저장
            // 흐름을 마친 뒤 `quit_app` 으로 돌아오면 `FORCE_QUIT` 가
            // 켜진 채 두 번째 `CloseRequested` 가 자연스럽게 통과한다.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if FORCE_QUIT.load(Ordering::SeqCst) {
                    return;
                }
                api.prevent_close();
                let _ = window.app_handle().emit("quit-requested", ());
            }
        })
        .setup(|app| {
            let config_dir: PathBuf = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| PathBuf::from(".sbmd"));
            let saved = Settings::load(&config_dir);

            let hangul = make_hangul_layout(&saved.hangul_layout_id)
                .unwrap_or_else(|| Box::new(SebeolsikFinal));
            let latin = make_latin_layout(&saved.latin_layout_id)
                .unwrap_or_else(|| Box::new(Qwerty));
            let mut sess = ImeSession::new(hangul, latin);

            // Merge user abbreviations from `<config>/abbreviations.toml` into
            // the built-in starter dictionary. On first launch (file absent),
            // we seed it with a commented-out sample so users have a template.
            let abbr_path = config_dir.join("abbreviations.toml");
            if !abbr_path.exists() {
                if !config_dir.exists() {
                    let _ = std::fs::create_dir_all(&config_dir);
                }
                let _ = std::fs::write(&abbr_path, ABBR_SAMPLE_FILE);
            }
            // Compose engine dict honoring the saved use_user_abbrs flag.
            // `build_merged_abbrs` pulls starter + (when enabled) user
            // file + learned_ngrams.toml in one place so startup, reset,
            // scan, and the 자동완성 관리 toggle all stay in sync.
            sess.use_user_abbrs = saved.use_user_abbrs;
            let merged = build_merged_abbrs(&config_dir, saved.use_user_abbrs);
            eprintln!(
                "sbmd: abbr engine seeded with {} entr(y/ies) (use_user_abbrs={})",
                merged.len(),
                saved.use_user_abbrs
            );
            sess.abbr.set_abbreviations(merged);
            sess.active_mode = InputMode::parse(&saved.input_mode).unwrap_or(InputMode::Hangul);
            sess.output_form = parse_output_form(&saved.output_form)
                .unwrap_or(OutputForm::NfcSyllable);
            let mode = parse_compose_mode(&saved.compose_mode).unwrap_or(
                if sess.hangul_layout.supports_moachigi() {
                    ComposeMode::Moachigi
                } else {
                    ComposeMode::Sequential
                },
            );
            sess.fsm.set_mode(mode);
            sess.suggestions_enabled = saved.suggestions_enabled;
            sess.backspace_mode = if saved.backspace_mode == "jamo" {
                "jamo".into()
            } else {
                "syllable".into()
            };

            // Canonicalize the file on disk — rewrites the settings
            // file after migration / fallback so corrupt legacy files
            // heal on first launch.
            persist_current(&sess, &config_dir);

            app.manage(ImeState(Mutex::new(sess)));
            app.manage(SettingsPath(config_dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            current_layout,
            set_layout,
            set_latin_layout,
            set_input_mode,
            set_output_form,
            set_compose_mode,
            set_suggestions_enabled,
            set_backspace_mode,
            ime_key_input,
            layout_map,
            list_abbreviations,
            apply_abbreviation,
            cancel_composition,
            cancel_abbr_match,
            flush,
            print_webview,
            pick_directory,
            pick_markdown_file,
            list_markdown_files,
            read_markdown_file,
            write_markdown_file,
            save_as_markdown,
            export_file,
            reset_settings,
            pick_and_load_abbr_dict,
            scan_and_build_ngram_dict,
            reset_abbr_dict,
            get_abbr_manage_info,
            set_use_user_abbrs,
            set_window_title,
            quit_app,
            open_url,
            is_dev_build,
            fs_create_file,
            fs_create_dir,
            fs_rename,
            fs_duplicate_file,
            fs_delete,
            fs_reveal,
            app_version,
            check_for_update,
            install_update,
            set_always_on_top,
            set_window_fullscreen,
            is_window_fullscreen,
            toggle_window_maximize,
            set_menu_check,
            get_menu_check,
            set_theme_check_exclusive,
            set_webview_zoom,
        ])
        .build(tauri::generate_context!())
        .expect("error while building 새별 마크다운 에디터")
        .run(|app_handle, event| {
            // ⌘Q · Dock "종료" · 메뉴 Quit 은 `WindowEvent::CloseRequested`
            // 가 아닌 `RunEvent::ExitRequested` 로 들어온다. X 버튼과 동일하게
            // prevent 후 프론트로 위임해, 저장 확인 후 `quit_app` 으로 다시
            // 들어와야만 실제로 빠져나간다.
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if FORCE_QUIT.load(Ordering::SeqCst) {
                    return;
                }
                api.prevent_exit();
                let _ = app_handle.emit("quit-requested", ());
            }
        });
}

#[cfg(test)]
mod integration_tests {
    //! Session-level tests that mirror the `ime_key_input` commit path
    //! without booting the Tauri runtime. If these fail, the in-app
    //! behavior fails too — no IPC or frontend masking.

    use super::*;

    /// Replicate the subset of `ime_key_input` that `LayoutOutput::Jamo`
    /// keys exercise: map the key, feed the FSM, mirror preedit into
    /// the abbr engine exactly like the live command path does.
    ///
    /// `within_chord` simulates the live handler's timing hint — the
    /// live handler derives it from `Instant::now()`, but tests inject
    /// it directly to stay deterministic.
    fn feed_key(
        sess: &mut ImeSession,
        code: KeyCode,
        shift: bool,
        within_chord: bool,
    ) -> (String, String) {
        let kev = KeyEvent {
            code,
            mods: if shift { Modifiers::SHIFT } else { Modifiers::NONE },
            repeat: false,
        };
        let (commit, preedit) = match sess.active_layout().map(&kev) {
            LayoutOutput::Jamo(input) => {
                let fsm_ev = sess.fsm.feed_with(input, FeedOptions { within_chord });
                let raw_commit = fsm_ev.commit_str().unwrap_or("").to_string();
                if !raw_commit.is_empty() {
                    let _ = sess.abbr.on_commit(&to_nfc_syllable(&raw_commit));
                }
                let preedit = to_display_text(&sess.fsm.preedit_string());
                sess.abbr.set_preedit(&to_nfc_syllable(&sess.fsm.preedit_string()));
                (raw_commit, preedit)
            }
            other => panic!("unexpected layout output: {:?}", other),
        };
        (commit, preedit)
    }

    /// When the user presses 4 keys "simultaneously" the OS may deliver
    /// them in a non-canonical order. For 원 on 세벌식 최종, the keys are
    /// J(ᄋ) 9(ᅮ) T(ᅥ) S(ᆫ); a common permutation is J→9→S→T (jong
    /// arrives before the 2nd compound-vowel component). With the chord
    /// hint set (simulating taps within ~50 ms), the FSM must still
    /// converge to 원.
    #[test]
    fn sebeolsik_final_moachigi_weon_simultaneous_j_9_s_t() {
        let mut sess = ImeSession::new(
            Box::new(SebeolsikFinal),
            Box::new(Qwerty),
        );
        sess.fsm.set_mode(ComposeMode::Moachigi);

        // First key has no predecessor, so within_chord is false.
        let _ = feed_key(&mut sess, KeyCode::KeyJ, false, false);
        // Subsequent keys arrive within the chord window.
        let _ = feed_key(&mut sess, KeyCode::Digit9, false, true);
        let _ = feed_key(&mut sess, KeyCode::KeyS, false, true); // jong arrives early
        let _ = feed_key(&mut sess, KeyCode::KeyT, false, true); // 2nd jung arrives last

        let nfc = to_nfc_syllable(&sess.fsm.preedit_string());
        assert_eq!(nfc, "원", "got {nfc}, preedit={:?}", sess.fsm.preedit_string());
    }

    #[test]
    fn sebeolsik_final_moachigi_weon_j_9_t_s() {
        // 세벌식 최종 + 모아치기 로 J(ᄋ) 9(ᅮ) T(ᅥ) S(ᆫ) 치면 '원'.
        let mut sess = ImeSession::new(
            Box::new(SebeolsikFinal),
            Box::new(Qwerty),
        );
        sess.fsm.set_mode(ComposeMode::Moachigi);

        let (c1, p1) = feed_key(&mut sess, KeyCode::KeyJ, false, false);
        assert_eq!(c1, "", "J should not commit");
        assert_eq!(p1, "ㅇ", "after J preedit should be ㅇ (compat), got {p1}");

        let (c2, p2) = feed_key(&mut sess, KeyCode::Digit9, false, true);
        assert_eq!(c2, "", "9 should not commit");
        assert_eq!(p2, "우", "after 9 preedit should be 우, got {p2}");

        let (c3, p3) = feed_key(&mut sess, KeyCode::KeyT, false, true);
        assert_eq!(c3, "", "T should not commit");
        assert_eq!(p3, "워", "after T preedit should be 워, got {p3}");

        let (c4, p4) = feed_key(&mut sess, KeyCode::KeyS, false, true);
        assert_eq!(c4, "", "S should not commit (still composing 원)");
        assert_eq!(p4, "원", "after S preedit should be 원, got {p4}");

        assert_eq!(to_nfc_syllable(&sess.fsm.preedit_string()), "원");
    }

    /// Deliberate sequential typing (slow) must NOT bleed the next
    /// vowel into a finished syllable. Without a chord hint the guard
    /// stays active: 곡 followed by ᅡ must read as 곡 + 아, not 곽.
    #[test]
    fn moachigi_without_chord_keeps_syllable_boundary() {
        let mut sess = ImeSession::new(
            Box::new(SebeolsikFinal),
            Box::new(Qwerty),
        );
        sess.fsm.set_mode(ComposeMode::Moachigi);

        // K(ᄀ) V(ᅩ) X(ᆨ) — completes 곡.
        let _ = feed_key(&mut sess, KeyCode::KeyK, false, false);
        let _ = feed_key(&mut sess, KeyCode::KeyV, false, false);
        let _ = feed_key(&mut sess, KeyCode::KeyX, false, false);
        // Then, after a pause (within_chord=false), F(ᅡ) arrives.
        let (commit, _) = feed_key(&mut sess, KeyCode::KeyF, false, false);
        // Expect: "곡" committed, fresh syllable started with bare ᅡ.
        // The ᅡ is stranded (no cho yet) in conjoining form, which is
        // the moachigi-intended behavior — *not* a silent compound
        // back into "곽".
        assert_eq!(to_nfc_syllable(&commit), "곡");
        assert_eq!(sess.fsm.preedit_string(), "\u{1161}");
    }
}
