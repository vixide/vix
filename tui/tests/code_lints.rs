//! Project-specific lints (T833, `spec/test` "Repository invariants"): rules
//! clippy cannot express, checked by scanning the source tree so they run under
//! `cargo test` (and so under `scripts/check` and CI) with no extra tool.
//!
//! 1. **No `println!`, `eprintln!`, `print!`, `eprint!` or `dbg!` in library
//!    code** under `crates/*/src` and `tui/src`. The TUI owns the terminal, so a
//!    stray print corrupts the screen; a diagnostic belongs in the message
//!    drawer. Only the files in [`PRINT_ALLOWED`] may print, each the exact
//!    number of times listed, so a new print in an allowlisted file still fails.
//! 2. **Literals given to the App's user-message sinks go through `t!`.** A
//!    statement `self.messages.{info,warn,error,push}(…)` or `self.status = …`
//!    (or `app.` for either, or `.status.push_str(…)`) whose text holds a string
//!    literal with a letter in it (outside `{…}` placeholders) must contain a
//!    `t!(` call.
//!
//! Test code is exempt from rules 1 and 2: an item under `#[cfg(test)]` (or any
//! `cfg` naming `test` without `not(test)`) and every file of a module
//! declared under one. Comments and the contents of string and character
//! literals are never read as code, so a snippet body such as `"println!($0);"`
//! is not a print. Examples, benches, build scripts and `tests/` directories
//! live outside `src/` and are not scanned.
//!
//! 3. **Every `#[ignore]` says why** (T831, `spec/test` "Flaky tests"), in
//!    every Rust file of `tui/`, `crates/*/` and `gui/*/` (`src/`, `tests/`,
//!    `examples/` and `benches/`, test code included; T863): a bare `#[ignore]` fails, and a
//!    quarantined flaky test must read `#[ignore = "flaky: T<n>, <reason>"]`
//!    with `T<n>` an open task in `tasks.md`, so a quarantine cannot outlive
//!    the work that is meant to end it.
//!
//! A last test keeps the GitHub and Forgejo pull-request templates identical,
//! and another the `ignore` lists of the `cargo` entries in
//! `.github/dependabot.yml` (T832, `spec/dependabot`).
//!
//! "No blocking I/O in async code" is not checked: the workspace has only two
//! async call paths and both run on a private single-task runtime on their own
//! thread (see `spec/test`).

use std::collections::BTreeSet;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// `(file, how many print macros it holds, why it may print)`.
const PRINT_ALLOWED: &[(&str, usize, &str)] = &[
    (
        "tui/src/main.rs",
        13,
        "the `vix` command-line entry point: `--version`, `--json`, usage errors and \
         start-up failures print to stdout/stderr before or after the TUI owns the terminal",
    ),
    (
        "tui/src/app.rs",
        2,
        "`App::on_exit` (T544): `main` calls it after `ratatui::restore()`, so a failed \
         session or settings save can only reach the user on stderr (both strings are `t!`)",
    ),
    (
        "tui/src/crash.rs",
        1,
        "the panic hook: the TUI is gone, stderr is the only channel left to say where the \
         crash report was saved",
    ),
    (
        "tui/src/mcp.rs",
        1,
        "`vix --mcp`: stdout carries the protocol and there is no TUI, so a start-up failure \
         goes to stderr (a `t!` string)",
    ),
];

/// `(file, the start of the offending statement, why it is not user-facing)`.
/// Empty: every literal reaching a sink today goes through `t!`. An entry
/// here needs a reason and, for a real violation, a `TODO(T833)`.
const SINK_ALLOWED: &[(&str, &str, &str)] = &[];

/// The print macros rule 1 forbids.
const PRINT_MACROS: &[&str] = &["println!", "eprintln!", "print!", "eprint!", "dbg!"];

/// The receivers rule 2 watches, each followed by its sink.
const SINKS: &[&str] = &[
    ".messages.info(",
    ".messages.warn(",
    ".messages.error(",
    ".messages.push(",
    ".status.push_str(",
    ".status =",
];

/// A source file with its comments and literal contents masked out.
struct Masked {
    /// The text with every comment byte and every byte inside a string or
    /// character literal replaced by a space (newlines kept), so offsets match
    /// the original and nothing in it is anything but code.
    code: Vec<u8>,
    /// The byte ranges of string-literal contents in the original text.
    strings: Vec<Range<usize>>,
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn blank(code: &mut [u8], range: Range<usize>) {
    for b in code.get_mut(range).into_iter().flatten() {
        if *b != b'\n' {
            *b = b' ';
        }
    }
}

/// Whether `text[i]` starts a raw string (`r"`, `r#"`, `br"`, `cr#"`, …);
/// returns the number of `#`s and the offset just past the opening quote.
fn raw_string_at(text: &[u8], i: usize) -> Option<(usize, usize)> {
    if text.get(i) != Some(&b'r') {
        return None;
    }
    let before = i.checked_sub(1).and_then(|p| text.get(p)).copied();
    let prefix_ok = match before {
        None => true,
        Some(b'b' | b'c') => i
            .checked_sub(2)
            .and_then(|p| text.get(p))
            .is_none_or(|b| !is_ident(*b)),
        Some(b) => !is_ident(b),
    };
    if !prefix_ok {
        return None;
    }
    let mut j = i + 1;
    while text.get(j) == Some(&b'#') {
        j += 1;
    }
    (text.get(j) == Some(&b'"')).then_some((j - i - 1, j + 1))
}

fn mask(text: &str) -> Masked {
    let bytes = text.as_bytes();
    let mut code = bytes.to_vec();
    let mut strings = Vec::new();
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        let next = bytes.get(i + 1).copied();
        if b == b'/' && next == Some(b'/') {
            let end = bytes[i..]
                .iter()
                .position(|&c| c == b'\n')
                .map_or(bytes.len(), |p| i + p);
            blank(&mut code, i..end);
            i = end;
        } else if b == b'/' && next == Some(b'*') {
            let (mut j, mut depth) = (i + 2, 1);
            while depth > 0 && j < bytes.len() {
                if bytes[j..].starts_with(b"/*") {
                    depth += 1;
                    j += 2;
                } else if bytes[j..].starts_with(b"*/") {
                    depth -= 1;
                    j += 2;
                } else {
                    j += 1;
                }
            }
            blank(&mut code, i..j);
            i = j;
        } else if let Some((hashes, start)) = raw_string_at(bytes, i) {
            let mut close = b"\"".to_vec();
            close.extend(std::iter::repeat_n(b'#', hashes));
            let end = bytes[start..]
                .windows(close.len())
                .position(|w| w == close.as_slice())
                .map_or(bytes.len(), |p| start + p);
            blank(&mut code, start..end);
            strings.push(start..end);
            i = (end + close.len()).min(bytes.len());
        } else if b == b'"' {
            let mut j = i + 1;
            while let Some(&c) = bytes.get(j) {
                match c {
                    b'\\' => j += 2,
                    b'"' => break,
                    _ => j += 1,
                }
            }
            let end = j.min(bytes.len());
            blank(&mut code, i + 1..end);
            strings.push(i + 1..end);
            i = end + 1;
        } else if b == b'\'' {
            // A character literal, or a lifetime / loop label (left alone).
            if next == Some(b'\\') {
                let end = bytes[i + 2..]
                    .iter()
                    .position(|&c| c == b'\'')
                    .map_or(bytes.len(), |p| i + 2 + p);
                blank(&mut code, i + 1..end);
                i = end + 1;
            } else {
                let width = text
                    .get(i + 1..)
                    .and_then(|s| s.chars().next())
                    .map_or(1, char::len_utf8);
                if bytes.get(i + 1 + width) == Some(&b'\'') {
                    blank(&mut code, i + 1..i + 1 + width);
                    i += 2 + width;
                } else {
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }
    Masked { code, strings }
}

/// The end of the item that starts at or after `from` in masked code: just
/// past its first top-level `;` or its matching `}`.
fn item_end(code: &[u8], from: usize) -> usize {
    let mut depth = 0usize;
    let mut i = from;
    while let Some(&b) = code.get(i) {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return i + 1;
                }
            }
            b';' if depth == 0 => return i + 1,
            _ => {}
        }
        i += 1;
    }
    code.len()
}

/// A module declared (`mod name;`) inside a test item.
enum TestMod {
    /// `mod name;`: `name.rs` or `name/` beside the declaring module.
    Name(String),
    /// `#[path = "p"] mod name;`: `p`, relative to the declaring file's directory.
    Path(String),
}

/// The end of the expression statement starting at `from` in masked code:
/// the first `;` or `,` outside brackets, or the bracket that closes one
/// opened before `from` (the end of a match arm or a block's tail).
fn expr_end(code: &[u8], from: usize) -> usize {
    let mut depth = 0usize;
    let mut i = from;
    while let Some(&b) = code.get(i) {
        match b {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => match depth.checked_sub(1) {
                Some(d) => depth = d,
                None => return i,
            },
            b';' | b',' if depth == 0 => return i,
            _ => {}
        }
        i += 1;
    }
    code.len()
}

/// The byte ranges of `#[cfg(…test…)]` items in masked `code`, and the
/// modules declared inside them (read from the original `text`).
fn test_regions(code: &[u8], original: &str) -> (Vec<Range<usize>>, Vec<TestMod>) {
    let text = String::from_utf8_lossy(code);
    let mut regions = Vec::new();
    let mut mods = Vec::new();
    let mut from = 0;
    while let Some(p) = text[from..].find("#[cfg(") {
        let start = from + p;
        let attr_end = text[start..]
            .find(']')
            .map_or(text.len(), |e| start + e + 1);
        let attr = &text[start..attr_end];
        let names_test = attr.match_indices("test").any(|(k, _)| {
            let before = attr.as_bytes().get(k.wrapping_sub(1)).copied();
            let after = attr.as_bytes().get(k + 4).copied();
            !before.is_some_and(is_ident) && !after.is_some_and(is_ident)
        });
        if names_test && !attr.contains("not(test") {
            let end = item_end(code, attr_end);
            let item = &text[attr_end..end];
            let mut at = attr_end;
            for decl in item.split(';') {
                let words: Vec<&str> = decl.split_whitespace().collect();
                if let [.., "mod", name] = words.as_slice() {
                    let raw = original.get(at..at + decl.len()).unwrap_or("");
                    let path = raw
                        .split_once("#[path")
                        .and_then(|(_, rest)| rest.split('"').nth(1));
                    mods.push(match path {
                        Some(p) => TestMod::Path(p.to_string()),
                        None => TestMod::Name((*name).to_string()),
                    });
                }
                at += decl.len() + 1;
            }
            regions.push(start..end);
            from = end;
        } else {
            from = attr_end;
        }
    }
    (regions, mods)
}

/// A scanned source file.
struct Source {
    /// Repository-relative path with `/` separators.
    rel: String,
    text: String,
    masked: Masked,
    tests: Vec<Range<usize>>,
}

impl Source {
    fn in_test(&self, at: usize) -> bool {
        self.tests.iter().any(|r| r.contains(&at))
    }
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// The directory a module file's child modules live in.
fn child_dir(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    match path.file_stem().and_then(|s| s.to_str()) {
        Some("mod" | "lib" | "main") | None => parent.to_path_buf(),
        Some(stem) => parent.join(stem),
    }
}

/// Every non-test `.rs` file under `tui/src` and `crates/*/src`.
fn library_sources() -> Vec<Source> {
    // Paths are repository-relative (this test's package lives in `tui/`).
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let mut files = Vec::new();
    rust_files(&root.join("tui/src"), &mut files);
    if let Ok(crates) = std::fs::read_dir(root.join("crates")) {
        for e in crates.flatten() {
            rust_files(&e.path().join("src"), &mut files);
        }
    }
    let mut sources = Vec::new();
    let mut test_dirs: BTreeSet<PathBuf> = BTreeSet::new();
    let mut test_files: BTreeSet<PathBuf> = BTreeSet::new();
    for f in &files {
        let Ok(text) = std::fs::read_to_string(f) else {
            continue;
        };
        let masked = mask(&text);
        let (tests, mods) = test_regions(&masked.code, &text);
        for m in mods {
            match m {
                TestMod::Name(name) => {
                    let dir = child_dir(f);
                    test_files.insert(dir.join(format!("{name}.rs")));
                    test_dirs.insert(dir.join(&name));
                }
                TestMod::Path(p) => {
                    let dir = f.parent().unwrap_or_else(|| Path::new(""));
                    test_files.insert(dir.join(&p));
                    test_dirs.insert(child_dir(&dir.join(&p)));
                }
            }
        }
        let rel = f
            .strip_prefix(root)
            .unwrap_or(f)
            .to_string_lossy()
            .replace('\\', "/");
        sources.push((
            f.clone(),
            Source {
                rel,
                text,
                masked,
                tests,
            },
        ));
    }
    sources
        .into_iter()
        .filter(|(p, _)| !test_files.contains(p) && !test_dirs.iter().any(|d| p.starts_with(d)))
        .map(|(_, s)| s)
        .collect()
}

/// The offsets of print-macro calls in `src`'s code, outside test items.
fn prints(src: &Source) -> Vec<usize> {
    let code = &src.masked.code;
    let mut found = Vec::new();
    for m in PRINT_MACROS {
        let needle = m.as_bytes();
        let mut i = 0;
        while let Some(p) = code
            .get(i..)
            .and_then(|rest| rest.windows(needle.len()).position(|w| w == needle))
        {
            let at = i + p;
            let before = at.checked_sub(1).and_then(|b| code.get(b)).copied();
            if !before.is_some_and(is_ident) && !src.in_test(at) {
                found.push(at);
            }
            i = at + needle.len();
        }
    }
    found
}

/// Whether `code` (masked) contains a `t!(` call.
fn calls_t(code: &[u8]) -> bool {
    code.windows(3).enumerate().any(|(k, w)| {
        w == b"t!("
            && !k
                .checked_sub(1)
                .and_then(|b| code.get(b))
                .is_some_and(|b| is_ident(*b))
    })
}

/// Whether a literal's text has a letter outside `{…}` format placeholders.
fn has_words(literal: &str) -> bool {
    let mut depth = 0usize;
    literal.chars().any(|c| match c {
        '{' => {
            depth += 1;
            false
        }
        '}' => {
            depth = depth.saturating_sub(1);
            false
        }
        _ => depth == 0 && c.is_alphabetic(),
    })
}

/// The sink statements in `src` that pass an untranslated literal: `(offset,
/// statement text)`.
fn untranslated_sinks(src: &Source) -> Vec<(usize, String)> {
    let code = &src.masked.code;
    let text = String::from_utf8_lossy(code);
    let mut found = Vec::new();
    for sink in SINKS {
        for (p, _) in text.match_indices(sink) {
            let receiver = text[..p]
                .rsplit(|c: char| !(c.is_alphanumeric() || c == '_'))
                .next()
                .unwrap_or("");
            if !matches!(receiver, "self" | "app") || src.in_test(p) {
                continue;
            }
            let after = p + sink.len();
            if sink.ends_with('=') && code.get(after) == Some(&b'=') {
                continue; // `==`, a comparison
            }
            let end = expr_end(code, p);
            if calls_t(&code[p..end]) {
                continue;
            }
            let words = src
                .masked
                .strings
                .iter()
                .filter(|r| r.start >= p && r.end <= end)
                .any(|r| has_words(&src.text[r.clone()]));
            if words {
                let stmt = src.text[p - receiver.len()..end]
                    .split_whitespace()
                    .collect::<Vec<_>>();
                found.push((p, stmt.join(" ")));
            }
        }
    }
    found
}

/// The directories rule 3 reads under each package directory.
const IGNORE_DIRS: &[&str] = &["src", "tests", "examples", "benches"];

/// Every `.rs` file under `src/`, `tests/`, `examples/` and `benches/` of `tui/`, `crates/*/` and
/// `gui/*/`, test code included: `(repository-relative path, text)`.
fn all_rust_sources(root: &Path) -> Vec<(String, String)> {
    let mut packages = vec![root.join("tui")];
    for group in ["crates", "gui"] {
        if let Ok(entries) = std::fs::read_dir(root.join(group)) {
            packages.extend(entries.flatten().map(|e| e.path()));
        }
    }
    let mut files = Vec::new();
    for package in packages {
        for dir in IGNORE_DIRS {
            rust_files(&package.join(dir), &mut files);
        }
    }
    files.sort();
    files
        .into_iter()
        .filter_map(|f| {
            let text = std::fs::read_to_string(&f).ok()?;
            let rel = f
                .strip_prefix(root)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            Some((rel, text))
        })
        .collect()
}

/// The `#[ignore]` attributes in `text` (comments and literals are not
/// read): `(offset, reason)`, the reason `None` for a bare `#[ignore]`.
fn ignores(text: &str) -> Vec<(usize, Option<String>)> {
    let masked = mask(text);
    let code = String::from_utf8_lossy(&masked.code);
    let needle = "#[ignore";
    let mut found = Vec::new();
    for (p, _) in code.match_indices(needle) {
        let after = p.saturating_add(needle.len());
        let rest = code.get(after..).unwrap_or("").trim_start();
        if rest.starts_with(']') {
            found.push((p, None));
        } else if rest.starts_with('=') {
            let reason = masked
                .strings
                .iter()
                .find(|r| r.start > after)
                .and_then(|r| text.get(r.clone()))
                .map(str::to_string);
            found.push((p, reason));
        }
        // Anything else (`#[ignore_foo]`) is another attribute.
    }
    found
}

/// What is wrong with an `#[ignore]` reason, given the open task ids
/// (`"T831"`, …): `None` when it is fine.
fn ignore_problem(reason: Option<&str>, open: &BTreeSet<String>) -> Option<String> {
    let Some(reason) = reason.map(str::trim) else {
        return Some("a bare #[ignore]: write #[ignore = \"why\"]".into());
    };
    if reason.is_empty() {
        return Some("an empty #[ignore] reason".into());
    }
    if !reason.to_ascii_lowercase().starts_with("flaky") {
        return None;
    }
    let shape = "a flaky quarantine reads #[ignore = \"flaky: T<n>, <reason>\"]";
    let Some(rest) = reason.strip_prefix("flaky: T") else {
        return Some(shape.into());
    };
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    let (number, tail) = rest.split_at(digits);
    let Some(why) = tail.strip_prefix(", ") else {
        return Some(shape.into());
    };
    if number.is_empty() || why.trim().is_empty() {
        return Some(shape.into());
    }
    let id = format!("T{number}");
    (!open.contains(&id)).then(|| {
        format!("{id} is not an open task in tasks.md: a quarantine needs one (`- [ ] **{id} — …`)")
    })
}

/// The ids of the open tasks (`- [ ] **T<n>`) in `tasks.md`'s text.
fn open_tasks(tasks: &str) -> BTreeSet<String> {
    tasks
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix("- [ ] **T"))
        .map(|rest| {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            format!("T{digits}")
        })
        .filter(|id| id.len() > 1)
        .collect()
}

fn line_of(text: &str, at: usize) -> usize {
    text.get(..at).map_or(0, |t| t.matches('\n').count()) + 1
}

#[test]
fn library_code_does_not_print() {
    let sources = library_sources();
    // A broken test-module filter would quietly scan nothing.
    assert!(sources.len() > 400, "only {} files scanned", sources.len());
    let mut offenders = Vec::new();
    let mut counted = Vec::new();
    for src in sources {
        let found = prints(&src);
        let allowed = PRINT_ALLOWED.iter().find(|(f, _, _)| *f == src.rel);
        if allowed.is_some() {
            counted.push(src.rel.clone());
        }
        let limit = allowed.map_or(0, |(_, n, _)| *n);
        if found.len() != limit {
            let lines: Vec<usize> = found.iter().map(|&at| line_of(&src.text, at)).collect();
            offenders.push(format!(
                "{} has {} print macro(s) (allowed {limit}) at lines {lines:?}",
                src.rel,
                found.len()
            ));
        }
    }
    for (f, _, _) in PRINT_ALLOWED {
        assert!(
            counted.iter().any(|c| c == f),
            "{f} is in PRINT_ALLOWED but was not scanned; drop it so the list stays honest"
        );
    }
    assert!(
        offenders.is_empty(),
        "print macros in library code: {offenders:#?}. Report through the message drawer \
         instead, or (for a real stdout/stderr channel) update PRINT_ALLOWED in \
         tui/tests/code_lints.rs with the reason."
    );
}

#[test]
fn user_message_literals_go_through_t() {
    let mut offenders = Vec::new();
    let mut used = BTreeSet::new();
    for src in library_sources()
        .iter()
        .filter(|s| s.rel.starts_with("tui/src/"))
    {
        for (at, stmt) in untranslated_sinks(src) {
            let allowed = SINK_ALLOWED
                .iter()
                .position(|(f, start, _)| *f == src.rel && stmt.starts_with(start));
            if let Some(k) = allowed {
                used.insert(k);
            } else {
                offenders.push(format!("{}:{}: {stmt}", src.rel, line_of(&src.text, at)));
            }
        }
    }
    for (k, (f, start, _)) in SINK_ALLOWED.iter().enumerate() {
        assert!(
            used.contains(&k),
            "SINK_ALLOWED entry {f} / {start:?} no longer matches; drop it"
        );
    }
    assert!(
        offenders.is_empty(),
        "untranslated user-facing text: {offenders:#?}. Add an i18n key to every locale in \
         crates/vix-i18n/locales/ and pass t!(\"…\") instead of the literal."
    );
}

#[test]
fn the_scanner_reads_code_not_text() {
    let src = |text: &str| {
        let masked = mask(text);
        let (tests, _) = test_regions(&masked.code, text);
        Source {
            rel: "x.rs".into(),
            text: text.into(),
            masked,
            tests,
        }
    };
    // Literals, raw strings, comments and character literals are not code.
    let quiet = src(
        "fn a() { let s = \"println!(1)\"; let r = r#\"dbg!(\"x\")\"#; let c = '\"'; }\n\
         // println!(\"hi\");\n/* eprintln!() /* nested */ print!() */\n\
         fn b<'a>(x: &'a str) -> &'a str { x }\n",
    );
    assert_eq!(prints(&quiet), Vec::<usize>::new());
    // Test items are exempt; everything else is found.
    let loud = src(
        "fn a() { println!(\"x\"); }\n#[cfg(test)]\nmod tests { fn t() { dbg!(1); } }\n\
         #[cfg(not(test))]\nfn b() { eprint!(\"y\"); }\n",
    );
    assert_eq!(prints(&loud).len(), 2);
    // A sink with a bare literal is reported; with t!, a variable, a
    // placeholder-only format or a comparison it is not.
    let sinks = src("fn a(&mut self) {\n self.status = \"Saved\".to_string();\n\
         self.messages.warn(format!(\"{e}: {}\", x));\n\
         self.messages.error(t!(\"msg.x\", e = \"y\").to_string());\n\
         self.status = msg;\n if self.status == \"Done\" {}\n\
         app.messages.push(Level::Info, format!(\"{n} files\"));\n\
         match r { Err(e) => self.messages.error(format!(\"Paste failed: {e}\")), _ => {} }\n\
         match r { Ok(()) => self.status = String::new(), Err(_) => run(\"quit\") }\n}\n");
    let found: Vec<String> = untranslated_sinks(&sinks)
        .into_iter()
        .map(|(_, s)| s)
        .collect();
    assert_eq!(found.len(), 3, "{found:?}");
    assert!(found.iter().any(|s| s.ends_with("{e}\"))")), "{found:?}");
    assert!(found.iter().any(|s| s.contains("Saved")), "{found:?}");
    assert!(has_words("{n} files") && !has_words("{e}: {}") && !has_words("—"));
}

#[test]
fn every_ignored_test_says_why() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let tasks = std::fs::read_to_string(root.join("tasks.md")).unwrap_or_default();
    let open = open_tasks(&tasks);
    let sources = all_rust_sources(root);
    assert!(sources.len() > 400, "only {} files scanned", sources.len());
    // An example or a benchmark can hold a test too (T863): the scan must
    // reach both kinds of directory, which the workspace has today.
    for dir in ["/examples/", "/benches/"] {
        assert!(
            sources.iter().any(|(rel, _)| rel.contains(dir)),
            "no file under {dir} scanned"
        );
    }
    let mut offenders = Vec::new();
    let mut seen = 0usize;
    for (rel, text) in &sources {
        for (at, reason) in ignores(text) {
            seen = seen.saturating_add(1);
            if let Some(problem) = ignore_problem(reason.as_deref(), &open) {
                offenders.push(format!("{rel}:{}: {problem}", line_of(text, at)));
            }
        }
    }
    // The workspace has ignored tests today; finding none means a broken scan.
    assert!(seen > 0, "no #[ignore] found at all");
    assert!(
        offenders.is_empty(),
        "#[ignore] without a proper reason: {offenders:#?}. Say what the test needs that CI \
         cannot provide, or quarantine a known-flaky test with #[ignore = \"flaky: T<n>, \
         <reason>\"] naming an open task (spec/test \"Flaky tests\")."
    );
}

#[test]
fn the_ignore_rule_reads_attributes_and_reasons() {
    let text = "#[test]\n#[ignore]\nfn a() {}\n#[ignore = \"needs a real Vim\"]\nfn b() {}\n\
                // #[ignore]\nconst S: &str = \"#[ignore]\";\n#[ignore_me]\nfn c() {}\n\
                #[ignore   =   \"flaky: T9, races the watcher\"]\nfn d() {}\n";
    let found = ignores(text);
    let reasons: Vec<Option<&str>> = found.iter().map(|(_, r)| r.as_deref()).collect();
    assert_eq!(
        reasons,
        [
            None,
            Some("needs a real Vim"),
            Some("flaky: T9, races the watcher")
        ]
    );
    let open = open_tasks("- [ ] **T9 — Fix it.**\n- [x] **T10 — Done.**\n  - [ ] **T11 x\n");
    assert_eq!(
        open,
        ["T11", "T9"]
            .into_iter()
            .map(String::from)
            .collect::<BTreeSet<_>>()
    );
    let problem = |r: Option<&str>| ignore_problem(r, &open);
    assert!(problem(None).is_some());
    assert!(problem(Some("  ")).is_some());
    assert!(problem(Some("needs rust-analyzer")).is_none());
    assert!(problem(Some("flaky: T9, races the watcher")).is_none());
    assert!(problem(Some("flaky: T10, the task is done")).is_some());
    assert!(problem(Some("flaky: T12, no such task")).is_some());
    assert!(problem(Some("flaky, sometimes")).is_some());
    assert!(problem(Some("Flaky: T9, wrong case")).is_some());
    assert!(problem(Some("flaky: T9,")).is_some());
    assert!(problem(Some("flaky: T, no number")).is_some());
}

#[test]
fn the_pull_request_templates_match() {
    // GitHub reads `.github/`, Codeberg (Forgejo) `.forgejo/`; one checklist.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let github = std::fs::read_to_string(root.join(".github/pull_request_template.md")).unwrap();
    let forgejo = std::fs::read_to_string(root.join(".forgejo/pull_request_template.md")).unwrap();
    assert_eq!(
        github, forgejo,
        ".forgejo/pull_request_template.md differs from .github/pull_request_template.md; copy one over the other"
    );
}

/// The `ignore:` block of each `cargo` entry in a `dependabot.yml`, as
/// `(directory, the block's lines)`; an entry without one has an empty block.
fn dependabot_cargo_ignores(yml: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for entry in yml.split("\n  - package-ecosystem: ").skip(1) {
        if !entry.starts_with("\"cargo\"") {
            continue;
        }
        let directory = entry
            .lines()
            .find_map(|l| l.trim().strip_prefix("directory:"))
            .unwrap_or("")
            .trim()
            .to_string();
        let mut block = String::new();
        let mut inside = false;
        for line in entry.lines() {
            if line == "    ignore:" {
                inside = true;
            } else if inside && line.starts_with("    ") && !line.starts_with("     ") {
                inside = false;
            }
            if inside {
                block.push_str(line);
                block.push('\n');
            }
        }
        out.push((directory, block));
    }
    out
}

#[test]
fn the_dependabot_cargo_ignore_lists_match() {
    // gui/ and matrix/ reach root-workspace crates through path dependencies,
    // so an update there can bump a crate the root entry ignores.
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let yml = std::fs::read_to_string(root.join(".github/dependabot.yml")).unwrap();
    let entries = dependabot_cargo_ignores(&yml);
    let directories: Vec<&str> = entries.iter().map(|(d, _)| d.as_str()).collect();
    assert_eq!(
        directories,
        ["\"/\"", "\"/gui\"", "\"/matrix\""],
        "the cargo entries changed; update this test and spec/dependabot"
    );
    let (_, first) = &entries[0];
    assert!(
        first.contains("dependency-name"),
        "the root cargo entry has no ignore list"
    );
    for (directory, block) in &entries[1..] {
        assert_eq!(
            block, first,
            "the ignore list of the {directory} cargo entry differs from the root entry's; keep them in step"
        );
    }
}

#[test]
fn the_dependabot_reader_finds_each_ignore_block() {
    let yml = "updates:\n  - package-ecosystem: \"cargo\"\n    directory: \"/\"\n    ignore:\n      - dependency-name: \"a\"\n    groups:\n      g:\n  - package-ecosystem: \"github-actions\"\n    directory: \"/\"\n  - package-ecosystem: \"cargo\"\n    directory: \"/gui\"\n";
    let entries = dependabot_cargo_ignores(yml);
    assert_eq!(
        entries,
        [
            (
                "\"/\"".to_string(),
                "    ignore:\n      - dependency-name: \"a\"\n".to_string()
            ),
            ("\"/gui\"".to_string(), String::new()),
        ]
    );
}
