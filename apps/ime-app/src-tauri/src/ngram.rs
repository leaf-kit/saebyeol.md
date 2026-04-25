//! Learn-from-documents autocomplete: scan a directory of `.txt` / `.md`
//! files, tokenize into 어절s (whitespace-separated tokens), build 1-,
//! 2-, and 3-gram phrases with frequency, and write them as a TOML
//! abbreviation dict that the existing loader can re-read on startup.
//!
//! Registration scheme:
//!   - 1-gram `W`          → trigger `W`,       body `W`           (prefix-match completion)
//!   - 2-gram `A B`        → trigger `A`,       body `A B`         (complete to 2-word phrase)
//!   - 3-gram `A B C`      → trigger `A B`,     body `A B C`       (complete to 3-word phrase)
//!
//! The TriggerEvent is always `Explicit` (Tab) to prevent false-positive
//! expansions. Priority = observed frequency clamped to [1, 200].

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use lib_ime::{Abbreviation, Trigger, TriggerEvent};

/// Minimum occurrences for an n-gram to be kept. Singletons are noise.
const MIN_FREQ: u32 = 2;
/// Cap on number of learned entries. Avoids enormous dicts + file sizes
/// when scanning a large corpus — top-N by frequency.
const MAX_ENTRIES: usize = 5000;
/// Priority cap (engine range is 1..=10000; 200 keeps learned entries
/// from overwhelming hand-curated 100-priority ones).
const MAX_PRIORITY: u32 = 200;

/// Strip leading/trailing punctuation and quote-like chars — so "안녕하세요."
/// becomes "안녕하세요" for indexing. Cleaner triggers, higher hit rate.
fn clean_token(tok: &str) -> &str {
    let is_junk = |c: char| -> bool {
        c.is_ascii_punctuation()
            || matches!(
                c,
                '。' | '、' | '，' | '．' | '！' | '？' | '：' | '；'
                    | '·' | '…' | '‥'
                    // U+201C / U+201D curly double quotes
                    | '\u{201C}' | '\u{201D}'
                    // U+2018 / U+2019 curly single quotes (written as
                    // escapes because the raw glyphs collide with Rust's
                    // char-literal delimiter)
                    | '\u{2018}' | '\u{2019}'
                    | '「' | '」' | '『' | '』'
                    | '（' | '）' | '〔' | '〕' | '［' | '］' | '｛' | '｝'
            )
    };
    tok.trim_matches(is_junk)
}

/// Whether a cleaned token is worth indexing. Rejects empty strings and
/// tokens made of only digits/punctuation (no letter / CJK).
fn is_useful(tok: &str) -> bool {
    if tok.is_empty() {
        return false;
    }
    tok.chars().any(|c| c.is_alphabetic())
}

/// Recursively walk `dir`, invoking `f` for every `.txt` / `.md` /
/// `.markdown` file discovered. Hidden directories (leading `.`) are
/// skipped so we don't recurse into `.git`, vendor caches, etc.
fn walk_txt_md<F: FnMut(&Path)>(dir: &Path, f: &mut F) -> io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let ft = entry.file_type()?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') {
            continue;
        }
        if ft.is_dir() {
            let _ = walk_txt_md(&path, f);
        } else if ft.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let lower = ext.to_ascii_lowercase();
                if matches!(lower.as_str(), "txt" | "md" | "markdown" | "mdx") {
                    f(&path);
                }
            }
        }
    }
    Ok(())
}

/// Result of a scan: the learned abbreviations plus a few stats.
pub struct ScanResult {
    pub abbrs: Vec<Abbreviation>,
    pub files_scanned: usize,
    pub tokens_read: usize,
}

/// Scan `dir` and return learned-ngram abbreviations.
pub fn scan_dir(dir: &Path) -> io::Result<ScanResult> {
    // Keyed by (trigger, body) → frequency.
    let mut freq: HashMap<(String, String), u32> = HashMap::new();
    let mut files = 0usize;
    let mut tokens_total = 0usize;

    walk_txt_md(dir, &mut |path| {
        let Ok(text) = std::fs::read_to_string(path) else { return; };
        files += 1;
        // Tokenize into 어절: whitespace-split, then strip trailing
        // punctuation. Drop tokens that are empty / pure-symbol.
        let tokens: Vec<&str> = text
            .split_whitespace()
            .map(clean_token)
            .filter(|t| is_useful(t))
            .collect();
        tokens_total += tokens.len();

        // 1-grams: self-echo (for prefix completion).
        for t in &tokens {
            if t.chars().count() < 2 {
                // Skip single-char tokens — too noisy for completion.
                continue;
            }
            let key = (t.to_string(), t.to_string());
            *freq.entry(key).or_insert(0) += 1;
        }
        // 2-grams: trigger = first word, body = "first second".
        for w in tokens.windows(2) {
            let body = format!("{} {}", w[0], w[1]);
            let key = (w[0].to_string(), body);
            *freq.entry(key).or_insert(0) += 1;
        }
        // 3-grams: trigger = "first second", body = "first second third".
        for w in tokens.windows(3) {
            let trigger = format!("{} {}", w[0], w[1]);
            let body = format!("{} {} {}", w[0], w[1], w[2]);
            let key = (trigger, body);
            *freq.entry(key).or_insert(0) += 1;
        }
    })?;

    // Drop singletons + cap to MAX_ENTRIES by frequency desc.
    let mut filtered: Vec<((String, String), u32)> = freq
        .into_iter()
        .filter(|(_, f)| *f >= MIN_FREQ)
        .collect();
    filtered.sort_by(|a, b| b.1.cmp(&a.1));
    if filtered.len() > MAX_ENTRIES {
        filtered.truncate(MAX_ENTRIES);
    }

    let abbrs: Vec<Abbreviation> = filtered
        .into_iter()
        .map(|((trigger, body), f)| Abbreviation {
            id: format!("learned:{trigger}::{body}"),
            trigger: Trigger::Literal(trigger),
            body,
            trigger_on: TriggerEvent::Explicit,
            priority: f.clamp(1, MAX_PRIORITY),
        })
        .collect();

    Ok(ScanResult {
        abbrs,
        files_scanned: files,
        tokens_read: tokens_total,
    })
}

/// Serialize `abbrs` into TOML that the existing `load_user_abbrs`
/// loader can parse. Matches the `[[abbr]]` schema defined by
/// `abbr::loader::AbbrEntry`.
pub fn to_toml(abbrs: &[Abbreviation]) -> String {
    let mut out = String::new();
    out.push_str(
        "# 자동-생성된 학습 n-gram 사전 — 손으로 고치지 마세요.\n\
         # 파일 메뉴 → 자동완성 학습 폴더 선택… 을 다시 실행하면\n\
         # 새 내용으로 덮어 씌워집니다.\n\n",
    );
    for a in abbrs {
        let trigger_str = match &a.trigger {
            Trigger::Literal(s) | Trigger::Ending(s) => s.clone(),
            Trigger::ChoSeq(cs) => cs
                .iter()
                .filter_map(|c| char::from_u32(*c))
                .collect::<String>(),
        };
        let kind = match &a.trigger {
            Trigger::Literal(_) => "literal",
            Trigger::Ending(_) => "ending",
            Trigger::ChoSeq(_) => "cho_seq",
        };
        let trigger_on = match a.trigger_on {
            TriggerEvent::Immediate => "immediate",
            TriggerEvent::Space => "space",
            TriggerEvent::Enter => "enter",
            TriggerEvent::Punctuation => "punctuation",
            TriggerEvent::JongCompletion => "jong_completion",
            TriggerEvent::Explicit => "explicit",
        };
        out.push_str("[[abbr]]\n");
        out.push_str(&format!("id         = {}\n", toml_string(&a.id)));
        out.push_str(&format!("trigger    = {}\n", toml_string(&trigger_str)));
        out.push_str(&format!("kind       = \"{kind}\"\n"));
        out.push_str(&format!("body       = {}\n", toml_string(&a.body)));
        out.push_str(&format!("trigger_on = \"{trigger_on}\"\n"));
        out.push_str(&format!("priority   = {}\n\n", a.priority));
    }
    out
}

/// Path of the generated dict inside the app config dir.
pub fn dict_path(config_dir: &Path) -> PathBuf {
    config_dir.join("learned_ngrams.toml")
}

/// TOML basic-string encoding: wrap in quotes, escape `\`, `"`, control chars.
fn toml_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lib_ime::parse_abbr_toml;

    fn scan_text(tag: &str, text: &str) -> Vec<Abbreviation> {
        // Unique-per-test temp dir — cargo runs tests in parallel, and
        // a shared path would race.
        let tmp = std::env::temp_dir().join(format!("sbmd-ngram-test-{tag}"));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("a.md"), text).unwrap();
        let r = scan_dir(&tmp).unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
        r.abbrs
    }

    #[test]
    fn builds_2_and_3_grams_above_min_freq() {
        let text = "\
            안녕 하세요 반갑 습니다\n\
            안녕 하세요 반갑 습니다\n\
            안녕 하세요 반갑 습니다\n";
        let abbrs = scan_text("ngrams", text);
        // Expect 2-grams and 3-grams each seen 3x, priority 3.
        let has = |trigger: &str, body: &str| {
            abbrs.iter().any(|a| match &a.trigger {
                Trigger::Literal(t) => t == trigger && a.body == body,
                _ => false,
            })
        };
        assert!(has("안녕", "안녕 하세요"));
        assert!(has("하세요", "하세요 반갑"));
        assert!(has("안녕 하세요", "안녕 하세요 반갑"));
        assert!(has("하세요 반갑", "하세요 반갑 습니다"));
    }

    #[test]
    fn strips_trailing_punctuation() {
        let text = "안녕하세요. 안녕하세요. 반가워요!\n안녕하세요. 안녕하세요. 반가워요!";
        let abbrs = scan_text("punct", text);
        // Trigger should be "안녕하세요" not "안녕하세요."
        let found = abbrs.iter().any(|a| match &a.trigger {
            Trigger::Literal(t) => t == "안녕하세요",
            _ => false,
        });
        assert!(found, "trailing period not stripped");
    }

    #[test]
    fn serialized_toml_roundtrips() {
        let text = "알파 베타 감마\n알파 베타 감마\n";
        let abbrs = scan_text("roundtrip", text);
        assert!(!abbrs.is_empty());
        let toml = to_toml(&abbrs);
        let parsed = parse_abbr_toml(&toml).expect("round-trip parses");
        assert_eq!(parsed.len(), abbrs.len());
    }

    #[test]
    fn tokens_with_quotes_and_backslashes_roundtrip() {
        let abbrs = vec![Abbreviation {
            id: "t-1".into(),
            trigger: Trigger::Literal("he said".into()),
            body: r#"he said "hi\there""#.into(),
            trigger_on: TriggerEvent::Explicit,
            priority: 5,
        }];
        let toml = to_toml(&abbrs);
        let parsed = parse_abbr_toml(&toml).expect("escaping round-trips");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].body, r#"he said "hi\there""#);
    }
}
