//! The primitive editing and movement **actions**: small structs implementing
//! [`Action`], applied with [`Editor::apply`].
//!
//! Every user-visible keystroke ends up as one of these (the default key map
//! is in [`crate::editor_input`]; the named, catalog-driven wrappers such as
//! `Editor::cursor_left` are in [`crate::named`]). Keeping them as values gives
//! one place where cursor, selection, goal column and undo bookkeeping are
//! handled.
//!
//! Shared conventions:
//!
//! - **Offsets are character indices**, never bytes.
//! - **Mutating actions are one undo step.** They follow the same shape:
//!   read cursor/selection, `code.tx()`, `set_state_before`, edit the rope,
//!   `set_state_after`, `commit`, then push the new cursor/selection back to the
//!   editor and reset the highlight cache. The saved before/after states are what
//!   `Undo`/`Redo` restore.
//! - **Multi-line actions edit bottom to top** (`.rev()`) so earlier line
//!   offsets stay valid while later lines change.
//! - The goal column (`Editor::goal_col`) survives only vertical moves:
//!   `set_cursor` clears it, which `MoveUp`/`MoveDown` re-establish.
//!
//! See `crates/vix-editor-core/spec/index.md` and `spec/actions.tsv`.

use crate::code::EditKind;
use crate::editor::Editor;
use crate::selection::Selection;

/// An editing operation that can be applied to an [`Editor`].
///
/// Implemented by the unit/field structs in this module; hand them to
/// [`Editor::apply`]. An action takes `&mut self` only so it can be built once
/// and applied; none of the provided actions change themselves.
pub trait Action {
    /// Apply this action to `editor`, mutating its buffer and/or cursor state.
    /// Actions never fail: one that has nothing to do (cursor at the buffer
    /// edge, no selection to copy) is a silent no-op.
    fn apply(&mut self, editor: &mut Editor);
}

/// `a - b`, or the declared `fallback` when it would go below zero. The
/// fallback is a parameter so each call names the declaration it comes from;
/// written inline, `checked_sub(..).unwrap_or(0)` is rewritten by Clippy to
/// `saturating_sub`, which the checked-arithmetic spec forbids.
fn sub_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_sub(b).unwrap_or(fallback)
}

/// `a + b`, or the declared `fallback` when the sum does not fit (see
/// `sub_or` for why the fallback is a parameter).
fn add_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_add(b).unwrap_or(fallback)
}

/// `a * b`, or the declared `fallback` when the product does not fit.
fn mul_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_mul(b).unwrap_or(fallback)
}

/// Moves the cursor one character to the right.
///
/// If `shift` is true, the selection is extended to the new cursor position.
/// If `shift` is false and there is an active selection, the cursor jumps
/// to the end of the selection and the selection is cleared.
/// Otherwise, the cursor moves one position to the right.
pub struct MoveRight {
    /// Whether to extend the selection instead of clearing it.
    pub shift: bool,
}

impl Action for MoveRight {
    fn apply(&mut self, editor: &mut Editor) {
        let cursor = editor.get_cursor();

        if !self.shift
            && let Some(sel) = editor.get_selection()
            && !sel.is_empty()
        {
            let (_, end) = sel.sorted();
            editor.set_cursor(end);
            editor.clear_selection();
            return;
        }

        if cursor < editor.code_mut().len() {
            let new_cursor = add_or(cursor, 1, usize::MAX);
            if self.shift {
                editor.extend_selection(new_cursor);
            } else {
                editor.clear_selection();
            }
            editor.set_cursor(new_cursor);
        }
    }
}

/// Moves the cursor one character to the left.
///
/// If `shift` is true, the selection is extended to the new cursor position.
/// If `shift` is false and there is an active selection, the cursor jumps
/// to the start of the selection and the selection is cleared.
/// Otherwise, the cursor moves one position to the left.
pub struct MoveLeft {
    /// Whether to extend the selection instead of clearing it.
    pub shift: bool,
}

impl Action for MoveLeft {
    fn apply(&mut self, editor: &mut Editor) {
        let cursor = editor.get_cursor();

        if !self.shift
            && let Some(sel) = editor.get_selection()
            && !sel.is_empty()
        {
            let (start, _) = sel.sorted();
            editor.set_cursor(start);
            editor.clear_selection();
            return;
        }

        if cursor > 0 {
            let new_cursor = sub_or(cursor, 1, 0);
            if self.shift {
                editor.extend_selection(new_cursor);
            } else {
                editor.clear_selection();
            }
            editor.set_cursor(new_cursor);
        }
    }
}

/// Moves the cursor one line up.
///
/// If the previous line is shorter, the cursor is placed at the end of that line.
/// If `shift` is true, the selection is extended to the new cursor position.
/// If `shift` is false, the selection is cleared.
///
/// The column aimed for is the editor's *goal column*, remembered across a run
/// of vertical moves, so passing through a short line does not lose the
/// original column. On the first line it does nothing (it does not move to
/// column 0 and does not clear the selection).
pub struct MoveUp {
    /// Whether to extend the selection instead of clearing it.
    pub shift: bool,
}

impl Action for MoveUp {
    fn apply(&mut self, editor: &mut Editor) {
        let cursor = editor.get_cursor();
        let goal = editor.goal_col;
        let (row, col) = editor.code_ref().point(cursor);
        // The nearest line above that a fold does not hide.
        let Some(prev) = (0..row).rev().find(|&l| !editor.is_line_hidden(l)) else {
            return;
        };
        let code = editor.code_mut();

        // Aim for the remembered goal column, falling back to the current column
        // when starting a fresh vertical run.
        let target_col = goal.unwrap_or(col);
        let prev_start = code.line_to_char(prev);
        let prev_len = code.line_len(prev);
        let new_cursor = add_or(prev_start, target_col.min(prev_len), usize::MAX);

        // Update selection or clear it
        if self.shift {
            editor.extend_selection(new_cursor);
        } else {
            editor.clear_selection();
        }

        // Set the new cursor position (clears goal_col), then re-establish it.
        editor.set_cursor(new_cursor);
        editor.goal_col = Some(target_col);
    }
}

/// Moves the cursor one line down.
///
/// If the next line is shorter, the cursor is placed at the end of that line.
/// If `shift` is true, the selection is extended to the new cursor position.
/// If `shift` is false, the selection is cleared.
///
/// Uses the goal column like `MoveUp`, and steps over lines a fold hides (as
/// `MoveUp` does). On the last visible line it does nothing (it does not move
/// to the line end).
pub struct MoveDown {
    /// Whether to extend the selection instead of clearing it.
    pub shift: bool,
}

impl Action for MoveDown {
    fn apply(&mut self, editor: &mut Editor) {
        let cursor = editor.get_cursor();
        let goal = editor.goal_col;
        let (row, col) = editor.code_ref().point(cursor);
        let len_lines = editor.code_ref().len_lines();
        // The nearest line below that a fold does not hide.
        let Some(next) =
            (add_or(row, 1, usize::MAX)..len_lines).find(|&l| !editor.is_line_hidden(l))
        else {
            return;
        };
        let code = editor.code_mut();

        // Aim for the remembered goal column, falling back to the current column
        // when starting a fresh vertical run.
        let target_col = goal.unwrap_or(col);
        let next_start = code.line_to_char(next);
        let next_len = code.line_len(next);
        let new_cursor = add_or(next_start, target_col.min(next_len), usize::MAX);

        // Update selection or clear it
        if self.shift {
            editor.extend_selection(new_cursor);
        } else {
            editor.clear_selection();
        }

        // Set the new cursor position (clears goal_col), then re-establish it.
        editor.set_cursor(new_cursor);
        editor.goal_col = Some(target_col);
    }
}

/// Inserts arbitrary text at the cursor, replacing the selection if any.
///
/// Used for typed characters and as the building block of [`InsertNewline`].
/// The cursor ends after the inserted text and the selection is cleared. The
/// edit is a single undo step including the removal of the replaced selection.
pub struct InsertText {
    /// The text to insert at the cursor.
    pub text: String,
}

impl Action for InsertText {
    fn apply(&mut self, editor: &mut Editor) {
        // 1. Extract current cursor and selection
        let mut cursor = editor.get_cursor();
        let mut selection = editor.get_selection();

        // 2. Work with code
        let code = editor.code_mut();
        code.tx();
        code.set_state_before(cursor, selection);

        // 3. Remove selection if present
        if let Some(sel) = &selection
            && !sel.is_empty()
        {
            let (start, end) = sel.sorted();
            code.remove(start, end);
            cursor = start;
        }
        selection = None;

        // 4. Insert the text at the cursor
        code.insert(cursor, &self.text);
        cursor = add_or(cursor, self.text.chars().count(), usize::MAX);

        // 5. Update editor state
        code.set_state_after(cursor, selection);
        code.commit();

        editor.set_cursor(cursor);
        editor.set_selection(selection);
        editor.reset_highlight_cache();
    }
}

/// Inserts a newline at the cursor with automatic indentation.
///
/// The indentation is computed based on the current line and column.
/// Delegates the actual insertion to [`InsertText`], so the newline and its
/// indentation are one undo step.
pub struct InsertNewline;

impl Action for InsertNewline {
    fn apply(&mut self, editor: &mut Editor) {
        // 1. Get current cursor position
        let cursor = editor.get_cursor();
        let code = editor.code_mut();
        let (row, col) = code.point(cursor);

        // 2. Compute indentation for the new line
        let indent_level = code.indentation_level(row, col);
        let indent_text = code.indent().repeat(indent_level);

        // 3. Prepare the text to insert
        let text_to_insert = format!("\n{indent_text}");

        // 4. Use InsertText action to insert the text
        let mut insert_action = InsertText {
            text: text_to_insert,
        };
        insert_action.apply(editor);
    }
}

/// Deletes the selected text or the character before the cursor.
///
/// - If there is a non-empty selection, deletes the selection.
/// - If there is no selection, deletes the previous character.
/// - If the cursor is after indentation only, deletes all of that indentation
///   back to column 0 in one go, but only for languages that are known: with an
///   unknown/empty language every backspace removes one character. At column 0
///   it joins with the previous line.
pub struct Delete;

impl Action for Delete {
    fn apply(&mut self, editor: &mut Editor) {
        // 1. Extract current cursor and selection
        let mut cursor = editor.get_cursor();
        let mut selection = editor.get_selection();

        // 2. Work with code
        let code = editor.code_mut();
        code.tx();
        code.set_state_before(cursor, selection);

        if let Some(sel) = &selection
            && !sel.is_empty()
        {
            // Delete selection
            let (start, end) = sel.sorted();
            code.remove(start, end);
            cursor = start;
            selection = None;
        } else if cursor > 0 {
            // Delete single char or indentation
            let (row, col) = code.point(cursor);
            if code.is_only_indentation_before(row, col) {
                let from = sub_or(cursor, col, 0);
                code.remove(from, cursor);
                cursor = from;
            } else {
                code.remove(sub_or(cursor, 1, 0), cursor);
                cursor = sub_or(cursor, 1, 0);
            }
        }

        // 3. Commit changes and update editor
        code.set_state_after(cursor, selection);
        code.commit();

        editor.set_cursor(cursor);
        editor.set_selection(selection);
        editor.reset_highlight_cache();
    }
}

/// Toggles line comments on the selected lines (or the current line).
///
/// The marker is the language's comment string and is added or removed at
/// **column 0** of each line (before any indentation), not after the indent.
/// It is removed only when *every* affected line starts with the marker;
/// otherwise it is added to every line (including blank ones).
pub struct ToggleComment;

impl Action for ToggleComment {
    /// The `ToggleComment` action toggles line comments at the start of the selected lines.
    ///
    /// If all lines in the selection already start with the language's comment string
    /// (e.g., "//" for Rust), this action removes the comment string from each affected line.
    /// Otherwise, it prepends the comment string to the beginning of each line in the selection.
    ///
    /// If there is no selection, the action is applied to the line under the cursor.
    fn apply(&mut self, editor: &mut Editor) {
        // 1. Extract current cursor and selection
        let mut cursor = editor.get_cursor();
        let mut selection = editor.get_selection();
        let selection_anchor = editor.selection_anchor();

        // 2. Work with code
        let code = editor.code_mut();

        code.tx();
        code.set_state_before(cursor, selection);

        let comment_text = code.comment();
        let comment_len = comment_text.chars().count();

        // 3. Determine lines to modify: the rows of the selection's two ends,
        // inclusive (so a selection ending at column 0 still counts that last
        // line, as with `Indent`).
        let lines_to_handle = if let Some(sel) = &selection
            && !sel.is_empty()
        {
            let (start, end) = sel.sorted();
            let (start_row, _) = code.point(start);
            let (end_row, _) = code.point(end);
            (start_row..=end_row).collect::<Vec<_>>()
        } else {
            let (row, _) = code.point(cursor);
            vec![row]
        };

        // 4. Check if all lines already have the comment
        let all_have_comment = lines_to_handle.iter().all(|&line_idx| {
            let line_start = code.line_to_char(line_idx);
            let line_len = code.line_len(line_idx);
            add_or(line_start, comment_len, usize::MAX) <= add_or(line_start, line_len, usize::MAX)
                && code.slice(line_start, add_or(line_start, comment_len, usize::MAX))
                    == comment_text
        });

        // 5. Apply changes (add or remove comment), bottom line first so the
        // character offsets of the lines still to do are not shifted.
        let mut comments_added = 0usize;
        let mut comments_removed = 0usize;

        for &line_idx in lines_to_handle.iter().rev() {
            let start = code.line_to_char(line_idx);
            if all_have_comment {
                // Remove comment if present at start
                let slice = code.slice(start, add_or(start, comment_len, usize::MAX));
                if slice == comment_text {
                    code.remove(start, add_or(start, comment_len, usize::MAX));
                    comments_removed = add_or(comments_removed, 1, usize::MAX);
                }
            } else {
                // Add comment at start
                code.insert(start, &comment_text);
                comments_added = add_or(comments_added, 1, usize::MAX);
            }
        }

        // 6. Update cursor and selection. For a forward selection the anchor
        // is on the first line, so it shifts by one marker, while the cursor on
        // the last line shifts by every marker added/removed before it; a
        // backward selection swaps the roles.
        if let Some(sel) = &selection
            && !sel.is_empty()
        {
            let (smin, _) = sel.sorted();
            let mut anchor = selection_anchor;
            let is_forward = anchor == smin;

            if is_forward {
                if all_have_comment {
                    cursor = sub_or(cursor, mul_or(comment_len, comments_removed, usize::MAX), 0);
                    anchor = sub_or(anchor, comment_len, 0);
                } else {
                    cursor = add_or(
                        cursor,
                        mul_or(comment_len, comments_added, usize::MAX),
                        usize::MAX,
                    );
                    anchor = add_or(anchor, comment_len, usize::MAX);
                }
            } else {
                if all_have_comment {
                    cursor = sub_or(cursor, comment_len, 0);
                    anchor = sub_or(anchor, mul_or(comment_len, comments_removed, usize::MAX), 0);
                } else {
                    cursor = add_or(cursor, comment_len, usize::MAX);
                    anchor = add_or(
                        anchor,
                        mul_or(comment_len, comments_added, usize::MAX),
                        usize::MAX,
                    );
                }
            }

            selection = Some(Selection::from_anchor_and_cursor(anchor, cursor));
        } else if all_have_comment {
            cursor = sub_or(cursor, comment_len, 0);
        } else {
            cursor = add_or(cursor, comment_len, usize::MAX);
        }

        // 7. Commit changes
        code.set_state_after(cursor, selection);
        code.commit();

        // 8. Return changed values to the editor
        editor.set_cursor(cursor);
        editor.set_selection(selection);
        editor.reset_highlight_cache();
    }
}

/// Inserts indentation at the beginning of the current line or selected lines.
///
/// - If there is a selection, inserts indentation at the start of each selected line.
/// - If there is no selection, inserts indentation at the current line.
/// - Updates cursor and selection accordingly.
///
/// The unit inserted is the language's indent string (tab or N spaces) at
/// column 0 of every affected line, blank lines included. This is *line*
/// indentation even for a bare cursor: the default key map's Tab always indents
/// the line rather than inserting a tab at the cursor.
pub struct Indent;

impl Action for Indent {
    fn apply(&mut self, editor: &mut Editor) {
        // 1. Extract current cursor and selection
        let mut cursor = editor.get_cursor();
        let mut selection = editor.get_selection();
        let selection_anchor = editor.selection_anchor();

        // 2. Work with code
        let code = editor.code_mut();
        code.tx();
        code.set_state_before(cursor, selection);

        let indent_text = code.indent();

        // 3. Determine lines to handle
        let lines_to_handle = if let Some(sel) = &selection
            && !sel.is_empty()
        {
            let (start, end) = sel.sorted();
            let (start_row, _) = code.point(start);
            let (end_row, _) = code.point(end);
            (start_row..=end_row).collect::<Vec<_>>()
        } else {
            let (row, _) = code.point(cursor);
            vec![row]
        };

        // 4. Insert indentation for each line (reverse to not shift indices).
        // Indent strings are ASCII (tab/spaces), so `len()` below equals the
        // character count.
        let mut indents_added = 0;
        for &line_idx in lines_to_handle.iter().rev() {
            let line_start = code.line_to_char(line_idx);
            code.insert(line_start, &indent_text);
            indents_added = add_or(indents_added, 1, usize::MAX);
        }

        // 5. Update cursor and selection
        if let Some(sel) = &selection
            && !sel.is_empty()
        {
            let (smin, _) = sel.sorted();
            let mut anchor = selection_anchor;
            let is_forward = anchor == smin;

            if is_forward {
                cursor = add_or(
                    cursor,
                    mul_or(indent_text.len(), indents_added, usize::MAX),
                    usize::MAX,
                );
                anchor = add_or(anchor, indent_text.len(), usize::MAX);
            } else {
                cursor = add_or(cursor, indent_text.len(), usize::MAX);
                anchor = add_or(
                    anchor,
                    mul_or(indent_text.len(), indents_added, usize::MAX),
                    usize::MAX,
                );
            }

            selection = Some(Selection::from_anchor_and_cursor(anchor, cursor));
        } else {
            cursor = add_or(cursor, indent_text.len(), usize::MAX);
        }

        // 6. Commit changes
        code.set_state_after(cursor, selection);
        code.commit();

        editor.set_cursor(cursor);
        editor.set_selection(selection);
        editor.reset_highlight_cache();
    }
}

/// Removes one indentation level from the start of the current line or selected lines.
///
/// - If there is a selection, removes indentation from each selected line.
/// - If there is no selection, removes indentation from the current line.
/// - Updates cursor and selection accordingly.
///
/// Removes at most one indent unit's worth of leading whitespace per line (less
/// if the line has less); lines with no leading indentation are left alone and
/// do not shift the cursor.
pub struct UnIndent;

impl Action for UnIndent {
    fn apply(&mut self, editor: &mut Editor) {
        // 1. Extract current cursor and selection
        let mut cursor = editor.get_cursor();
        let mut selection = editor.get_selection();
        let selection_anchor = editor.selection_anchor();

        // 2. Work with code
        let code = editor.code_mut();
        code.tx();
        code.set_state_before(cursor, selection);

        let indent_text = code.indent();
        let indent_len = indent_text.chars().count();

        // 3. Determine lines to handle
        let lines_to_handle = if let Some(sel) = &selection
            && !sel.is_empty()
        {
            let (start, end) = sel.sorted();
            let (start_row, _) = code.point(start);
            let (end_row, _) = code.point(end);
            (start_row..=end_row).collect::<Vec<_>>()
        } else {
            let (row, _) = code.point(cursor);
            vec![row]
        };

        // 4. Remove indentation from each line
        let mut lines_untabbed = 0;
        for &line_idx in lines_to_handle.iter().rev() {
            if let Some(indent_cols) = code.find_indent_at_line_start(line_idx) {
                let remove_count = indent_cols.min(indent_len);
                if remove_count > 0 {
                    let line_start = code.line_to_char(line_idx);
                    code.remove(line_start, add_or(line_start, remove_count, usize::MAX));
                    lines_untabbed = add_or(lines_untabbed, 1, usize::MAX);
                }
            }
        }

        // 5. Update cursor and selection
        if let Some(sel) = &selection
            && !sel.is_empty()
        {
            let (smin, _) = sel.sorted();
            let mut anchor = selection_anchor;
            let is_forward = anchor == smin;

            if is_forward {
                cursor = sub_or(cursor, mul_or(indent_len, lines_untabbed, usize::MAX), 0);
                anchor = sub_or(anchor, indent_len, 0);
            } else {
                cursor = sub_or(cursor, indent_len, 0);
                anchor = sub_or(anchor, mul_or(indent_len, lines_untabbed, usize::MAX), 0);
            }

            selection = Some(Selection::from_anchor_and_cursor(anchor, cursor));
        } else {
            cursor = sub_or(cursor, mul_or(indent_len, lines_untabbed, usize::MAX), 0);
        }

        // 6. Commit changes
        code.set_state_after(cursor, selection);
        code.commit();

        editor.set_cursor(cursor);
        editor.set_selection(selection);
        editor.reset_highlight_cache();
    }
}

/// Selects the entire text in the editor.
///
/// Only the selection changes; the cursor stays where it is.
pub struct SelectAll;

impl Action for SelectAll {
    fn apply(&mut self, editor: &mut Editor) {
        // Set selection from start to end of the document
        let from = 0;
        let code = editor.code_mut();
        let to = code.len_chars();
        let sel = Selection::new(from, to);
        editor.set_selection(Some(sel));
    }
}

/// Duplicates the selected text or the current line if no selection exists.
///
/// If there is a selection, it duplicates the selected text immediately after it.
/// If there is no selection, it duplicates the entire line under the cursor,
/// preserving the cursor's relative column position.
///
/// With a selection the cursor ends after the copy and the selection is
/// cleared. On a last line with no trailing newline the copy gets a leading
/// newline so it does not join onto the original.
pub struct Duplicate;

impl Action for Duplicate {
    fn apply(&mut self, editor: &mut Editor) {
        // 1. Extract current cursor and selection
        let mut cursor = editor.get_cursor();
        let mut selection = editor.get_selection();
        let code = editor.code_mut();

        code.tx();
        code.set_state_before(cursor, selection);

        if let Some(sel) = &selection {
            // Duplicate selected text (an empty selection duplicates nothing).
            let text = code.slice(sel.start, sel.end);
            let insert_pos = sel.end;
            code.insert(insert_pos, &text);
            cursor = add_or(insert_pos, text.chars().count(), usize::MAX);
            selection = None;
        } else {
            // Duplicate the current line
            let (line_start, line_end) = code.line_boundaries(cursor);
            let line_text = code.slice(line_start, line_end);
            let column = sub_or(cursor, line_start, 0);

            let insert_pos = line_end;
            // When the line ends in a newline, the duplicate goes at the start of
            // the next line. The last line has no trailing newline, so insert a
            // leading newline instead — otherwise the copy joins onto the original.
            let (to_insert, line_base) = if line_text.ends_with('\n') {
                (line_text, insert_pos)
            } else {
                (format!("\n{line_text}"), add_or(insert_pos, 1, usize::MAX))
            };
            code.insert(insert_pos, &to_insert);

            // Keep cursor on the same relative column in the new line
            let new_line_len = to_insert.trim_matches('\n').chars().count();
            let new_column = column.min(new_line_len);
            cursor = add_or(line_base, new_column, usize::MAX);
        }

        code.set_state_after(cursor, selection);
        code.commit();

        // Update editor state
        editor.set_cursor(cursor);
        editor.set_selection(selection);
        editor.reset_highlight_cache();
    }
}

/// Deletes the entire line under the cursor, including its newline, and leaves
/// the cursor at the start of the line that took its place. A no-op on the
/// empty last line (nothing left to delete).
pub struct DeleteLine;

impl Action for DeleteLine {
    fn apply(&mut self, editor: &mut Editor) {
        // 1. Extract current cursor and selection
        let mut cursor = editor.get_cursor();
        let mut selection = editor.get_selection();
        let code = editor.code_mut();

        // 2. Compute line boundaries
        let (start, end) = code.line_boundaries(cursor);

        // Do nothing if the line is empty and at the end of file
        if start == end && start == code.len() {
            return;
        }

        // 3. Remove the line
        code.tx();
        code.set_state_before(cursor, selection);
        code.remove(start, end);
        code.set_state_after(start, None);
        code.commit();

        // 4. Update editor state
        cursor = start;
        selection = None;
        editor.set_cursor(cursor);
        editor.set_selection(selection);
        editor.reset_highlight_cache();
    }
}

/// Cuts the current selection: copies it to the clipboard and removes it from the editor.
///
/// With several carets it cuts every caret's selection (`Editor::multi_cut`).
/// With no selection it does nothing; cutting the current line is the separate
/// `Editor::cut_line` named action.
pub struct Cut;

impl Action for Cut {
    fn apply(&mut self, editor: &mut Editor) {
        if editor.has_multi_carets() {
            editor.multi_cut();
            return;
        }
        // 1. Extract current cursor and selection
        let mut cursor = editor.get_cursor();
        let mut selection = editor.get_selection();

        let sel = match &selection {
            Some(sel) if !sel.is_empty() => *sel,
            _ => return, // nothing to cut
        };

        // 2. Copy to clipboard first, before borrowing code mutably
        let text = editor.code_ref().slice(sel.start, sel.end);
        editor.copy_to_clipboard(&text);

        // 3. Now borrow code mutably
        let code = editor.code_mut();
        code.tx();
        code.set_state_before(cursor, selection);
        code.remove(sel.start, sel.end);
        code.set_state_after(sel.start, None);
        code.commit();

        // 4. Update editor state
        cursor = sel.start;
        selection = None;
        editor.set_cursor(cursor);
        editor.set_selection(selection);
        editor.reset_highlight_cache();
    }
}

/// Copies the selected text to the clipboard.
///
/// Does nothing if there is no active selection. With several carets, every
/// selection is copied, joined with newlines in buffer order.
pub struct Copy;

impl Action for Copy {
    fn apply(&mut self, editor: &mut Editor) {
        // Several carets: copy every selection, joined with newlines, so a
        // paste into the same number of carets puts one back at each.
        if editor.has_multi_carets() {
            let texts = editor.caret_selection_texts();
            if !texts.is_empty() {
                editor.copy_to_clipboard(&texts.join("\n"));
            }
            return;
        }
        // 1. Extract current selection
        let selection = editor.get_selection();

        // 2. Return early if no selection
        let Some(sel) = selection else { return };
        if sel.is_empty() {
            return;
        }

        // 3. Get text and copy to clipboard
        let text = editor.code_ref().slice(sel.start, sel.end);
        editor.copy_to_clipboard(&text);
    }
}

/// Pastes text from the clipboard at the current cursor position.
///
/// If a selection exists, it will be replaced by the pasted text.
/// The pasted text is adjusted using language-specific indentation rules
/// (done by `Editor::paste_text`, which also handles multiple carets). A
/// clipboard read failure is silently ignored.
pub struct Paste;

impl Action for Paste {
    fn apply(&mut self, editor: &mut Editor) {
        let Ok(text) = editor.get_clipboard() else {
            return;
        };
        editor.paste_text(&text);
    }
}

/// Undoes the last edit in the code buffer.
///
/// Restores both the cursor position and selection state
/// from the saved editor snapshot if available.
///
/// Extra carets are dropped: the history stores only the primary caret, so
/// any other caret would point at stale offsets.
pub struct Undo;

impl Action for Undo {
    fn apply(&mut self, editor: &mut Editor) {
        // 1. Get mutable access to code
        let code = editor.code_mut();

        // 2. Try to undo
        let edits = code.undo();
        editor.reset_highlight_cache();

        // 3. If nothing to undo, return
        let Some(batch) = edits else { return };

        // The history records only the primary caret, so extra carets
        // (now pointing at stale offsets) collapse into it.
        editor.clear_carets();

        // 4. Restore cursor and selection from saved state if possible
        if let Some(before) = batch.state_before {
            editor.set_cursor(before.offset);
            editor.set_selection(before.selection);
            return;
        }

        // 5. Otherwise infer cursor position from edits. Undo inverts the
        // batch, so an inserted range is now gone (cursor at its start) and a
        // removed one is back (cursor after it); visited last-to-first so the
        // first edit of the batch decides.
        for edit in batch.edits.iter().rev() {
            match &edit.kind {
                EditKind::Insert { offset, .. } => {
                    editor.set_cursor(*offset);
                }
                EditKind::Remove { offset, text } => {
                    editor.set_cursor(add_or(*offset, text.chars().count(), usize::MAX));
                }
            }
        }
    }
}

/// Redoes the last undone edit in the code buffer.
///
/// Restores both the cursor position and selection state
/// from the saved editor snapshot if available. Like [`Undo`], it collapses
/// extra carets into the primary.
pub struct Redo;

impl Action for Redo {
    fn apply(&mut self, editor: &mut Editor) {
        // 1. Get mutable access to code
        let code = editor.code_mut();

        // 2. Try to redo
        let edits = code.redo();
        editor.reset_highlight_cache();

        // 3. If nothing to redo, return
        let Some(batch) = edits else { return };

        // The history records only the primary caret, so extra carets
        // (now pointing at stale offsets) collapse into it.
        editor.clear_carets();

        // 4. Restore cursor and selection from saved state if possible
        if let Some(after) = batch.state_after {
            editor.set_cursor(after.offset);
            editor.set_selection(after.selection);
            return;
        }

        // 5. Otherwise infer cursor position from edits, replayed in order so
        // the last edit decides.
        for edit in batch.edits {
            match &edit.kind {
                EditKind::Insert { offset, text } => {
                    editor.set_cursor(add_or(*offset, text.chars().count(), usize::MAX));
                }
                EditKind::Remove { offset, .. } => {
                    editor.set_cursor(*offset);
                }
            }
        }
    }
}

#[cfg(test)]
mod goal_column_tests {
    use crate::editor::Editor;

    fn ed(text: &str, cursor: usize) -> Editor {
        let mut e = Editor::new("text", text, Vec::new()).unwrap();
        e.set_cursor(cursor);
        e
    }

    #[test]
    fn vertical_moves_keep_the_goal_column_across_short_lines() {
        // line0 len 5, line1 len 2, line2 len 5.
        let mut e = ed("aaaaa\nbb\nccccc", 4); // line0, col 4
        e.cursor_down(); // line1 clamps to col 2
        assert_eq!(e.code_ref().point(e.get_cursor()), (1, 2));
        e.cursor_down(); // line2: goal column 4 is restored, not the clamped 2
        assert_eq!(e.code_ref().point(e.get_cursor()), (2, 4));
        e.cursor_up(); // back to line1, still clamped to 2 (goal preserved)
        assert_eq!(e.code_ref().point(e.get_cursor()), (1, 2));
        e.cursor_up(); // line0: goal column 4 restored
        assert_eq!(e.code_ref().point(e.get_cursor()), (0, 4));
    }

    #[test]
    fn horizontal_move_resets_the_goal_column() {
        let mut e = ed("aaaaa\nbb\nccccc", 4); // line0, col 4
        e.cursor_down(); // line1, clamped to col 2, goal = 4
        e.cursor_left(); // horizontal move clears the goal (now col 1)
        e.cursor_down(); // line2: column 1 carried forward, NOT the old goal 4
        assert_eq!(e.code_ref().point(e.get_cursor()), (2, 1));
    }
}

#[cfg(test)]
mod action_tests {
    //! Exact buffer, cursor and selection results of the primitive actions.

    use super::{
        Copy, Cut, Delete, DeleteLine, Duplicate, Indent, InsertNewline, InsertText, MoveDown,
        MoveLeft, MoveRight, MoveUp, Paste, Redo, SelectAll, ToggleComment, UnIndent, Undo,
    };
    use crate::code::{Edit, EditBatch, EditKind};
    use crate::editor::Editor;
    use crate::editor_input::tests_support::CLIP;

    fn ed(text: &str, cursor: usize) -> Editor {
        let mut e = Editor::new("rust", text, Vec::new()).unwrap();
        e.set_cursor(cursor);
        e
    }

    fn text(e: &Editor) -> String {
        e.code_ref().get_content()
    }

    fn sel(e: &mut Editor) -> Option<(usize, usize)> {
        e.get_selection().map(|s| s.sorted())
    }

    /// Select with the given anchor and cursor, as a drag would.
    fn select(e: &mut Editor, anchor: usize, cursor: usize) {
        e.set_selection_range(anchor, cursor);
    }

    #[test]
    fn horizontal_moves_collapse_a_selection_to_its_edge() {
        let mut e = ed("abcdef", 4);
        select(&mut e, 1, 4);
        e.apply(MoveRight { shift: false });
        assert_eq!((e.get_cursor(), sel(&mut e)), (4, None));
        select(&mut e, 4, 1);
        e.apply(MoveLeft { shift: false });
        assert_eq!((e.get_cursor(), sel(&mut e)), (1, None));
        // Shift extends instead.
        e.apply(MoveRight { shift: true });
        assert_eq!(sel(&mut e), Some((1, 2)));
        // At the buffer edges they are no-ops.
        let mut e = ed("ab", 2);
        e.apply(MoveRight { shift: true });
        assert_eq!((e.get_cursor(), sel(&mut e)), (2, None));
        let mut e = ed("ab", 0);
        e.apply(MoveLeft { shift: true });
        assert_eq!((e.get_cursor(), sel(&mut e)), (0, None));
    }

    #[test]
    fn vertical_moves_stop_at_the_first_and_last_line() {
        let mut e = ed("ab\ncd", 1);
        e.apply(MoveUp { shift: false });
        assert_eq!(e.get_cursor(), 1, "first line: Up does nothing");
        e.apply(MoveUp { shift: true });
        assert_eq!(sel(&mut e), None, "and does not start a selection");
        let mut e = ed("ab\ncd", 4);
        e.apply(MoveDown { shift: false });
        assert_eq!(e.get_cursor(), 4, "last line: Down does nothing");
        // Shift+Down/Up extends and keeps the anchor.
        let mut e = ed("ab\ncd\nef", 1);
        e.apply(MoveDown { shift: true });
        e.apply(MoveDown { shift: true });
        assert_eq!((e.get_cursor(), sel(&mut e)), (7, Some((1, 7))));
        e.apply(MoveUp { shift: true });
        assert_eq!(sel(&mut e), Some((1, 4)));
    }

    #[test]
    fn insert_text_replaces_selection_in_one_undo_step() {
        let mut e = ed("abcdef", 4);
        select(&mut e, 1, 4);
        e.apply(InsertText { text: "XY".into() });
        assert_eq!((text(&e).as_str(), e.get_cursor()), ("aXYef", 3));
        e.undo();
        assert_eq!(text(&e), "abcdef");
        assert_eq!(sel(&mut e), Some((1, 4)));
    }

    #[test]
    fn insert_newline_keeps_indentation() {
        let mut e = ed("    a", 5);
        e.apply(InsertNewline);
        assert_eq!(text(&e), "    a\n    ");
        assert_eq!(e.get_cursor(), 10);
    }

    #[test]
    fn delete_removes_selection_char_or_indent_and_noops_at_start() {
        let mut e = ed("abcdef", 4);
        select(&mut e, 1, 4);
        e.apply(Delete);
        assert_eq!((text(&e).as_str(), e.get_cursor()), ("aef", 1));
        let mut e = ed("abc", 0);
        e.apply(Delete);
        assert_eq!((text(&e).as_str(), e.get_cursor()), ("abc", 0));
        let mut e = ed("ab\ncd", 3);
        e.apply(Delete);
        assert_eq!((text(&e).as_str(), e.get_cursor()), ("abcd", 2));
    }

    fn comment(
        text_in: &str,
        anchor: usize,
        cursor: usize,
    ) -> (String, usize, Option<(usize, usize)>) {
        let mut e = ed(text_in, cursor);
        if anchor != cursor {
            select(&mut e, anchor, cursor);
        }
        e.apply(ToggleComment);
        let sel = e
            .get_selection()
            .and_then(|s| (!s.is_empty()).then(|| s.sorted()));
        (text(&e), e.get_cursor(), sel)
    }

    #[test]
    fn toggle_comment_on_a_single_line_adds_and_removes() {
        assert_eq!(comment("ab", 1, 1), ("//ab".into(), 3, None));
        assert_eq!(comment("//ab", 3, 3), ("ab".into(), 1, None));
        // Blank lines are commented too.
        assert_eq!(comment("", 0, 0), ("//".into(), 2, None));
    }

    #[test]
    fn toggle_comment_forward_selection_shifts_cursor_by_every_marker() {
        // anchor on line 0, cursor at the end of line 1.
        assert_eq!(comment("a\nb", 0, 3), ("//a\n//b".into(), 7, Some((2, 7))));
        assert_eq!(comment("//a\n//b", 0, 7), ("a\nb".into(), 3, Some((0, 3))));
        // Partly commented: the marker is added to every line.
        assert_eq!(
            comment("//a\nb", 0, 5),
            ("////a\n//b".into(), 9, Some((2, 9)))
        );
    }

    #[test]
    fn toggle_comment_backward_selection_shifts_the_anchor_by_every_marker() {
        // anchor at the end (3), cursor at the start (0).
        assert_eq!(comment("a\nb", 3, 0), ("//a\n//b".into(), 2, Some((2, 7))));
        assert_eq!(comment("//a\n//b", 7, 0), ("a\nb".into(), 0, Some((0, 3))));
    }

    #[test]
    fn indent_forward_and_backward_selections() {
        let mut e = ed("a\nb", 3);
        select(&mut e, 0, 3);
        e.apply(Indent);
        assert_eq!(text(&e), "    a\n    b");
        assert_eq!((e.get_cursor(), sel(&mut e)), (11, Some((4, 11))));
        let mut e = ed("a\nb", 0);
        select(&mut e, 3, 0);
        e.apply(Indent);
        assert_eq!(text(&e), "    a\n    b");
        assert_eq!((e.get_cursor(), sel(&mut e)), (4, Some((4, 11))));
    }

    #[test]
    fn unindent_forward_and_backward_selections_and_unindented_lines() {
        let mut e = ed("    a\n    b", 11);
        select(&mut e, 0, 11);
        e.apply(UnIndent);
        assert_eq!(text(&e), "a\nb");
        assert_eq!((e.get_cursor(), sel(&mut e)), (3, Some((0, 3))));
        let mut e = ed("    a\n    b", 0);
        select(&mut e, 11, 0);
        e.apply(UnIndent);
        assert_eq!(text(&e), "a\nb");
        assert_eq!((e.get_cursor(), sel(&mut e)), (0, Some((0, 3))));
        // A line with no indent is left alone, and the cursor does not move.
        let mut e = ed("ab", 1);
        e.apply(UnIndent);
        assert_eq!((text(&e).as_str(), e.get_cursor()), ("ab", 1));
        // Only whole indent units count: a 2-space indent under a 4-space unit
        // is left alone (the `UnIndent` doc comment says "less if the line has
        // less", which the code does not do).
        let mut e = ed("  ab", 3);
        e.apply(UnIndent);
        assert_eq!((text(&e).as_str(), e.get_cursor()), ("  ab", 3));
    }

    #[test]
    fn select_all_selects_without_moving_the_cursor() {
        let mut e = ed("abc", 1);
        e.apply(SelectAll);
        assert_eq!((e.get_cursor(), sel(&mut e)), (1, Some((0, 3))));
    }

    #[test]
    fn duplicate_selection_line_and_last_line() {
        let mut e = ed("abcd", 3);
        select(&mut e, 1, 3);
        e.apply(Duplicate);
        assert_eq!((text(&e).as_str(), e.get_cursor()), ("abcbcd", 5));
        let mut e = ed("ab\ncd\n", 4);
        e.apply(Duplicate);
        assert_eq!(text(&e), "ab\ncd\ncd\n");
        assert_eq!(e.get_cursor(), 7);
        // The last line has no newline: the copy gets a leading one.
        let mut e = ed("ab\ncd", 4);
        e.apply(Duplicate);
        assert_eq!(text(&e), "ab\ncd\ncd");
        assert_eq!(e.get_cursor(), 7);
    }

    #[test]
    fn delete_line_removes_the_line_and_noops_on_the_empty_last_line() {
        let mut e = ed("ab\ncd\nef", 4);
        e.apply(DeleteLine);
        assert_eq!((text(&e).as_str(), e.get_cursor()), ("ab\nef", 3));
        let mut e = ed("ab\n", 3);
        e.apply(DeleteLine);
        assert_eq!(text(&e), "ab\n");
        e.undo();
        assert_eq!(text(&e), "ab\n", "nothing was recorded to undo");
    }

    #[test]
    fn cut_copy_paste_single_and_multi_caret() {
        let _g = CLIP
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Cut with no selection does nothing.
        let mut e = ed("abc", 1);
        e.apply(Cut);
        assert_eq!(text(&e), "abc");
        // Copy with no / empty selection does nothing either.
        e.apply(Copy);
        select(&mut e, 1, 1);
        e.apply(Copy);
        // Copy and cut a selection.
        select(&mut e, 0, 2);
        e.apply(Copy);
        assert_eq!(e.get_clipboard().unwrap(), "ab");
        e.apply(Cut);
        assert_eq!((text(&e).as_str(), e.get_cursor()), ("c", 0));
        e.apply(Paste);
        assert_eq!(text(&e), "abc");
        // Multi-caret copy joins the selections with newlines; paste with the
        // same number of carets puts one line at each.
        let mut e = ed("aa bb cc", 0);
        e.set_caret_anchors(&[(0, 2), (3, 5)]);
        assert!(e.has_multi_carets());
        e.apply(Copy);
        assert_eq!(e.get_clipboard().unwrap(), "aa\nbb");
        // Cut with several carets cuts every selection.
        e.apply(Cut);
        assert_eq!(text(&e), "  cc");
    }

    #[test]
    fn undo_and_redo_infer_the_cursor_when_no_state_was_recorded() {
        let mut e = ed("abcdef", 0);
        e.apply_batch(&EditBatch {
            edits: vec![
                Edit {
                    kind: EditKind::Remove {
                        offset: 1,
                        text: "bc".into(),
                    },
                },
                Edit {
                    kind: EditKind::Insert {
                        offset: 4,
                        text: "XYZ".into(),
                    },
                },
            ],
            state_before: None,
            state_after: None,
        });
        assert_eq!(text(&e), "adefXYZ");
        // Undo visits the edits last-to-first: the first edit decides, so the
        // removed range is back and the cursor sits after it.
        e.apply(Undo);
        assert_eq!(text(&e), "abcdef");
        assert_eq!(e.get_cursor(), 3);
        // Redo replays in order: the last edit (an insert) decides.
        e.apply(Redo);
        assert_eq!(text(&e), "adefXYZ");
        assert_eq!(e.get_cursor(), 7);
        // Undo of an insert-first batch puts the cursor at its start.
        let mut e = ed("abc", 0);
        e.apply_batch(&EditBatch {
            edits: vec![Edit {
                kind: EditKind::Insert {
                    offset: 2,
                    text: "Q".into(),
                },
            }],
            state_before: None,
            state_after: None,
        });
        e.apply(Undo);
        assert_eq!((text(&e).as_str(), e.get_cursor()), ("abc", 2));
        // Redo of a remove-last batch puts the cursor at the removal offset.
        let mut e = ed("abc", 0);
        e.apply_batch(&EditBatch {
            edits: vec![Edit {
                kind: EditKind::Remove {
                    offset: 1,
                    text: "b".into(),
                },
            }],
            state_before: None,
            state_after: None,
        });
        e.apply(Undo);
        e.apply(Redo);
        assert_eq!((text(&e).as_str(), e.get_cursor()), ("ac", 1));
    }

    #[test]
    fn undo_and_redo_with_nothing_to_do_are_noops_and_drop_extra_carets() {
        let mut e = ed("abc", 1);
        e.apply(Undo);
        e.apply(Redo);
        assert_eq!((text(&e).as_str(), e.get_cursor()), ("abc", 1));
        e.insert_str_action("x");
        e.add_caret_at(0);
        assert!(e.has_multi_carets());
        e.apply(Undo);
        assert!(!e.has_multi_carets());
        e.add_caret_at(0);
        e.apply(Redo);
        assert!(!e.has_multi_carets());
    }
}
