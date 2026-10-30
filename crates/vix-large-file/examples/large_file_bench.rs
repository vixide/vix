//! The Large File Viewer benchmark (T733): generate a big log-like file, then
//! open it headlessly and time what a user waits for.
//!
//! ```sh
//! cargo run --release -p vix-large-file --example large_file_bench -- generate FILE GIB
//! cargo run --release -p vix-large-file --example large_file_bench -- measure FILE
//! ```
//!
//! `scripts/bench-large-file` runs both for 1 GiB and 4 GiB under
//! `/usr/bin/time` (for peak RSS) and deletes the files afterward.

use std::error::Error;
use std::io::Write as _;
use std::path::Path;
use std::time::{Duration, Instant};

use vix_input::{KeyCode, KeyEvent, KeyModifiers};
use vix_large_file::{Found, Search, Target, Viewer};

/// The rows and columns of the headless "screen".
const ROWS: usize = 50;
const COLS: usize = 200;

/// The line `measure` searches for: the file's last line.
const NEEDLE: &str = "VIX-BENCH-NEEDLE-AT-THE-END";

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [mode, path, gib] if mode == "generate" => generate(Path::new(path), gib.parse()?),
        [mode, path] if mode == "measure" => measure(Path::new(path)),
        _ => Err("usage: large_file_bench generate FILE GIB | measure FILE".into()),
    }
}

/// Write `gib` GiB of ~100-byte log lines to `path`, ending with [`NEEDLE`].
fn generate(path: &Path, gib: u64) -> Result<(), Box<dyn Error>> {
    let target = gib * 1024 * 1024 * 1024;
    let mut out = std::io::BufWriter::with_capacity(8 << 20, std::fs::File::create(path)?);
    let mut written = 0u64;
    let mut line = Vec::with_capacity(160);
    let mut i = 0u64;
    while written < target {
        line.clear();
        writeln!(
            line,
            "2026-10-30T{:02}:{:02}:{:02}.{:03}Z INFO  [worker-{:02}] request id={i:010} path=/api/v1/items/{} status=200 bytes={}",
            (i / 3_600_000) % 24,
            (i / 60_000) % 60,
            (i / 1000) % 60,
            i % 1000,
            i % 16,
            i % 100_000,
            (i * 7919) % 100_000,
        )?;
        out.write_all(&line)?;
        written += line.len() as u64;
        i += 1;
    }
    writeln!(out, "{NEEDLE}")?;
    out.flush()?;
    println!(
        "generated {} ({written} bytes, {} lines)",
        path.display(),
        i + 1
    );
    Ok(())
}

fn ms(d: Duration) -> String {
    format!("{:.1} ms", d.as_secs_f64() * 1000.0)
}

/// Open `path` in a [`Viewer`] and time the first screen, goto-end, the full
/// index, a goto into the middle, and a whole-file search.
fn measure(path: &Path) -> Result<(), Box<dyn Error>> {
    let len = std::fs::metadata(path)?.len();
    let start = Instant::now();
    let mut v = Viewer::open(path)?;
    let rows = v.view(ROWS, COLS);
    let first_screen = start.elapsed();
    if rows.len() != ROWS {
        return Err("first screen is short".into());
    }

    let t = Instant::now();
    v.handle_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE), ROWS);
    let rows = v.view(ROWS, COLS);
    let goto_end = t.elapsed();
    if !rows.last().is_some_and(|r| r.text.starts_with(NEEDLE)) {
        return Err("goto end did not reach the last line".into());
    }
    let indexed_at_goto_end = v.index().percent();

    v.index_mut().wait();
    let full_index = start.elapsed();
    let lines = v.index().total_lines().unwrap_or(0);

    let t = Instant::now();
    v.goto(Target::Line(lines / 2));
    v.view(ROWS, COLS);
    let goto_middle = t.elapsed();
    if v.status().line != Some(lines / 2) {
        return Err("goto middle landed elsewhere".into());
    }

    let t = Instant::now();
    let search = Search::start(
        path,
        len,
        NEEDLE.as_bytes(),
        0,
        vix_large_file::Direction::Forward,
    )?;
    let found = search.wait();
    let search_all = t.elapsed();
    if !matches!(found, Found::At(_)) {
        return Err("search did not find the needle".into());
    }

    let gib = f64::from(u32::try_from(len >> 20).unwrap_or(u32::MAX)) / 1024.0;
    println!(
        "file:                 {} ({len} bytes, {gib:.2} GiB, {lines} lines)",
        path.display()
    );
    println!("time to first screen: {}", ms(first_screen));
    println!(
        "goto end:             {} (index {indexed_at_goto_end}% done at the time)",
        ms(goto_end)
    );
    println!("time to full index:   {}", ms(full_index));
    println!("goto middle line:     {}", ms(goto_middle));
    println!("search whole file:    {}", ms(search_all));
    println!("checkpoints:          {}", v.index().checkpoints().len());
    Ok(())
}
