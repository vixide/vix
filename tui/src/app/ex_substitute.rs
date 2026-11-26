//! Ex `:s` / `:%s`, the bounded literal substitute (T841).
//!
//! The parsing and the matching are pure (`vix_find_panel::substitute`); this
//! file applies a parsed command to the active buffer. Without the `c` flag the
//! whole command is one `set_content`, so one undo step. With `c` it reuses the
//! query-replace session (`QueryReplace`, the `y`/`n`/`!`/`q` bar): the matches
//! are planned up front, each decision only records or skips one, and the
//! accepted ones are written together when the session ends, again as one undo
//! step. See `spec/find-and-replace/index.md` ("Ex substitute").

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

use std::collections::VecDeque;

use regex::Regex;

use crate::find_panel::substitute::{self, Substitute, SubstituteError};

use super::{App, Decision, QueryReplace, highlight_match};

/// The state of a `:s///c` session beside its [`QueryReplace`] bar: the matches
/// still to ask about and the ones accepted so far, as character ranges in the
/// buffer as it was when the command ran (it is not edited until the end).
pub(crate) struct ExConfirm {
    /// The planned matches not yet offered, in buffer order.
    pending: VecDeque<(usize, usize)>,
    /// The matches the user accepted, in buffer order.
    accepted: Vec<(usize, usize)>,
    /// The literal replacement text.
    replacement: String,
}

/// The translated status-line text for a refused `:s`.
fn error_text(e: &SubstituteError) -> String {
    match e {
        SubstituteError::Range(r) => t!("status.ex_sub_range", range = r),
        SubstituteError::Delimiter(c) => t!("status.ex_sub_delimiter", delim = c.to_string()),
        SubstituteError::EmptyPattern => t!("status.ex_sub_empty"),
        SubstituteError::Pattern(tok) => t!("status.ex_sub_pattern", token = tok),
        SubstituteError::Replacement(tok) => t!("status.ex_sub_replacement", token = tok),
        SubstituteError::Flag(c) => t!("status.ex_sub_flag", flag = c.to_string()),
        // `SubstituteError` is non-exhaustive: a future refusal still reads as one.
        _ => t!("status.vim_no_command", cmd = e.to_string()),
    }
    .to_string()
}

/// The 1-based line and column of the first non-blank character on the line
/// holding character offset `at` in `text`.
fn first_non_blank(text: &str, at: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut line_start_byte = 0usize;
    for (i, (b, c)) in text.char_indices().enumerate() {
        if i >= at {
            break;
        }
        if c == '\n' {
            line = line.saturating_add(1);
            line_start_byte = b.saturating_add(1);
        }
    }
    let blanks = text
        .get(line_start_byte..)
        .unwrap_or("")
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .count();
    (line, blanks.saturating_add(1))
}

impl App {
    /// Run `cmd` as `:s` when it is one, returning `false` when it is not (so
    /// the caller tries its other Ex commands). A refused command reports why
    /// and leaves the buffer untouched.
    pub(super) fn try_ex_substitute(&mut self, cmd: &str) -> bool {
        let Some(parsed) = substitute::parse(cmd) else {
            return false;
        };
        match parsed {
            Err(e) => self.status = error_text(&e),
            Ok(sub) => self.ex_substitute(&sub),
        }
        true
    }

    /// Apply a parsed `:s` to the active buffer (see the module docs).
    fn ex_substitute(&mut self, sub: &Substitute) {
        if self.active_read_only() {
            self.status = t!("status.read_only_blocked").to_string();
            return;
        }
        let cursor_line = self.editor.cursor_line();
        let Some(tab) = self.editor.active_tab() else {
            return;
        };
        let text = tab.text();
        let ranges = substitute::plan(&text, sub, cursor_line);
        let Some(&first) = ranges.first() else {
            self.status = t!("status.ex_sub_not_found", pattern = sub.pattern).to_string();
            return;
        };
        if sub.confirm {
            self.begin_ex_confirm(sub, ranges, first);
        } else {
            self.finish_ex_substitute(&text, &ranges, &sub.replacement);
        }
    }

    /// Start the `c`-flag session on the first planned match.
    fn begin_ex_confirm(
        &mut self,
        sub: &Substitute,
        ranges: Vec<(usize, usize)>,
        first: (usize, usize),
    ) {
        // The bar's `QueryReplace` needs a regex; the literal pattern escaped is
        // exactly it (the ranges come from the plan, never from this).
        let Ok(re) = Regex::new(&regex::escape(&sub.pattern)) else {
            return;
        };
        let area = self.editor_view();
        if let Some(t) = self.editor.active_tab_mut() {
            highlight_match(t, first.0, first.1, area);
        }
        let mut pending: VecDeque<(usize, usize)> = ranges.into();
        pending.pop_front();
        self.ex_confirm = Some(ExConfirm {
            pending,
            accepted: Vec::new(),
            replacement: sub.replacement.clone(),
        });
        self.query_replace = Some(QueryReplace {
            re,
            template: sub.replacement.clone(),
            regex: false,
            current: first,
            replaced: 0,
            label: sub.pattern.clone(),
        });
        self.status = t!("status.qr_keys").into();
    }

    /// One `y`/`n`/`!`/`q` decision in a `:s///c` session. Returns `false` when
    /// no such session is running (the caller handles a plain query-replace).
    pub(super) fn ex_confirm_apply(&mut self, decision: Decision) -> bool {
        let Some(ex) = self.ex_confirm.as_mut() else {
            return false;
        };
        let Some(qr) = self.query_replace.as_mut() else {
            self.ex_confirm = None;
            return true;
        };
        let current = qr.current;
        let next = match decision {
            Decision::Replace => {
                ex.accepted.push(current);
                qr.replaced = qr.replaced.saturating_add(1);
                ex.pending.pop_front()
            }
            Decision::Skip => ex.pending.pop_front(),
            Decision::ReplaceRest => {
                ex.accepted.push(current);
                ex.accepted.extend(ex.pending.drain(..));
                qr.replaced = ex.accepted.len();
                None
            }
            Decision::Quit => None,
        };
        if let Some((s, e)) = next {
            qr.current = (s, e);
            let area = self.editor_view();
            if let Some(t) = self.editor.active_tab_mut() {
                highlight_match(t, s, e, area);
            }
            return true;
        }
        // The session is over: write what was accepted, as one edit.
        self.query_replace = None;
        let Some(ex) = self.ex_confirm.take() else {
            return true;
        };
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.remove_marks();
            t.editor.set_selection(None);
        }
        if ex.accepted.is_empty() {
            self.status = t!("status.qr_replaced", count = 0).to_string();
            return true;
        }
        let Some(text) = self.editor.active_tab().map(crate::editor::Tab::text) else {
            return true;
        };
        self.finish_ex_substitute(&text, &ex.accepted, &ex.replacement);
        true
    }

    /// Write `ranges` of `text` (the active buffer's text) replaced by
    /// `replacement` as one undo step, put the cursor on the first non-blank of
    /// the last changed line (as Vim does) and report the count.
    fn finish_ex_substitute(&mut self, text: &str, ranges: &[(usize, usize)], replacement: &str) {
        let new_text = substitute::apply(text, ranges, replacement);
        // Where the last replacement ends in the new text: its old start, less
        // the matched text before it, plus the replacements before it and itself.
        let rep_len = replacement.chars().count();
        let (mut removed, mut added, mut end_in_new) = (0usize, 0usize, 0usize);
        for &(s, e) in ranges {
            end_in_new = s
                .saturating_sub(removed)
                .saturating_add(added)
                .saturating_add(rep_len);
            removed = removed.saturating_add(e.saturating_sub(s));
            added = added.saturating_add(rep_len);
        }
        let lines = changed_lines(text, ranges);
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        tab.editor.set_content(&new_text);
        tab.editor.remove_marks();
        tab.editor.set_selection(None);
        tab.dirty = true;
        tab.preview = false;
        // The replacement's last character (its position when it is empty).
        let at = end_in_new.saturating_sub(usize::from(rep_len > 0));
        let (line, col) = first_non_blank(&new_text, at);
        let area = self.editor_view();
        self.editor.goto(line, Some(col), area);
        self.status = t!("status.ex_sub_done", count = ranges.len(), lines = lines).to_string();
    }
}

/// How many distinct lines of `text` the ascending `ranges` start on.
fn changed_lines(text: &str, ranges: &[(usize, usize)]) -> usize {
    let mut chars = text.chars();
    let (mut pos, mut line, mut lines) = (0usize, 0usize, 0usize);
    let mut last: Option<usize> = None;
    for &(s, _) in ranges {
        while pos < s {
            if chars.next() == Some('\n') {
                line = line.saturating_add(1);
            }
            pos = pos.saturating_add(1);
        }
        if last != Some(line) {
            lines = lines.saturating_add(1);
            last = Some(line);
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::cov4::*;
    use vix_input::{KeyCode, KeyEvent, KeyModifiers};

    fn vi_app(tag: &str, text: &str, cursor: usize) -> App {
        let mut s = crate::settings::Settings::default();
        s.keymap = "vi".into();
        let mut app = app_with(&temp(tag), s);
        buffer_with(&mut app, text, cursor);
        app.editor.active_tab_mut().unwrap().dirty = false;
        app
    }

    fn press(app: &mut App, c: char) {
        app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }

    fn dirty(app: &App) -> bool {
        app.editor.active_tab().unwrap().dirty
    }

    #[test]
    fn whole_buffer_substitute_is_one_undo_step() {
        let text = "foo foo\nbar foo\n".repeat(50);
        let mut app = vi_app("t841-all", &text, 0);
        app.run_vim_command("%s/foo/quux/g");
        let after = content(&mut app);
        assert_eq!(after.matches("quux").count(), 150);
        assert!(!after.contains("foo"));
        assert!(dirty(&app));
        assert_eq!(
            app.status,
            t!("status.ex_sub_done", count = 150, lines = 100).to_string()
        );
        // The cursor lands on the last changed line.
        assert_eq!(app.editor.cursor_line(), 99);
        app.run_action("edit.undo");
        assert_eq!(content(&mut app), text, "one undo restores every line");
    }

    #[test]
    fn no_range_is_the_cursor_line_and_no_g_is_the_first_match() {
        let mut app = vi_app("t841-line", "a a\nb a a\na\n", 5);
        app.run_vim_command("s/a/X/");
        assert_eq!(content(&mut app), "a a\nb X a\na\n");
        app.run_vim_command("s/a/X/g");
        assert_eq!(content(&mut app), "a a\nb X X\na\n");
        app.run_vim_command("%s/a/[&]/");
        assert_eq!(content(&mut app), "[a] a\nb X X\n[a]\n");
    }

    #[test]
    fn escaped_and_alternate_delimiters_match_literally() {
        let mut app = vi_app("t841-delim", "path a/b.c\n", 0);
        app.run_vim_command(r"s/a\/b\.c/x\/y/");
        assert_eq!(content(&mut app), "path x/y\n");
        app.run_vim_command("s#x/y#a/b#");
        assert_eq!(content(&mut app), "path a/b\n");
    }

    #[test]
    fn a_parse_failure_leaves_the_buffer_unchanged() {
        let text = "one.two\nthree\n";
        let mut app = vi_app("t841-bad", text, 0);
        for (cmd, status) in [
            ("s/one.two/x/", t!("status.ex_sub_pattern", token = ".")),
            (r"s/\(one\)/x/", t!("status.ex_sub_pattern", token = r"\(")),
            ("s/^one/x/", t!("status.ex_sub_pattern", token = "^")),
            ("s/one/~/", t!("status.ex_sub_replacement", token = "~")),
            ("1,2s/one/x/", t!("status.ex_sub_range", range = "1,2")),
            ("s/one/x/gi", t!("status.ex_sub_flag", flag = "i")),
            ("s|one|x|", t!("status.ex_sub_delimiter", delim = "|")),
            ("s//x/", t!("status.ex_sub_empty")),
            ("s/zzz/x/", t!("status.ex_sub_not_found", pattern = "zzz")),
        ] {
            app.run_vim_command(cmd);
            assert_eq!(app.status, status.to_string(), "{cmd}");
            assert_eq!(content(&mut app), text, "{cmd} must not edit");
            assert!(!dirty(&app), "{cmd} must not dirty the buffer");
        }
        assert!(app.query_replace.is_none());
    }

    #[test]
    fn read_only_buffers_refuse_a_substitute() {
        let mut app = vi_app("t841-ro", "abc\n", 0);
        app.editor.active_tab_mut().unwrap().read_only = true;
        app.run_vim_command("s/a/b/");
        assert_eq!(content(&mut app), "abc\n");
        assert_eq!(app.status, t!("status.read_only_blocked").to_string());
    }

    #[test]
    fn confirm_asks_per_match_and_writes_one_undo_step() {
        let text = "x1 x2\nx3\n";
        let mut app = vi_app("t841-confirm", text, 0);
        app.run_vim_command("%s/x/Y/gc");
        assert!(app.query_replace.is_some());
        assert_eq!(content(&mut app), text, "nothing is written while asking");
        assert_eq!(app.query_replace.as_ref().unwrap().current, (0, 1));
        press(&mut app, 'y');
        assert_eq!(app.query_replace.as_ref().unwrap().current, (3, 4));
        press(&mut app, 'n');
        assert_eq!(app.query_replace.as_ref().unwrap().current, (6, 7));
        press(&mut app, 'y');
        assert!(app.query_replace.is_none() && app.ex_confirm.is_none());
        assert_eq!(content(&mut app), "Y1 x2\nY3\n");
        app.run_action("edit.undo");
        assert_eq!(content(&mut app), text, "one undo step");

        // `!` takes this one and the rest.
        app.run_vim_command("%s/x/Y/gc");
        press(&mut app, 'n');
        press(&mut app, '!');
        assert_eq!(content(&mut app), "x1 Y2\nY3\n");
    }

    #[test]
    fn cancelling_confirm_keeps_only_what_was_accepted() {
        let text = "x x x\n";
        let mut app = vi_app("t841-cancel", text, 0);
        app.run_vim_command("s/x/Y/gc");
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.query_replace.is_none() && app.ex_confirm.is_none());
        assert_eq!(
            content(&mut app),
            text,
            "Esc at the first match changes nothing"
        );
        assert!(!dirty(&app));

        app.run_vim_command("s/x/Y/gc");
        press(&mut app, 'y');
        press(&mut app, 'q');
        assert_eq!(
            content(&mut app),
            "Y x x\n",
            "q keeps the substitutions so far"
        );
    }

    #[test]
    fn the_find_box_query_replace_is_unaffected() {
        let mut app = vi_app("t841-qr", "ab ab\n", 0);
        app.run_vim_command("s/ab/x/c");
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.ex_confirm.is_none());
        // A plain query-replace session afterwards edits as it always did.
        app.query_replace = Some(QueryReplace {
            re: Regex::new("ab").unwrap(),
            template: "z".into(),
            regex: false,
            current: (0, 2),
            replaced: 0,
            label: "ab".into(),
        });
        press(&mut app, 'y');
        assert_eq!(content(&mut app), "z ab\n");
    }

    #[test]
    fn first_non_blank_and_changed_lines() {
        assert_eq!(first_non_blank("a\n  b\n", 3), (2, 3));
        assert_eq!(first_non_blank("abc", 0), (1, 1));
        assert_eq!(changed_lines("a a\nb\na", &[(0, 1), (2, 3), (6, 7)]), 2);
        assert_eq!(changed_lines("", &[]), 0);
    }
}
