//! Edit → Undo Tree… (T638): draw the active buffer's undo tree, preview a
//! state live as the selection moves, keep it on `Enter`, or put everything
//! back on `Esc`. The graph and the selection live in
//! `vix_undo_tree_panel`; the tree walk in
//! `vix_editor_core::Editor::goto_undo_state`.
//!
//! A preview is a real buffer change, so while the panel is open `tick`
//! skips auto-save, the hot-exit backup pass and the reload-on-change check
//! (see [`App::undo_tree_previewing`]), and quitting restores first.

use vix_input::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use vix_undo_tree_panel::{Panel, TreeNode};

use super::{App, rect_contains};
use crate::editor_core::history::History;
use crate::editor_core::selection::Selection;

/// The open panel plus everything `Esc` restores.
pub(crate) struct UndoTreeState {
    /// The graph and selection.
    pub(crate) panel: Panel,
    /// The tab being previewed.
    tab: usize,
    /// The history as it was on open (every branch's redo pointer).
    history: History,
    /// The cursor on open.
    cursor: usize,
    /// The selection on open.
    selection: Option<Selection>,
    /// The tab's modified flag on open.
    dirty: bool,
}

impl App {
    /// Whether the undo tree panel is open, previewing states of a buffer.
    pub(crate) const fn undo_tree_previewing(&self) -> bool {
        self.undo_tree.is_some()
    }

    /// `edit.undo_branch`: cycle which branch the next redo follows, and say
    /// whether there was another one.
    pub(super) fn switch_undo_branch(&mut self) {
        self.status = if self.editor.switch_undo_branch() {
            t!("status.undo_branch_switched").to_string()
        } else {
            t!("status.undo_branch_none").to_string()
        };
    }

    /// `edit.undo_tree`: open the panel over the active buffer, or say why
    /// not (a shared buffer, where local undo is off, or no history yet).
    pub(super) fn open_undo_tree(&mut self) {
        let tab = self.editor.active;
        let Some(t) = self.editor.tabs.get_mut(tab) else {
            return;
        };
        if t.editor.is_edit_logging() {
            self.status = t!("status.undo_tree_shared").to_string();
            return;
        }
        let history = t.editor.code_ref().history().clone();
        let nodes: Vec<TreeNode> = history
            .nodes()
            .into_iter()
            .map(|n| TreeNode {
                seq: n.seq,
                parent: n.parent,
                children: n.children,
                timestamp: n.timestamp,
            })
            .collect();
        if nodes.len() < 2 {
            self.status = t!("status.undo_tree_empty").to_string();
            return;
        }
        let panel = Panel::open(&nodes, history.current());
        self.undo_tree = Some(UndoTreeState {
            panel,
            tab,
            cursor: t.editor.get_cursor(),
            selection: t.editor.get_selection(),
            dirty: t.dirty,
            history,
        });
    }

    /// Handle a key in the undo-tree panel: movement keys select a state and live-preview it in
    /// the buffer; Enter commits the previewed state; Esc cancels and restores the original.
    pub(super) fn undo_tree_key(&mut self, key: KeyEvent) {
        let page = (self.layout.undo_tree.height as usize).max(1);
        let Some(state) = self.undo_tree.as_mut() else {
            return;
        };
        let p = &mut state.panel;
        match key.code {
            KeyCode::Up => p.up(),
            KeyCode::Down => p.down(),
            KeyCode::PageUp => p.page_up(page),
            KeyCode::PageDown => p.page_down(page),
            KeyCode::Home => p.home(),
            KeyCode::End => p.end(),
            KeyCode::Enter => {
                self.commit_undo_tree();
                return;
            }
            KeyCode::Esc => {
                self.cancel_undo_tree();
                return;
            }
            _ => return,
        }
        self.preview_undo_tree();
    }

    /// Handle a mouse event in the undo-tree panel: a left click on a row selects and previews
    /// that state.
    pub(super) fn undo_tree_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let rect = self.layout.undo_tree;
        if !rect_contains(rect, mouse.column, mouse.row) {
            return;
        }
        let Some(state) = self.undo_tree.as_mut() else {
            return;
        };
        let row = state.panel.scroll + (mouse.row - rect.y) as usize;
        if state.panel.select_row(row) {
            self.preview_undo_tree();
        }
    }

    /// Move the buffer to the selected state. If the buffer became shared
    /// meanwhile (local undo is off), close the panel and say so.
    fn preview_undo_tree(&mut self) {
        let Some(state) = self.undo_tree.as_ref() else {
            return;
        };
        let (tab, seq) = (state.tab, state.panel.selected_seq());
        let Some(seq) = seq else {
            return;
        };
        let moved = self
            .editor
            .tabs
            .get_mut(tab)
            .is_some_and(|t| t.editor.goto_undo_state(seq));
        if !moved {
            self.undo_tree = None;
            self.status = t!("status.undo_tree_shared").to_string();
        }
    }

    /// `Enter`: keep the previewed state.
    fn commit_undo_tree(&mut self) {
        let Some(state) = self.undo_tree.take() else {
            return;
        };
        let Some(seq) = state.panel.selected_seq() else {
            return;
        };
        if seq != state.panel.start
            && let Some(t) = self.editor.tabs.get_mut(state.tab)
        {
            t.dirty = true;
            self.status = t!("status.undo_tree_moved", state = seq).to_string();
        }
    }

    /// `Esc` (and quitting with the panel open): put back the starting text,
    /// cursor, selection, modified flag and branch pointers.
    pub(super) fn cancel_undo_tree(&mut self) {
        let Some(state) = self.undo_tree.take() else {
            return;
        };
        let Some(t) = self.editor.tabs.get_mut(state.tab) else {
            return;
        };
        if t.editor.goto_undo_state(state.panel.start) {
            t.editor.code_mut().set_history(state.history);
            t.editor.set_cursor(state.cursor);
            t.editor.set_selection(state.selection);
            t.dirty = state.dirty;
        }
    }
}
