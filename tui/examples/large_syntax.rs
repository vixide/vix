//! Measure Tree-sitter on very large files (T830): open, first highlighted
//! frame, one keystroke and resident memory for a generated Rust file of a
//! given size.
//!
//! Run with: `cargo run --release --example large_syntax -p vix-tui -- <MiB> [plain]`
//!
//! `plain` opens the same text as `.txt` (no grammar), so the difference in
//! resident memory is what the parse tree costs. Each size runs in its own
//! process so the memory numbers do not mix. The text is deterministic: the
//! same size always gives the same bytes.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]
#![warn(clippy::pedantic)]

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use ratatui::{Terminal, backend::TestBackend};
use vix::app::App;
use vix::settings::Settings;
use vix_input::{KeyCode, KeyEvent, KeyModifiers};

/// One module's worth of real-looking Rust: a doc comment, a struct, an impl
/// with a `match`, a string with an escape, a trait impl and a test.
fn chunk(out: &mut String, i: usize) {
    write!(
        out,
        r#"/// A record of kind {i}, read from the wire.
#[derive(Debug, Clone, PartialEq)]
pub struct Record{i} {{
    /// The identifier.
    pub id: u64,
    name: String,
    tags: Vec<&'static str>,
}}

impl Record{i} {{
    /// Parse one line such as `"{i}:name:a,b"`.
    pub fn parse(line: &str) -> Result<Self, String> {{
        let mut parts = line.splitn(3, ':');
        let id = parts.next().ok_or("no id")?.parse::<u64>().map_err(|e| e.to_string())?;
        let name = parts.next().unwrap_or_default().to_owned();
        match parts.next() {{
            Some(t) if !t.is_empty() => Ok(Self {{ id, name, tags: vec!["{i}", "tagged"] }}),
            Some(_) | None => Err(format!("record {{id}} has no tags\n")),
        }}
    }}

    // Weight is a small arithmetic mix so numbers and operators appear.
    fn weight(&self) -> f64 {{
        (self.id as f64) * 1.5 + {i}.0 / (self.tags.len() as f64 + 1.0)
    }}
}}

impl std::fmt::Display for Record{i} {{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{
        write!(f, "{{}}={{:.2}}", self.name, self.weight())
    }}
}}

#[cfg(test)]
mod tests_{i} {{
    use super::*;
    #[test]
    fn parses() {{
        assert!(Record{i}::parse("1:x:y").is_ok());
    }}
}}

"#
    )
    .expect("writing to a String cannot fail");
}

/// Whole modules of generated Rust, as many as fit in `bytes`.
fn source(bytes: usize) -> String {
    let mut out = String::with_capacity(bytes + 2048);
    for i in 0.. {
        let before = out.len();
        chunk(&mut out, i);
        if out.len() > bytes {
            out.truncate(before);
            break;
        }
    }
    out
}

fn mib(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss, reason = "a report, not arithmetic")]
    let m = bytes as f64 / f64::from(1 << 20);
    format!("{m:.1} MiB")
}

fn rss() -> u64 {
    vix_system_information_panel::resident_memory().unwrap_or(0)
}

/// Draw one frame and return how long it took.
fn frame(term: &mut Terminal<TestBackend>, app: &mut App) -> Duration {
    let t = Instant::now();
    term.draw(|f| vix::ui::draw(app, f)).unwrap();
    t.elapsed()
}

/// Whether the visible rows are syntax-colored: the distinct foreground
/// colors of the text cells (plain text has one or two: text and gutter).
fn highlighted(term: &Terminal<TestBackend>) -> bool {
    let buf = term.backend().buffer();
    let mut colors = std::collections::HashSet::new();
    for y in 3..buf.area.height.saturating_sub(3) {
        for x in 0..buf.area.width {
            let c = &buf[(x, y)];
            if c.symbol().chars().any(char::is_alphanumeric) {
                colors.insert(c.fg);
            }
        }
    }
    colors.len() > 3
}

/// Poll until the background parse is installed; the wall time it took.
fn wait_parse(app: &mut App) -> Duration {
    let t = Instant::now();
    while app.parse_busy() {
        app.poll_parse();
        std::thread::sleep(Duration::from_micros(200));
    }
    app.poll_parse();
    t.elapsed()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let size_mib: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(1);
    let plain = args.next().as_deref() == Some("plain");
    let text = source(size_mib << 20);
    let lines = text.lines().count();
    let dir = std::env::temp_dir().join(format!("vix-large-syntax-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dir = std::fs::canonicalize(&dir).unwrap();
    let file = dir.join(if plain { "big.txt" } else { "big.rs" });
    std::fs::write(&file, &text).unwrap();
    drop(text);
    let mut settings = Settings::default();
    // No "open anyway?" question for the 64 MiB case.
    settings.editor_behavior.large_file_mb = 0;
    let mut app = App::new(dir.clone(), settings);
    let mut term = Terminal::new(TestBackend::new(200, 50)).unwrap();
    frame(&mut term, &mut app);
    let rss_before = rss();

    let t = Instant::now();
    app.open_initial(&file);
    let open = t.elapsed();
    let first_frame = frame(&mut term, &mut app);
    let first_lit = highlighted(&term);
    let parse = wait_parse(&mut app);
    let lit_frame = frame(&mut term, &mut app);
    let lit = highlighted(&term);
    let steady = frame(&mut term, &mut app);
    let rss_open = rss();

    // Move to the middle of the file and type there.
    if let Some(tab) = app.editor.active_tab_mut() {
        let mid = tab.editor.code_ref().len_chars() / 2;
        tab.editor.set_cursor(mid);
    }
    frame(&mut term, &mut app);
    wait_parse(&mut app);
    let key = |c| KeyEvent::new(c, KeyModifiers::NONE);
    let mut keys = Vec::new();
    let mut key_frames = Vec::new();
    let mut reparses = Vec::new();
    for round in 0..5 {
        let code = if round % 2 == 0 {
            KeyCode::Char('x')
        } else {
            KeyCode::Backspace
        };
        let t = Instant::now();
        app.on_key(key(code));
        keys.push(t.elapsed());
        key_frames.push(frame(&mut term, &mut app));
        reparses.push(wait_parse(&mut app));
        frame(&mut term, &mut app);
    }
    let rss_end = rss();
    let median = |v: &mut Vec<Duration>| {
        v.sort();
        v[v.len() / 2]
    };
    println!(
        "size={size_mib}MiB lines={lines} plain={plain}\n\
         open_initial={open:.2?} first_frame={first_frame:.2?} (highlighted: {first_lit})\n\
         full_parse_wall={parse:.2?} first_highlighted_frame={lit_frame:.2?} (highlighted: {lit}) steady_frame={steady:.2?}\n\
         keystroke(on_key)={:.2?} frame_after_key={:.2?} reparse_wall={:.2?}\n\
         rss before={} after_open={} end={} (open added {})",
        median(&mut keys),
        median(&mut key_frames),
        median(&mut reparses),
        mib(rss_before),
        mib(rss_open),
        mib(rss_end),
        mib(rss_open.saturating_sub(rss_before)),
    );
    let _ = std::fs::remove_dir_all(&dir);
}
