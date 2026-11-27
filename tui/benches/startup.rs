//! Cold-start and project-open cost.
//!
//! Two kinds of measurement live here:
//!
//! - **Criterion micro-benchmarks** (plan T121/T122): `startup/app_new`
//!   (`App::new` — theme scan, editor/menu/LSP-client setup, everything on
//!   the path before the first frame can draw) and `startup/refresh_git`
//!   (three `git` subprocesses — repo?/branch/status — deliberately moved
//!   *off* that path in `main.rs`), benchmarked separately since T122's fix
//!   is exactly that these two are no longer paid at the same point.
//! - **The large-checkout open-time exercise** (T823): a deterministic
//!   synthetic checkout (20,000 files by default) is generated in a temp
//!   directory and opened again and again with a fresh `App` each run,
//!   measuring *time to first editable frame* and *time to first Quick Open
//!   result*, once through the local authority and once through the
//!   in-process `FakeAuthority` (a remote authority with zero transport
//!   latency). It prints median / p90 / min / max / sample count together
//!   with the tool versions, workspace size, enabled capabilities and load
//!   average. *First semantic definition* and *first actionable diagnostic*
//!   need a real language server, so they run only when `VIX_BENCH_LSP=1`
//!   and a working `rust-analyzer` is found; the default run starts no
//!   server (language servers are switched off in its settings).
//!
//! Where every interval starts and ends is spelled out on each measuring
//! function below and in `docs/performance/index.md` (§ Opening a large
//! checkout), which also holds the numbers this produced.
//!
//! Knobs (environment): `VIX_BENCH_OPEN_FILES` (default 20000),
//! `VIX_BENCH_OPEN_RUNS` (default 10), `VIX_BENCH_LSP=1`,
//! `VIX_BENCH_LSP_RUNS` (default 3), `VIX_BENCH_RUST_ANALYZER` (the server
//! program, default `rust-analyzer`). A filter argument selects parts the
//! way Criterion's does: `cargo bench --bench startup -- open` runs only the
//! open-time exercise, `-- startup/` only the micro-benchmarks.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use criterion::Criterion;
use ratatui::{Terminal, backend::TestBackend};
use std::fmt::Write as _;
use std::fs;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};
use vix::app::App;
use vix::authority::{Authority, FakeAuthority};
use vix::cli::Cli;
use vix::settings::{LspServer, Settings};
use vix_input::{KeyCode, KeyEvent, KeyModifiers};

/// A fresh, empty fixture directory — cleared first so a leftover `.git`
/// from a prior run never leaks between benchmark runs.
fn fixture_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vix-bench-startup-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create fixture directory");
    dir
}

/// Short Criterion settings: the machine this runs on is often busy, and
/// these are indicative numbers, not a gate.
fn criterion() -> Criterion {
    Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .sample_size(10)
        .configure_from_args()
}

/// `App::new`: theme-directory scan, editor/menu/LSP-client setup — real
/// work, but never touches `git` (that's `refresh_git`, benchmarked
/// separately below, and — since T122 — no longer called before the first
/// frame at all).
fn bench_app_new(c: &mut Criterion) {
    let dir = fixture_dir("app-new");
    c.bench_function("startup/app_new", |b| {
        b.iter(|| black_box(App::new(black_box(dir.clone()), Settings::default())));
    });
}

/// `refresh_git` against a real (if minimal) repository, so the benchmark
/// exercises the same three-subprocess path (repo?/branch/status) a real
/// workspace does, not the cheap "not a repo" short-circuit. Skipped with a
/// note if `git` itself isn't on `PATH` — this is a developer-run benchmark,
/// not a CI gate (`scripts/check` never runs `cargo bench`).
fn bench_refresh_git(c: &mut Criterion) {
    let dir = fixture_dir("refresh-git");
    let init = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&dir)
        .status();
    if !init.is_ok_and(|s| s.success()) {
        eprintln!("skipping startup/refresh_git: `git init` did not succeed");
        return;
    }
    let mut app = App::new(dir, Settings::default());
    c.bench_function("startup/refresh_git", |b| {
        b.iter(|| app.refresh_git());
    });
}

// ---------------------------------------------------------------------------
// The large-checkout open-time exercise (T823).
// ---------------------------------------------------------------------------

/// The Quick Open query typed in every run. Every generated crate has a
/// `lib.rs`, so it matches files wherever the file-index walk starts.
const QUICK_OPEN_QUERY: &str = "lib.rs";

/// The terminal size every measured frame is drawn into.
const SCREEN: (u16, u16) = (120, 40);

/// An environment knob as a number, or its default.
fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

/// A small deterministic generator (xorshift64), so the checkout is the same
/// bytes on every run and every machine.
struct Rng(u64);

impl Rng {
    /// The next value in `0..bound` (`bound` > 0).
    fn below(&mut self, bound: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0.checked_rem(bound).unwrap_or(0)
    }
}

/// What was generated, for the report.
struct Checkout {
    /// The checkout's root on the local disk.
    root: PathBuf,
    /// The file every run opens (relative to `root`).
    open: PathBuf,
    /// Regular files written.
    files: usize,
    /// Directories created (the root included).
    dirs: usize,
    /// Bytes written.
    bytes: u64,
}

/// A Rust-shaped source file of `items` functions.
fn rust_source(items: u64, salt: u64) -> String {
    let mut out = String::new();
    for i in 0..items {
        writeln!(
            out,
            "/// Item {i} of {salt}.\npub fn item_{i}(value: &str) -> usize {{\n    value.len().saturating_add({i})\n}}\n"
        )
        .expect("writing to a String cannot fail");
    }
    out
}

/// Writes the checkout and counts what it wrote.
struct Writer {
    /// The checkout root.
    root: PathBuf,
    /// Regular files written.
    files: usize,
    /// Bytes written.
    bytes: u64,
    /// Every directory below the root that holds a file.
    dirs: std::collections::BTreeSet<PathBuf>,
}

impl Writer {
    /// Write `text` to `rel`, creating its directories.
    fn put(&mut self, rel: &Path, text: &str) {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create checkout directory");
        }
        let mut dir = rel.parent();
        while let Some(d) = dir.filter(|d| !d.as_os_str().is_empty()) {
            self.dirs.insert(d.to_path_buf());
            dir = d.parent();
        }
        self.bytes = self
            .bytes
            .saturating_add(u64::try_from(text.len()).unwrap_or(u64::MAX));
        fs::write(path, text).expect("write checkout file");
        self.files = self.files.saturating_add(1);
    }
}

/// Write `files` source files under `root`, Cargo-workspace shaped: 100
/// crates (`crates/cNNN`), each with a `lib.rs` and modules nested two
/// directories deep, plus a root `Cargo.toml`, `README.md` and
/// `.gitignore`. Seeded, so the same `files` always gives the same tree.
fn generate_checkout(root: &Path, files: usize) -> Checkout {
    const CRATES: usize = 100;
    let _ = fs::remove_dir_all(root);
    fs::create_dir_all(root).expect("create checkout root");
    let mut rng = Rng(0x5eed_0823_u64);
    let mut w = Writer {
        root: root.to_path_buf(),
        files: 0,
        bytes: 0,
        dirs: std::collections::BTreeSet::new(),
    };
    w.put(
        Path::new("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*\"]\n",
    );
    w.put(
        Path::new("README.md"),
        "# Synthetic checkout (vix T823 bench)\n",
    );
    w.put(Path::new(".gitignore"), "/target\n");
    for c in 0..CRATES {
        // The opened file is the one large file a real project has.
        let items = if c == 0 { 400 } else { 20 };
        let rel = PathBuf::from(format!("crates/c{c:03}/src/lib.rs"));
        w.put(&rel, &rust_source(items, u64::try_from(c).unwrap_or(0)));
    }
    let mut i = 0_usize;
    while w.files < files {
        let krate = i.checked_rem(CRATES).unwrap_or(0);
        let module = rng.below(8);
        let sub = rng.below(4);
        let items = rng.below(30).saturating_add(1);
        let rel = PathBuf::from(format!(
            "crates/c{krate:03}/src/m{module}/s{sub}/f{i:05}.rs"
        ));
        w.put(&rel, &rust_source(items, u64::try_from(i).unwrap_or(0)));
        i = i.saturating_add(1);
    }
    Checkout {
        root: root.to_path_buf(),
        open: PathBuf::from("crates/c000/src/lib.rs"),
        files: w.files,
        dirs: w.dirs.len().saturating_add(1),
        bytes: w.bytes,
    }
}

/// One drawn frame of `app`, as a headless terminal would show it.
fn draw(app: &mut App) {
    let mut term = Terminal::new(TestBackend::new(SCREEN.0, SCREEN.1)).expect("test terminal");
    term.draw(|f| vix::ui::draw(app, f)).expect("draw a frame");
}

/// Settings for the default (no language server) runs: everything at its
/// default except `subsystems.lsp_enabled`, which is off so no server can
/// start and no language-server trust prompt covers the first frame.
fn no_lsp_settings() -> Settings {
    let mut settings = Settings::default();
    settings.subsystems.lsp_enabled = false;
    settings
}

/// One run's timings.
struct OpenRun {
    /// `App::new` through the first drawn frame showing the opened file.
    first_frame: Duration,
    /// Invoking Quick Open through the first frame listing files (empty
    /// query).
    quick_open_listed: Duration,
    /// Invoking Quick Open through the frame listing a result for the
    /// typed query.
    quick_open: Duration,
    /// Authority calls made for the first frame (remote runs only).
    frame_calls: usize,
    /// Authority calls made for Quick Open (remote runs only).
    quick_open_calls: usize,
}

/// One project open, timed the way a user waits for it.
///
/// - **First editable frame** starts just before `App::new` and ends when
///   the first frame — with the file given on the command line open and
///   focused — has been drawn: `App::new`, `App::start` (scripts, project
///   settings, key overrides, welcome screen, opening the file) and one
///   `ui::draw`, the same sequence `main` runs, minus terminal set-up.
/// - **First Quick Open result** starts when `tools.palette` (Quick Open)
///   is invoked on that app. It is taken twice: when the first frame of
///   the open picker (empty query, files listed) has been drawn, and when
///   the frame showing at least one result for [`QUICK_OPEN_QUERY`], typed
///   one key at a time after that, has been drawn. Both include building the
///   file index (the walk of the checkout), which opening the picker does.
///
/// `root` and `file` are paths as the installed authority sees them.
fn open_once(root: &Path, file: &Path, session: &Path, fake: Option<&FakeAuthority>) -> OpenRun {
    if let Some(f) = fake {
        f.clear_calls();
    }
    let cli = <Cli as clap::Parser>::parse_from([PathBuf::from("vix"), file.to_path_buf()]);
    let start = Instant::now();
    let mut app = App::new(root.to_path_buf(), no_lsp_settings()).with_session_path(session);
    app.start(&cli).expect("start the app");
    draw(&mut app);
    let first_frame = start.elapsed();
    let tab = app.editor.active_tab().expect("the opened file has a tab");
    assert_eq!(
        tab.path.as_deref(),
        Some(file),
        "the opened file is focused"
    );
    let frame_calls = fake.map_or(0, |f| f.calls().len());

    if let Some(f) = fake {
        f.clear_calls();
    }
    let start = Instant::now();
    app.run_action("tools.palette");
    draw(&mut app);
    let quick_open_listed = start.elapsed();
    let listed = app.palette.as_ref().map_or(0, |p| p.entries.len());
    assert!(listed > 0, "Quick Open listed no files");
    for ch in QUICK_OPEN_QUERY.chars() {
        app.on_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
    }
    draw(&mut app);
    let quick_open = start.elapsed();
    let hits = app.palette.as_ref().map_or(0, |p| p.entries.len());
    assert!(
        hits > 0,
        "Quick Open found nothing for {QUICK_OPEN_QUERY:?}"
    );
    let quick_open_calls = fake.map_or(0, |f| f.calls().len());
    OpenRun {
        first_frame,
        quick_open_listed,
        quick_open,
        frame_calls,
        quick_open_calls,
    }
}

/// Median, p90 (nearest rank), min and max of `samples`, in milliseconds.
fn summary(samples: &[Duration]) -> String {
    let mut ms: Vec<f64> = samples.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
    ms.sort_by(f64::total_cmp);
    let n = ms.len();
    let at = |rank: usize| {
        ms.get(rank.min(n.saturating_sub(1)))
            .copied()
            .unwrap_or(0.0)
    };
    // Nearest-rank percentile: ceil(p * n) - 1, in integer arithmetic.
    let p = |pct: usize| at(pct.saturating_mul(n).div_ceil(100).saturating_sub(1));
    format!(
        "{:.1} | {:.1} | {:.1} | {:.1} | {n} |",
        p(50),
        p(90),
        at(0),
        at(n.saturating_sub(1))
    )
}

/// A command's first output line, when it runs and succeeds.
fn first_line_of(program: &str, args: &[&str]) -> Option<String> {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.lines().next().map(str::to_owned))
}

/// A command's first output line, or a note that it could not run.
fn version_of(program: &str, args: &[&str]) -> String {
    first_line_of(program, args).unwrap_or_else(|| format!("{program}: not available"))
}

/// The 1/5/15-minute load average, as the OS reports it.
fn load_average() -> String {
    if let Ok(text) = fs::read_to_string("/proc/loadavg") {
        return text
            .split_whitespace()
            .take(3)
            .collect::<Vec<_>>()
            .join(" ");
    }
    version_of("sysctl", &["-n", "vm.loadavg"])
        .trim_matches(|c: char| c == '{' || c == '}' || c.is_whitespace())
        .to_owned()
}

/// The environment block printed before every table.
fn print_environment(checkout: Option<&Checkout>) {
    println!(
        "vix {} ({} profile)",
        env!("CARGO_PKG_VERSION"),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "optimized"
        }
    );
    println!("{}", version_of("rustc", &["--version"]));
    println!("{}", version_of("git", &["--version"]));
    println!("os: {} {}", std::env::consts::OS, std::env::consts::ARCH);
    println!(
        "capabilities: {}",
        vix::capability::enabled_names().join(", ")
    );
    println!("load average (1/5/15 min): {}", load_average());
    if let Some(c) = checkout {
        // Tenths of a MiB, in integer arithmetic.
        let tenths = c
            .bytes
            .saturating_mul(10)
            .checked_div(1024 * 1024)
            .unwrap_or(0);
        println!(
            "workspace: {} files, {} directories, {}.{} MiB (synthetic, seed 0x5eed0823)",
            c.files,
            c.dirs,
            tenths.checked_div(10).unwrap_or(0),
            tenths.checked_rem(10).unwrap_or(0)
        );
    }
}

/// Run the open-time exercise locally and through the fake authority, and
/// print both distributions.
fn open_time_report() {
    let files = env_usize("VIX_BENCH_OPEN_FILES", 20_000).max(200);
    let runs = env_usize("VIX_BENCH_OPEN_RUNS", 10).max(2);
    let scratch = fixture_dir("open");
    let fake = Arc::new(FakeAuthority::new(scratch.join("remote")).expect("fake authority"));
    let checkout = generate_checkout(&fake.backing().join("proj"), files);
    let session = scratch.join("session.toml");

    println!();
    println!("== open/large_checkout (T823): {runs} runs per authority, first run = cold ==");
    print_environment(Some(&checkout));

    let local_file = checkout.root.join(&checkout.open);
    let local: Vec<OpenRun> = (0..runs)
        .map(|_| open_once(&checkout.root, &local_file, &session, None))
        .collect();

    // The fake's virtual namespace: `/proj` exists only inside it.
    let remote_root = PathBuf::from("/proj");
    let remote_file = remote_root.join(&checkout.open);
    let previous =
        vix::authority::install(Arc::<FakeAuthority>::clone(&fake) as Arc<dyn Authority>);
    let remote: Vec<OpenRun> = (0..runs)
        .map(|_| open_once(&remote_root, &remote_file, &session, Some(&fake)))
        .collect();
    vix::authority::install(previous);

    println!();
    println!("| Measurement | Authority | Run | Median ms | p90 ms | Min ms | Max ms | Samples |");
    println!("| ----------- | --------- | --- | --------- | ------ | ------ | ------ | ------- |");
    for (name, rows) in [("local", &local), ("fake (remote)", &remote)] {
        for (label, pick) in [
            (
                "first editable frame",
                (|r: &OpenRun| r.first_frame) as fn(&OpenRun) -> Duration,
            ),
            ("Quick Open, files listed", |r: &OpenRun| {
                r.quick_open_listed
            }),
            ("Quick Open, query result", |r: &OpenRun| r.quick_open),
        ] {
            let all: Vec<Duration> = rows.iter().map(pick).collect();
            let (cold, warm) = all.split_first().expect("at least two runs");
            println!(
                "| {label} | {name} | cold | {}",
                summary(std::slice::from_ref(cold))
            );
            println!("| {label} | {name} | warm | {}", summary(warm));
        }
    }
    if let Some(r) = remote.last() {
        println!();
        println!(
            "authority calls per open through the fake: first frame {}, Quick Open {}",
            r.frame_calls, r.quick_open_calls
        );
    }
    let _ = fs::remove_dir_all(&scratch);
    let _ = fs::remove_dir_all(scratch.join("remote.trash"));
}

// ---------------------------------------------------------------------------
// Opt-in: first semantic definition and first actionable diagnostic.
// ---------------------------------------------------------------------------

/// How long one language-server run may take before it is given up.
const LSP_TIMEOUT: Duration = Duration::from_secs(300);

/// A small Cargo crate for the language server: `src/lib.rs` calls
/// `a::target` (the definition to find) and has one type error (the
/// diagnostic to wait for), plus `modules` filler modules so the server has
/// a crate to index, not a single file.
fn generate_lsp_crate(root: &Path, modules: usize) -> PathBuf {
    let _ = fs::remove_dir_all(root);
    fs::create_dir_all(root.join("src/gen")).expect("create crate directories");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"vix_bench_lsp\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n",
    )
    .expect("write Cargo.toml");
    fs::write(
        root.join("src/a.rs"),
        "/// The definition.\npub fn target() -> u32 {\n    7\n}\n",
    )
    .expect("write a.rs");
    let mut gen_mod = String::new();
    for m in 0..modules {
        writeln!(gen_mod, "pub mod m{m};").expect("writing to a String cannot fail");
        fs::write(
            root.join(format!("src/gen/m{m}.rs")),
            rust_source(20, u64::try_from(m).unwrap_or(0)),
        )
        .expect("write a filler module");
    }
    fs::write(root.join("src/gen/mod.rs"), gen_mod).expect("write gen/mod.rs");
    // Line 4, column 8 (1-based) is the `t` of `target`.
    fs::write(
        root.join("src/lib.rs"),
        "mod a;\npub mod gen;\npub fn caller() -> u32 {\n    a::target()\n}\npub fn broken() -> u32 {\n    \"not a number\"\n}\n",
    )
    .expect("write lib.rs");
    root.join("src/lib.rs")
}

/// The language-server program to use, if it really runs: a rustup proxy
/// with no `rust-analyzer` component installed is on `PATH` but fails.
fn rust_analyzer() -> Result<(String, String), String> {
    let program =
        std::env::var("VIX_BENCH_RUST_ANALYZER").unwrap_or_else(|_| "rust-analyzer".to_owned());
    match first_line_of(&program, &["--version"]) {
        Some(version) if version.starts_with("rust-analyzer") => Ok((program, version)),
        _ => Err(format!("`{program} --version` did not run")),
    }
}

/// One language-server run's timings, both from the start of the open.
struct LspRun {
    /// Until the active tab is `a.rs` after go-to-definition.
    definition: Option<Duration>,
    /// Until the server has published an error for `lib.rs`.
    diagnostic: Option<Duration>,
}

/// One open with a real server: the interval starts just before `App::new`
/// (so it includes the server's start-up and indexing — what a user waits
/// for after launching Vix on a project) and ends, for each measurement, at
/// the first tick after which the result is visible: the cursor in `a.rs`
/// after `nav.goto_definition` (re-requested every second, since a server
/// still indexing answers "nothing"), or an error-severity diagnostic for
/// `lib.rs` in the client's store.
fn lsp_once(root: &Path, lib: &Path, program: &str, session: &Path) -> LspRun {
    let mut settings = Settings::default();
    settings.subsystems.lsp_enabled = true;
    settings.lsp_servers = vec![LspServer {
        language_id: "rust".to_owned(),
        extensions: vec!["rs".to_owned()],
        // Never reach the network: the crate has no dependencies anyway.
        command: vec![
            "env".to_owned(),
            "CARGO_NET_OFFLINE=true".to_owned(),
            program.to_owned(),
        ],
    }];
    let cli = <Cli as clap::Parser>::parse_from([PathBuf::from("vix"), lib.to_path_buf()]);
    let start = Instant::now();
    let mut app = App::new(root.to_path_buf(), settings).with_session_path(session);
    app.start(&cli).expect("start the app");
    draw(&mut app);
    let area = ratatui::layout::Rect::new(0, 0, SCREEN.0, SCREEN.1);
    let mut run = LspRun {
        definition: None,
        diagnostic: None,
    };
    let mut asked: Option<Instant> = None;
    while start.elapsed() < LSP_TIMEOUT && (run.definition.is_none() || run.diagnostic.is_none()) {
        app.tick();
        draw(&mut app);
        let active = app.editor.active_tab().and_then(|t| t.path.clone());
        if run.definition.is_none() {
            if active.as_deref().is_some_and(|p| p.ends_with("src/a.rs")) {
                run.definition = Some(start.elapsed());
            } else if asked.is_none_or(|t| t.elapsed() >= Duration::from_secs(1)) {
                if active.as_deref() == Some(lib) {
                    app.editor.goto(4, Some(8), area);
                    app.run_action("nav.goto_definition");
                }
                asked = Some(Instant::now());
            }
        }
        if run.diagnostic.is_none()
            && app
                .lsp
                .diagnostics_for(lib)
                .iter()
                .any(|d| matches!(d.severity, vix::lsp_core::Severity::Error))
        {
            run.diagnostic = Some(start.elapsed());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    app.lsp.shutdown();
    run
}

/// The opt-in language-server exercise; prints why when it is skipped.
fn lsp_report() {
    if std::env::var("VIX_BENCH_LSP").ok().as_deref() != Some("1") {
        println!();
        println!(
            "skipping lsp/large_checkout: set VIX_BENCH_LSP=1 to measure the first semantic definition and diagnostic with rust-analyzer"
        );
        return;
    }
    let (program, version) = match rust_analyzer() {
        Ok(found) => found,
        Err(why) => {
            println!();
            println!("skipping lsp/large_checkout: {why}");
            return;
        }
    };
    let runs = env_usize("VIX_BENCH_LSP_RUNS", 3).max(2);
    let modules = env_usize("VIX_BENCH_LSP_MODULES", 200);
    let scratch = fixture_dir("lsp");
    let root = fs::canonicalize(&scratch)
        .expect("canonical scratch")
        .join("crate");
    let lib = generate_lsp_crate(&root, modules);
    let session = scratch.join("session.toml");
    println!();
    println!("== lsp/large_checkout (T823): {runs} runs, first run = cold, local server ==");
    print_environment(None);
    println!("{version}");
    println!("crate: 1 package, {} modules", modules.saturating_add(2));
    let rows: Vec<LspRun> = (0..runs)
        .map(|_| lsp_once(&root, &lib, &program, &session))
        .collect();
    println!();
    println!("| Measurement | Run | Median ms | p90 ms | Min ms | Max ms | Samples |");
    println!("| ----------- | --- | --------- | ------ | ------ | ------ | ------- |");
    for (label, pick) in [
        (
            "first semantic definition",
            (|r: &LspRun| r.definition) as fn(&LspRun) -> Option<Duration>,
        ),
        ("first actionable diagnostic", |r: &LspRun| r.diagnostic),
    ] {
        let all: Vec<Option<Duration>> = rows.iter().map(pick).collect();
        let missed = all.iter().filter(|d| d.is_none()).count();
        let (cold, warm) = all.split_first().expect("at least two runs");
        let warm: Vec<Duration> = warm.iter().flatten().copied().collect();
        match cold {
            Some(d) => println!("| {label} | cold | {}", summary(std::slice::from_ref(d))),
            None => println!("| {label} | cold | timed out | | | | 1 |"),
        }
        println!("| {label} | warm | {}", summary(&warm));
        if missed > 0 {
            println!(
                "({label}: {missed} of {runs} runs timed out after {} s)",
                LSP_TIMEOUT.as_secs()
            );
        }
    }
    let _ = fs::remove_dir_all(&scratch);
}

/// The first non-flag argument: the filter `cargo bench -- FILTER` passes.
fn filter() -> Option<String> {
    std::env::args().skip(1).find(|a| !a.starts_with('-'))
}

fn main() {
    let filter = filter();
    let wants = |name: &str| {
        filter
            .as_deref()
            .is_none_or(|f| name.contains(f) || f.contains(name))
    };
    let mut c = criterion();
    bench_app_new(&mut c);
    bench_refresh_git(&mut c);
    c.final_summary();
    // `cargo test --benches` runs bench targets without `--bench`: only the
    // Criterion smoke run then, never the multi-second open-time exercise.
    if !std::env::args().any(|a| a == "--bench") {
        return;
    }
    if wants("open/large_checkout") {
        open_time_report();
    }
    if wants("lsp/large_checkout") {
        lsp_report();
    }
}
