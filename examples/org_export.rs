//! Export an Org document to Markdown and HTML (improvement plan T503) --
//! `vix_org::to_markdown`/`to_html` are pure `&str -> String` functions, the
//! same ones Org -> Export uses, so no editor is needed at all.
//!
//! Run with: `cargo run --example org_export -- [file.org]`
//! With no argument, exports a small built-in sample.

#![warn(clippy::pedantic)]

const SAMPLE: &str = "\
#+title: Example

* TODO Write the example
  A /pure/ function turns this whole buffer into Markdown or HTML --
  no editor, no App, just text in and text out.

** DONE A finished subtask
";

fn main() {
    let org = match std::env::args().nth(1) {
        Some(path) => std::fs::read_to_string(&path).unwrap_or_else(|e| {
            eprintln!("couldn't read {path}: {e}");
            std::process::exit(1);
        }),
        None => SAMPLE.to_string(),
    };

    println!("--- Markdown ---\n{}", vix::org::to_markdown(&org));
    println!("--- HTML ---\n{}", vix::org::to_html(&org));
}
