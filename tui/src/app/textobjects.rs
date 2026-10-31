//! Tree-sitter textobjects (T616, `crates/vix-editor-core/spec/textobjects`):
//! select the function/class/parameter around the cursor (inside or
//! around), jump to the next or previous function/class, and the offline
//! shrink that undoes structural expansion.
//!
//! The ranges come from `Code::enclosing_textobjects`/`next_textobject`,
//! which query only a narrow window of the existing parse tree; a large
//! buffer whose first parse is still running reports "unavailable" instead
//! of parsing on the spot. The Vi/Spacemacs modal engine reuses
//! [`App::textobject_selection`] and [`App::textobject_jump_target`] for
//! `if`/`af`/`ic`/`ac`/`ia`/`aa` and `]f`/`[f`/`]c`/`[c`/`]a`/`[a`
//! (`tui/src/app/modal.rs`).

use super::App;
use crate::editor_core::textobject::{TextObjectDir, TextObjectKind};

/// One offline structural expansion, remembered so Shrink Selection can
/// undo it without a language server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NodeExpansion {
    /// The buffer revision the expansion was made at; an edit invalidates it.
    revision: u64,
    /// The selection (or caret, as an empty range) before expanding.
    before: (usize, usize),
    /// The selection the expansion produced.
    after: (usize, usize),
}

/// Why a textobject lookup found nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextObjectMiss {
    /// No active tab, no grammar or query for its language, or no parse
    /// tree yet.
    Unavailable,
    /// The buffer has textobjects, just none of this kind here.
    NotFound,
}

impl App {
    /// Dispatch a textobject action. Returns `true` if `action` was one.
    pub(super) fn run_textobject_action(&mut self, action: &str) -> bool {
        use TextObjectDir::{Next, Previous};
        use TextObjectKind::{Class, Function, Parameter};
        match action {
            "edit.select_function_inside" => self.select_textobject(Function, true),
            "edit.select_function_around" => self.select_textobject(Function, false),
            "edit.select_class_inside" => self.select_textobject(Class, true),
            "edit.select_class_around" => self.select_textobject(Class, false),
            "edit.select_parameter_inside" => self.select_textobject(Parameter, true),
            "edit.select_parameter_around" => self.select_textobject(Parameter, false),
            "nav.next_function" => self.goto_textobject(Function, Next),
            "nav.prev_function" => self.goto_textobject(Function, Previous),
            "nav.next_class" => self.goto_textobject(Class, Next),
            "nav.prev_class" => self.goto_textobject(Class, Previous),
            _ => return false,
        }
        true
    }

    /// The active tab's selection as an ordered range, or the caret as an
    /// empty one.
    fn current_range(&mut self) -> Option<(usize, usize)> {
        let t = self.editor.active_tab_mut()?;
        let cursor = t.editor.get_cursor();
        Some(t.editor.get_selection().map_or((cursor, cursor), |s| {
            (s.start.min(s.end), s.start.max(s.end))
        }))
    }

    /// The `count`-th `kind` textobject (inside or around) enclosing the
    /// current selection, skipping one equal to the selection itself so
    /// repeating the command grows outward.
    pub(crate) fn textobject_selection(
        &mut self,
        kind: TextObjectKind,
        inside: bool,
        count: usize,
    ) -> Result<(usize, usize), TextObjectMiss> {
        let (s, e) = self.current_range().ok_or(TextObjectMiss::Unavailable)?;
        let t = self
            .editor
            .active_tab()
            .ok_or(TextObjectMiss::Unavailable)?;
        let code = t.editor.code_ref();
        let candidates = code.enclosing_textobjects(kind, s);
        if candidates.is_empty() {
            return Err(Self::miss_reason(code));
        }
        candidates
            .into_iter()
            .filter(|o| o.around.0 <= s && e <= o.around.1)
            .map(|o| if inside { o.inside } else { o.around })
            .filter(|&r| s == e || r != (s, e))
            .nth(count.max(1) - 1)
            .ok_or(TextObjectMiss::NotFound)
    }

    /// The `count`-th `kind` textobject (inside or around) enclosing the
    /// cursor, innermost first, ignoring any selection: the range an
    /// operator such as `daf` acts on.
    pub(crate) fn textobject_at_cursor(
        &self,
        kind: TextObjectKind,
        inside: bool,
        count: usize,
    ) -> Option<(usize, usize)> {
        let t = self.editor.active_tab()?;
        let obj = *t
            .editor
            .code_ref()
            .enclosing_textobjects(kind, t.editor.get_cursor())
            .get(count.max(1) - 1)?;
        Some(if inside { obj.inside } else { obj.around })
    }

    /// Unavailable when the buffer cannot have textobjects right now, else
    /// simply not found.
    fn miss_reason(code: &crate::editor_core::code::Code) -> TextObjectMiss {
        if code.has_textobjects() && !code.parse_pending() {
            TextObjectMiss::NotFound
        } else {
            TextObjectMiss::Unavailable
        }
    }

    /// Put the status message for a miss.
    pub(crate) fn report_textobject_miss(&mut self, miss: TextObjectMiss) {
        self.status = match miss {
            TextObjectMiss::Unavailable => t!("status.textobject_unavailable"),
            TextObjectMiss::NotFound => t!("status.textobject_not_found"),
        }
        .to_string();
    }

    /// `edit.select_*_{inside,around}`: select the textobject, remembering
    /// the previous selection so Shrink Selection can return to it.
    fn select_textobject(&mut self, kind: TextObjectKind, inside: bool) {
        match self.textobject_selection(kind, inside, 1) {
            Ok((s, e)) => self.select_remembering(s, e),
            Err(miss) => self.report_textobject_miss(miss),
        }
    }

    /// Where the `count`-th `kind` textobject start is, searching `dir` from
    /// the cursor (wrapping at the buffer's ends), and whether it wrapped.
    pub(crate) fn textobject_jump_target(
        &self,
        kind: TextObjectKind,
        dir: TextObjectDir,
        count: usize,
    ) -> Result<(usize, bool), TextObjectMiss> {
        let t = self
            .editor
            .active_tab()
            .ok_or(TextObjectMiss::Unavailable)?;
        let code = t.editor.code_ref();
        let start = t.editor.get_cursor();
        let mut pos = start;
        let mut wrapped = false;
        for _ in 0..count.max(1) {
            let Some(next) = code.next_textobject(kind, dir, pos) else {
                return Err(Self::miss_reason(code));
            };
            wrapped |= match dir {
                TextObjectDir::Next => next <= pos,
                TextObjectDir::Previous => next >= pos,
            };
            pos = next;
        }
        Ok((pos, wrapped))
    }

    /// `nav.{next,prev}_{function,class}`: move the cursor to the start of
    /// the next or previous one.
    fn goto_textobject(&mut self, kind: TextObjectKind, dir: TextObjectDir) {
        match self.textobject_jump_target(kind, dir, 1) {
            Ok((pos, wrapped)) => {
                let area = self.editor_view();
                self.editor.goto_offset(pos, area);
                if wrapped {
                    self.status = t!("status.textobject_wrapped").to_string();
                }
            }
            Err(miss) => self.report_textobject_miss(miss),
        }
    }

    /// Select `start..end` (caret at `end`), pushing the previous selection
    /// on the expansion stack Shrink Selection pops when there is no
    /// language server.
    pub(crate) fn select_remembering(&mut self, start: usize, end: usize) {
        let Some(before) = self.current_range() else {
            return;
        };
        let area = self.editor_view();
        let Some(t) = self.editor.active_tab_mut() else {
            return;
        };
        let revision = t.editor.code_ref().revision();
        t.editor.set_selection_range(start, end);
        t.editor.focus(&area);
        if self
            .node_expansions
            .last()
            .is_some_and(|top| top.revision != revision || top.after != before)
        {
            self.node_expansions.clear();
        }
        self.node_expansions.push(NodeExpansion {
            revision,
            before,
            after: (start, end),
        });
    }

    /// Offline Shrink Selection: restore the selection from before the last
    /// structural expansion or textobject selection, if the buffer and the
    /// selection are still exactly as that step left them.
    pub(crate) fn shrink_selection_offline(&mut self) {
        let current = self.current_range();
        let revision = self
            .editor
            .active_tab()
            .map(|t| t.editor.code_ref().revision());
        let top = self.node_expansions.pop();
        let Some(top) = top.filter(|t| Some(t.after) == current && Some(t.revision) == revision)
        else {
            self.node_expansions.clear();
            self.status = t!("status.no_shrink_selection").to_string();
            return;
        };
        let area = self.editor_view();
        if let Some(t) = self.editor.active_tab_mut() {
            let (s, e) = top.before;
            if s == e {
                t.editor.clear_selection();
                t.editor.set_cursor(s);
            } else {
                t.editor.set_selection_range(s, e);
            }
            t.editor.focus(&area);
        }
    }
}
