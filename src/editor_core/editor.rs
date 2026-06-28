#![warn(clippy::pedantic)]
use std::time::Duration;
use crate::editor_core::click::{ClickKind, ClickTracker};
use crate::editor_core::code::Code;
use crate::editor_core::code::{EditKind, EditBatch};
use crate::editor_core::code::{RopeGraphemes, grapheme_width_and_chars_len, grapheme_width};
use crate::editor_core::selection::{Selection, SelectionSnap};
use crate::editor_core::actions::Action;
use crate::editor_core::utils;
use std::collections::HashMap;
use std::cell::RefCell;
use std::cmp::Ordering;
use anyhow::{Result, anyhow};
use ratatui_core::layout::Rect;
use ratatui_core::style::{Color, Modifier, Style};

/// Serializes all system-clipboard access. The platform clipboard backends
/// (notably macOS's Cocoa `NSPasteboard`) are not thread-safe, so concurrent
/// `arboard` calls from parallel test threads corrupt memory and crash the
/// process. Holding this lock around every clipboard operation makes access
/// sequential; in the single-threaded app it is uncontended.
static CLIPBOARD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

// keyword and ratatui style
type Theme = HashMap<String, Style>;
// start byte, end byte, style
type Hightlight = (usize, usize, Style);
// start offset, end offset
type HightlightCache = HashMap<(usize, usize), Vec<Hightlight>>;

/// Represents the text editor, which holds the code buffer, cursor, selection,
/// theme, scroll offsets, highlight cache, clipboard, and user mark intervals.
// Independent display/behavior flags (line numbers, whitespace, soft wrap,
// auto-pair); each is a distinct toggle, so grouping them would not help.
#[allow(clippy::struct_excessive_bools)]
pub struct Editor {
    /// Code buffer and editing/highlighting logic for the current language
    pub(crate) code: Code,
    /// Current cursor position as a character index in the document
    pub(crate) cursor: usize,

    /// Vertical scroll offset: index of the first visible line
    pub(crate) offset_y: usize,

    /// Horizontal scroll offset in characters (visual columns)
    pub(crate) offset_x: usize,

    /// Syntax theme: mapping of token name to ratatui Style
    pub(crate) theme: Theme,

    /// Current text selection, if any
    pub(crate) selection: Option<Selection>,

    /// Click tracker to detect single/double/triple clicks
    pub(crate) clicks: ClickTracker,

    /// Selection snapping mode (to word, to line, or none)
    pub(crate) selection_snap: SelectionSnap,

    /// Fallback clipboard storage when the system clipboard is unavailable
    pub(crate) clipboard: Option<String>,

    /// User marks for intervals: (start, end, color)
    pub(crate) marks: Option<Vec<(usize, usize, Color)>>,

    /// Spell-check marks: char ranges of misspelled words, drawn as a red
    /// underline on a separate channel from `marks` (which the host uses for
    /// search hits).
    pub(crate) spell_marks: Option<Vec<(usize, usize)>>,

    /// LSP diagnostic marks: `(start char, end char, color)`, drawn as a colored
    /// underline. A separate channel from `spell_marks` so the two coexist.
    pub(crate) diagnostic_marks: Option<Vec<(usize, usize, Color)>>,

    /// Git diff gutter marks: `(line index, color)`, drawn as a colored bar in
    /// the line-number gutter.
    pub(crate) gutter_marks: Option<Vec<(usize, Color)>>,

    /// Foldable line ranges (from the language server): `(start_line, end_line)`,
    /// both 0-based. A range can be folded by its start line.
    pub(crate) fold_ranges: Vec<(usize, usize)>,

    /// Currently-folded ranges: `(start_line, end_line)`. The start line stays
    /// visible (with a marker); lines `start+1..=end` are hidden.
    pub(crate) folds: Vec<(usize, usize)>,

    /// LSP inlay hints: `(line, char column within line, label)`, 0-based. Drawn
    /// inline (dimmed), shifting the real glyphs at/after the column to the right.
    pub(crate) inlay_hints: Vec<(usize, usize, String)>,

    /// Syntax highlight cache by intervals to speed up rendering
    pub(crate) highlights_cache: RefCell<HightlightCache>,

    /// Controls when to show the line numbers
    pub(crate) show_line_numbers: bool,

    /// When true, render visible glyphs for whitespace (space, tab, line ending).
    pub(crate) show_whitespace: bool,

    /// When true, long logical lines wrap across several screen rows instead of
    /// scrolling horizontally.
    pub(crate) soft_wrap: bool,

    /// When true, typing an opening bracket/quote auto-inserts its closer (and
    /// Backspace between an empty pair deletes both). Host-configurable.
    pub(crate) auto_pair: bool,

    /// Optional end-of-line virtual note `(line, text)` drawn dimmed after that
    /// line's content (e.g. inline git blame for the cursor line). Non-wrapped
    /// view only.
    pub(crate) eol_note: Option<(usize, String)>,

    /// Debugger breakpoint lines (0-based), drawn as a marker in the gutter.
    pub(crate) breakpoints: Vec<usize>,

    /// The debugger's currently-stopped line (0-based), drawn as a gutter arrow.
    pub(crate) debug_line: Option<usize>,

    /// Style for the visible-whitespace glyphs (typically dimmed).
    pub(crate) whitespace_style: Style,

    /// Style for the bracket matching the one at the cursor.
    pub(crate) bracket_style: Style,

    /// Controls the left padding before writing the code
    pub(crate) left_code_padding: usize,

    /// Style for ordinary (untokenized) text. Configurable so the host can
    /// match its own theme (e.g. white-on-dark vs. black-on-light).
    pub(crate) text_style: Style,

    /// Style for the line-number gutter.
    pub(crate) line_number_style: Style,

    /// Style applied to the active selection. Defaults to reversed video, which
    /// reads correctly on any background.
    pub(crate) selection_style: Style,

    /// Style for the one-cell block cursor at the caret. `None` draws no cursor
    /// (the original behavior); `Some` draws a visible block cursor so the host
    /// can theme it (e.g. a custom cursor color).
    pub(crate) cursor_style: Option<Style>,

    /// Extra cursors beyond the primary one (multiple-cursor editing). Empty for
    /// the normal single-cursor case. See `multicursor.rs`.
    pub(crate) carets: Vec<crate::editor_core::multicursor::Caret>,

    /// "Goal column" for vertical movement: the char column the caret tries to
    /// keep as it moves up/down across lines of differing length. `None` until a
    /// vertical move establishes it; cleared by any other cursor move (every
    /// non-vertical motion routes through [`Editor::set_cursor`]). This stops the
    /// classic drift where moving through a short line snaps the column inward.
    pub(crate) goal_col: Option<usize>,
}

impl Editor {
    /// Create an editor for the given language, text, and syntax theme.
    ///
    /// # Errors
    /// Returns an error when neither the requested language grammar nor the
    /// `text` fallback grammar can be loaded.
    pub fn new(lang: &str, text: &str, theme: Vec<(&str, &str)>) -> Result<Self> {
        Self::new_with_highlights(lang, text, theme, None)
    }

    /// Create an editor with optional custom highlight overrides.
    ///
    /// # Errors
    /// Returns an error when neither the requested language grammar nor the
    /// `text` fallback grammar can be loaded.
    pub fn new_with_highlights(
        lang: &str,
        text: &str,
        theme: Vec<(&str, &str)>,
        custom_highlights: Option<HashMap<String, String>>,
    ) -> Result<Self> {
        let code = Code::new(text, lang, custom_highlights.clone())
            .or_else(|_| Code::new(text, "text", custom_highlights))?;

        let theme = Self::build_theme(theme);
        let highlights_cache = RefCell::new(HashMap::new());

        Ok(Self {
            code,
            cursor: 0,
            offset_y: 0,
            offset_x: 0,
            theme,
            selection: None,
            clicks: ClickTracker::new(Duration::from_millis(700)),
            selection_snap: SelectionSnap::None,
            clipboard: None,
            marks: None,
            spell_marks: None,
            diagnostic_marks: None,
            gutter_marks: None,
            fold_ranges: Vec::new(),
            folds: Vec::new(),
            inlay_hints: Vec::new(),
            highlights_cache,
            show_line_numbers: true,
            show_whitespace: false,
            soft_wrap: false,
            auto_pair: true,
            eol_note: None,
            breakpoints: Vec::new(),
            debug_line: None,
            whitespace_style: Style::default().fg(Color::DarkGray),
            bracket_style: Style::default().add_modifier(Modifier::REVERSED),
            left_code_padding: 2,
            text_style: Style::default().fg(Color::White),
            line_number_style: Style::default().fg(Color::DarkGray),
            selection_style: Style::default().add_modifier(Modifier::REVERSED),
            cursor_style: None,
            carets: Vec::new(),
            goal_col: None,
        })
    }

    /// Set the style for ordinary text (the foreground of unhighlighted code).
    pub fn set_text_style(&mut self, style: Style) {
        self.text_style = style;
    }

    /// Set the style for the line-number gutter.
    pub fn set_line_number_style(&mut self, style: Style) {
        self.line_number_style = style;
    }

    /// Set the block-cursor style. Pass `Some(style)` to draw a visible cursor
    /// cell (its `bg` is the cursor color), or `None` to draw no cursor.
    pub fn set_cursor_style(&mut self, style: Option<Style>) {
        self.cursor_style = style;
    }

    /// Replace the syntax-highlight theme (token name -> `#rrggbb`) and drop the
    /// cached highlights so the new colors take effect on the next render.
    pub fn set_syntax_theme(&mut self, theme: &[(&str, &str)]) {
        self.theme = theme
            .iter()
            .map(|(name, hex)| {
                let (r, g, b) = utils::rgb(hex);
                ((*name).to_string(), Style::default().fg(Color::Rgb(r, g, b)))
            })
            .collect();
        self.highlights_cache.borrow_mut().clear();
    }

    /// Set the style applied to the active selection.
    pub fn set_selection_style(&mut self, style: Style) {
        self.selection_style = style;
    }

    pub(crate) fn get_line_number_width(&self) -> usize {
        if self.show_line_numbers {
            let total_lines = self.code.len_lines();
            let max_line_number = total_lines.max(1);
            let line_number_digits = max_line_number.to_string().len().max(5);
            line_number_digits + self.left_code_padding 
        } else {
            self.left_code_padding
        }
    } 

    /// Scroll the viewport so the cursor is visible within `area`.
    pub fn focus(&mut self, area: &Rect) {
        let width = area.width as usize;
        let height = area.height as usize;
        let line_number_width = self.get_line_number_width();

        let line = self.code.char_to_line(self.cursor);
        let col = self.cursor - self.code.line_to_char(line);

        if self.soft_wrap {
            // No horizontal scroll; scroll vertically by logical line until the
            // cursor's visual row is within the viewport.
            self.offset_x = 0;
            if line < self.offset_y {
                self.offset_y = line;
            }
            let text_width = width.saturating_sub(line_number_width);
            while self.offset_y < line {
                let rows = self.visual_rows(text_width, height.max(1));
                let visible = rows
                    .iter()
                    .any(|r| self.cursor >= r.start && self.cursor <= r.end);
                if visible {
                    break;
                }
                self.offset_y += 1;
            }
            return;
        }

        let visible_width = width.saturating_sub(line_number_width);
        let visible_height = height;

        let step_size = 10;
        if col < self.offset_x {
            self.offset_x = col.saturating_sub(step_size);
        } else if col >= self.offset_x + visible_width {
            self.offset_x = col.saturating_sub(visible_width - step_size);
        }
    
        if line < self.offset_y {
            self.offset_y = line;
        } else if line >= self.offset_y + visible_height {
            self.offset_y = line.saturating_sub(visible_height - 1);
        }
    }

    /// Handles a mouse button press at the given cursor position, updating selection and click state.
    pub fn handle_mouse_down(&mut self, cursor: usize) {
        let kind = self.clicks.register(cursor);
        let (start, end, snap) = match kind {
            ClickKind::Triple => {
                let (line_start, line_end) = self.code.line_boundaries(cursor);
                (line_start, line_end, SelectionSnap::Line { anchor: cursor })
            }
            ClickKind::Double => {
                let (word_start, word_end) = self.code.word_boundaries(cursor);
                (word_start, word_end, SelectionSnap::Word { anchor: cursor })
            }
            ClickKind::Single => (cursor, cursor, SelectionSnap::None),
        };

        self.selection = Some(Selection::from_anchor_and_cursor(start, end));
        self.cursor = end;
        self.selection_snap = snap;
    }

    /// Handles a mouse drag event at the given cursor position, extending the selection.
    pub fn handle_mouse_drag(&mut self, cursor: usize) {
        match self.selection_snap {
            SelectionSnap::Line { anchor } => {
                let (anchor_start, anchor_end) = self.code.line_boundaries(anchor);
                let (cur_start, cur_end) = self.code.line_boundaries(cursor);
        
                let (sel_start, sel_end, new_cursor) = match cursor.cmp(&anchor) {
                    Ordering::Greater => (anchor_start, cur_end, cur_end),   // forward
                    Ordering::Less => (cur_start, anchor_end, cur_start),    // backward
                    Ordering::Equal => (anchor_start, anchor_end, anchor_end), 
                };
        
                self.selection = Some(Selection::from_anchor_and_cursor(sel_start, sel_end));
                self.cursor = new_cursor;
            }
            SelectionSnap::Word { anchor } => {
                let (anchor_start, anchor_end) = self.code.word_boundaries(anchor);
                let (cur_start, cur_end) = self.code.word_boundaries(cursor);
        
                let (sel_start, sel_end, new_cursor) = match cursor.cmp(&anchor) {
                    Ordering::Greater => (anchor_start, cur_end, cur_end),   // forward
                    Ordering::Less => (cur_start, anchor_end, cur_start),    // backward
                    Ordering::Equal => (anchor_start, anchor_end, anchor_end),
                };
        
                self.selection = Some(Selection::from_anchor_and_cursor(sel_start, sel_end));
                self.cursor = new_cursor;
            }
            SelectionSnap::None => {
                let anchor = self.selection_anchor();
                self.selection = Some(Selection::from_anchor_and_cursor(anchor, cursor));
                self.cursor = cursor;
            }
        }
    }

    /// Converts mouse coordinates to a cursor position within the editor area, returning `None` if outside.
    pub fn cursor_from_mouse(
        &self, mouse_x: u16, mouse_y: u16, area: &Rect
    ) -> Option<usize> {
        let line_number_width = u16::try_from(self.get_line_number_width()).unwrap_or(u16::MAX);
    
        if mouse_y < area.top()
            || mouse_y >= area.bottom()
            || mouse_x < area.left() + line_number_width
        {
            return None;
        }

        if self.soft_wrap {
            let screen_row = (mouse_y - area.top()) as usize;
            let text_width = (area.width as usize).saturating_sub(line_number_width as usize);
            let rows = self.visual_rows(text_width, screen_row + 1);
            let vr = rows.get(screen_row)?;
            let clicked_col = (mouse_x - area.left() - line_number_width) as usize;
            let slice = self.code.char_slice(vr.start, vr.end);
            let mut cur_col = 0usize;
            let mut ch = vr.start;
            for g in RopeGraphemes::new(&slice) {
                let (gw0, gc) = grapheme_width_and_chars_len(g);
                let gw = if g.chars().next() == Some('\t') { 1 } else { gw0 };
                if cur_col + gw > clicked_col { break; }
                cur_col += gw;
                ch += gc;
            }
            return Some(ch);
        }

        let clicked_row = (mouse_y - area.top()) as usize + self.offset_y;
        if clicked_row >= self.code.len_lines() {
            return None;
        }
    
        let clicked_col = (mouse_x - area.left() - line_number_width) as usize;
    
        let line_start_char = self.code.line_to_char(clicked_row);
        let line_len = self.code.line_len(clicked_row);
    
        let start_col = self.offset_x.min(line_len);
        let end_col = line_len;
    
        let char_start = line_start_char + start_col;
        let char_end = line_start_char + end_col;
    
        let mut current_col = 0;
        let mut char_idx = start_col;        
        let visible_chars = self.code.char_slice(char_start, char_end);
        for g in RopeGraphemes::new(&visible_chars) {
            let (g_width, g_chars) = grapheme_width_and_chars_len(g);        
            if current_col + g_width > clicked_col { break; }
            current_col += g_width;
            char_idx += g_chars;
        }
    
        let line = self.code.char_slice(line_start_char, line_start_char + line_len);
        let visual_width: usize = RopeGraphemes::new(&line).map(grapheme_width).sum();
    
        if clicked_col + self.offset_x >= visual_width {
            let mut end_idx = line.len_chars();
            if end_idx > 0 && line.char(end_idx - 1) == '\n' {
                end_idx -= 1;
            }
            char_idx = end_idx;
        }
    
        Some(line_start_char + char_idx)
    }

    /// Clears any active selection.
    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Extends or starts a selection from the current cursor to `new_cursor`.
    pub fn extend_selection(&mut self, new_cursor: usize) {
        // If there was already a selection, preserve the anchor (start point)
        // otherwise, use the current cursor as the anchor.
        let anchor = self.selection_anchor();
        self.selection = Some(Selection::from_anchor_and_cursor(anchor, new_cursor));
    }
    
    /// Returns the selection anchor position, or the cursor if no selection exists.
    pub fn selection_anchor(&self) -> usize {
        self.selection
            .as_ref()
            .map_or(self.cursor, |s| if self.cursor == s.start { s.end } else { s.start })
    }

    /// Apply an editing action to the buffer.
    pub fn apply<A: Action>(&mut self, mut action: A) {
        action.apply(self);
    }

    /// Replace the entire buffer contents with `content` as a single undo step.
    pub fn set_content(&mut self, content: &str) {
        self.code.tx();
        self.code.set_state_before(self.cursor, self.selection);
        self.code.remove(0, self.code.len());
        self.code.insert(0, content);
        self.code.set_state_after(self.cursor, self.selection);
        self.code.commit();
        self.reset_highlight_cache();
    }

    /// Apply a batch of edits as a single undo step.
    pub fn apply_batch(&mut self, batch: &EditBatch) {
        self.code.tx();

        if let Some(state) = &batch.state_before {
            self.code.set_state_before(state.offset, state.selection);
        }
        if let Some(state) = &batch.state_after {
            self.code.set_state_after(state.offset, state.selection);
        }
        
        for edit in &batch.edits {
            match &edit.kind {
                EditKind::Insert { offset, text } => {
                    self.code.insert(*offset, text);
                }
                EditKind::Remove { offset, text } => {
                    self.code.remove(*offset, *offset + text.chars().count());
                }
            }
        }
        self.code.commit();
        self.reset_highlight_cache();
    }

    /// Move the cursor to the given char offset, clamped to the buffer. Clears the
    /// vertical-movement goal column; line up/down re-establish it afterward.
    pub fn set_cursor(&mut self, cursor: usize) {
        self.goal_col = None;
        self.cursor = cursor;
        self.fit_cursor();
    }

    /// Select the range `[anchor, cursor)`, putting the caret at `cursor`. An
    /// empty range (`anchor == cursor`) just clears the selection.
    pub fn set_selection_range(&mut self, anchor: usize, cursor: usize) {
        if anchor == cursor {
            self.selection = None;
        } else {
            self.selection = Some(Selection::from_anchor_and_cursor(anchor, cursor));
        }
        self.selection_snap = SelectionSnap::None;
        self.goal_col = None;
        self.cursor = cursor;
        self.fit_cursor();
    }

    /// Clamp the cursor so it stays within the buffer and its current line.
    pub fn fit_cursor(&mut self) {
        // make sure cursor is not out of bounds
        let len = self.code.len_chars();
        self.cursor = self.cursor.min(len);

        // make sure cursor is not out of bounds on the line
        let (row, col) = self.code.point(self.cursor);
        if col > self.code.line_len(row) {
            self.cursor = self.code.line_to_char(row) + self.code.line_len(row);
        }
    }

    /// Scroll the viewport up one line.
    pub fn scroll_up(&mut self) {
        if self.offset_y > 0 {
            self.offset_y -= 1;
        }
    }

    /// Scroll the viewport down one line, bounded by the last line.
    pub fn scroll_down(&mut self, area_height: usize) {
        let len_lines = self.code.len_lines();
        if self.offset_y < len_lines.saturating_sub(area_height) {
            self.offset_y += 1;
        }
    }

    fn build_theme(theme: Vec<(&str, &str)>) -> Theme {
        theme.into_iter()
            .map(|(name, hex)| {
                let (r, g, b) = utils::rgb(hex);
                (name.to_string(), Style::default().fg(Color::Rgb(r, g, b)))
            })
            .collect()
    }

    /// Return the entire buffer contents as an owned string.
    pub fn get_content(&self) -> String {
        self.code.get_content()
    }

    /// Return the text of the char range `[start, end)` as an owned string.
    pub fn get_content_slice(&self, start: usize, end: usize) -> String {
        self.code.slice(start, end)
    }

    /// Return the cursor's char offset.
    pub fn get_cursor(&self) -> usize {
        self.cursor
    }

    /// The 0-based line (row) the cursor is on.
    #[must_use]
    pub fn cursor_line(&self) -> usize {
        self.code.char_to_line(self.cursor)
    }

    /// Move the cursor to the start of 0-based `line` (clamped to the buffer) and
    /// clear any selection.
    pub fn set_cursor_line(&mut self, line: usize) {
        let last = self.code.len_lines().saturating_sub(1);
        self.cursor = self.code.line_to_char(line.min(last));
        self.selection = None;
    }

    /// Set the clipboard text, falling back to an in-memory clipboard when the
    /// system clipboard is unavailable.
    ///
    /// # Errors
    /// Currently always returns `Ok`; the system-clipboard failure path falls
    /// back to the in-memory clipboard.
    pub fn set_clipboard(&mut self, text: &str) -> Result<()> {
        let _guard = CLIPBOARD_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        arboard::Clipboard::new()
            .and_then(|mut c| c.set_text(text.to_string()))
            .unwrap_or_else(|_| self.clipboard = Some(text.to_string()));
        Ok(())
    }

    /// Return the clipboard text, preferring the system clipboard and falling
    /// back to the in-memory clipboard.
    ///
    /// # Errors
    /// Returns an error when neither the system nor in-memory clipboard has text.
    pub fn get_clipboard(&self) -> Result<String> {
        let _guard = CLIPBOARD_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        arboard::Clipboard::new()
            .and_then(|mut c| c.get_text())
            .ok()
            .or_else(|| self.clipboard.clone())
            .ok_or_else(|| anyhow!("cant get clipboard"))
    }

    /// Set the highlight marks as `(start char, end char, hex color)` ranges.
    pub fn set_marks(&mut self, marks: Vec<(usize, usize, &str)>) {
        self.marks = Some(
            marks.into_iter()
                .map(|(start, end, color)| {
                    let (r, g, b) = utils::rgb(color);
                    (start, end, Color::Rgb(r, g, b))
                })
                .collect()
        );
    }

    /// Clear all highlight marks.
    pub fn remove_marks(&mut self) {
        self.marks = None;
    }

    /// Record the foldable line ranges reported by the language server. Drops any
    /// active fold whose range is no longer offered.
    pub fn set_fold_ranges(&mut self, ranges: Vec<(usize, usize)>) {
        self.folds.retain(|f| ranges.iter().any(|r| r.0 == f.0 && r.1 == f.1));
        self.fold_ranges = ranges;
    }

    /// Toggle the fold whose start is `line`: unfold it if folded, else fold it
    /// if a foldable range starts there. Returns `true` if anything changed.
    pub fn toggle_fold(&mut self, line: usize) -> bool {
        if let Some(i) = self.folds.iter().position(|f| f.0 == line) {
            self.folds.remove(i);
            return true;
        }
        if let Some(&range) = self.fold_ranges.iter().find(|r| r.0 == line && r.1 > r.0) {
            self.folds.push(range);
            // Keep the caret out of the now-hidden region.
            let cursor_line = self.code_ref().char_to_line(self.cursor);
            if self.is_line_hidden(cursor_line) {
                let start = self.code_ref().line_to_char(range.0);
                self.set_cursor(start);
            }
            return true;
        }
        false
    }

    /// Whether `line` is hidden inside a currently-folded range
    /// (`start < line <= end`).
    #[must_use]
    pub fn is_line_hidden(&self, line: usize) -> bool {
        self.folds.iter().any(|&(s, e)| s < line && line <= e)
    }

    /// Whether any fold is currently active.
    #[must_use]
    pub fn has_folds(&self) -> bool {
        !self.folds.is_empty()
    }

    /// Fold every foldable range.
    pub fn fold_all(&mut self) {
        self.folds = self.fold_ranges.iter().copied().filter(|r| r.1 > r.0).collect();
    }

    /// Unfold every active fold.
    pub fn unfold_all(&mut self) {
        self.folds.clear();
    }

    /// Set the inline hints to display: `(line, char column within line, label)`.
    pub fn set_inlay_hints(&mut self, hints: Vec<(usize, usize, String)>) {
        self.inlay_hints = hints;
    }

    /// Whether any inline hints are set.
    #[must_use]
    pub fn has_inlay_hints(&self) -> bool {
        !self.inlay_hints.is_empty()
    }

    /// The fold marker for `line`: `Some(true)` if a folded range starts here,
    /// `Some(false)` if a foldable (but open) range starts here, else `None`.
    #[must_use]
    pub fn fold_marker(&self, line: usize) -> Option<bool> {
        if self.folds.iter().any(|f| f.0 == line) {
            Some(true)
        } else if self.fold_ranges.iter().any(|r| r.0 == line && r.1 > r.0) {
            Some(false)
        } else {
            None
        }
    }

    /// Set the spell-check underline marks (char ranges of misspelled words).
    pub fn set_spell_marks(&mut self, marks: Vec<(usize, usize)>) {
        self.spell_marks = if marks.is_empty() { None } else { Some(marks) };
    }

    /// Clear all spell-check underline marks.
    pub fn clear_spell_marks(&mut self) {
        self.spell_marks = None;
    }

    /// The current spell-check underline marks (char ranges), if any.
    #[must_use]
    pub fn spell_marks(&self) -> Option<&Vec<(usize, usize)>> {
        self.spell_marks.as_ref()
    }

    /// Set the LSP diagnostic underline marks: `(start char, end char, color)`.
    pub fn set_diagnostic_marks(&mut self, marks: Vec<(usize, usize, Color)>) {
        self.diagnostic_marks = if marks.is_empty() { None } else { Some(marks) };
    }

    /// Clear all LSP diagnostic underline marks.
    pub fn clear_diagnostic_marks(&mut self) {
        self.diagnostic_marks = None;
    }

    /// The current LSP diagnostic underline marks, if any.
    #[must_use]
    pub fn diagnostic_marks(&self) -> Option<&Vec<(usize, usize, Color)>> {
        self.diagnostic_marks.as_ref()
    }

    /// Set the git diff gutter marks: `(line index, hex color)` per changed line.
    pub fn set_gutter_marks(&mut self, marks: Vec<(usize, &str)>) {
        self.gutter_marks = if marks.is_empty() {
            None
        } else {
            Some(
                marks
                    .into_iter()
                    .map(|(line, color)| {
                        let (r, g, b) = utils::rgb(color);
                        (line, Color::Rgb(r, g, b))
                    })
                    .collect(),
            )
        };
    }

    /// Clear all git diff gutter marks.
    pub fn clear_gutter_marks(&mut self) {
        self.gutter_marks = None;
    }

    /// The current git diff gutter marks (`line index`, color), if any.
    #[must_use]
    pub fn gutter_marks(&self) -> Option<&Vec<(usize, Color)>> {
        self.gutter_marks.as_ref()
    }

    /// Char ranges of comment and string tokens in the buffer, for the host's
    /// spell checker to scan. Empty when the language has no Tree-sitter query.
    #[must_use]
    pub fn comment_string_ranges(&self) -> Vec<(usize, usize)> {
        self.code.comment_string_ranges()
    }

    /// The text of the char range `[start, end)` as an owned string.
    #[must_use]
    pub fn char_text(&self, start: usize, end: usize) -> String {
        self.code.slice(start, end)
    }

    /// The word (and its char range) at `pos`, using the buffer's word
    /// boundaries; `None` when `pos` is not inside a word.
    #[must_use]
    pub fn word_at(&self, pos: usize) -> Option<(usize, usize, String)> {
        let (start, end) = self.code.word_boundaries(pos);
        if start >= end {
            return None;
        }
        Some((start, end, self.code.slice(start, end)))
    }

    /// Whether any highlight marks are set.
    pub fn has_marks(&self) -> bool {
        self.marks.is_some()
    }

    /// Return the highlight marks as `(start, end, color)` ranges, if any.
    pub fn get_marks(&self) -> Option<&Vec<(usize, usize, Color)>> {
        self.marks.as_ref()
    }

    /// Return the text of the current selection, or `None` when empty.
    pub fn get_selection_text(&mut self) -> Option<String> {
        if let Some(selection) = &self.selection && !selection.is_empty() {
            let text = self.code.slice(selection.start, selection.end);
            return Some(text);
        }
        None
    }

    /// Return the current selection, if any.
    pub fn get_selection(&mut self) -> Option<Selection> {
       self.selection
    }

    /// Set (or clear) the current selection.
    pub fn set_selection(&mut self, selection: Option<Selection>) {
        self.selection = selection;
    }

    /// Set the vertical scroll offset (first visible line).
    pub fn set_offset_y(&mut self, offset_y: usize) {
        self.offset_y = offset_y;
    }

    /// Set the horizontal scroll offset (first visible column).
    pub fn set_offset_x(&mut self, offset_x: usize) {
        self.offset_x = offset_x;
    }

    /// Return the vertical scroll offset (first visible line).
    pub fn get_offset_y(&self) -> usize {
        self.offset_y
    }

    /// Return the horizontal scroll offset (first visible column).
    pub fn get_offset_x(&self) -> usize {
        self.offset_x
    }

    /// Whether soft wrap is on (no horizontal scrolling when it is).
    #[must_use]
    pub fn soft_wrap_enabled(&self) -> bool {
        self.soft_wrap
    }

    /// Width (columns) of the line-number gutter, so the host can compute the
    /// visible text width for a horizontal scrollbar.
    #[must_use]
    pub fn gutter_width(&self) -> usize {
        self.get_line_number_width()
    }

    /// The longest line's width in characters (used to size a horizontal
    /// scrollbar). A tab counts as one character.
    #[must_use]
    pub fn max_line_width(&self) -> usize {
        self.get_content().lines().map(|l| l.chars().count()).max().unwrap_or(0)
    }

    /// Return a mutable reference to the underlying code buffer.
    pub fn code_mut(&mut self) -> &mut Code {
        &mut self.code
    }

    /// Return a shared reference to the underlying code buffer.
    pub fn code_ref(&self) -> &Code {
        &self.code
    }

    /// A counter that increases on every content edit, for cheap change detection
    /// (e.g. deciding when to push a `didChange` to a language server).
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.code.revision()
    }

    /// The buffer's language identifier (e.g. `"rust"`, `"text"`).
    pub fn language(&self) -> &str {
        self.code.lang()
    }

    /// The buffer's line ending: `"LF"` or `"CRLF"`.
    pub fn line_ending(&self) -> &'static str {
        self.code.first_line_ending()
    }

    /// The current selection as a sorted `(start, end)` char range, or `None`
    /// when there is no (non-empty) selection.
    pub fn selection_span(&self) -> Option<(usize, usize)> {
        self.selection
            .as_ref()
            .filter(|s| !s.is_empty())
            .map(|s| (s.start.min(s.end), s.start.max(s.end)))
    }

    /// Set the change callback function for handling document changes
    pub fn set_change_callback(
        &mut self, callback: Box<dyn Fn(Vec<crate::editor_core::code::ChangeEvent>)>
    ) {
        self.code.set_change_callback(callback);
    }

    /// Return cached syntax highlights for the char range `[start, end)`.
    pub fn highlight_interval(
        &self, start: usize, end: usize, theme: &Theme
    ) -> Vec<(usize, usize, Style)> {
        let mut cache = self.highlights_cache.borrow_mut();
        let key = (start, end);
        if let Some(v) = cache.get(&key) {
            return v.clone();
        }

        let highlights = self.code.highlight_interval(start, end, theme);
        cache.insert(key, highlights.clone());
        highlights
    }

    /// Clear the cached syntax highlights so they are recomputed on next render.
    pub fn reset_highlight_cache(&self) {
        self.highlights_cache.borrow_mut().clear();
    }

    /// Compute the cursor's visible `(x, y)` cell within `area`, or `None` when
    /// the cursor is scrolled out of view.
    pub fn get_visible_cursor(
        &self, area: &Rect
    ) -> Option<(u16, u16)> {
        let line_number_width = self.get_line_number_width();

        let (cursor_line, cursor_char_col) = self.code.point(self.cursor);
        
        if cursor_line >= self.offset_y && cursor_line < self.offset_y + area.height as usize {
            let line_start_char = self.code.line_to_char(cursor_line);
            let line_len = self.code.line_len(cursor_line);
        
            let max_x = (area.width as usize).saturating_sub(line_number_width);
            let start_col = self.offset_x;
                
            let cursor_visual_col: usize = {
                let slice = self.code.char_slice(line_start_char, line_start_char + cursor_char_col.min(line_len));
                RopeGraphemes::new(&slice).map(grapheme_width).sum()
            };
            
            let offset_visual_col: usize = {
                let slice = self.code.char_slice(line_start_char, line_start_char + start_col.min(line_len));
                RopeGraphemes::new(&slice).map(grapheme_width).sum()
            };
        
            let relative_visual_col = cursor_visual_col.saturating_sub(offset_visual_col);
            let visible_x = relative_visual_col.min(max_x);
        
            let cursor_x = area.left() + u16::try_from(line_number_width + visible_x).unwrap_or(u16::MAX);
            let cursor_y = area.top() + u16::try_from(cursor_line - self.offset_y).unwrap_or(u16::MAX);
        
            if cursor_x < area.right() && cursor_y < area.bottom() {
                return Some((cursor_x, cursor_y));
            }
        }
        
        None
    }

    /// Show or hide the line-number gutter.
    pub fn show_line_numbers(&mut self, show: bool) {
        self.show_line_numbers = show;
    }

    /// Toggle the line-number gutter; returns the new visibility.
    pub fn toggle_line_numbers(&mut self) -> bool {
        self.show_line_numbers = !self.show_line_numbers;
        self.show_line_numbers
    }

    /// Show or hide visible-whitespace glyphs (space, tab, line ending).
    pub fn show_whitespace(&mut self, show: bool) {
        self.show_whitespace = show;
    }

    /// Enable or disable bracket/quote auto-pairing.
    pub fn set_auto_pair(&mut self, on: bool) {
        self.auto_pair = on;
    }

    /// Set (or clear with `None`) the end-of-line virtual note: `(line, text)`
    /// drawn dimmed after that line's content. Used for inline git blame.
    pub fn set_eol_note(&mut self, note: Option<(usize, String)>) {
        self.eol_note = note;
    }

    /// Set the debugger breakpoint lines (0-based) drawn in the gutter.
    pub fn set_breakpoints(&mut self, lines: Vec<usize>) {
        self.breakpoints = lines;
    }

    /// Set (or clear) the debugger's currently-stopped line (0-based).
    pub fn set_debug_line(&mut self, line: Option<usize>) {
        self.debug_line = line;
    }

    /// Enable or disable soft wrap (long lines wrap instead of scrolling).
    pub fn set_soft_wrap(&mut self, on: bool) {
        self.soft_wrap = on;
        if on {
            self.offset_x = 0;
        }
    }

    /// Toggle soft wrap; returns the new state.
    pub fn toggle_soft_wrap(&mut self) -> bool {
        self.set_soft_wrap(!self.soft_wrap);
        self.soft_wrap
    }

    /// Set the indent string inserted by Tab / the `Indent` action (spaces or a
    /// tab). `None` restores the per-language default.
    pub fn set_indent(&mut self, indent: Option<String>) {
        self.code.set_indent(indent);
    }

    /// Toggle visible-whitespace glyphs; returns the new visibility.
    pub fn toggle_whitespace(&mut self) -> bool {
        self.show_whitespace = !self.show_whitespace;
        self.show_whitespace
    }

    /// Set the style used for visible-whitespace glyphs (typically dimmed).
    pub fn set_whitespace_style(&mut self, style: Style) {
        self.whitespace_style = style;
    }

    /// Set the style used to highlight the bracket matching the one at the cursor.
    pub fn set_bracket_style(&mut self, style: Style) {
        self.bracket_style = style;
    }

    /// Set the left padding (in characters) between the gutter and the code.
    pub fn set_left_code_padding(&mut self, char_count: usize) {
        self.left_code_padding = char_count;
    }
}
