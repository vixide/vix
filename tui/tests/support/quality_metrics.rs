//! The quality-metrics dashboard (T833): `docs/reference/quality-metrics.md`,
//! written by the `list_commands` example with
//! `#[path = "../tests/support/quality_metrics.rs"] mod quality_metrics;` and
//! tested by `tests/quality_metrics.rs`. Every number is read from the source
//! tree as text (no build, no coverage run, no network), so the page is the
//! same on every machine for the same commit and takes well under a second.
#![allow(
    dead_code,
    reason = "shared by a test binary and an example, each using a part"
)]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

type Chars<'a> = std::iter::Peekable<std::str::Chars<'a>>;

/// Push `c` blanked: a newline stays, any other character becomes as many
/// spaces as its UTF-8 length, so byte offsets do not move.
fn blank(out: &mut String, c: char) {
    if c == '\n' {
        out.push('\n');
    } else {
        for _ in 0..c.len_utf8() {
            out.push(' ');
        }
    }
}

/// Whether `c` can be part of an identifier.
fn is_ident(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_alphanumeric() || c == '_')
}

/// After the opening `/*`: blank up to and including the matching `*/`.
fn mask_block_comment(chars: &mut Chars<'_>, out: &mut String) {
    let mut depth = 1usize;
    let mut last = ' ';
    for c in chars.by_ref() {
        blank(out, c);
        if last == '/' && c == '*' {
            depth = depth.saturating_add(1);
            last = ' ';
        } else if last == '*' && c == '/' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return;
            }
            last = ' ';
        } else {
            last = c;
        }
    }
}

/// After the opening `"`: blank the contents, keep the closing quote.
fn mask_string(chars: &mut Chars<'_>, out: &mut String) {
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                blank(out, c);
                if let Some(e) = chars.next() {
                    blank(out, e);
                }
            }
            '"' => {
                out.push('"');
                return;
            }
            _ => blank(out, c),
        }
    }
}

/// After an `r` that may start a raw string: when `#…"` follows, keep the
/// delimiters and blank the contents; otherwise (`r#ident`) do nothing.
fn mask_raw_string(chars: &mut Chars<'_>, out: &mut String) {
    let mut look = chars.clone();
    let mut hashes = 0usize;
    while look.next_if_eq(&'#').is_some() {
        hashes = hashes.saturating_add(1);
    }
    if look.peek() != Some(&'"') {
        return;
    }
    for _ in 0..hashes {
        chars.next();
        out.push('#');
    }
    chars.next();
    out.push('"');
    while let Some(c) = chars.next() {
        if c == '"' {
            let mut end = chars.clone();
            if (0..hashes).all(|_| end.next() == Some('#')) {
                out.push('"');
                for _ in 0..hashes {
                    chars.next();
                    out.push('#');
                }
                return;
            }
        }
        blank(out, c);
    }
}

/// After a `'`: blank a character literal's contents and keep its closing
/// quote; leave a lifetime alone.
fn mask_char(chars: &mut Chars<'_>, out: &mut String) {
    let mut look = chars.clone();
    let literal = match look.next() {
        Some('\\') => true,
        Some(_) => look.next() == Some('\''),
        None => false,
    };
    if !literal {
        return;
    }
    let mut escaped = false;
    for c in chars.by_ref() {
        if c == '\'' && !escaped {
            out.push('\'');
            return;
        }
        escaped = c == '\\' && !escaped;
        blank(out, c);
    }
}

/// Blank out comments and the contents of string and character literals,
/// byte for byte (newlines and the quote marks themselves are kept), so
/// offsets in the result are offsets in `text` and an attribute or macro
/// name inside a literal or a doc example is not read as code.
pub fn mask(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let (mut prev, mut prev2): (Option<char>, Option<char>) = (None, None);
    while let Some(c) = chars.next() {
        match c {
            '/' if chars.peek() == Some(&'/') => {
                out.push(' ');
                for c in chars.by_ref() {
                    blank(&mut out, c);
                    if c == '\n' {
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                out.push(' ');
                chars.next();
                out.push(' ');
                mask_block_comment(&mut chars, &mut out);
            }
            '"' => {
                out.push('"');
                mask_string(&mut chars, &mut out);
            }
            'r' if !is_ident(prev) || (prev == Some('b') && !is_ident(prev2)) => {
                out.push('r');
                mask_raw_string(&mut chars, &mut out);
            }
            '\'' => {
                out.push('\'');
                mask_char(&mut chars, &mut out);
            }
            _ => out.push(c),
        }
        prev2 = prev;
        prev = Some(c);
    }
    out
}

/// What one Rust file holds, counted on its [`mask`]ed text.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FileCounts {
    /// Lines in the file.
    pub lines: usize,
    /// `#[test]` and `#[tokio::test]` functions, property tests included.
    pub tests: usize,
    /// `#[test]` functions inside a `proptest!` block.
    pub properties: usize,
    /// `proptest!` blocks.
    pub proptest_blocks: usize,
    /// `#[ignore]` attributes, with a reason or without.
    pub ignored: usize,
    /// `#[ignore = "flaky…"]`: tests in quarantine.
    pub flaky: usize,
    /// `#[allow(…)]`, `#![allow(…)]`, `#[expect(…)]` and `#![expect(…)]`.
    pub suppressions: usize,
}

impl FileCounts {
    /// The field-by-field sum of `self` and `other`.
    pub fn add(&mut self, other: &Self) {
        self.lines = self.lines.saturating_add(other.lines);
        self.tests = self.tests.saturating_add(other.tests);
        self.properties = self.properties.saturating_add(other.properties);
        self.proptest_blocks = self.proptest_blocks.saturating_add(other.proptest_blocks);
        self.ignored = self.ignored.saturating_add(other.ignored);
        self.flaky = self.flaky.saturating_add(other.flaky);
        self.suppressions = self.suppressions.saturating_add(other.suppressions);
    }
}

/// Count what [`FileCounts`] describes in the Rust source `text`.
pub fn count_file(text: &str) -> FileCounts {
    let code = mask(text);
    let tests = code
        .matches("#[test]")
        .count()
        .saturating_add(code.matches("#[tokio::test").count());
    let suppressions = ["#[allow(", "#![allow(", "#[expect(", "#![expect("]
        .iter()
        .map(|n| code.matches(n).count())
        .fold(0usize, usize::saturating_add);
    let mut counts = FileCounts {
        lines: text.lines().count(),
        tests,
        suppressions,
        ..FileCounts::default()
    };
    for (p, needle) in code.match_indices("#[ignore") {
        let after = p.saturating_add(needle.len());
        let rest = code.get(after..).unwrap_or("").trim_start();
        if rest.starts_with(']') {
            counts.ignored = counts.ignored.saturating_add(1);
        } else if rest.starts_with('=') {
            counts.ignored = counts.ignored.saturating_add(1);
            let reason = code
                .get(after..)
                .and_then(|r| {
                    r.find('"')
                        .map(|q| q.saturating_add(after).saturating_add(1))
                })
                .and_then(|open| {
                    let close = code.get(open..)?.find('"')?.saturating_add(open);
                    text.get(open..close)
                })
                .unwrap_or("");
            if reason.trim_start().starts_with("flaky") {
                counts.flaky = counts.flaky.saturating_add(1);
            }
        }
    }
    for (p, needle) in code.match_indices("proptest!") {
        let before = code.get(..p).and_then(|b| b.chars().next_back());
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let after = p.saturating_add(needle.len());
        let Some(body) = code.get(after..).and_then(|rest| {
            let open = rest.find(['{', '(', '['])?;
            let mut depth = 0usize;
            for (i, c) in rest.char_indices().skip_while(|(i, _)| *i < open) {
                match c {
                    '{' | '(' | '[' => depth = depth.saturating_add(1),
                    '}' | ')' | ']' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            return rest.get(open..i);
                        }
                    }
                    _ => {}
                }
            }
            rest.get(open..)
        }) else {
            continue;
        };
        counts.proptest_blocks = counts.proptest_blocks.saturating_add(1);
        counts.properties = counts
            .properties
            .saturating_add(body.matches("#[test]").count());
    }
    counts
}

/// One package's numbers.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Package {
    /// The package name (its directory name; `vix-tui` for `tui/`).
    pub name: String,
    /// Files under `src/`.
    pub src: FileCounts,
    /// Files under `tests/`.
    pub tests: FileCounts,
    /// Every other Rust file: `benches/`, `examples/`, `build.rs`.
    pub other: FileCounts,
    /// Rust files directly under `benches/`.
    pub benches: usize,
}

impl Package {
    /// All of the package's files together.
    pub fn total(&self) -> FileCounts {
        let mut t = self.src;
        t.add(&self.tests);
        t.add(&self.other);
        t
    }
}

/// Every `.rs` file under `dir`, skipping `target` and hidden directories.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        let name = e.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if name != "target" && !name.starts_with('.') {
                rust_files(&path, out);
            }
        } else if path.extension().is_some_and(|x| x == "rs") {
            out.push(path);
        }
    }
}

/// Scan the package in `dir`.
pub fn scan_package(name: &str, dir: &Path) -> Package {
    let mut files = Vec::new();
    rust_files(dir, &mut files);
    files.sort();
    let mut package = Package {
        name: name.to_owned(),
        ..Package::default()
    };
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        let counts = count_file(&text);
        let rel = f.strip_prefix(dir).unwrap_or(&f);
        let mut parts = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned());
        let top = parts.next().unwrap_or_default();
        let is_direct = parts.next().is_some() && parts.next().is_none();
        match top.as_str() {
            "src" => package.src.add(&counts),
            "tests" => package.tests.add(&counts),
            _ => {
                if top == "benches" && is_direct {
                    package.benches = package.benches.saturating_add(1);
                }
                package.other.add(&counts);
            }
        }
    }
    package
}

/// The packages of the main workspace (`tui/` as `vix-tui`, then
/// `crates/*`) and the desktop GUIs (`gui/vix-*`), each sorted by name.
pub fn packages(root: &Path) -> (Vec<Package>, Vec<Package>) {
    fn dirs(root: &Path, group: &str) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(root.join(group))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().join("Cargo.toml").is_file())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.starts_with("vix-"))
            .collect();
        names.sort();
        names
    }
    let mut workspace = vec![scan_package("vix-tui", &root.join("tui"))];
    workspace.extend(
        dirs(root, "crates")
            .iter()
            .map(|n| scan_package(n, &root.join("crates").join(n))),
    );
    workspace.sort_by(|a, b| a.name.cmp(&b.name));
    let gui = dirs(root, "gui")
        .iter()
        .map(|n| scan_package(n, &root.join("gui").join(n)))
        .collect();
    (workspace, gui)
}

/// The translation catalog: how many keys, and how many each locale has.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Catalog {
    /// Catalog files read.
    pub files: usize,
    /// Keys across them (`_version` and other `_` entries do not count).
    pub keys: usize,
    /// Locale code -> keys with a non-empty string in that locale.
    pub per_locale: BTreeMap<String, usize>,
}

/// Add the YAML catalog file `text` to `catalog`.
///
/// # Errors
///
/// When `text` is not a YAML mapping.
pub fn add_catalog_file(catalog: &mut Catalog, text: &str) -> Result<(), String> {
    let map: BTreeMap<String, serde_norway::Value> =
        serde_norway::from_str(text).map_err(|e| e.to_string())?;
    catalog.files = catalog.files.saturating_add(1);
    for (key, value) in map {
        if key.starts_with('_') {
            continue;
        }
        catalog.keys = catalog.keys.saturating_add(1);
        let Some(locales) = value.as_mapping() else {
            continue;
        };
        for (locale, s) in locales {
            let (Some(locale), Some(s)) = (locale.as_str(), s.as_str()) else {
                continue;
            };
            if !s.is_empty() {
                let n = catalog.per_locale.entry(locale.to_owned()).or_insert(0);
                *n = n.saturating_add(1);
            }
        }
    }
    Ok(())
}

/// Read every `*.yml` file in `dir`.
///
/// # Panics
///
/// When a catalog file is not valid YAML: the generator should fail loudly.
pub fn catalog(dir: &Path) -> Catalog {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "yml"))
        .collect();
    files.sort();
    let mut c = Catalog::default();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap_or_default();
        add_catalog_file(&mut c, &text).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
    }
    c
}

/// `(entries, the sum of their counts)` of the `const NAME: &[(…)] = &[…];`
/// allowlist in `src`, where an entry is a `(` line and its count is the
/// first line of it that is a bare number (`PRINT_ALLOWED`'s prints).
/// `None` when `src` has no such constant.
pub fn allowlist(src: &str, name: &str) -> Option<(usize, usize)> {
    let start = src.find(&format!("const {name}:"))?;
    let rest = src.get(start..)?;
    let decl_end = rest.find('\n').unwrap_or(rest.len());
    if rest.get(..decl_end)?.trim_end().ends_with("&[];") {
        return Some((0, 0));
    }
    let body = rest.get(..rest.find("\n];")?)?;
    let mut entries = 0usize;
    let mut sum = 0usize;
    let mut counted = true;
    for line in body.lines() {
        let line = line.trim();
        if line == "(" {
            entries = entries.saturating_add(1);
            counted = false;
        } else if !counted && let Ok(n) = line.trim_end_matches(',').parse::<usize>() {
            sum = sum.saturating_add(n);
            counted = true;
        }
    }
    Some((entries, sum))
}

/// `n` as a percentage of `total` with one decimal, rounded down:
/// `"99.9%"`; `"n/a"` for an empty total.
pub fn percent(n: usize, total: usize) -> String {
    let Some(per_mille) = n.checked_mul(1000).and_then(|x| x.checked_div(total)) else {
        return "n/a".to_owned();
    };
    format!("{}.{}%", per_mille / 10, per_mille % 10)
}

/// `n` with a comma between each group of three digits: `"330,838"`.
pub fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len().saturating_add(digits.len() / 3));
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len().saturating_sub(i)) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The length of the `key = [ … ]` array under `[table]` (or at the top
/// level for an empty `table`) of the TOML `text`; 0 when it is missing.
pub fn toml_array_len(text: &str, table: &str, key: &str) -> usize {
    let Ok(doc) = text.parse::<toml::Table>() else {
        return 0;
    };
    let scope = if table.is_empty() {
        Some(&doc)
    } else {
        doc.get(table).and_then(toml::Value::as_table)
    };
    scope
        .and_then(|t| t.get(key))
        .and_then(toml::Value::as_array)
        .map_or(0, Vec::len)
}

/// The summary table's totals over `packages`: `(all files, src, tests,
/// elsewhere, benchmark files)`.
fn totals(packages: &[Package]) -> (FileCounts, FileCounts, FileCounts, FileCounts, usize) {
    let mut all = FileCounts::default();
    let (mut src, mut tests, mut other) = (
        FileCounts::default(),
        FileCounts::default(),
        FileCounts::default(),
    );
    let mut benches = 0usize;
    for p in packages {
        all.add(&p.total());
        src.add(&p.src);
        tests.add(&p.tests);
        other.add(&p.other);
        benches = benches.saturating_add(p.benches);
    }
    (all, src, tests, other, benches)
}

/// The page's "Summary" section.
fn write_summary(
    out: &mut String,
    root: &Path,
    workspace: &[Package],
    gui: &[Package],
    cat: &Catalog,
) {
    let everything: Vec<Package> = workspace.iter().chain(gui).cloned().collect();
    let (all, src, tests, other, benches) = totals(&everything);
    let fuzz_targets = std::fs::read_dir(root.join("fuzz/fuzz_targets"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
        .count();
    let total_tests = all.tests;
    let ignored_note = if all.flaky == 0 {
        "none in flaky quarantine".to_owned()
    } else {
        format!("{} in flaky quarantine", all.flaky)
    };
    let _ = write!(
        *out,
        "## Summary\n\n\
         | Metric | Value |\n\
         |---|---|\n\
         | Workspace crates (`tui/` and `crates/`) | {} |\n\
         | Desktop GUI crates (`gui/`) | {} |\n\
         | Lines of Rust in `src/` | {} |\n\
         | Lines of Rust in `tests/` | {} |\n\
         | Lines of Rust elsewhere (`benches/`, `examples/`, `build.rs`) | {} |\n\
         | Test functions (`#[test]`, `#[tokio::test]`) | {} |\n\
         | Property tests (`#[test]` in `proptest!`) | {} in {} blocks |\n\
         | Fuzz targets (`fuzz/fuzz_targets/`) | {} |\n\
         | Criterion benchmark files (`benches/`) | {} |\n\
         | Ignored tests (`#[ignore]`) | {} ({ignored_note}) |\n\
         | Lint suppressions in `src/` (`allow`/`expect` attributes) | {} |\n\
         | Translation keys (`crates/vix-i18n/locales/`) | {} in {} files |\n\n",
        workspace.len(),
        gui.len(),
        thousands(src.lines),
        thousands(tests.lines),
        thousands(other.lines),
        thousands(total_tests),
        thousands(all.properties),
        all.proptest_blocks,
        fuzz_targets,
        benches,
        all.ignored,
        src.suppressions,
        thousands(cat.keys),
        cat.files,
    );
}

/// The page's "Crates per layer" section.
fn write_layers(out: &mut String, layers: &BTreeMap<String, usize>) {
    let top = layers.values().copied().max().unwrap_or(0);
    out.push_str(
        "## Crates per layer\n\n\
         A crate's layer is the longest chain of workspace dependencies below it;\n\
         each crate is listed in [Crate Layers](crate-layers.md).\n\n\
         | Layer | Crates |\n\
         |---|---|\n",
    );
    for n in (0..=top).rev() {
        let count = layers.values().filter(|l| **l == n).count();
        let _ = writeln!(*out, "| {n} | {count} |");
    }
}

/// The page's "Translations" section.
fn write_translations(out: &mut String, cat: &Catalog) {
    let mut locales: Vec<(&String, &usize)> = cat.per_locale.iter().collect();
    locales.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    let _ = write!(
        *out,
        "\n## Translations\n\n\
         {} keys; a locale missing a key falls back to `en`. Locales with only a\n\
         few keys are deliberate partial catalogs.\n\n\
         | Locale | Keys | Missing | Complete |\n\
         |---|---|---|---|\n",
        thousands(cat.keys)
    );
    for (locale, n) in locales {
        let _ = writeln!(
            *out,
            "| `{locale}` | {} | {} | {} |",
            thousands(*n),
            thousands(cat.keys.saturating_sub(*n)),
            percent(*n, cat.keys)
        );
    }
}

/// The page's "Allowlists" section.
fn write_allowlists(out: &mut String, root: &Path) {
    let read = |rel: &str| std::fs::read_to_string(root.join(rel)).unwrap_or_default();
    let lints = read("tui/tests/code_lints.rs");
    let (print_files, prints) = allowlist(&lints, "PRINT_ALLOWED").unwrap_or((0, 0));
    let (sink_entries, _) = allowlist(&lints, "SINK_ALLOWED").unwrap_or((0, 0));
    let secret_allows = read("scripts/secret-patterns")
        .lines()
        .filter(|l| l.starts_with("allow\t"))
        .count();
    let equivalent_mutants = toml_array_len(&read(".cargo/mutants.toml"), "", "exclude_re");
    let advisories = toml_array_len(&read("deny.toml"), "advisories", "ignore");
    let _ = write!(
        *out,
        "\n## Allowlists\n\n\
         Each entry is an exception someone wrote down with a reason; a growing\n\
         count is worth a look.\n\n\
         | Allowlist | Entries | Where |\n\
         |---|---|---|\n\
         | Files that may print | {print_files} ({prints} print macros) | `PRINT_ALLOWED` in `tui/tests/code_lints.rs` |\n\
         | Untranslated message-sink literals | {sink_entries} | `SINK_ALLOWED` in `tui/tests/code_lints.rs` |\n\
         | Files that may hold secret-shaped text | {secret_allows} | `allow` lines in `scripts/secret-patterns` |\n\
         | Equivalent mutants | {equivalent_mutants} | `exclude_re` in `.cargo/mutants.toml` |\n\
         | Ignored security advisories | {advisories} | `[advisories] ignore` in `deny.toml` |\n"
    );
}

/// The page's "Per crate" and "Desktop GUIs" tables.
fn write_per_crate(out: &mut String, workspace: &[Package], gui: &[Package]) {
    out.push_str(
        "\n## Per crate\n\n\
         *src* and *tests* are lines of Rust under `src/` (in-file\n\
         `#[cfg(test)]` modules included) and `tests/`; *test fns* counts every\n\
         file of the crate, property tests included.\n\n\
         | Crate | src | tests | Test fns | Property tests | Ignored |\n\
         |---|---|---|---|---|---|\n",
    );
    let row = |out: &mut String, p: &Package| {
        let t = p.total();
        let _ = writeln!(
            *out,
            "| `{}` | {} | {} | {} | {} | {} |",
            p.name,
            thousands(p.src.lines),
            thousands(p.tests.lines),
            thousands(t.tests),
            t.properties,
            t.ignored
        );
    };
    for p in workspace {
        row(out, p);
    }
    out.push_str(
        "\n## Desktop GUIs\n\n\
         | Crate | src | tests | Test fns | Property tests | Ignored |\n\
         |---|---|---|---|---|---|\n",
    );
    for p in gui {
        row(out, p);
    }
}

/// `docs/reference/quality-metrics.md` for the repository at `root`, given
/// each workspace crate's layer (`crate_graph::layers`).
pub fn render(root: &Path, layers: &BTreeMap<String, usize>) -> String {
    let (workspace, gui) = packages(root);
    let cat = catalog(&root.join("crates/vix-i18n/locales"));
    let mut out = String::from(
        "# Quality Metrics\n\n\
         Generated by `cargo run --example list_commands -- --write` (T833) from the\n\
         source tree; do not edit by hand. `scripts/check` fails when it is stale.\n\n\
         Every number here is read from the files as text, with comments and the\n\
         contents of string literals skipped: no build, no test run, no network. So\n\
         the page is the same on every machine for a given commit, and a change\n\
         that adds tests, translations or allowlist entries shows up in its diff.\n\
         The counts are a scan, not the compiler's view: a test generated by a\n\
         macro other than `proptest!` is not counted, and lines include blank\n\
         lines and comments.\n\n",
    );
    write_summary(&mut out, root, &workspace, &gui, &cat);
    write_layers(&mut out, layers);
    write_translations(&mut out, &cat);
    write_allowlists(&mut out, root);
    write_per_crate(&mut out, &workspace, &gui);
    out.push_str(
        "\n## Not on this page\n\n\
         Metrics that need a build, a long run or a particular machine are kept\n\
         where they are measured:\n\n\
         - **Line coverage**: `cargo tarpaulin --config tarpaulin.toml` (LLVM\n  \
         engine; `cargo llvm-cov` works too), locally through `bin/test-coverage`\n  \
         and in CI's coverage job; see [`spec/ci`](../../spec/ci/index.md).\n\
         - **Mutation score**: `.github/workflows/mutants.yml` runs `cargo mutants`\n  \
         weekly and uploads `mutants.out/`; see [`spec/test`](../../spec/test/index.md#mutation-testing).\n\
         - **Flakiness**: `scripts/flaky-hunt` reruns a test under load; see\n  \
         [`spec/test`](../../spec/test/index.md#flaky-tests).\n\
         - **Speed and memory**: the Criterion benchmarks and the `VIX_PROFILE`\n  \
         frame profile; see [Performance](../performance/index.md).\n",
    );
    out
}
