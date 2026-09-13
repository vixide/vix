//! Generates the man page (`vix.1`) from the real `Cli` definition
//! (`vix::cli::Cli`, shared with `src/main.rs`) via `clap_mangen`, so the
//! page can never drift from the actual flags (T307, `tasks.md`).
//!
//! Run with: `cargo run --example generate_man` — writes `man/vix.1`.
//!
//! CI (matching T305's `docs/reference/` convention): regenerate, then
//! `git diff --exit-code -- man/`.

#![warn(clippy::pedantic)]

use std::fs;
use std::path::PathBuf;

use clap::CommandFactory;
use vix::cli::Cli;

fn main() {
    let cmd = Cli::command();
    let man = clap_mangen::Man::new(cmd);
    let mut buffer = Vec::new();
    man.render(&mut buffer).expect("rendering the man page");

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out_dir = root.join("man");
    fs::create_dir_all(&out_dir).unwrap_or_else(|e| panic!("creating {}: {e}", out_dir.display()));
    let out_path = out_dir.join("vix.1");
    fs::write(&out_path, &buffer).unwrap_or_else(|e| panic!("writing {}: {e}", out_path.display()));

    println!("Wrote {}", out_path.display());
}
