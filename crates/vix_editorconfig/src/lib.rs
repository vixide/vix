//! Minimal [EditorConfig](https://editorconfig.org/) support: resolve the indent
//! and on-save normalization properties that apply to a file.
//!
//! This is a focused, dependency-free reader (it reuses the crate's `regex`). It
//! walks up from the file's directory collecting `.editorconfig` files, stopping
//! at one whose preamble sets `root = true`, parses the INI-style sections, and
//! matches their glob patterns against the file. Properties from `.editorconfig`
//! files nearer the file win; within a file, a later matching section wins.
//!
//! Recognized properties: `indent_style`, `indent_size`, `tab_width`,
//! `trim_trailing_whitespace`, `insert_final_newline`. The glob matcher supports
//! the common forms (`*`, `**`, `?`, `[...]`, `{a,b}`, `{s1,s2}` extension lists);
//! exotic patterns (`{num1..num2}`) are not handled.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::path::Path;

/// The `EditorConfig` properties that apply to a file, each `None` when unset.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Resolved {
    /// `Some(true)` for `indent_style = tab`, `Some(false)` for `space`.
    pub indent_is_tab: Option<bool>,
    /// `indent_size` as a number (`indent_size = tab` is recorded as `None` here
    /// and falls back to `tab_width`).
    pub indent_size: Option<usize>,
    /// `tab_width`.
    pub tab_width: Option<usize>,
    /// `trim_trailing_whitespace`.
    pub trim_trailing_whitespace: Option<bool>,
    /// `insert_final_newline`.
    pub insert_final_newline: Option<bool>,
}

impl Resolved {
    /// The indent string Tab should insert per this config, if it specifies one:
    /// a tab for `indent_style = tab`, else `indent_size`/`tab_width` spaces.
    #[must_use]
    pub fn indent_string(&self) -> Option<String> {
        if self.indent_is_tab? {
            Some("\t".to_string())
        } else {
            let n = self.indent_size.or(self.tab_width).unwrap_or(4).max(1);
            Some(" ".repeat(n))
        }
    }
}

/// Resolve the `EditorConfig` properties for `path` by reading `.editorconfig`
/// files from its directory upward. Returns an all-`None` [`Resolved`] when there
/// are no applicable files.
#[must_use]
pub fn resolve(path: &Path) -> Resolved {
    let mut chain: Vec<(std::path::PathBuf, String)> = Vec::new();
    let mut dir = path.parent();
    while let Some(d) = dir {
        if let Ok(text) = std::fs::read_to_string(d.join(".editorconfig")) {
            let is_root = preamble_is_root(&text);
            chain.push((d.to_path_buf(), text));
            if is_root {
                break;
            }
        }
        dir = d.parent();
    }
    let mut out = Resolved::default();
    // Farthest first so nearer files override.
    for (base, text) in chain.iter().rev() {
        apply_file(&mut out, base, text, path);
    }
    out
}

/// Whether the preamble (lines before the first `[section]`) sets `root = true`.
fn preamble_is_root(text: &str) -> bool {
    for line in text.lines() {
        let line = strip_comment(line).trim();
        if line.starts_with('[') {
            break;
        }
        if let Some((k, v)) = split_kv(line)
            && k.eq_ignore_ascii_case("root")
        {
            return v.eq_ignore_ascii_case("true");
        }
    }
    false
}

/// Apply every matching section of one `.editorconfig` (`text`, located at `base`)
/// to `out`, in file order so later sections win.
fn apply_file(out: &mut Resolved, base: &Path, text: &str, path: &Path) {
    let rel = path.strip_prefix(base).unwrap_or(path);
    let rel = rel.to_string_lossy().replace('\\', "/");
    let mut matching = false;
    for line in text.lines() {
        let line = strip_comment(line).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(pat) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            matching = glob_matches(pat, &rel);
            continue;
        }
        if !matching {
            continue;
        }
        if let Some((k, v)) = split_kv(line) {
            apply_kv(out, &k.to_ascii_lowercase(), v.trim());
        }
    }
}

/// Apply one `key = value` pair to `out`.
fn apply_kv(out: &mut Resolved, key: &str, value: &str) {
    match key {
        "indent_style" => match value.to_ascii_lowercase().as_str() {
            "tab" => out.indent_is_tab = Some(true),
            "space" => out.indent_is_tab = Some(false),
            _ => {}
        },
        "indent_size" => {
            if !value.eq_ignore_ascii_case("tab")
                && let Ok(n) = value.parse()
            {
                out.indent_size = Some(n);
            }
        }
        "tab_width" => {
            if let Ok(n) = value.parse() {
                out.tab_width = Some(n);
            }
        }
        "trim_trailing_whitespace" => out.trim_trailing_whitespace = parse_bool(value),
        "insert_final_newline" => out.insert_final_newline = parse_bool(value),
        _ => {}
    }
}

/// Parse an `EditorConfig` boolean (`true`/`false`, case-insensitive).
fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Strip a trailing `;`/`#` comment from a line (`EditorConfig` comments).
fn strip_comment(line: &str) -> &str {
    let cut = line.find(['#', ';']).unwrap_or(line.len());
    &line[..cut]
}

/// Split a `key = value` line, trimming both sides.
fn split_kv(line: &str) -> Option<(&str, &str)> {
    let (k, v) = line.split_once('=')?;
    Some((k.trim(), v.trim()))
}

/// Whether `EditorConfig` glob `pat` matches the file path `rel` (relative to the
/// `.editorconfig` directory, with `/` separators). A pattern without a `/`
/// matches the file name in any directory.
fn glob_matches(pat: &str, rel: &str) -> bool {
    let anchored = pat.contains('/');
    let subject = if anchored {
        rel.to_string()
    } else {
        rel.rsplit('/').next().unwrap_or(rel).to_string()
    };
    let re = format!("^{}$", glob_to_regex(pat));
    regex::Regex::new(&re).is_ok_and(|r| r.is_match(&subject))
}

/// Translate an `EditorConfig` glob into a regex body.
fn glob_to_regex(pat: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = pat.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '*' => {
                if chars.get(i + 1) == Some(&'*') {
                    out.push_str(".*");
                    i += 1;
                } else {
                    out.push_str("[^/]*");
                }
            }
            '?' => out.push_str("[^/]"),
            '{' => {
                // Brace alternation: {a,b,c} -> (a|b|c).
                let mut j = i + 1;
                let mut inner = String::new();
                while j < chars.len() && chars[j] != '}' {
                    inner.push(chars[j]);
                    j += 1;
                }
                if j < chars.len() {
                    let alts: Vec<String> = inner.split(',').map(regex_escape).collect();
                    out.push('(');
                    out.push_str(&alts.join("|"));
                    out.push(')');
                    i = j;
                } else {
                    out.push_str("\\{");
                }
            }
            '[' => {
                // Character class: pass through, mapping a leading ! to ^.
                out.push('[');
                let mut j = i + 1;
                if chars.get(j) == Some(&'!') {
                    out.push('^');
                    j += 1;
                }
                while j < chars.len() && chars[j] != ']' {
                    out.push(chars[j]);
                    j += 1;
                }
                out.push(']');
                i = j;
            }
            '.' | '+' | '(' | ')' | '|' | '^' | '$' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
        i += 1;
    }
    out
}

/// Escape regex metacharacters in a literal brace alternative.
fn regex_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if ".+*?()|[]{}^$\\".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_star_matches_extension() {
        assert!(glob_matches("*.rs", "src/main.rs"));
        assert!(!glob_matches("*.rs", "src/main.py"));
    }

    #[test]
    fn glob_brace_extension_list() {
        assert!(glob_matches("*.{js,ts}", "a/b.ts"));
        assert!(glob_matches("*.{js,ts}", "a/b.js"));
        assert!(!glob_matches("*.{js,ts}", "a/b.rs"));
    }

    #[test]
    fn glob_doublestar_crosses_directories() {
        assert!(glob_matches("**/*.rs", "deep/nested/x.rs"));
        assert!(glob_matches("*", "anything"));
    }

    #[test]
    fn resolved_indent_string() {
        let tab = Resolved {
            indent_is_tab: Some(true),
            ..Default::default()
        };
        assert_eq!(tab.indent_string(), Some("\t".to_string()));
        let spaces = Resolved {
            indent_is_tab: Some(false),
            indent_size: Some(2),
            ..Default::default()
        };
        assert_eq!(spaces.indent_string(), Some("  ".to_string()));
        assert_eq!(Resolved::default().indent_string(), None);
    }

    #[test]
    fn resolve_reads_nearest_with_root() {
        let dir = std::env::temp_dir().join(format!("vix-ec-{}", std::process::id()));
        let sub = dir.join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(
            dir.join(".editorconfig"),
            "root = true\n[*]\nindent_style = space\nindent_size = 4\n",
        )
        .unwrap();
        std::fs::write(sub.join(".editorconfig"), "[*.rs]\nindent_size = 2\n").unwrap();
        let r = resolve(&sub.join("main.rs"));
        // Nearer file overrides size; style inherited from the root file.
        assert_eq!(r.indent_is_tab, Some(false));
        assert_eq!(r.indent_size, Some(2));
        std::fs::remove_dir_all(&dir).ok();
    }
}
