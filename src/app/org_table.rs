//! Org tables (structural edits + `TBLFM` formulas, via the
//! `vix-org-table` crate) and the interactive Column View overlay that
//! renders them, including dynamic-block (`#+BEGIN: columnview`) insert
//! and refresh.
//!
//! Moved out of `app.rs` verbatim (T141, slice 8 -- a sub-slice of the
//! larger "org" area). Named `org_table`, not `column_view`, even
//! though it also owns the Column View glue: `app.rs` already has an
//! unrelated top-level `crate::column_view` module (a different,
//! crate-root module, not an `app` submodule), referenced only fully
//! qualified there -- no hard collision, but reusing that name here would
//! have made two same-named modules confusingly coexist at different
//! levels of the tree. Two contiguous clusters, bundled together since
//! Column View renders interactive `TBLFM` tables.

#![warn(clippy::pedantic)]

use crossterm::event::{KeyCode, KeyEvent};

use super::{App, Focus, Prompt, PromptKind};

impl App {
    /// The active tab's cursor as `(0-based line, byte offset within that
    /// line)` — the column convention `vix_org_table`'s functions expect,
    /// converted from the editor's char-offset cursor.
    pub(super) fn org_table_cursor_pos(tab: &crate::editor::Tab) -> (usize, usize) {
        let code = tab.editor.code_ref();
        let cursor = tab.editor.get_cursor();
        let line = code.char_to_line(cursor);
        let line_start = code.line_to_char(line);
        let char_col = cursor - line_start;
        let line_text = code.slice(line_start, line_start + code.line_len(line));
        let byte_col = line_text
            .char_indices()
            .nth(char_col)
            .map_or(line_text.len(), |(b, _)| b);
        (line, byte_col)
    }

    /// Move the active tab's cursor to `line`'s byte offset `byte_col` (a
    /// position returned by a `vix_org_table` function against the buffer's
    /// *current* content — call this only after `set_content`).
    pub(super) fn org_table_set_cursor(tab: &mut crate::editor::Tab, line: usize, byte_col: usize) {
        let code = tab.editor.code_ref();
        let line = line.min(code.len_lines().saturating_sub(1));
        let line_start = code.line_to_char(line);
        let line_text = code.slice(line_start, line_start + code.line_len(line));
        let b = byte_col.min(line_text.len());
        let char_col = line_text[..b].chars().count();
        tab.editor.set_cursor(line_start + char_col);
    }

    /// The 0-indexed `|`-delimited field that byte offset `col` falls into on
    /// `line` — a small, deliberately duplicated miniature of
    /// `vix_org_table`'s private `field_index_at` (not part of that crate's
    /// public API), used only to translate a cursor/selection position into
    /// the field-index corners the rectangle functions take.
    pub(super) fn org_table_field_index_at(line: &str, col: usize) -> usize {
        let delims: Vec<usize> = line.match_indices('|').map(|(i, _)| i).collect();
        if delims.len() < 2 {
            return 0;
        }
        let max_field = delims.len() - 2;
        let mut idx = 0;
        for (i, &p) in delims.iter().enumerate() {
            if p < col {
                idx = i;
            } else {
                break;
            }
        }
        idx.min(max_field)
    }

    /// Whether the active tab is a `.org` file and the cursor is inside a
    /// pipe table. Used to context-sensitively shadow chords that mean
    /// something else outside a table (e.g. `C-c C-x C-w`/`C-y`/`M-w`).
    pub(super) fn org_table_active_at_cursor(&self) -> bool {
        if !self.active_is_org() {
            return false;
        }
        self.editor.active_tab().is_some_and(|t| {
            let text = t.editor.get_content();
            let line = t.editor.cursor_line();
            crate::org_table::table_range(&text, line).is_some()
        })
    }

    /// Context-sensitive Org table keys: `Tab`/`S-Tab`/`RET` field & row
    /// navigation, `S-RET` copy-down, and Meta-/Shift-arrow structural edits —
    /// matching Emacs's `org-table-*` bindings. Only intercepts when the
    /// active tab is a `.org` file, is not read-only, AND the cursor is
    /// actually inside a pipe table (`vix_org_table::table_range`); otherwise
    /// returns `false` immediately so every other keymap's bindings for these
    /// same keys (word motion, jump history, line move, selection extension,
    /// …) are completely unaffected outside tables. Called from
    /// [`App::on_key`] ahead of the keymap-specific dispatch — earlier than
    /// [`App::org_view_key`] — because `Alt`+arrow and `Ctrl+Shift`+arrow are
    /// already bound globally in [`App::global_shared_key`], which runs
    /// before the per-focus [`App::editor_key`] ever sees the key; inside an
    /// actual table those global bindings are deliberately shadowed.
    pub(super) fn org_table_key(&mut self, key: KeyEvent) -> bool {
        if self.focus != Focus::Editor || self.active_read_only() || !self.active_is_org() {
            return false;
        }
        let Some(tab) = self.editor.active_tab() else {
            return false;
        };
        let text = tab.editor.get_content();
        let line = tab.editor.cursor_line();
        if crate::org_table::table_range(&text, line).is_none() {
            return false;
        }
        let (_, byte_col) = Self::org_table_cursor_pos(tab);

        let ctrl = Self::ctrl(&key);
        let alt = Self::alt(&key);
        let shift = Self::shift(&key);

        let result: Option<(String, usize, usize)> = match key.code {
            KeyCode::Tab if !ctrl && !alt && !shift => {
                crate::org_table::next_field(&text, line, byte_col)
            }
            KeyCode::Tab if !ctrl && !alt && shift => {
                crate::org_table::previous_field(&text, line, byte_col)
            }
            KeyCode::BackTab if !ctrl && !alt => {
                crate::org_table::previous_field(&text, line, byte_col)
            }
            KeyCode::Enter if !ctrl && !alt && !shift => {
                crate::org_table::next_row(&text, line, byte_col)
            }
            KeyCode::Enter if !ctrl && !alt && shift => {
                crate::org_table::copy_down(&text, line, byte_col, false)
            }
            KeyCode::Left if alt && !ctrl && shift => {
                crate::org_table::delete_column(&text, line, byte_col).map(|(t, c)| (t, line, c))
            }
            KeyCode::Right if alt && !ctrl && shift => {
                crate::org_table::insert_column(&text, line, byte_col).map(|(t, c)| (t, line, c))
            }
            KeyCode::Up if alt && !ctrl && shift => {
                crate::org_table::kill_row(&text, line).map(|t| (t, line, byte_col))
            }
            // `M-S-<down>`: insert a new row *above* the current one (Org's
            // own default for this chord — the "below" variant is the
            // prefix-argument case, not wired here).
            KeyCode::Down if alt && !ctrl && shift => {
                crate::org_table::insert_row(&text, line, false).map(|(t, l)| (t, l, byte_col))
            }
            KeyCode::Left if alt && !ctrl && !shift => {
                crate::org_table::move_column_left(&text, line, byte_col).map(|(t, c)| (t, line, c))
            }
            KeyCode::Right if alt && !ctrl && !shift => {
                crate::org_table::move_column_right(&text, line, byte_col)
                    .map(|(t, c)| (t, line, c))
            }
            KeyCode::Up if alt && !ctrl && !shift => {
                crate::org_table::move_row_up(&text, line).map(|(t, l)| (t, l, byte_col))
            }
            KeyCode::Down if alt && !ctrl && !shift => {
                crate::org_table::move_row_down(&text, line).map(|(t, l)| (t, l, byte_col))
            }
            KeyCode::Up if shift && !ctrl && !alt => {
                crate::org_table::move_cell_up(&text, line, byte_col)
            }
            KeyCode::Down if shift && !ctrl && !alt => {
                crate::org_table::move_cell_down(&text, line, byte_col)
            }
            KeyCode::Left if shift && !ctrl && !alt => {
                crate::org_table::move_cell_left(&text, line, byte_col)
            }
            KeyCode::Right if shift && !ctrl && !alt => {
                crate::org_table::move_cell_right(&text, line, byte_col)
            }
            _ => return false,
        };

        if let Some((new_text, new_line, new_col)) = result
            && let Some(t) = self.editor.active_tab_mut()
        {
            t.editor.set_content(&new_text);
            Self::org_table_set_cursor(t, new_line, new_col);
            t.dirty = true;
        }
        // Consumed either way: a `None` result (e.g. Shift-Tab at the first
        // field, or a column-move with no neighbor) is a silent no-op,
        // matching Emacs — the key must not leak through to any other
        // handler while inside the table.
        true
    }

    /// Run an Org-table transform keyed only on the cursor line
    /// (`(text, line) -> Option<new text>`), keeping the cursor's line.
    /// Mirrors [`Self::org_rewrite_line`]; reports `status.org_table_no_table`
    /// on a miss (cursor not inside a table).
    fn org_table_line_op(&mut self, f: impl FnOnce(&str, usize) -> Option<String>) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        if let Some(new) = f(&text, line) {
            tab.editor.set_content(&new);
            tab.editor
                .set_cursor_line(line.min(new.split('\n').count().saturating_sub(1)));
            tab.dirty = true;
        } else {
            self.status = t!("status.org_table_no_table").to_string();
        }
    }

    /// Run an Org-table row transform (`(text, line) -> Option<(new text, new
    /// line)>`), following the cursor to the returned line. Mirrors
    /// [`Self::org_move_subtree`].
    fn org_table_row_op(&mut self, f: impl FnOnce(&str, usize) -> Option<(String, usize)>) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        if let Some((new, new_line)) = f(&text, line) {
            tab.editor.set_content(&new);
            tab.editor.set_cursor_line(new_line);
            tab.dirty = true;
        } else {
            self.status = t!("status.org_table_no_table").to_string();
        }
    }

    /// Run an Org-table column transform (`(text, line, byte col) ->
    /// Option<(new text, new byte col on the same line)>`), following the
    /// cursor to the returned field.
    fn org_table_col_op(&mut self, f: impl FnOnce(&str, usize, usize) -> Option<(String, usize)>) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let (line, byte_col) = Self::org_table_cursor_pos(tab);
        let text = tab.editor.get_content();
        if let Some((new, new_col)) = f(&text, line, byte_col) {
            tab.editor.set_content(&new);
            Self::org_table_set_cursor(tab, line, new_col);
            tab.dirty = true;
        } else {
            self.status = t!("status.org_table_no_table").to_string();
        }
    }

    /// Run an Org-table cell transform (`(text, line, byte col) ->
    /// Option<(new text, new line, new byte col)>`), following the cursor to
    /// the returned field.
    fn org_table_cell_op(
        &mut self,
        f: impl FnOnce(&str, usize, usize) -> Option<(String, usize, usize)>,
    ) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let (line, byte_col) = Self::org_table_cursor_pos(tab);
        let text = tab.editor.get_content();
        if let Some((new, new_line, new_col)) = f(&text, line, byte_col) {
            tab.editor.set_content(&new);
            Self::org_table_set_cursor(tab, new_line, new_col);
            tab.dirty = true;
        } else {
            self.status = t!("status.org_table_no_table").to_string();
        }
    }

    /// `C-c C-c` / `C-u C-c C-c` on a table (or its `#+TBLFM:` line):
    /// realign it, applying its `#+TBLFM:` formulas if any immediately
    /// follow (`vix_org_table::recalc`). Returns `true` when this context
    /// matched (whether or not the buffer actually changed), so callers give
    /// it priority over other `C-c C-c` meanings (checkbox toggle, statistics
    /// refresh) without clobbering them outside a table.
    pub(super) fn org_table_recalc(&mut self) -> bool {
        if !self.active_is_org() {
            return false;
        }
        let Some(tab) = self.editor.active_tab_mut() else {
            return false;
        };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        let is_tblfm = text
            .split('\n')
            .nth(line)
            .map(str::trim_start)
            .is_some_and(|l| {
                l.get(..8)
                    .is_some_and(|p| p.eq_ignore_ascii_case("#+TBLFM:"))
            });
        let target_line = if is_tblfm {
            line.checked_sub(1)
        } else {
            Some(line)
        };
        let Some(target_line) = target_line else {
            return false;
        };
        if crate::org_table::table_range(&text, target_line).is_none() {
            return false;
        }
        if let Some(new) = crate::org_table::recalc(&text, target_line) {
            let new_last = new.split('\n').count().saturating_sub(1);
            tab.editor.set_content(&new);
            tab.editor.set_cursor_line(line.min(new_last));
            tab.dirty = true;
        }
        true
    }

    /// Sum the numeric cells of the column under the cursor (`C-c +`),
    /// showing the total on the status line and copying it to the system
    /// clipboard, matching `org-table-sum`.
    fn org_table_sum_column(&mut self) {
        let Some(tab) = self.editor.active_tab() else {
            return;
        };
        let text = tab.editor.get_content();
        let (line, byte_col) = Self::org_table_cursor_pos(tab);
        match crate::org_table::sum_column(&text, line, byte_col) {
            Some(sum) => {
                let formatted = Self::format_table_number(sum);
                let _ = vix_clipboard::set(&formatted);
                self.status = t!("status.org_table_sum", sum = formatted).to_string();
            }
            None => self.status = t!("status.org_table_no_table").to_string(),
        }
    }

    /// Format a table-formula-style number: integers with no decimal point,
    /// non-integers to 2 decimal places with trailing zeros trimmed.
    fn format_table_number(v: f64) -> String {
        if v.fract().abs() < 1e-9 {
            #[allow(clippy::cast_possible_truncation)] // pragmatic: table sums are small
            return (v.round() as i64).to_string();
        }
        let s = format!("{v:.2}");
        let trimmed = s.trim_end_matches('0').trim_end_matches('.');
        if trimmed.is_empty() || trimmed == "-" {
            "0".to_string()
        } else {
            trimmed.to_string()
        }
    }

    /// Open the sort prompt for the table at the cursor (`C-c ^`), seeded
    /// with the cursor's 1-indexed column.
    fn org_table_sort_prompt(&mut self) {
        let Some(tab) = self.editor.active_tab() else {
            return;
        };
        let (_, byte_col) = Self::org_table_cursor_pos(tab);
        let line_text = tab
            .editor
            .get_content()
            .split('\n')
            .nth(tab.editor.cursor_line())
            .unwrap_or("")
            .to_string();
        let column = Self::org_table_field_index_at(&line_text, byte_col);
        self.prompt = Some(
            Prompt::new(
                PromptKind::OrgTableSort,
                t!("prompt.org_table_sort").to_string(),
            )
            .with_input((column + 1).to_string()),
        );
    }

    /// Accept the sort prompt (`C-c ^`): `<column> [a|n|t] [r]` — 1-indexed
    /// column, optional sort kind (Alphabetic/Numeric/Time, default
    /// Alphabetic), optional trailing `r` to reverse.
    pub(super) fn accept_org_table_sort(&mut self, input: &str) {
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.editor.get_content();
        let Some((first, last)) = crate::org_table::table_range(&text, line) else {
            self.status = t!("status.org_table_no_table").to_string();
            return;
        };
        let mut parts = input.split_whitespace();
        let column = parts
            .next()
            .and_then(|s| s.parse::<usize>().ok())
            .map_or(0, |n| n.saturating_sub(1));
        let mut kind = crate::org_table::SortKind::Alphabetic;
        let mut reverse = false;
        for tok in parts {
            match tok.to_ascii_lowercase().as_str() {
                "n" => kind = crate::org_table::SortKind::Numeric,
                "t" => kind = crate::org_table::SortKind::Time,
                "a" => kind = crate::org_table::SortKind::Alphabetic,
                "r" => reverse = true,
                _ => {}
            }
        }
        if let Some(new) =
            crate::org_table::sort_rows(&text, first, last, column, kind, reverse, false)
        {
            tab.editor.set_content(&new);
            tab.editor.set_cursor_line(line);
            tab.dirty = true;
            self.status = t!("status.org_table_sorted").to_string();
        } else {
            self.status = t!("status.org_table_no_table").to_string();
        }
    }

    /// `C-c |` / `org-table-create-or-convert-from-region`: convert the
    /// active selection's delimited text into a pipe table in place, or (with
    /// no selection) insert an empty one-field table at the cursor.
    fn org_table_create_from_region(&mut self) {
        let selection = self
            .editor
            .active_tab_mut()
            .and_then(|t| t.editor.get_selection_text());
        let table_text = match selection {
            Some(sel) if !sel.trim().is_empty() => crate::org_table::from_delimited(&sel),
            _ => "|   |".to_string(),
        };
        let area = self.editor_view();
        if self.editor.insert_str(&table_text, area) {
            self.status = t!("status.org_table_created").to_string();
        }
    }

    /// `org-table-export`: export the table at the cursor as tab-separated
    /// text in a new untitled tab (one-shot, no round-trip back to the
    /// source, matching the other `org.export_*` actions).
    fn org_table_export_tsv(&mut self) {
        let Some(tab) = self.editor.active_tab() else {
            return;
        };
        let text = tab.editor.get_content();
        let line = tab.editor.cursor_line();
        let Some((first, last)) = crate::org_table::table_range(&text, line) else {
            self.status = t!("status.org_table_no_table").to_string();
            return;
        };
        let table = crate::org_table::parse(&text, first, last);
        let tsv = crate::org_table::to_tsv(&table);
        self.editor.new_tab_with_content(&tsv);
        self.status = t!("status.org_exported", ext = "tsv").to_string();
    }

    /// The Org table rectangle corners implied by the active selection, or
    /// (with none) just the field under the cursor — "If there is no active
    /// region, copy just the current field", per the manual. Corners are
    /// `(line, field index)`, the convention `copy_rectangle`/`cut_rectangle`
    /// take. `None` outside a table.
    fn org_table_selection_corners(&mut self) -> Option<((usize, usize), (usize, usize))> {
        let tab = self.editor.active_tab_mut()?;
        let text = tab.editor.get_content();
        let cursor = tab.editor.get_cursor();
        let selection = tab.editor.get_selection();
        let code = tab.editor.code_ref();
        let (a_char, b_char) = match selection {
            Some(sel) if !sel.is_empty() => (sel.start, sel.end.saturating_sub(1).max(sel.start)),
            _ => (cursor, cursor),
        };
        let to_field = |ch: usize| -> (usize, usize) {
            let line = code.char_to_line(ch.min(code.len_chars()));
            let line_start = code.line_to_char(line);
            let line_text = code.slice(line_start, line_start + code.line_len(line));
            let char_col = ch.saturating_sub(line_start);
            let byte_col = line_text
                .char_indices()
                .nth(char_col)
                .map_or(line_text.len(), |(b, _)| b);
            (line, Self::org_table_field_index_at(&line_text, byte_col))
        };
        let corner_a = to_field(a_char);
        let corner_b = to_field(b_char);
        crate::org_table::table_range(&text, corner_a.0)?;
        Some((corner_a, corner_b))
    }

    /// `C-c C-x M-w`: copy the rectangle implied by the active selection (or
    /// just the current field) into the table rectangle clipboard.
    fn org_table_copy_rectangle(&mut self) {
        let Some((a, b)) = self.org_table_selection_corners() else {
            self.status = t!("status.org_table_no_table").to_string();
            return;
        };
        let Some(tab) = self.editor.active_tab() else {
            return;
        };
        let text = tab.editor.get_content();
        self.table_rectangle_clip = Some(crate::org_table::copy_rectangle(&text, a, b));
        self.status = t!("status.org_table_rectangle_copied").to_string();
    }

    /// `C-c C-x C-w`: like [`Self::org_table_copy_rectangle`], but also
    /// blanks the copied fields in the source table.
    fn org_table_cut_rectangle(&mut self) {
        let Some((a, b)) = self.org_table_selection_corners() else {
            self.status = t!("status.org_table_no_table").to_string();
            return;
        };
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let text = tab.editor.get_content();
        let line = tab.editor.cursor_line();
        let (new_text, rect) = crate::org_table::cut_rectangle(&text, a, b);
        tab.editor.set_content(&new_text);
        tab.editor.set_cursor_line(line);
        tab.dirty = true;
        self.table_rectangle_clip = Some(rect);
        self.status = t!("status.org_table_rectangle_cut").to_string();
    }

    /// `C-c C-x C-y`: paste the table rectangle clipboard with its upper-left
    /// cell at the field under the cursor.
    fn org_table_paste_rectangle(&mut self) {
        let Some(rect) = self.table_rectangle_clip.clone() else {
            self.status = t!("status.org_table_rectangle_empty").to_string();
            return;
        };
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let (line, byte_col) = Self::org_table_cursor_pos(tab);
        let text = tab.editor.get_content();
        if crate::org_table::table_range(&text, line).is_none() {
            self.status = t!("status.org_table_no_table").to_string();
            return;
        }
        let new_text = crate::org_table::paste_rectangle(&text, line, byte_col, &rect);
        tab.editor.set_content(&new_text);
        tab.editor.set_cursor_line(line);
        tab.dirty = true;
        self.status = t!("status.org_table_rectangle_pasted").to_string();
    }

    /// Dispatch an `org.table.*` action. Returns `true` if `action` matched.
    /// Extracted to keep [`App::org_action`] within the line limit.
    pub(super) fn org_table_action(&mut self, action: &str) -> bool {
        match action {
            "org.table.align" => self.org_table_line_op(crate::org_table::align),
            "org.table.recalc" => {
                if !self.org_table_recalc() {
                    self.status = t!("status.org_table_no_table").to_string();
                }
            }
            "org.table.insert_row_above" => {
                self.org_table_row_op(|t, l| crate::org_table::insert_row(t, l, false));
            }
            "org.table.insert_row_below" => {
                self.org_table_row_op(|t, l| crate::org_table::insert_row(t, l, true));
            }
            "org.table.kill_row" => self.org_table_line_op(crate::org_table::kill_row),
            "org.table.move_row_up" => self.org_table_row_op(crate::org_table::move_row_up),
            "org.table.move_row_down" => self.org_table_row_op(crate::org_table::move_row_down),
            "org.table.insert_hline" => {
                self.org_table_line_op(|t, l| crate::org_table::insert_hline(t, l, false));
            }
            "org.table.hline_and_move" => self.org_table_row_op(crate::org_table::hline_and_move),
            "org.table.insert_column" => self.org_table_col_op(crate::org_table::insert_column),
            "org.table.delete_column" => self.org_table_col_op(crate::org_table::delete_column),
            "org.table.move_column_left" => {
                self.org_table_col_op(crate::org_table::move_column_left);
            }
            "org.table.move_column_right" => {
                self.org_table_col_op(crate::org_table::move_column_right);
            }
            "org.table.sort" => self.org_table_sort_prompt(),
            "org.table.sum_column" => self.org_table_sum_column(),
            "org.table.copy_down" => {
                self.org_table_cell_op(|t, l, c| crate::org_table::copy_down(t, l, c, false));
            }
            "org.table.transpose" => self.org_table_line_op(crate::org_table::transpose),
            "org.table.create_from_region" => self.org_table_create_from_region(),
            "org.table.export_tsv" => self.org_table_export_tsv(),
            "org.table.copy_rectangle" => self.org_table_copy_rectangle(),
            "org.table.cut_rectangle" => self.org_table_cut_rectangle(),
            "org.table.paste_rectangle" => self.org_table_paste_rectangle(),
            _ => return false,
        }
        true
    }

    /// Open the interactive Column View overlay (Org `C-c C-x C-c`) on the
    /// active `.org` buffer, anchored at the cursor line. Warns when there is
    /// no editable Org buffer.
    pub(super) fn open_column_view(&mut self) {
        let Some(tab) = self.editor.active_tab() else {
            self.messages
                .warn(t!("msg.column_view_no_buffer").to_string());
            return;
        };
        if tab.is_image() || !self.active_is_org() {
            self.messages
                .warn(t!("msg.column_view_no_buffer").to_string());
            return;
        }
        let text = tab.text();
        let line = tab.editor.cursor_line();
        let file_name = self
            .active_path()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
        let today = Self::today_ymd();
        self.column_view = Some(crate::column_view::ColumnView::open(
            &text, line, today, file_name,
        ));
    }

    /// Route a key to the open Column View overlay and act on its outcome.
    pub(super) fn column_view_key(&mut self, key: KeyEvent) {
        let Some(mut text) = self.editor.active_tab().map(crate::editor::Tab::text) else {
            self.column_view = None;
            return;
        };
        let Some(view) = self.column_view.as_mut() else {
            return;
        };
        let outcome = view.handle_key(key, &mut text);
        let status = view.take_status();
        if let Some(msg) = status {
            self.status = msg;
        }
        if let Some(tab) = self.editor.active_tab_mut()
            && tab.text() != text
        {
            tab.editor.set_content(&text);
            tab.dirty = true;
        }
        match outcome {
            crate::column_view::Outcome::Close => self.column_view = None,
            crate::column_view::Outcome::NeedsColumnPrompt => {
                self.prompt = Some(Prompt::new(
                    PromptKind::OrgColumnsInsertColumn,
                    t!("prompt.org_columns_insert_column").to_string(),
                ));
            }
            crate::column_view::Outcome::Consumed => {}
        }
    }

    /// Accept the `S-M-Right` "insert column" prompt: add a blank column
    /// named `property` before the current one in the open Column View.
    pub(super) fn org_columns_insert_column(&mut self, property: &str) {
        let property = property.trim();
        if property.is_empty() {
            return;
        }
        let Some(text) = self.editor.active_tab().map(crate::editor::Tab::text) else {
            return;
        };
        if let Some(view) = self.column_view.as_mut() {
            view.insert_column_before(property, &text);
        }
    }

    /// Open the `:id` scope prompt for `org.columns.insert_dblock`.
    pub(super) fn org_columns_insert_dblock_prompt(&mut self) {
        if self.editor.active_tab().is_some() {
            self.prompt = Some(Prompt::new(
                PromptKind::OrgColumnsInsertDblock,
                t!("prompt.org_columns_insert_dblock").to_string(),
            ));
        }
    }

    /// Insert a fresh `#+BEGIN: columnview :id <id> ... #+END:` dynamic
    /// block at the cursor, scoped by the prompt's `:id` answer (blank
    /// defaults to `local`, matching [`vix_org::parse_dblock_params`]).
    pub(super) fn org_columns_insert_dblock(&mut self, id: &str) {
        let today = Self::today_ymd();
        let file_name = self
            .active_path()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.text();
        let id = id.trim();
        let params = crate::org::DblockParams {
            id: if id.is_empty() {
                "local".to_string()
            } else {
                id.to_string()
            },
            hlines: None,
            vlines: false,
            maxlevel: None,
            skip_empty_rows: false,
            exclude_tags: Vec::new(),
            indent: false,
            link: false,
            format: None,
        };
        let rendered =
            crate::org::render_columnview_dblock(&text, line, &params, today, file_name.as_deref());
        let mut lines: Vec<&str> = text.split('\n').collect();
        let rendered_lines: Vec<&str> = rendered.split('\n').collect();
        lines.splice(line..line, rendered_lines.iter().copied());
        let new_text = lines.join("\n");
        tab.editor.set_content(&new_text);
        tab.editor.set_cursor_line(line);
        tab.dirty = true;
        self.status = t!("status.org_columns_dblock_inserted").to_string();
    }

    /// `org.columns.update_dblock`: recompute the `columnview` dblock whose
    /// `#+BEGIN:` line the cursor sits on. Reports a status note off one.
    pub(super) fn org_columns_update_dblock(&mut self) {
        let today = Self::today_ymd();
        let file_name = self
            .active_path()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let line = tab.editor.cursor_line();
        let text = tab.text();
        match crate::org::update_columnview_dblock(&text, line, today, file_name.as_deref()) {
            Some(new_text) => {
                tab.editor.set_content(&new_text);
                tab.editor.set_cursor_line(line);
                tab.dirty = true;
                self.status = t!("status.org_columns_dblock_updated").to_string();
            }
            None => self.status = t!("status.org_columns_dblock_not_found").to_string(),
        }
    }

    /// `org.columns.update_all_dblocks`: recompute every `columnview` dblock
    /// found anywhere in the active buffer.
    pub(super) fn org_columns_update_all_dblocks(&mut self) {
        let today = Self::today_ymd();
        let file_name = self
            .active_path()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        let text = tab.text();
        let new_text =
            crate::org::update_all_columnview_dblocks(&text, today, file_name.as_deref());
        if new_text != text {
            let line = tab.editor.cursor_line();
            let clamped = line.min(new_text.split('\n').count().saturating_sub(1));
            tab.editor.set_content(&new_text);
            tab.editor.set_cursor_line(clamped);
            tab.dirty = true;
        }
        self.status = t!("status.org_columns_dblocks_updated_all").to_string();
    }
}
