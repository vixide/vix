//! Small pure helpers shared by the editor: extension/shebang language
//! detection, per-language indent and comment conventions, indent counting, hex
//! colour parsing and end-position arithmetic. Language names here are the
//! Tree-sitter grammar keys used by [`crate::code`]. See
//! `crates/vix-editor-core/spec/index.md`.

/// `a + b`, or the declared `fallback` when the sum does not fit (see
/// `sub_or` for why the fallback is a parameter).
fn add_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_add(b).unwrap_or(fallback)
}

/// Return the language name inferred from a filename's extension, or `"unknown"`.
///
/// Only the final extension is consulted (case-sensitive); the names returned
/// are grammar keys, e.g. `"c_sharp"` for `.cs`.
///
/// # Examples
///
/// ```
/// use vix_editor_core::utils::get_lang;
/// assert_eq!(get_lang("src/main.rs"), "rust");
/// assert_eq!(get_lang("Makefile"), "unknown");
/// ```
#[must_use]
pub fn get_lang(filename: &str) -> String {
    let extension = std::path::Path::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");

    match extension {
        "rs" => "rust",
        "js" | "jsx" => "javascript",
        "ts" | "tsx" => "typescript",
        "py" => "python",
        "go" => "go",
        "java" => "java",
        "cpp" => "cpp",
        "c" => "c",
        "cs" => "c_sharp",
        "html" => "html",
        "css" => "css",
        "json" => "json",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "sh" | "bash" => "shell",
        "md" => "markdown",
        "org" => "org",
        _ => "unknown",
    }
    .to_string()
}

/// Like [`get_lang`], but a file with no extension falls back to its `#!` line,
/// so `#!/usr/bin/env python3` reads as Python (T658).
///
/// A file with *any* extension never consults the shebang, even if the
/// extension is unrecognized.
///
/// # Examples
///
/// ```
/// use vix_editor_core::utils::get_lang_for;
/// assert_eq!(get_lang_for("run", "#!/usr/bin/env python3\n"), "python");
/// assert_eq!(get_lang_for("notes.txt", "#!/bin/sh\n"), "unknown");
/// ```
#[must_use]
pub fn get_lang_for(filename: &str, text: &str) -> String {
    let lang = get_lang(filename);
    if lang != "unknown" || std::path::Path::new(filename).extension().is_some() {
        return lang;
    }
    shebang_lang(text.lines().next().unwrap_or("")).map_or(lang, str::to_string)
}

/// The language a `#!` line names, by its interpreter: `#!/bin/sh`,
/// `#!/usr/bin/env python3`, `#!/usr/bin/env -S node --flag`. Version suffixes
/// (`python3.12`) are ignored. `None` for anything else.
#[must_use]
pub fn shebang_lang(first_line: &str) -> Option<&'static str> {
    let mut words = first_line.strip_prefix("#!")?.split_whitespace();
    let mut program = words.next()?.rsplit('/').next()?;
    if program == "env" {
        program = words.find(|w| !w.starts_with('-') && !w.contains('='))?;
    }
    let name = program.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
    match name {
        "sh" | "bash" | "zsh" | "dash" | "ksh" | "ash" => Some("shell"),
        "python" | "pypy" => Some("python"),
        "node" | "nodejs" => Some("javascript"),
        "ts-node" | "tsx" => Some("typescript"),
        _ => None,
    }
}

/// Return the default indentation string for a language (spaces or a tab).
///
/// Four spaces for the languages listed, a tab for Go and C#, two spaces
/// otherwise.
///
/// # Examples
///
/// ```
/// use vix_editor_core::utils::indent;
/// assert_eq!(indent("rust"), "    ");
/// assert_eq!(indent("go"), "\t");
/// assert_eq!(indent("yaml"), "  ");
/// ```
#[must_use]
pub fn indent(lang: &str) -> String {
    match lang {
        "rust" | "python" | "php" | "toml" | "c" | "cpp" | "zig" | "kotlin" | "erlang" | "html"
        | "sql" => "    ".to_string(),
        "go" | "c_sharp" => "\t".to_string(),

        _ => "  ".to_string(),
    }
}

/// Return the line-comment prefix for a language (e.g. `"//"`, `"#"`, `"--"`).
///
/// Unknown languages fall back to `"//"`.
///
/// # Examples
///
/// ```
/// use vix_editor_core::utils::comment;
/// assert_eq!(comment("python"), "#");
/// assert_eq!(comment("sql"), "--");
/// assert_eq!(comment("rust"), "//");
/// ```
#[must_use]
pub fn comment(lang: &str) -> &'static str {
    match lang {
        "python" | "shell" | "toml" | "yaml" | "org" => "#",
        "lua" | "sql" => "--",
        _ => "//",
    }
}

/// Count how many whole `indent_unit` strings prefix `line`, up to `max_col` columns.
///
/// Only *complete* units count: a partial trailing unit (e.g. three spaces
/// with a four-space unit) is ignored. `max_col`, when given, stops counting
/// once that many characters have been consumed (the check happens after each
/// whole unit, so the result can overshoot `max_col` by less than one unit). An
/// empty `indent_unit` yields `0`.
#[must_use]
pub fn count_indent_units(
    line: ropey::RopeSlice<'_>,
    indent_unit: &str,
    max_col: Option<usize>,
) -> usize {
    if indent_unit.is_empty() {
        return 0;
    }

    let mut chars = line.chars();
    let mut count = 0;
    let mut col = 0;
    let indent_chars: Vec<char> = indent_unit.chars().collect();

    'outer: loop {
        for &ch in &indent_chars {
            match chars.next() {
                Some(c) if c == ch => col = add_or(col, 1, usize::MAX),
                _ => break 'outer,
            }
        }
        count = add_or(count, 1, usize::MAX);
        if let Some(max) = max_col
            && col >= max
        {
            break;
        }
    }

    count
}

/// Parse a `#RRGGBB` (or `RRGGBB`) hex string into an `(r, g, b)` triple.
///
/// Any component that is missing or invalid is treated as `0`. Delegates to
/// `vix-hex-rgb` (Run H, T530) -- this crate and `vix-base16` used to each
/// hand-roll an identical lenient hex parser.
#[must_use]
pub fn rgb(hex: &str) -> (u8, u8, u8) {
    vix_hex_rgb::rgb(hex)
}

/// Calculate end position by walking through the text
/// Returns (`end_row`, `end_col`) starting from (`start_row`, `start_col`)
///
/// Columns count `char`s (not bytes or graphemes); each `'\n'` bumps the row
/// and resets the column to 0.
///
/// # Examples
///
/// ```
/// use vix_editor_core::utils::calculate_end_position;
/// assert_eq!(calculate_end_position(2, 4, "ab"), (2, 6));
/// assert_eq!(calculate_end_position(2, 4, "ab\ncde"), (3, 3));
/// ```
#[must_use]
pub fn calculate_end_position(start_row: usize, start_col: usize, text: &str) -> (usize, usize) {
    let mut end_row = start_row;
    let mut end_col = start_col;

    for ch in text.chars() {
        if ch == '\n' {
            end_row = add_or(end_row, 1, usize::MAX);
            end_col = 0;
        } else {
            end_col = add_or(end_col, 1, usize::MAX);
        }
    }

    (end_row, end_col)
}

#[cfg(test)]
mod tests {
    use super::{
        calculate_end_position, comment, count_indent_units, get_lang, get_lang_for, indent, rgb,
        shebang_lang,
    };

    /// T658: an extensionless script takes its language from the `#!` line.
    #[test]
    fn shebang_names_the_language() {
        assert_eq!(shebang_lang("#!/bin/sh"), Some("shell"));
        assert_eq!(shebang_lang("#!/usr/bin/env python3"), Some("python"));
        assert_eq!(shebang_lang("#!/usr/bin/python3.12 -u"), Some("python"));
        assert_eq!(
            shebang_lang("#!/usr/bin/env -S node --no-warnings"),
            Some("javascript")
        );
        assert_eq!(shebang_lang("#!/usr/bin/env LANG=C bash"), Some("shell"));
        assert_eq!(shebang_lang("#!/usr/bin/env perl"), None);
        assert_eq!(shebang_lang("# not a shebang"), None);
    }

    /// The extension wins; the shebang only fills in for files without one.
    #[test]
    fn extension_beats_shebang() {
        assert_eq!(get_lang_for("deploy", "#!/bin/bash\necho hi\n"), "shell");
        assert_eq!(get_lang_for("main.rs", "#!/bin/bash\n"), "rust");
        assert_eq!(get_lang_for("notes.txt", "#!/bin/bash\n"), "unknown");
        assert_eq!(get_lang_for("README", "hello\n"), "unknown");
    }

    #[test]
    fn get_lang_maps_every_known_extension() {
        for (file, lang) in [
            ("a.rs", "rust"),
            ("a.js", "javascript"),
            ("a.jsx", "javascript"),
            ("a.ts", "typescript"),
            ("a.tsx", "typescript"),
            ("a.py", "python"),
            ("a.go", "go"),
            ("a.java", "java"),
            ("a.cpp", "cpp"),
            ("a.c", "c"),
            ("a.cs", "c_sharp"),
            ("a.html", "html"),
            ("a.css", "css"),
            ("a.json", "json"),
            ("a.toml", "toml"),
            ("a.yaml", "yaml"),
            ("a.yml", "yaml"),
            ("a.sh", "shell"),
            ("a.bash", "shell"),
            ("a.md", "markdown"),
            ("a.org", "org"),
            ("a.zzz", "unknown"),
            ("Makefile", "unknown"),
            // Extensions are case-sensitive.
            ("A.RS", "unknown"),
        ] {
            assert_eq!(get_lang(file), lang, "{file}");
        }
    }

    #[test]
    fn shebang_handles_more_interpreters_and_degenerate_lines() {
        assert_eq!(shebang_lang("#!/usr/bin/zsh"), Some("shell"));
        assert_eq!(shebang_lang("#!/usr/bin/env pypy3"), Some("python"));
        assert_eq!(shebang_lang("#!/usr/bin/nodejs"), Some("javascript"));
        assert_eq!(shebang_lang("#!/usr/bin/env tsx"), Some("typescript"));
        assert_eq!(shebang_lang("#!/usr/bin/env ts-node"), Some("typescript"));
        // `env` with nothing runnable after it, and a bare `#!`.
        assert_eq!(shebang_lang("#!/usr/bin/env -S"), None);
        assert_eq!(shebang_lang("#!"), None);
    }

    #[test]
    fn indent_and_comment_conventions() {
        assert_eq!(indent("rust"), "    ");
        assert_eq!(indent("html"), "    ");
        assert_eq!(indent("go"), "\t");
        assert_eq!(indent("c_sharp"), "\t");
        assert_eq!(indent("yaml"), "  ");
        assert_eq!(indent("nonsense"), "  ");
        assert_eq!(comment("python"), "#");
        assert_eq!(comment("org"), "#");
        assert_eq!(comment("lua"), "--");
        assert_eq!(comment("sql"), "--");
        assert_eq!(comment("rust"), "//");
        assert_eq!(comment("nonsense"), "//");
    }

    #[test]
    fn count_indent_units_counts_whole_units_only() {
        let r = |s: &str| ropey::Rope::from_str(s);
        assert_eq!(
            count_indent_units(r("        x").slice(..), "    ", None),
            2
        );
        // A partial trailing unit is ignored.
        assert_eq!(count_indent_units(r("      x").slice(..), "    ", None), 1);
        assert_eq!(count_indent_units(r("\t\tx").slice(..), "\t", None), 2);
        assert_eq!(count_indent_units(r("x").slice(..), "  ", None), 0);
        assert_eq!(count_indent_units(r("    x").slice(..), "", None), 0);
        // `max_col` stops counting once that many columns are consumed.
        assert_eq!(
            count_indent_units(r("            x").slice(..), "    ", Some(4)),
            1
        );
        assert_eq!(
            count_indent_units(r("            x").slice(..), "    ", Some(8)),
            2
        );
    }

    #[test]
    fn rgb_and_end_position() {
        assert_eq!(rgb("#ff8000"), (255, 128, 0));
        assert_eq!(rgb("00ff00"), (0, 255, 0));
        assert_eq!(rgb("nonsense"), (0, 0, 0));
        assert_eq!(calculate_end_position(2, 4, "ab"), (2, 6));
        assert_eq!(calculate_end_position(2, 4, "ab\ncde"), (3, 3));
        assert_eq!(calculate_end_position(0, 0, ""), (0, 0));
    }
}
