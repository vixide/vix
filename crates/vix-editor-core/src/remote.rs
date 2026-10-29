//! The editor side of Cowork (T692, T693): everything that lets another
//! participant's edits and cursor live in this buffer without the editor
//! knowing about the network.
//!
//! - [`Editor::start_edit_log`] / [`Editor::take_edit_log`] report every local
//!   content change as a [`Splice`], including undo, redo and whole-buffer
//!   replacement. The collaboration layer feeds them to its CRDT.
//! - [`Editor::apply_remote_edits`] replays another participant's splices,
//!   keeps the local cursor, selection and extra carets on their characters,
//!   and records nothing (no log entry, no undo step).
//! - [`Editor::set_remote_carets`] gives the renderer other participants'
//!   carets and selections to draw beneath the local cursor.
//!
//! While a buffer is shared its undo must not use this editor's history,
//! whose stored offsets go stale when a remote edit lands; the collaboration
//! layer routes undo through its own per-participant stack and replays the
//! result here with [`Editor::apply_remote_edits`].

pub use crate::code::Splice;
use crate::editor::Editor;
use crate::selection::Selection;
use ratatui_core::style::{Color, Style};

/// Another participant's caret, and optionally their selection, as character
/// offsets in this buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteCaret {
    /// The caret.
    pub head: usize,
    /// The selection as `(start, end)`, when non-empty.
    pub selection: Option<(usize, usize)>,
    /// The participant's color (already contrast-checked by the caller).
    pub color: Color,
}

/// Where a character offset ends up after `edits` are applied in order. An
/// insertion exactly at the offset leaves it in front of the new text.
#[must_use]
pub fn transform_offset(mut pos: usize, edits: &[Splice]) -> usize {
    for e in edits {
        if e.delete > 0 {
            let end = e.offset.saturating_add(e.delete);
            if pos >= end {
                pos -= e.delete;
            } else if pos > e.offset {
                pos = e.offset;
            }
        }
        let n = e.insert.chars().count();
        if n > 0 && e.offset < pos {
            pos += n;
        }
    }
    pos
}

/// A readable text color over `bg`: black on light colors, white on dark.
fn text_on(bg: Color) -> Color {
    match bg {
        Color::Rgb(r, g, b) => {
            // Rec. 601 luma in integer math: no float casts needed.
            let luma = (u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114) / 1000;
            if luma > 140 {
                Color::Black
            } else {
                Color::White
            }
        }
        Color::Black | Color::DarkGray | Color::Blue | Color::Red | Color::Magenta => Color::White,
        _ => Color::Black,
    }
}

impl Editor {
    /// Start recording local content changes (idempotent).
    pub fn start_edit_log(&mut self) {
        self.code.start_edit_log();
    }

    /// Stop recording local content changes and drop anything pending.
    pub fn stop_edit_log(&mut self) {
        self.code.stop_edit_log();
    }

    /// The local changes since the last call, oldest first. Empty when
    /// logging is off.
    pub fn take_edit_log(&mut self) -> Vec<Splice> {
        self.code.take_edit_log()
    }

    /// Replay another participant's edits, in order.
    ///
    /// The text changes without an undo step or a log entry. The cursor, the
    /// selection and any extra carets are moved so each stays on the character
    /// it was on; the viewport does not move.
    pub fn apply_remote_edits(&mut self, edits: &[Splice]) {
        if edits.is_empty() {
            return;
        }
        self.code.apply_remote(edits);
        self.cursor = transform_offset(self.cursor, edits);
        self.selection = self.selection.map(|s| {
            Selection::new(
                transform_offset(s.start, edits),
                transform_offset(s.end, edits),
            )
        });
        for caret in &mut self.carets {
            caret.pos = transform_offset(caret.pos, edits);
            caret.anchor = caret.anchor.map(|a| transform_offset(a, edits));
        }
        let len = self.code.len();
        self.cursor = self.cursor.min(len);
        self.goal_col = None;
        self.reset_highlight_cache();
    }

    /// Set the other participants' carets to draw; an empty list clears them.
    pub fn set_remote_carets(&mut self, carets: Vec<RemoteCaret>) {
        self.remote_carets = carets;
    }

    /// The other participants' carets currently set.
    #[must_use]
    pub fn remote_carets(&self) -> &[RemoteCaret] {
        &self.remote_carets
    }

    /// Draw the remote selections (tinted) and carets (a colored cell) into
    /// `buf`. Called by the renderer after the local selection and before the
    /// local cursor, so the local cursor is always on top.
    pub(crate) fn draw_remote_carets(
        &self,
        area: ratatui_core::layout::Rect,
        buf: &mut ratatui_core::buffer::Buffer,
        line_number_width: usize,
        total_chars: usize,
    ) {
        for rc in &self.remote_carets {
            let style = Style::default().bg(rc.color).fg(text_on(rc.color));
            if let Some((start, end)) = rc.selection {
                let end = end.min(total_chars);
                let start = start.min(end);
                if start < end {
                    // Selections are a lighter touch than the caret: reversed
                    // text keeps the code readable while marking the range.
                    self.highlight_char_range(
                        area,
                        buf,
                        line_number_width,
                        start,
                        end,
                        Style::default()
                            .fg(rc.color)
                            .add_modifier(ratatui_core::style::Modifier::REVERSED),
                    );
                }
            }
            self.draw_caret_block(
                area,
                buf,
                line_number_width,
                rc.head.min(total_chars),
                false,
                style,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor(text: &str) -> Editor {
        Editor::new("text", text, Vec::new()).unwrap()
    }

    fn sp(offset: usize, delete: usize, insert: &str) -> Splice {
        Splice {
            offset,
            delete,
            insert: insert.to_string(),
        }
    }

    #[test]
    fn local_edits_are_logged_remote_edits_are_not() {
        let mut e = editor("hello world");
        e.start_edit_log();
        e.set_content("hello brave world");
        let log = e.take_edit_log();
        assert!(!log.is_empty(), "set_content is a local change");
        assert_eq!(e.take_edit_log(), Vec::new(), "the log is drained");

        e.apply_remote_edits(&[sp(0, 0, ">> ")]);
        assert_eq!(e.take_edit_log(), Vec::new(), "remote edits are not echoed");
        assert_eq!(e.get_content(), ">> hello brave world");
    }

    #[test]
    fn replaying_the_log_reproduces_the_buffer() {
        let mut e = editor("one two three");
        e.start_edit_log();
        e.set_content("one 2 three!");
        let mut mirror: Vec<char> = "one two three".chars().collect();
        for s in e.take_edit_log() {
            mirror.splice(s.offset..s.offset + s.delete, s.insert.chars());
        }
        assert_eq!(mirror.into_iter().collect::<String>(), e.get_content());
    }

    #[test]
    fn undo_and_redo_are_logged_too() {
        let mut e = editor("abc");
        e.start_edit_log();
        e.set_content("abcd");
        e.take_edit_log();
        e.apply(crate::actions::Undo);
        let log = e.take_edit_log();
        assert_ne!(log, Vec::new());
        assert_eq!(e.get_content(), "abc");
    }

    #[test]
    fn the_local_cursor_stays_on_its_character() {
        let mut e = editor("hello world");
        e.set_cursor(6);
        e.apply_remote_edits(&[sp(0, 0, ">>> ")]);
        assert_eq!(e.get_cursor(), 10);
        e.apply_remote_edits(&[sp(10, 0, "X")]);
        assert_eq!(e.get_cursor(), 10, "typing at the cursor does not drag it");
        e.apply_remote_edits(&[sp(0, 4, "")]);
        assert_eq!(e.get_cursor(), 6);
        e.apply_remote_edits(&[sp(3, 10, "")]);
        assert_eq!(e.get_cursor(), 3, "a deletion over the cursor collapses it");
        assert!(e.get_cursor() <= e.get_content().chars().count());
    }

    #[test]
    fn the_selection_follows_remote_edits() {
        let mut e = editor("hello world");
        e.set_selection_range(6, 11);
        e.apply_remote_edits(&[sp(0, 0, "abc ")]);
        assert_eq!(e.selection_span(), Some((10, 15)));
    }

    #[test]
    fn out_of_range_remote_edits_are_clamped_not_panics() {
        let mut e = editor("abc");
        e.apply_remote_edits(&[sp(99, 5, "z"), sp(1, 99, "")]);
        assert_eq!(e.get_content(), "a");
    }

    #[test]
    fn remote_edits_do_not_enter_the_undo_history() {
        let mut e = editor("abc");
        e.apply_remote_edits(&[sp(3, 0, "d")]);
        e.apply(crate::actions::Undo);
        assert_eq!(e.get_content(), "abcd", "nothing local to undo");
    }

    #[test]
    fn transform_matches_the_collaboration_cores_rule() {
        assert_eq!(transform_offset(5, &[sp(0, 0, "abc")]), 8);
        assert_eq!(transform_offset(5, &[sp(5, 0, "abc")]), 5);
        assert_eq!(transform_offset(5, &[sp(0, 2, "")]), 3);
        assert_eq!(transform_offset(5, &[sp(3, 10, "")]), 3);
    }

    #[test]
    fn remote_carets_round_trip_and_draw_without_panicking() {
        use ratatui_core::{buffer::Buffer, layout::Rect, widgets::Widget};
        let mut e = editor("hello world\nsecond line\n");
        e.set_remote_carets(vec![RemoteCaret {
            head: 6,
            selection: Some((0, 5)),
            color: Color::Rgb(230, 74, 74),
        }]);
        assert_eq!(e.remote_carets().len(), 1);
        let area = Rect::new(0, 0, 30, 5);
        let mut buf = Buffer::empty(area);
        (&e).render(area, &mut buf);
        let caret_cells = buf
            .content()
            .iter()
            .filter(|c| c.bg == Color::Rgb(230, 74, 74))
            .count();
        assert_eq!(caret_cells, 1, "exactly one caret cell in the peer color");
        // An out-of-range caret is clamped, not a panic.
        e.set_remote_carets(vec![RemoteCaret {
            head: 9999,
            selection: Some((5, 9999)),
            color: Color::Green,
        }]);
        let mut buf = Buffer::empty(area);
        (&e).render(area, &mut buf);
        e.set_remote_carets(Vec::new());
        assert_eq!(e.remote_carets(), &[]);
    }
}
