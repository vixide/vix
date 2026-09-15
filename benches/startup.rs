//! Cold-start cost (plan T121/T122): `App::new` (theme scan, editor/menu/LSP
//! setup — everything on the path before the first frame can draw) and
//! `refresh_git` (three separate `git` subprocesses — repo?/branch/status —
//! deliberately moved *off* that path in `main.rs`, so a real cold start no
//! longer waits on it) benchmarked separately, since T122's fix is exactly
//! that these two are no longer paid at the same point in startup. See
//! `docs/performance/index.md` for the numbers this produced and the
//! before/after story.

use criterion::{Criterion, criterion_group, criterion_main};
use std::fs;
use std::hint::black_box;
use std::path::PathBuf;
use vix::app::App;
use vix::settings::Settings;

/// A fresh, empty fixture directory — cleared first so a leftover `.git`
/// from a prior run never leaks between benchmark runs.
fn fixture_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vix-bench-startup-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create fixture directory");
    dir
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
    let init = std::process::Command::new("git")
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

criterion_group!(benches, bench_app_new, bench_refresh_git);
criterion_main!(benches);
