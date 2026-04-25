//! Interactive terminal demo for the 새별 마크다운 에디터 (sbmd) input engine.
//!
//! Usage:
//!   `ime-demo`                    — launch the interactive TUI
//!   `ime-demo --script "dkssud"`  — run the string through Dubeolsik
//!                                   and print the composed result
//!   `ime-demo --layout sebeolsik --script "bj"`
//!                                 — same, using Sebeolsik 390
//!
//! Interactive controls (shown in the footer):
//!   * `Tab`         cycle through available layouts
//!   * `F2`          toggle output form (Conjoining / NFC / Compat)
//!   * `Esc`         cancel current composition
//!   * `Backspace`   delete one Jamo (or one committed character when idle)
//!   * `Enter`       flush preedit + newline
//!   * `Ctrl-C`      exit

use std::io::{self, Write};

use crossterm::cursor::{MoveTo, Show};
use crossterm::event::{self, Event, KeyCode as CtKey, KeyEventKind, KeyModifiers};
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, queue};

use lib_ime::{
    to_nfc_syllable, Dubeolsik, FsmEvent, HangulFsm, KeyCode, KeyEvent, Layout, LayoutOutput,
    Modifiers, OutputForm, Sebeolsik390, SebeolsikFinal,
};

struct LayoutSlot {
    label: &'static str,
    layout: Box<dyn Layout>,
}

fn default_layouts() -> Vec<LayoutSlot> {
    vec![
        LayoutSlot {
            label: "두벌식 표준",
            layout: Box::new(Dubeolsik),
        },
        LayoutSlot {
            label: "세벌식 390",
            layout: Box::new(Sebeolsik390),
        },
        LayoutSlot {
            label: "세벌식 최종",
            layout: Box::new(SebeolsikFinal),
        },
    ]
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let layouts = default_layouts();

    // Parse CLI flags manually to avoid pulling in clap for a demo.
    let mut layout_idx = 0usize;
    let mut script: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            "--layout" => {
                let name = args.get(i + 1).cloned().unwrap_or_default();
                layout_idx = match name.as_str() {
                    "dubeolsik" | "두벌식" => 0,
                    "sebeolsik" | "sebeolsik-390" | "세벌식" => 1,
                    "sebeolsik-final" | "세벌식-최종" | "최종" => 2,
                    other => {
                        eprintln!("unknown --layout {other:?}");
                        return Ok(());
                    }
                };
                i += 2;
            }
            "--script" => {
                script = args.get(i + 1).cloned();
                i += 2;
            }
            other => {
                eprintln!("unknown argument {other:?}");
                print_help();
                return Ok(());
            }
        }
    }

    if let Some(s) = script {
        run_script(&layouts[layout_idx], &s);
        return Ok(());
    }

    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen)?;
    let result = run(&mut stdout, &layouts);
    execute!(stdout, LeaveAlternateScreen, Show, ResetColor)?;
    terminal::disable_raw_mode()?;
    result
}

fn print_help() {
    println!(
        "새별 마크다운 에디터 (sbmd) terminal demo\n\
         \n\
         USAGE:\n    \
             ime-demo [--layout <name>] [--script <text>]\n\
         \n\
         FLAGS:\n    \
             --layout dubeolsik|sebeolsik   Select layout (default: dubeolsik)\n    \
             --script <text>                Run the given ASCII string through the\n                                    \
                                  layout instead of launching the TUI\n    \
             -h, --help                     Show this help\n\
         \n\
         EXAMPLES:\n    \
             ime-demo --script dkssudgktpdy                # → 안녕하세요\n    \
             ime-demo --layout sebeolsik --script bj       # → 가"
    );
}

/// Non-interactive mode: render `input` through the given layout and
/// print the composed result. Handy as a smoke test and for users on
/// non-TTY environments (CI, remote execution).
fn run_script(slot: &LayoutSlot, input: &str) {
    let mut fsm = HangulFsm::new();
    let mut out = String::new();
    for ch in input.chars() {
        if ch == ' ' {
            if let FsmEvent::Commit(s) = fsm.flush() {
                out.push_str(&s);
            }
            out.push(' ');
        } else if let Some((code, shift)) = map_char_to_keycode(ch) {
            let mods = if shift { Modifiers::SHIFT } else { Modifiers::NONE };
            let kev = KeyEvent { code, mods, repeat: false };
            match slot.layout.map(&kev) {
                LayoutOutput::Jamo(input) => {
                    let ev = fsm.feed(input);
                    if let Some(s) = ev.commit_str() {
                        out.push_str(s);
                    }
                }
                LayoutOutput::Char(c) => {
                    if let FsmEvent::Commit(s) = fsm.flush() {
                        out.push_str(&s);
                    }
                    out.push(c);
                }
                LayoutOutput::Passthrough | LayoutOutput::None => {
                    if let FsmEvent::Commit(s) = fsm.flush() {
                        out.push_str(&s);
                    }
                    out.push(ch);
                }
            }
        } else {
            if let FsmEvent::Commit(s) = fsm.flush() {
                out.push_str(&s);
            }
            out.push(ch);
        }
    }
    if let FsmEvent::Commit(s) = fsm.flush() {
        out.push_str(&s);
    }
    println!("Layout: {}", slot.label);
    println!("Input:  {input}");
    println!("Output: {}", to_nfc_syllable(&out));
}

fn run<W: Write>(out: &mut W, layouts: &[LayoutSlot]) -> io::Result<()> {
    let mut fsm = HangulFsm::new();
    let mut committed = String::new();
    let mut layout_idx = 0usize;
    let mut form = OutputForm::NfcSyllable;

    draw(out, layouts, layout_idx, &committed, &fsm, form)?;

    loop {
        let ev = match event::read()? {
            Event::Key(k) if k.kind == KeyEventKind::Press || k.kind == KeyEventKind::Repeat => k,
            Event::Resize(_, _) => {
                draw(out, layouts, layout_idx, &committed, &fsm, form)?;
                continue;
            }
            _ => continue,
        };

        // Exit shortcuts first.
        if ev.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(ev.code, CtKey::Char('c' | 'q'))
        {
            break;
        }

        match ev.code {
            CtKey::Tab => {
                layout_idx = (layout_idx + 1) % layouts.len();
                let _ = fsm.flush_and_collect(&mut committed, form);
            }
            CtKey::F(2) => {
                form = next_form(form);
            }
            CtKey::Esc => {
                fsm.cancel();
            }
            CtKey::Backspace => {
                if fsm.is_composing() {
                    let _ = fsm.backspace();
                } else {
                    pop_last_grapheme(&mut committed);
                }
            }
            CtKey::Enter => {
                fsm.flush_and_collect(&mut committed, form);
                committed.push('\n');
            }
            CtKey::Char(' ') => {
                fsm.flush_and_collect(&mut committed, form);
                committed.push(' ');
            }
            CtKey::Char(ch) => {
                if let Some((lib_code, shift)) = map_char_to_keycode(ch) {
                    let mods = Modifiers {
                        shift: shift || ev.modifiers.contains(KeyModifiers::SHIFT),
                        ctrl: ev.modifiers.contains(KeyModifiers::CONTROL),
                        alt: ev.modifiers.contains(KeyModifiers::ALT),
                        altgr: false,
                        meta: ev.modifiers.contains(KeyModifiers::SUPER),
                    };
                    let kev = KeyEvent {
                        code: lib_code,
                        mods,
                        repeat: ev.kind == KeyEventKind::Repeat,
                    };
                    match layouts[layout_idx].layout.map(&kev) {
                        LayoutOutput::Jamo(input) => {
                            let fsm_ev = fsm.feed(input);
                            append_commit(&mut committed, &fsm_ev, form);
                        }
                        LayoutOutput::Char(c) => {
                            fsm.flush_and_collect(&mut committed, form);
                            committed.push(c);
                        }
                        LayoutOutput::Passthrough => {
                            fsm.flush_and_collect(&mut committed, form);
                            committed.push(ch);
                        }
                        LayoutOutput::None => {}
                    }
                }
            }
            _ => {}
        }

        draw(out, layouts, layout_idx, &committed, &fsm, form)?;
    }

    Ok(())
}

fn append_commit(buf: &mut String, ev: &FsmEvent, form: OutputForm) {
    if let Some(s) = ev.commit_str() {
        buf.push_str(&form.render(s));
    }
}

/// Convenience extension used by the demo.
trait FsmDemoExt {
    fn flush_and_collect(&mut self, buf: &mut String, form: OutputForm) -> bool;
}

impl FsmDemoExt for HangulFsm {
    fn flush_and_collect(&mut self, buf: &mut String, form: OutputForm) -> bool {
        match self.flush() {
            FsmEvent::Commit(s) => {
                buf.push_str(&form.render(&s));
                true
            }
            _ => false,
        }
    }
}

fn pop_last_grapheme(buf: &mut String) {
    // Good enough for demo purposes: pop one scalar. Grapheme-cluster
    // boundaries would require the `unicode-segmentation` crate.
    buf.pop();
}

fn next_form(f: OutputForm) -> OutputForm {
    match f {
        OutputForm::NfcSyllable => OutputForm::JamoConjoining,
        OutputForm::JamoConjoining => OutputForm::JamoCompat,
        OutputForm::JamoCompat => OutputForm::NfcSyllable,
    }
}

fn form_label(f: OutputForm) -> &'static str {
    match f {
        OutputForm::NfcSyllable => "NFC",
        OutputForm::JamoConjoining => "Conjoining",
        OutputForm::JamoCompat => "Compat",
    }
}

fn draw<W: Write>(
    out: &mut W,
    layouts: &[LayoutSlot],
    layout_idx: usize,
    committed: &str,
    fsm: &HangulFsm,
    form: OutputForm,
) -> io::Result<()> {
    queue!(out, Clear(ClearType::All), MoveTo(0, 0))?;

    // Header.
    queue!(
        out,
        SetAttribute(Attribute::Bold),
        SetForegroundColor(Color::Cyan),
        Print("새별 마크다운 에디터 (sbmd) — Live Demo"),
        SetAttribute(Attribute::Reset),
        ResetColor,
    )?;
    let layout_name = layouts[layout_idx].label;
    let form_name = form_label(form);
    queue!(
        out,
        Print(format!(
            "     Layout: {layout_name}   Output: {form_name}\r\n"
        )),
    )?;

    queue!(
        out,
        SetForegroundColor(Color::DarkGrey),
        Print("─".repeat(72)),
        ResetColor,
        Print("\r\n\r\n"),
    )?;

    // Committed buffer.
    for line in committed.split('\n') {
        queue!(out, Print(line), Print("\r\n"))?;
    }

    // Preedit with underline.
    let preedit_raw = fsm.preedit_string();
    if !preedit_raw.is_empty() {
        let rendered = to_nfc_syllable(&preedit_raw);
        queue!(
            out,
            SetAttribute(Attribute::Underlined),
            SetForegroundColor(Color::Yellow),
            Print(rendered),
            SetAttribute(Attribute::Reset),
            ResetColor,
        )?;
    }

    // Footer.
    let (_, rows) = terminal::size().unwrap_or((80, 24));
    queue!(out, MoveTo(0, rows.saturating_sub(2)))?;
    queue!(
        out,
        SetForegroundColor(Color::DarkGrey),
        Print("─".repeat(72)),
        Print("\r\n"),
        ResetColor,
    )?;
    queue!(
        out,
        SetForegroundColor(Color::DarkCyan),
        Print("[Tab] 레이아웃  [F2] 출력형식  [Esc] 조합취소  [Backspace] 자모지움  [Ctrl-C] 종료"),
        ResetColor,
    )?;

    out.flush()
}

/// Map a crossterm `Char` event to a physical `KeyCode` + implicit shift.
///
/// crossterm delivers the uppercase letter when Shift is held, so the
/// case of `ch` also carries the shift signal for letter keys.
#[allow(clippy::too_many_lines)]
fn map_char_to_keycode(ch: char) -> Option<(KeyCode, bool)> {
    let (lower, shift) = if ch.is_ascii_uppercase() {
        (ch.to_ascii_lowercase(), true)
    } else {
        (ch, false)
    };
    let code = match lower {
        'a' => KeyCode::KeyA, 'b' => KeyCode::KeyB, 'c' => KeyCode::KeyC,
        'd' => KeyCode::KeyD, 'e' => KeyCode::KeyE, 'f' => KeyCode::KeyF,
        'g' => KeyCode::KeyG, 'h' => KeyCode::KeyH, 'i' => KeyCode::KeyI,
        'j' => KeyCode::KeyJ, 'k' => KeyCode::KeyK, 'l' => KeyCode::KeyL,
        'm' => KeyCode::KeyM, 'n' => KeyCode::KeyN, 'o' => KeyCode::KeyO,
        'p' => KeyCode::KeyP, 'q' => KeyCode::KeyQ, 'r' => KeyCode::KeyR,
        's' => KeyCode::KeyS, 't' => KeyCode::KeyT, 'u' => KeyCode::KeyU,
        'v' => KeyCode::KeyV, 'w' => KeyCode::KeyW, 'x' => KeyCode::KeyX,
        'y' => KeyCode::KeyY, 'z' => KeyCode::KeyZ,
        '0' => KeyCode::Digit0, '1' => KeyCode::Digit1, '2' => KeyCode::Digit2,
        '3' => KeyCode::Digit3, '4' => KeyCode::Digit4, '5' => KeyCode::Digit5,
        '6' => KeyCode::Digit6, '7' => KeyCode::Digit7, '8' => KeyCode::Digit8,
        '9' => KeyCode::Digit9,
        '-' => KeyCode::Minus, '=' => KeyCode::Equal,
        '[' => KeyCode::BracketLeft, ']' => KeyCode::BracketRight,
        '\\' => KeyCode::Backslash,
        ';' => KeyCode::Semicolon, '\'' => KeyCode::Quote,
        ',' => KeyCode::Comma, '.' => KeyCode::Period, '/' => KeyCode::Slash,
        '`' => KeyCode::Backquote,
        _ => return None,
    };
    Some((code, shift))
}
