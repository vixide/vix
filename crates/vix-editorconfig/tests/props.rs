//! Property tests for the two parsers in this crate (T831): Vim/Emacs
//! modelines and `.editorconfig` files.
//!
//! - [`modeline::parse`] never panics, never returns a width outside its
//!   bounds, sets nothing with `lines == 0`, and reads back a generated Vim
//!   or Emacs modeline exactly.
//! - [`resolve`] over a generated `.editorconfig` (sections over a fixed set
//!   of globs, recognized and junk keys, comments) gives what a naive model
//!   gets by applying the matching sections' keys in file order.

use std::fmt::Write as _;
use std::sync::atomic::{AtomicUsize, Ordering};

use proptest::prelude::*;
use vix_editorconfig::modeline::{self, Modeline};
use vix_editorconfig::{Resolved, resolve};

proptest! {
    /// Any text: no panic, every width in range, `lines == 0` is off.
    #[test]
    fn modeline_parse_is_total_and_bounded(
        head in "(?s).{0,80}",
        tail in "(?s).{0,80}",
        lines in 0usize..8,
    ) {
        let m = modeline::parse(&head, &tail, lines);
        for w in [m.tab_width, m.indent_size].into_iter().flatten() {
            prop_assert!((1..=16).contains(&w), "{m:?}");
        }
        if let Some(w) = m.wrap_column {
            prop_assert!((1..=1000).contains(&w), "{m:?}");
        }
        prop_assert!(modeline::parse(&head, &tail, 0).is_empty());
    }

    /// Settings written into a Vim `set` modeline, with any comment leader
    /// and either option spelling, are read back as written.
    #[test]
    fn vim_modeline_round_trips(
        leader in prop::sample::select(vec!["", "# ", "// ", "/* ", "-- "]),
        marker in prop::sample::select(vec!["vim:", "vi:", "ex:", "Vim:"]),
        ts in prop::option::of(1usize..=16),
        sw in prop::option::of(1usize..=16),
        tw in prop::option::of(1usize..=1000),
        tab in prop::option::of(any::<bool>()),
        long in any::<bool>(),
        body in "[a-z ]{0,20}",
    ) {
        let mut opts = Vec::new();
        let name = |short: &'static str, full: &'static str| if long { full } else { short };
        if let Some(n) = ts { opts.push(format!("{}={n}", name("ts", "tabstop"))); }
        if let Some(n) = sw { opts.push(format!("{}={n}", name("sw", "shiftwidth"))); }
        if let Some(n) = tw { opts.push(format!("{}={n}", name("tw", "textwidth"))); }
        if let Some(t) = tab {
            opts.push(match (t, long) {
                (true, false) => "noet",
                (true, true) => "noexpandtab",
                (false, false) => "et",
                (false, true) => "expandtab",
            }.to_string());
        }
        let text = format!("{leader}{marker} set {}:\n{body}\n", opts.join(" "));
        let m = modeline::parse(&text, &text, 5);
        prop_assert_eq!(m.tab_width, ts);
        prop_assert_eq!(m.indent_size, sw);
        prop_assert_eq!(m.wrap_column, tw);
        prop_assert_eq!(m.indent_is_tab, tab);
    }

    /// Settings written into an Emacs `-*- … -*-` first line (or second,
    /// after a `#!` line) are read back as written.
    #[test]
    fn emacs_first_line_round_trips(
        shebang in any::<bool>(),
        width in prop::option::of(1usize..=16),
        fill in prop::option::of(1usize..=1000),
        tabs in prop::option::of(any::<bool>()),
        offset in prop::option::of((prop::sample::select(vec!["c-basic-offset", "python-indent-offset", "js-indent-level"]), 1usize..=16)),
    ) {
        let mut vars = vec!["mode: text".to_string()];
        if let Some(n) = width { vars.push(format!("tab-width: {n}")); }
        if let Some(n) = fill { vars.push(format!("fill-column: {n}")); }
        if let Some(t) = tabs { vars.push(format!("indent-tabs-mode: {}", if t { "t" } else { "nil" })); }
        if let Some((name, n)) = offset { vars.push(format!("{name}: {n}")); }
        let first = format!("# -*- {} -*-\n", vars.join("; "));
        let text = if shebang { format!("#!/bin/sh\n{first}") } else { first };
        let m = modeline::parse(&text, "", 5);
        let mut want = Modeline::default();
        want.tab_width = width;
        want.wrap_column = fill;
        want.indent_is_tab = tabs;
        want.indent_size = offset.map(|(_, n)| n);
        prop_assert_eq!(m, want);
    }
}

/// A section glob and whether it matches `src/main.rs` relative to the
/// `.editorconfig`'s directory.
const GLOBS: &[(&str, bool)] = &[
    ("*", true),
    ("*.rs", true),
    ("*.py", false),
    ("**.rs", true),
    ("src/*.rs", true),
    ("lib/*.rs", false),
    ("*.{rs,py}", true),
    ("*.{js,ts}", false),
    ("main.?s", true),
    ("[mn]ain.rs", true),
    ("[!m]ain.rs", false),
    ("**/main.rs", true),
];

/// One `key = value` line: a recognized key with a valid or junk value, or
/// an unknown key.
fn kv() -> impl Strategy<Value = (String, String)> {
    let keys = vec![
        "indent_style",
        "indent_size",
        "tab_width",
        "trim_trailing_whitespace",
        "insert_final_newline",
        "unknown_key",
    ];
    let values = vec![
        "tab", "space", "2", "4", "8", "true", "false", "TRUE", "junk", "",
    ];
    (
        prop::sample::select(keys),
        prop::sample::select(values),
        any::<bool>(),
    )
        .prop_map(|(k, v, upper)| {
            let k = if upper {
                k.to_ascii_uppercase()
            } else {
                k.to_string()
            };
            (k, v.to_string())
        })
}

/// What a naive reader makes of one key, applied over `out`.
fn model_apply(out: &mut Resolved, key: &str, value: &str) {
    let lower = value.to_ascii_lowercase();
    match key.to_ascii_lowercase().as_str() {
        "indent_style" if lower == "tab" => out.indent_is_tab = Some(true),
        "indent_style" if lower == "space" => out.indent_is_tab = Some(false),
        "indent_size" => out.indent_size = value.parse().ok().or(out.indent_size),
        "tab_width" => out.tab_width = value.parse().ok().or(out.tab_width),
        "trim_trailing_whitespace" => out.trim_trailing_whitespace = bool_of(&lower),
        "insert_final_newline" => out.insert_final_newline = bool_of(&lower),
        _ => {}
    }
}

fn bool_of(lower: &str) -> Option<bool> {
    match lower {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// A directory of its own for each case.
fn scratch_dir() -> std::path::PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("vix-ec-props-{}-{n}", std::process::id()))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// `resolve` agrees with applying each matching section's keys in file
    /// order; a preamble key never applies.
    #[test]
    fn resolve_matches_a_naive_model(
        preamble in prop::collection::vec(kv(), 0..3),
        sections in prop::collection::vec(
            (0..GLOBS.len(), prop::collection::vec((kv(), any::<bool>()), 0..4)),
            0..5,
        ),
    ) {
        let mut text = String::from("root = true\n");
        for (k, v) in &preamble {
            let _ = writeln!(text, "{k} = {v}");
        }
        let mut want = Resolved::default();
        for (glob, keys) in &sections {
            let (pat, matches) = GLOBS[*glob];
            let _ = write!(text, "\n[{pat}]\n");
            for ((k, v), comment) in keys {
                if *comment {
                    text.push_str("# a comment = tab\n");
                }
                let _ = writeln!(text, "  {k} =  {v}  ");
                if matches {
                    model_apply(&mut want, k, v);
                }
            }
        }
        let dir = scratch_dir();
        let src = dir.join("src");
        std::fs::create_dir_all(&src).map_err(|e| TestCaseError::fail(e.to_string()))?;
        std::fs::write(dir.join(".editorconfig"), &text)
            .map_err(|e| TestCaseError::fail(e.to_string()))?;
        let got = resolve(&src.join("main.rs"));
        std::fs::remove_dir_all(&dir).ok();
        prop_assert_eq!(got, want, "{}", text);
    }
}
