/// Return the language name inferred from a filename's extension, or `"unknown"`.
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
        _ => "unknown",
    }
    .to_string()
}

/// Like [`get_lang`], but a file with no extension falls back to its `#!` line,
/// so `#!/usr/bin/env python3` reads as Python (T658).
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
#[must_use]
pub fn comment(lang: &str) -> &'static str {
    match lang {
        "python" | "shell" | "toml" | "yaml" => "#",
        "lua" | "sql" => "--",
        _ => "//",
    }
}

/// Count how many whole `indent_unit` strings prefix `line`, up to `max_col` columns.
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
                Some(c) if c == ch => col += 1,
                _ => break 'outer,
            }
        }
        count += 1;
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
#[must_use]
pub fn calculate_end_position(start_row: usize, start_col: usize, text: &str) -> (usize, usize) {
    let mut end_row = start_row;
    let mut end_col = start_col;

    for ch in text.chars() {
        if ch == '\n' {
            end_row += 1;
            end_col = 0;
        } else {
            end_col += 1;
        }
    }

    (end_row, end_col)
}

#[cfg(test)]
mod tests {
    use super::{get_lang_for, shebang_lang};

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
}
