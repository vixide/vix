//! The interactive Column View overlay (Org `C-c C-x C-c`): a live,
//! write-through spreadsheet view onto the outline, driven by the resolved
//! [`vix_org::ColumnsSpec`] ([`vix_org::resolve_columns_spec`]).
//!
//! Unlike [`crate::edit_table`]'s detached-grid-then-explicit-save model,
//! column view edits apply straight through to the real buffer text on every
//! commit: this matches Emacs's actual semantics (column view is a live
//! overlay on the *same* buffer, not a scratch copy — other Org commands see
//! an edited property value immediately) and this codebase's own dominant
//! Org convention ("read whole buffer text → pure transform → splice back").
//! [`ColumnView`] does not own the buffer text; the host ([`crate::app`])
//! passes the active tab's current text into [`ColumnView::handle_key`] as
//! `&mut String` and splices any change back via `Tab::editor::set_content`
//! (persisting to *disk* still goes through the normal save/dirty flow,
//! unchanged).
//!
//! Deliberately unimplemented subset of the Org manual's column-view keys
//! (a pragmatic subset, matching this codebase's convention elsewhere):
//! - `SPC` (a transient cell "peek" distinct from `v`) is not modeled; `v`
//!   alone shows the full value, via [`ColumnView::take_status`].
//! - `C-c C-o` (open the entry in another window) does not apply to a
//!   single-pane TUI overlay.
//! - Restricting `e`'s free-text edit to a column's allowed-value list is not
//!   enforced — `e` can type any value, matching this codebase's general
//!   "trust the user" convention for other free-text Org fields; `n`/`p`/
//!   digit-select are the constrained-cycle affordances instead.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::org::{
    ColumnDef, ColumnRow, ColumnsSpec, apply_column_edit, build_column_table, columns_spec_anchor,
    governing_subtree, headline_level, move_subtree_down, move_subtree_up, set_property,
    todo_keywords,
};

/// What the host should do after the overlay handled a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The key was handled internally; nothing further for the host to do.
    Consumed,
    /// The user asked to close the overlay (`q`/Esc, or `C-c C-c` off a
    /// checkbox-shaped field).
    Close,
    /// `S-M-Right`: the host should open a text prompt for a property name,
    /// then call [`ColumnView::insert_column_before`] with the answer.
    NeedsColumnPrompt,
}

/// The current interaction mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Navigating the field cursor and issuing commands.
    Normal,
    /// Editing the current field's raw value; keystrokes go to the edit buffer.
    Edit,
    /// Editing the current column's allowed-value (`PROPERTY_ALL`) list.
    EditAllowed,
}

/// Interactive Column View overlay state: the resolved format spec (mutable
/// in-session — columns can be inserted/deleted/reordered/resized), the
/// current display table, and the selected field.
pub struct ColumnView {
    spec: ColumnsSpec,
    rows: Vec<ColumnRow>,
    row: usize,
    col: usize,
    /// Per-column signed width delta layered on top of [`ColumnDef::width`]
    /// (or the auto-width the renderer computes from content) for `<`/`>`.
    width_overrides: Vec<i32>,
    mode: Mode,
    edit_buf: String,
    today: (i32, u32, u32),
    file_name: Option<String>,
    /// The line originally governing the view's scope, re-resolved after
    /// every buffer edit (see [`ColumnView::relocate_anchor`]) so a property
    /// drawer insertion elsewhere in the buffer cannot silently point the
    /// view at the wrong subtree.
    anchor_line: usize,
    /// The anchor headline's exact line text at open time (`None` when the
    /// view is file-scoped, i.e. opened before the first headline), used to
    /// relocate [`Self::anchor_line`] after edits shift line numbers.
    anchor_snapshot: Option<String>,
    /// True the instant after a first `C-c` while awaiting the second `C-c`
    /// of the `C-c C-c` chord.
    pending_ctrl_c: bool,
    /// First visible data row (vertical scroll).
    row_scroll: usize,
    /// A message for the host to show (e.g. `v`'s full-value peek), taken
    /// (and cleared) by [`ColumnView::take_status`].
    status_message: Option<String>,
}

impl ColumnView {
    /// Open a column view anchored at `line` of `text` (the cursor's line
    /// when the user invoked `org.column_view`). Resolves the effective
    /// [`ColumnsSpec`] and builds the initial display table.
    #[must_use]
    pub fn open(
        text: &str,
        line: usize,
        today: (i32, u32, u32),
        file_name: Option<String>,
    ) -> Self {
        let spec = crate::org::resolve_columns_spec(text, line);
        let (rows, _) = build_column_table(text, line, &spec, today, file_name.as_deref());
        // The headline that actually governs the view's scope (not
        // necessarily `line` itself, e.g. the cursor may sit on body text
        // under a headline) — `None` when `line` sits before the first
        // headline, meaning the resolved scope is the whole file.
        let anchor_headline = governing_subtree(text, line).map(|(start, _)| start);
        let anchor_snapshot =
            anchor_headline.and_then(|h| text.split('\n').nth(h).map(str::to_string));
        let width_overrides = vec![0; spec.columns.len()];
        ColumnView {
            spec,
            rows,
            row: 0,
            col: 0,
            width_overrides,
            mode: Mode::Normal,
            edit_buf: String::new(),
            today,
            file_name,
            anchor_line: anchor_headline.unwrap_or(0),
            anchor_snapshot,
            pending_ctrl_c: false,
            row_scroll: 0,
            status_message: None,
        }
    }

    // ----- read-only accessors for the renderer -----------------------------

    /// The active columns, in display order.
    #[must_use]
    pub fn columns(&self) -> &[ColumnDef] {
        &self.spec.columns
    }

    /// Number of data rows.
    #[must_use]
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Number of columns.
    #[must_use]
    pub fn col_count(&self) -> usize {
        self.spec.columns.len()
    }

    /// The selected row index.
    #[must_use]
    pub fn row(&self) -> usize {
        self.row
    }

    /// The selected column index.
    #[must_use]
    pub fn col(&self) -> usize {
        self.col
    }

    /// The first visible data row (vertical scroll offset).
    #[must_use]
    pub fn row_scroll(&self) -> usize {
        self.row_scroll
    }

    /// The display value at `(r, c)`, or `""` when out of range.
    #[must_use]
    pub fn cell(&self, r: usize, c: usize) -> &str {
        self.rows
            .get(r)
            .and_then(|row| row.values.get(c))
            .map_or("", String::as_str)
    }

    /// Whether the current field is being edited (raw-value edit mode).
    #[must_use]
    pub fn is_editing(&self) -> bool {
        self.mode == Mode::Edit
    }

    /// Whether the current column's allowed-value list is being edited.
    #[must_use]
    pub fn is_editing_allowed(&self) -> bool {
        self.mode == Mode::EditAllowed
    }

    /// The in-progress edit text (valid while [`Self::is_editing`] or
    /// [`Self::is_editing_allowed`]).
    #[must_use]
    pub fn edit_buffer(&self) -> &str {
        &self.edit_buf
    }

    /// The effective display width of column `i`: its declared width (or an
    /// auto width from header/content length, clamped to a sane `[3, 40]`
    /// band), plus that column's `<`/`>` override, floored at 3.
    #[must_use]
    pub fn column_width(&self, i: usize) -> usize {
        let Some(def) = self.spec.columns.get(i) else {
            return 3;
        };
        let base: i32 = def.width.map_or_else(
            || {
                let header_len = def
                    .title
                    .as_deref()
                    .unwrap_or(&def.property)
                    .chars()
                    .count();
                let content_max = self
                    .rows
                    .iter()
                    .filter_map(|r| r.values.get(i))
                    .map(|v| v.chars().count())
                    .max()
                    .unwrap_or(0);
                i32::try_from(header_len.max(content_max).clamp(3, 40)).unwrap_or(10)
            },
            i32::from,
        );
        let over = self.width_overrides.get(i).copied().unwrap_or(0);
        usize::try_from((base + over).max(3)).unwrap_or(3)
    }

    /// Adjust the vertical scroll so the selected row stays within a body
    /// window of `height` rows. Called before drawing.
    pub fn ensure_row_visible(&mut self, height: usize) {
        let height = height.max(1);
        let count = self.rows.len();
        if count == 0 {
            self.row_scroll = 0;
            return;
        }
        if self.row < self.row_scroll {
            self.row_scroll = self.row;
        } else if self.row >= self.row_scroll + height {
            self.row_scroll = self.row + 1 - height;
        }
        self.row_scroll = self.row_scroll.min(count.saturating_sub(height));
    }

    /// Take (clear) the pending status message, if any, for the host to show.
    pub fn take_status(&mut self) -> Option<String> {
        self.status_message.take()
    }

    // ----- key handling -------------------------------------------------

    /// Interpret a key event against `text` (the active tab's current
    /// buffer text), mutating both this overlay and, in place, `text` when
    /// the key commits an edit. Reports what the host should do next.
    pub fn handle_key(&mut self, key: KeyEvent, text: &mut String) -> Outcome {
        match self.mode {
            Mode::Edit => {
                self.edit_key(key, text);
                Outcome::Consumed
            }
            Mode::EditAllowed => {
                self.edit_allowed_key(key, text);
                Outcome::Consumed
            }
            Mode::Normal => self.normal_key(key, text),
        }
    }

    /// Handle a key while editing a field's raw value.
    fn edit_key(&mut self, key: KeyEvent, text: &mut String) {
        match key.code {
            KeyCode::Enter => {
                let value = std::mem::take(&mut self.edit_buf);
                self.commit_value(text, &value);
                self.mode = Mode::Normal;
            }
            KeyCode::Esc => {
                self.edit_buf.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.edit_buf.pop();
            }
            KeyCode::Char(c) => self.edit_buf.push(c),
            _ => {}
        }
    }

    /// Handle a key while editing a column's allowed-value list.
    fn edit_allowed_key(&mut self, key: KeyEvent, text: &mut String) {
        match key.code {
            KeyCode::Enter => self.commit_allowed_edit(text),
            KeyCode::Esc => {
                self.edit_buf.clear();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.edit_buf.pop();
            }
            KeyCode::Char(c) => self.edit_buf.push(c),
            _ => {}
        }
    }

    /// Handle a key in normal (navigation/command) mode.
    fn normal_key(&mut self, key: KeyEvent, text: &mut String) -> Outcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);

        if ctrl && key.code == KeyCode::Char('c') {
            if self.pending_ctrl_c {
                self.pending_ctrl_c = false;
                return self.ctrl_c_ctrl_c(text);
            }
            self.pending_ctrl_c = true;
            return Outcome::Consumed;
        }
        self.pending_ctrl_c = false;

        match key.code {
            KeyCode::Left if alt && shift => self.delete_column(text),
            KeyCode::Right if alt && shift => return Outcome::NeedsColumnPrompt,
            KeyCode::Left if alt => self.move_column(text, true),
            KeyCode::Right if alt => self.move_column(text, false),
            KeyCode::Up if alt => self.move_row(text, true),
            KeyCode::Down if alt => self.move_row(text, false),
            KeyCode::Right if shift => self.cycle_allowed(text, true),
            KeyCode::Left if shift => self.cycle_allowed(text, false),
            KeyCode::Left => self.col = self.col.saturating_sub(1),
            KeyCode::Right => {
                if self.col + 1 < self.spec.columns.len() {
                    self.col += 1;
                }
            }
            KeyCode::Up => self.row = self.row.saturating_sub(1),
            KeyCode::Down => {
                if self.row + 1 < self.rows.len() {
                    self.row += 1;
                }
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let n = c.to_digit(10).unwrap_or(0);
                self.jump_allowed(text, if n == 0 { 10 } else { n as usize });
            }
            KeyCode::Char('n') => self.cycle_allowed(text, true),
            KeyCode::Char('p') => self.cycle_allowed(text, false),
            KeyCode::Char('e') => self.begin_edit(),
            KeyCode::Char('a') => self.begin_edit_allowed(),
            KeyCode::Char('v') => self.show_value(),
            KeyCode::Char('<') => self.narrow(),
            KeyCode::Char('>') => self.widen(),
            KeyCode::Char('r' | 'g') => self.rebuild_rows(text),
            KeyCode::Char('q') | KeyCode::Esc => return Outcome::Close,
            _ => {}
        }
        Outcome::Consumed
    }

    /// `C-c C-c`: toggle a checkbox-shaped field in place, or close the
    /// overlay when the current field is not checkbox-shaped (Org's own
    /// "do the right thing" convention, matching `App::org_ctrl_c_ctrl_c`).
    fn ctrl_c_ctrl_c(&mut self, text: &mut String) -> Outcome {
        let raw = self.current_raw_value();
        let checkbox_summary = self
            .spec
            .columns
            .get(self.col)
            .and_then(|c| c.summary.as_deref())
            .is_some_and(|s| matches!(s, "X" | "X/" | "X%"));
        let looks_checkbox = matches!(raw.trim(), "[X]" | "[ ]" | "[-]")
            || (raw.trim().is_empty() && checkbox_summary);
        if !looks_checkbox {
            return Outcome::Close;
        }
        let new_value = if raw.trim() == "[X]" { "[ ]" } else { "[X]" };
        self.commit_value(text, new_value);
        Outcome::Consumed
    }

    // ----- field navigation & value edits ---------------------------------

    /// The current field's raw display value (`""` out of range).
    fn current_raw_value(&self) -> String {
        self.rows
            .get(self.row)
            .and_then(|r| r.values.get(self.col))
            .cloned()
            .unwrap_or_default()
    }

    /// Seed the edit buffer with the current field's raw value and enter
    /// [`Mode::Edit`].
    fn begin_edit(&mut self) {
        self.edit_buf = self.current_raw_value();
        self.mode = Mode::Edit;
    }

    /// Show the current field's full raw value via [`Self::status_message`]
    /// (there is no popup mechanism in this overlay).
    fn show_value(&mut self) {
        self.status_message = Some(self.current_raw_value());
    }

    /// `<`: narrow the current column by one.
    fn narrow(&mut self) {
        if let Some(w) = self.width_overrides.get_mut(self.col) {
            *w -= 1;
        }
    }

    /// `>`: widen the current column by one.
    fn widen(&mut self) {
        if let Some(w) = self.width_overrides.get_mut(self.col) {
            *w += 1;
        }
    }

    /// The allowed-value list for the current column: its declared
    /// `PROPERTY_ALL` list, or (only for `TODO`, with no declared list) the
    /// crate's fixed TODO-keyword fallback.
    fn allowed_values_for_current(&self) -> Vec<String> {
        let Some(col) = self.spec.columns.get(self.col) else {
            return Vec::new();
        };
        let declared = self.spec.allowed_values.get(&col.property).cloned();
        declared.unwrap_or_else(|| {
            if col.property.eq_ignore_ascii_case("TODO") {
                todo_keywords().iter().map(|s| (*s).to_string()).collect()
            } else {
                Vec::new()
            }
        })
    }

    /// `n`/`S-Right` (`forward`) or `p`/`S-Left`: cycle the current field to
    /// the next/previous allowed value, wrapping. No-op with no list.
    fn cycle_allowed(&mut self, text: &mut String, forward: bool) {
        let list = self.allowed_values_for_current();
        if list.is_empty() {
            return;
        }
        let current = self.current_raw_value();
        let idx = list.iter().position(|v| *v == current);
        let next = match idx {
            Some(i) if forward => (i + 1) % list.len(),
            Some(i) => (i + list.len() - 1) % list.len(),
            None => 0,
        };
        let value = list[next].clone();
        self.commit_value(text, &value);
    }

    /// `1`-`9`/`0`: jump the current field to the `n`th (1-indexed, `0` = 10th)
    /// allowed value. No-op with no list or fewer entries.
    fn jump_allowed(&mut self, text: &mut String, n: usize) {
        let list = self.allowed_values_for_current();
        let Some(value) = list.get(n.saturating_sub(1)).cloned() else {
            return;
        };
        self.commit_value(text, &value);
    }

    /// Apply `new_value` to the current field via [`apply_column_edit`],
    /// splice the rewritten text back into `text`, relocate the anchor, and
    /// rebuild the display table so ancestor summaries recompute instantly.
    fn commit_value(&mut self, text: &mut String, new_value: &str) {
        let Some(row_line) = self.rows.get(self.row).map(|r| r.line) else {
            return;
        };
        if let Some(new_text) = apply_column_edit(text, row_line, &self.spec, self.col, new_value) {
            self.relocate_anchor(&new_text);
            *text = new_text;
            self.rebuild_rows(text);
        }
    }

    // ----- allowed-value list editing ('a') --------------------------------

    /// `a`: seed the edit buffer with the current column's allowed-value
    /// list (space/quote-joined) and enter [`Mode::EditAllowed`].
    fn begin_edit_allowed(&mut self) {
        let existing = self.allowed_values_for_current();
        self.edit_buf = join_quoted(&existing);
        self.mode = Mode::EditAllowed;
    }

    /// Commit an edited allowed-value list: write a `PROPERTY_ALL` property
    /// into the drawer of whichever headline anchors the resolved spec (or a
    /// file-level drawer before the first headline when it's file-scoped),
    /// and update `spec.allowed_values` in-session so cycling sees it
    /// immediately.
    fn commit_allowed_edit(&mut self, text: &mut String) {
        self.mode = Mode::Normal;
        let Some(col) = self.spec.columns.get(self.col) else {
            self.edit_buf.clear();
            return;
        };
        let property = col.property.clone();
        let values = tokenize_quoted(&std::mem::take(&mut self.edit_buf));
        let joined = join_quoted(&values);
        let name = format!("{property}_ALL");
        let anchor = columns_spec_anchor(text, self.anchor_line);
        let new_text = match anchor {
            Some(h) => set_property(text, h, &name, &joined),
            None => Some(write_file_level_property(text, &name, &joined)),
        };
        if let Some(new_text) = new_text {
            self.relocate_anchor(&new_text);
            *text = new_text;
            self.spec.allowed_values.insert(property, values);
            self.rebuild_rows(text);
        }
    }

    // ----- structural column edits ------------------------------------------

    /// Insert a new blank column named `property` before the current column
    /// (`S-M-Right`, after the host's prompt returns an answer).
    pub fn insert_column_before(&mut self, property: &str, text: &str) {
        let property = property.trim();
        if property.is_empty() {
            return;
        }
        let idx = self.col.min(self.spec.columns.len());
        self.spec.columns.insert(
            idx,
            ColumnDef {
                property: property.to_string(),
                title: None,
                width: None,
                summary: None,
            },
        );
        self.width_overrides.insert(idx, 0);
        self.col = idx;
        self.rebuild_rows(text);
    }

    /// `S-M-Left`: delete the current column (a spec-only change; never
    /// deletes the last remaining column).
    fn delete_column(&mut self, text: &str) {
        if self.spec.columns.len() <= 1 {
            return;
        }
        self.spec.columns.remove(self.col);
        self.width_overrides.remove(self.col);
        self.col = self.col.min(self.spec.columns.len().saturating_sub(1));
        self.rebuild_rows(text);
    }

    /// `M-Left`/`M-Right` (`left`): swap the current column with its
    /// neighbor and follow the selection.
    fn move_column(&mut self, text: &str, left: bool) {
        let n = self.spec.columns.len();
        let other = if left {
            self.col.checked_sub(1)
        } else {
            (self.col + 1 < n).then_some(self.col + 1)
        };
        let Some(other) = other else {
            return;
        };
        self.spec.columns.swap(self.col, other);
        self.width_overrides.swap(self.col, other);
        self.col = other;
        self.rebuild_rows(text);
    }

    /// `M-Up`/`M-Down` (`up`): move the current row's headline itself in the
    /// outline via [`move_subtree_up`]/[`move_subtree_down`], then rebuild
    /// (line numbers throughout the buffer may shift) and follow the moved
    /// row. Silent no-op with no sibling to swap with.
    fn move_row(&mut self, text: &mut String, up: bool) {
        let Some(line) = self.rows.get(self.row).map(|r| r.line) else {
            return;
        };
        let f = if up {
            move_subtree_up
        } else {
            move_subtree_down
        };
        let Some((new_text, new_line)) = f(text, line) else {
            return;
        };
        self.relocate_anchor(&new_text);
        *text = new_text;
        self.rebuild_rows(text);
        if let Some(idx) = self.rows.iter().position(|r| r.line == new_line) {
            self.row = idx;
        }
    }

    // ----- rebuild & anchor tracking ---------------------------------------

    /// Recompute the display table from `text` for the current spec, at the
    /// current anchor, clamping the selection to the new bounds. Called
    /// after every edit (including a spec-only column change) and by
    /// `r`/`g` for explicit (usually redundant) recompute.
    fn rebuild_rows(&mut self, text: &str) {
        let (rows, _) = build_column_table(
            text,
            self.anchor_line,
            &self.spec,
            self.today,
            self.file_name.as_deref(),
        );
        self.rows = rows;
        self.row = self.row.min(self.rows.len().saturating_sub(1));
        self.col = self.col.min(self.spec.columns.len().saturating_sub(1));
    }

    /// Re-locate [`Self::anchor_line`] in `new_text` after an edit that may
    /// have shifted line numbers: searches for the anchor headline's exact
    /// original line text (stable unless *that* row's own `ITEM`/`TODO`/
    /// `PRIORITY`/`TAGS` was just edited, in which case the stale index is
    /// kept as a best-effort fallback — the anchor's own headline line never
    /// moves from edits made to *other* rows, only new lines get inserted
    /// below/after it). A no-op when the view is file-scoped (`None`
    /// snapshot): line 0 always resolves to whole-file scope.
    fn relocate_anchor(&mut self, new_text: &str) {
        let Some(snap) = &self.anchor_snapshot else {
            self.anchor_line = 0;
            return;
        };
        if let Some(idx) = new_text.split('\n').position(|l| l == snap) {
            self.anchor_line = idx;
        }
    }
}

/// Tokenize a value list the same way `vix_org`'s `PROPERTY_ALL` parser does:
/// a double-quoted run (which may contain spaces) counts as one token;
/// otherwise tokens are whitespace-separated.
fn tokenize_quoted(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c == '"' {
            chars.next();
            let mut tok = String::new();
            for c2 in chars.by_ref() {
                if c2 == '"' {
                    break;
                }
                tok.push(c2);
            }
            out.push(tok);
        } else {
            let mut tok = String::new();
            while let Some(&c2) = chars.peek() {
                if c2.is_whitespace() {
                    break;
                }
                tok.push(c2);
                chars.next();
            }
            out.push(tok);
        }
    }
    out
}

/// Join `values` back into a `PROPERTY_ALL` value list, quoting any entry
/// that is empty or contains whitespace.
fn join_quoted(values: &[String]) -> String {
    values
        .iter()
        .map(|v| {
            if v.is_empty() || v.contains(char::is_whitespace) {
                format!("\"{v}\"")
            } else {
                v.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Write (creating or updating) a file-level `:NAME: VALUE` property in the
/// drawer before the first headline — the file-scoped counterpart of
/// [`set_property`], which requires a governing headline. A minimal,
/// self-contained line-splice: `PROPERTY_ALL` lists are a secondary,
/// occasional-use feature, so this does not try to reuse the headline-drawer
/// machinery.
fn write_file_level_property(text: &str, name: &str, value: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let first_headline = lines
        .iter()
        .position(|l| headline_level(l).is_some())
        .unwrap_or(lines.len());
    let entry = format!(":{name}: {value}");
    let needle = format!(":{}:", name.to_ascii_lowercase());
    let mut i = 0;
    while i < first_headline {
        if lines[i].trim().eq_ignore_ascii_case(":PROPERTIES:")
            && let Some(end_rel) = lines[i + 1..first_headline]
                .iter()
                .position(|l| l.trim().eq_ignore_ascii_case(":END:"))
        {
            let end = i + 1 + end_rel;
            let mut out: Vec<String> = lines.iter().map(|s| (*s).to_string()).collect();
            if let Some(pi) = (i + 1..end).find(|&k| {
                lines[k]
                    .trim_start()
                    .to_ascii_lowercase()
                    .starts_with(&needle)
            }) {
                out[pi] = entry;
            } else {
                out.insert(end, entry);
            }
            return out.join("\n");
        }
        i += 1;
    }
    let mut out: Vec<String> = lines.iter().map(|s| (*s).to_string()).collect();
    out.splice(
        first_headline..first_headline,
        [":PROPERTIES:".to_string(), entry, ":END:".to_string()],
    );
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn code(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn shift(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::SHIFT)
    }

    fn alt(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::ALT)
    }

    fn alt_shift(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::ALT | KeyModifiers::SHIFT)
    }

    fn ctrl_c() -> KeyEvent {
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
    }

    const DOC: &str = "\
* Parent
:PROPERTIES:
:COLUMNS: %ITEM %TODO %25EFFORT(Effort){:mean} %Owner
:EFFORT: unused
:Owner_ALL: Tammy Mark
:END:
** Child
:PROPERTIES:
:EFFORT: 1h
:OWNER: Tammy
:END:
";

    fn spec_text() -> String {
        DOC.to_string()
    }

    #[test]
    fn opens_and_navigates() {
        let text = spec_text();
        let mut view = ColumnView::open(&text, 0, (2026, 8, 12), None);
        assert_eq!(view.row_count(), 2);
        assert_eq!(view.col_count(), 4);
        let mut buf = text.clone();
        assert_eq!(
            view.handle_key(code(KeyCode::Down), &mut buf),
            Outcome::Consumed
        );
        assert_eq!(view.row(), 1);
        view.handle_key(code(KeyCode::Right), &mut buf);
        assert_eq!(view.col(), 1);
        view.handle_key(code(KeyCode::Up), &mut buf);
        assert_eq!(view.row(), 0, "clamped at top");
    }

    #[test]
    fn edits_a_field_and_recomputes_ancestor_summary() {
        let text = spec_text();
        let mut view = ColumnView::open(&text, 0, (2026, 8, 12), None);
        let mut buf = text.clone();
        view.handle_key(code(KeyCode::Down), &mut buf); // Child row
        view.handle_key(code(KeyCode::Right), &mut buf); // TODO
        view.handle_key(code(KeyCode::Right), &mut buf); // EFFORT
        view.handle_key(key('e'), &mut buf);
        assert!(view.is_editing());
        assert_eq!(view.edit_buffer(), "1h", "seeded with the raw value");
        view.handle_key(code(KeyCode::Backspace), &mut buf);
        view.handle_key(code(KeyCode::Backspace), &mut buf);
        for c in "3h".chars() {
            view.handle_key(key(c), &mut buf);
        }
        view.handle_key(code(KeyCode::Enter), &mut buf);
        assert!(!view.is_editing());
        assert!(buf.contains(":EFFORT: 3h"), "{buf:?}");
        assert_eq!(view.cell(1, 2), "3h", "child's own display updated");
        assert_eq!(
            view.cell(0, 2),
            "3h 0min",
            "parent's :mean summary recomputed instantly"
        );
    }

    #[test]
    fn allowed_value_cycling_and_digit_select() {
        let text = spec_text();
        let mut view = ColumnView::open(&text, 0, (2026, 8, 12), None);
        let mut buf = text.clone();
        view.handle_key(code(KeyCode::Down), &mut buf);
        view.col = 3; // OWNER column, already "Tammy" (index 0 of the ALL list)
        view.handle_key(key('n'), &mut buf);
        assert_eq!(view.cell(1, 3), "Mark", "cycled forward from Tammy");
        view.handle_key(key('n'), &mut buf);
        assert_eq!(view.cell(1, 3), "Tammy", "wraps");
        view.handle_key(key('2'), &mut buf);
        assert_eq!(view.cell(1, 3), "Mark", "digit-select is 1-indexed");
        view.handle_key(shift(KeyCode::Left), &mut buf);
        assert_eq!(view.cell(1, 3), "Tammy", "S-Left cycles backward");
    }

    #[test]
    fn todo_column_falls_back_to_the_fixed_keyword_list() {
        let doc = "* TODO Task\n";
        let mut view = ColumnView::open(doc, 0, (2026, 8, 12), None);
        let mut buf = doc.to_string();
        view.col = 1; // TODO column of the hardcoded default spec
        view.handle_key(key('n'), &mut buf);
        assert!(
            todo_keywords().contains(&view.cell(0, 1)),
            "cycled into the fixed keyword list: {}",
            view.cell(0, 1)
        );
    }

    #[test]
    fn checkbox_toggle_via_ctrl_c_ctrl_c() {
        let doc = "#+COLUMNS: %ITEM %Done\n* Top\n** Child\n:PROPERTIES:\n:Done: [ ]\n:END:\n";
        let mut view = ColumnView::open(doc, 0, (2026, 8, 12), None);
        let mut buf = doc.to_string();
        view.handle_key(code(KeyCode::Down), &mut buf);
        view.handle_key(code(KeyCode::Right), &mut buf);
        assert_eq!(view.cell(1, 1), "[ ]");
        view.handle_key(ctrl_c(), &mut buf);
        assert_eq!(view.handle_key(ctrl_c(), &mut buf), Outcome::Consumed);
        assert_eq!(view.cell(1, 1), "[X]");
        view.handle_key(ctrl_c(), &mut buf);
        assert_eq!(view.handle_key(ctrl_c(), &mut buf), Outcome::Consumed);
        assert_eq!(view.cell(1, 1), "[ ]");
    }

    #[test]
    fn ctrl_c_ctrl_c_closes_off_a_non_checkbox_field() {
        let doc = "* Task\n";
        let mut view = ColumnView::open(doc, 0, (2026, 8, 12), None);
        let mut buf = doc.to_string();
        view.handle_key(ctrl_c(), &mut buf);
        assert_eq!(view.handle_key(ctrl_c(), &mut buf), Outcome::Close);
    }

    #[test]
    fn column_narrow_widen_insert_delete_move() {
        let doc = "* Top\n** Child\n";
        let mut view = ColumnView::open(doc, 0, (2026, 8, 12), None);
        let buf_text = doc.to_string();
        let base = view.column_width(0);
        view.handle_key(key('<'), &mut buf_text.clone());
        assert_eq!(view.column_width(0), base - 1);
        view.handle_key(key('>'), &mut buf_text.clone());
        view.handle_key(key('>'), &mut buf_text.clone());
        assert_eq!(view.column_width(0), base + 1);

        let before = view.col_count();
        view.insert_column_before("NEW", &buf_text);
        assert_eq!(view.col_count(), before + 1);
        assert_eq!(view.columns()[0].property, "NEW");

        let mut buf = buf_text.clone();
        view.handle_key(alt_shift(KeyCode::Left), &mut buf);
        assert_eq!(view.col_count(), before, "S-M-Left deletes the column");

        let mut buf = buf_text.clone();
        view.handle_key(alt(KeyCode::Right), &mut buf);
        assert_eq!(view.col(), 1, "M-Right swaps toward the end");
    }

    #[test]
    fn insert_column_prompt_outcome() {
        let doc = "* Top\n";
        let mut view = ColumnView::open(doc, 0, (2026, 8, 12), None);
        let mut buf = doc.to_string();
        assert_eq!(
            view.handle_key(alt_shift(KeyCode::Right), &mut buf),
            Outcome::NeedsColumnPrompt
        );
    }

    #[test]
    fn move_row_follows_the_moved_headline() {
        // A leading blank line puts the view before the first headline, so it
        // resolves file-scope (both top-level siblings visible) rather than
        // scoping to a single headline's own (single-node) subtree.
        let doc = "\n* A\n* B\n";
        let mut view = ColumnView::open(doc, 0, (2026, 8, 12), None);
        let mut buf = doc.to_string();
        view.handle_key(code(KeyCode::Down), &mut buf); // select B
        assert_eq!(view.cell(view.row(), 0), "B");
        view.handle_key(alt(KeyCode::Up), &mut buf);
        assert!(
            buf.find("* B").unwrap() < buf.find("* A").unwrap(),
            "B moved above A: {buf:?}"
        );
        assert_eq!(view.cell(view.row(), 0), "B", "selection follows the move");
    }

    #[test]
    fn allowed_value_list_edit_writes_property_all_and_updates_session() {
        let doc = "* Top\n** Child\n";
        let mut view = ColumnView::open(doc, 0, (2026, 8, 12), None);
        view.insert_column_before("Status", doc);
        let mut buf = doc.to_string();
        view.handle_key(key('a'), &mut buf);
        assert!(view.is_editing_allowed());
        for c in "Todo Doing Done".chars() {
            view.handle_key(key(c), &mut buf);
        }
        view.handle_key(code(KeyCode::Enter), &mut buf);
        assert!(!view.is_editing_allowed());
        assert!(buf.contains(":Status_ALL: Todo Doing Done"), "{buf:?}");
        view.handle_key(key('n'), &mut buf);
        assert_eq!(view.cell(view.row(), view.col()), "Todo");
    }

    #[test]
    fn quit_closes() {
        let doc = "* Task\n";
        let mut view = ColumnView::open(doc, 0, (2026, 8, 12), None);
        let mut buf = doc.to_string();
        assert_eq!(view.handle_key(key('q'), &mut buf), Outcome::Close);
        assert_eq!(
            view.handle_key(code(KeyCode::Esc), &mut buf),
            Outcome::Close
        );
    }
}
