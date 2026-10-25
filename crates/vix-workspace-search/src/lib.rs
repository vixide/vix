//! Workspace-wide search (and replace) across every file under the workspace root.
//!
//! Open buffers are searched in their current (possibly unsaved) state; other
//! files are read from disk. `App` owns the buffers, so it drives the actual
//! scanning; this crate just holds the panel state.

// Shared workspace i18n (see the vix_i18n crate).
#[macro_use]
extern crate vix_i18n;
vix_i18n::surface!();

use std::path::PathBuf;

use vix_find_panel::Field;

/// One matching line.
pub struct Hit {
    /// Absolute path of the file containing the match.
    pub path: PathBuf,
    /// The file's path relative to the workspace root, exactly as it
    /// appears in `display`'s prefix (used to re-identify a surviving line
    /// in a T211 editable-results buffer without resolving paths again).
    pub rel: String,
    /// 1-based line number of the match.
    pub line: usize,
    /// 1-based column of the match start.
    pub col: usize,
    /// The matching line's text, left-trimmed and clipped to 120 chars --
    /// exactly the part of `display` after its `rel:line: ` prefix.
    pub text: String,
    /// `relpath:line: text` shown in the results list.
    pub display: String,
}

/// One recorded baseline line for a T211 "editable search results" buffer,
/// built 1:1 from the [`Hit`]s shown when the buffer was opened: everything
/// needed to tell, once the user has edited and saved, whether a surviving
/// line changed and where to write it back. A key with no surviving line in
/// the saved buffer was deleted -- simply skipped, no bookkeeping needed.
#[derive(Clone)]
pub struct WgrepBaseline {
    /// Matches [`Hit::rel`].
    pub rel: String,
    /// Matches [`Hit::path`].
    pub path: PathBuf,
    /// Matches [`Hit::line`].
    pub line: usize,
    /// Matches [`Hit::text`] at the time the buffer was opened.
    pub text: String,
}

/// Parse one editable-results buffer line back into its `(rel, line, text)`
/// parts -- the inverse of `Hit`'s own `"{rel}:{line}: {text}"` display
/// format. `None` for a line that doesn't match the shape at all (e.g. one
/// the user typed from scratch rather than edited in place). Splits on the
/// *first* `:` and the *first* `": "` after it, so a `rel` containing a
/// literal `:` (technically legal but essentially never seen in practice)
/// would parse wrong -- a known, accepted limitation of the plain-text
/// format, not something this function tries to work around.
#[must_use]
pub fn parse_result_line(line: &str) -> Option<(&str, usize, &str)> {
    let (rel, rest) = line.split_once(':')?;
    let (num, text) = rest.split_once(": ")?;
    let n: usize = num.trim().parse().ok()?;
    Some((rel, n, text))
}

/// How many characters of a matching line (after its leading whitespace)
/// a [`Hit::text`] keeps.
pub const CLIP_CHARS: usize = 120;

/// A matching line as [`Hit::text`] shows it: leading whitespace trimmed and
/// clipped to [`CLIP_CHARS`] characters. The editable-results buffer
/// compares a file's *current* line through this same function, so the
/// stale-line check (T653) and the scan can never disagree on the shape.
#[must_use]
pub fn clip(line: &str) -> String {
    line.trim_start().chars().take(CLIP_CHARS).collect()
}

/// One edited line to write back from an editable-results buffer (T653).
pub struct LineEdit {
    /// 1-based line number in the target file.
    pub line: usize,
    /// The line's [`clip`]ped text when the results were gathered.
    pub baseline: String,
    /// The replacement for that clipped text.
    pub new_text: String,
}

/// What [`rewrite_lines`] did to one file's text.
pub struct LineRewrite {
    /// The file's new contents.
    pub content: String,
    /// Line numbers that were rewritten.
    pub applied: Vec<usize>,
    /// Line numbers left alone because the file's line no longer matches
    /// its baseline (or no longer exists): the file changed after the
    /// search, so writing there could replace the wrong text.
    pub drifted: Vec<usize>,
}

/// Apply `edits` to `content`, line by line (T653).
///
/// Each target line is first checked against its baseline: only a line
/// whose current [`clip`] still equals [`LineEdit::baseline`] is rewritten;
/// any other is reported in [`LineRewrite::drifted`] and left as is. A
/// rewritten line keeps its own leading whitespace and any text past the
/// [`CLIP_CHARS`] the buffer showed, so only the visible part changes. Every
/// line keeps its own terminator (`\r\n` or `\n`), and the file keeps its
/// final-newline state, so a CRLF file stays CRLF.
#[must_use]
pub fn rewrite_lines(content: &str, edits: &[LineEdit]) -> LineRewrite {
    let mut segments: Vec<(String, &str)> = content
        .split_inclusive('\n')
        .map(|seg| {
            let body_len = if seg.ends_with("\r\n") {
                seg.len() - 2
            } else if seg.ends_with('\n') {
                seg.len() - 1
            } else {
                seg.len()
            };
            (seg[..body_len].to_string(), &seg[body_len..])
        })
        .collect();
    let mut applied = Vec::new();
    let mut drifted = Vec::new();
    for edit in edits {
        let Some((body, _)) = edit.line.checked_sub(1).and_then(|i| segments.get_mut(i)) else {
            drifted.push(edit.line);
            continue;
        };
        if clip(body) != edit.baseline {
            drifted.push(edit.line);
            continue;
        }
        let trimmed = body.trim_start();
        let indent = &body[..body.len() - trimmed.len()];
        let tail = trimmed
            .char_indices()
            .nth(CLIP_CHARS)
            .map_or("", |(at, _)| &trimmed[at..]);
        *body = format!("{indent}{}{tail}", edit.new_text);
        applied.push(edit.line);
    }
    let content = segments.iter().fold(
        String::with_capacity(content.len()),
        |mut acc, (body, end)| {
            acc.push_str(body);
            acc.push_str(end);
            acc
        },
    );
    LineRewrite {
        content,
        applied,
        drifted,
    }
}

bitflags::bitflags! {
    /// [`WorkspaceSearch`]'s independent search toggles (T149): each bit is a
    /// distinct, freely-combinable preference, not one of several
    /// mutually-exclusive modes — grouping them into a plain sub-struct would
    /// only relocate `clippy::struct_excessive_bools`, since the lint counts
    /// `bool` fields anywhere, not just on `WorkspaceSearch` itself.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Flags: u8 {
        /// Whether the replacement field is shown and used.
        const REPLACING = 1 << 0;
        /// Match case exactly.
        const CASE_SENSITIVE = 1 << 1;
        /// Treat the query as a regular expression.
        const REGEX = 1 << 2;
        /// When set, the hit list is fixed (e.g. go-to-definition candidates)
        /// and typing does not re-run the search.
        const STATIC_RESULTS = 1 << 3;
    }
}

/// State of the workspace-wide search/replace panel.
pub struct WorkspaceSearch {
    /// Search-pattern text.
    pub query: String,
    /// Replacement text.
    pub replace: String,
    /// Regex limiting the search to file paths that match it (empty = no limit).
    pub include_path: String,
    /// Regex excluding file paths that match it (empty = no exclusion).
    pub exclude_path: String,
    /// Which input field has focus.
    pub field: Field,
    /// Independent toggles: replace mode, case sensitivity, regex mode, and
    /// whether the hit list is static.
    pub flags: Flags,
    /// Current matches.
    pub hits: Vec<Hit>,
    /// Index of the highlighted hit.
    pub selected: usize,
    /// Status/summary line shown under the inputs.
    pub status: String,
}

impl WorkspaceSearch {
    /// A fresh panel; `replacing` selects search-and-replace mode.
    #[must_use]
    pub fn new(replacing: bool) -> Self {
        let mut flags = Flags::empty();
        flags.set(Flags::REPLACING, replacing);
        WorkspaceSearch {
            query: String::new(),
            replace: String::new(),
            include_path: String::new(),
            exclude_path: String::new(),
            field: Field::Query,
            flags,
            hits: Vec::new(),
            selected: 0,
            status: t!("status.workspace_search_prompt").to_string(),
        }
    }

    /// Mutable access to the currently focused field's text.
    pub fn active_field_mut(&mut self) -> &mut String {
        match self.field {
            Field::Query => &mut self.query,
            Field::Replace => &mut self.replace,
            Field::IncludePath => &mut self.include_path,
            Field::ExcludePath => &mut self.exclude_path,
        }
    }

    /// Cycle focus across the visible fields: query → (replace, if replacing) →
    /// include-path → exclude-path → query.
    pub fn toggle_field(&mut self) {
        self.field = match self.field {
            Field::Query if self.flags.contains(Flags::REPLACING) => Field::Replace,
            Field::Query | Field::Replace => Field::IncludePath,
            Field::IncludePath => Field::ExcludePath,
            Field::ExcludePath => Field::Query,
        };
    }

    /// The compiled path filter from the include/exclude regexes.
    #[must_use]
    pub fn path_filter(&self) -> vix_find_panel::PathFilter {
        vix_find_panel::PathFilter::new(&self.include_path, &self.exclude_path)
    }

    /// Effective regex pattern from the query and toggles (no whole-word here).
    #[must_use]
    pub fn pattern(&self) -> Option<String> {
        if self.query.len() < 2 {
            return None;
        }
        let mut core = if self.flags.contains(Flags::REGEX) {
            self.query.clone()
        } else {
            regex::escape(&self.query)
        };
        if !self.flags.contains(Flags::CASE_SENSITIVE) {
            core = format!("(?i){core}");
        }
        Some(core)
    }

    /// Move the selection up one hit.
    pub fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Move the selection down one hit.
    pub fn down(&mut self) {
        if self.selected + 1 < self.hits.len() {
            self.selected += 1;
        }
    }

    /// The highlighted hit, if any.
    #[must_use]
    pub fn selected_hit(&self) -> Option<&Hit> {
        self.hits.get(self.selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(line: usize, baseline: &str, new_text: &str) -> LineEdit {
        LineEdit {
            line,
            baseline: baseline.to_string(),
            new_text: new_text.to_string(),
        }
    }

    #[test]
    fn rewrite_lines_keeps_crlf_endings_and_final_newline_state() {
        let out = rewrite_lines("a\r\nneedle\r\nb", &[edit(2, "needle", "pin")]);
        assert_eq!(out.content, "a\r\npin\r\nb");
        assert_eq!(out.applied, vec![2]);
        let out = rewrite_lines("needle\n", &[edit(1, "needle", "pin")]);
        assert_eq!(out.content, "pin\n");
    }

    #[test]
    fn rewrite_lines_skips_a_line_that_drifted_from_its_baseline() {
        let out = rewrite_lines("inserted\nneedle\n", &[edit(1, "needle", "pin")]);
        assert_eq!(out.content, "inserted\nneedle\n");
        assert_eq!(out.applied, Vec::<usize>::new());
        assert_eq!(out.drifted, vec![1]);
        let out = rewrite_lines("one\n", &[edit(5, "needle", "pin")]);
        assert_eq!(out.drifted, vec![5]);
    }

    #[test]
    fn rewrite_lines_keeps_indent_and_the_clipped_tail() {
        let out = rewrite_lines("    needle x\n", &[edit(1, "needle x", "pin x")]);
        assert_eq!(out.content, "    pin x\n");
        let long = format!("{}TAIL", "n".repeat(CLIP_CHARS));
        let base = clip(&long);
        let out = rewrite_lines(&long, &[edit(1, &base, "short")]);
        assert_eq!(out.content, "shortTAIL");
    }

    #[test]
    fn parse_result_line_splits_rel_line_and_text() {
        assert_eq!(
            parse_result_line("src/app.rs:42: let x = 1;"),
            Some(("src/app.rs", 42, "let x = 1;"))
        );
    }

    #[test]
    fn parse_result_line_handles_empty_text() {
        assert_eq!(parse_result_line("a.rs:1: "), Some(("a.rs", 1, "")));
    }

    #[test]
    fn parse_result_line_rejects_lines_with_no_shape() {
        assert_eq!(parse_result_line("just some prose"), None);
        assert_eq!(parse_result_line("a.rs no colon at all"), None);
        assert_eq!(parse_result_line("a.rs:not-a-number: text"), None);
    }
}
