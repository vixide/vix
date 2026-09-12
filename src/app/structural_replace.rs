//! Structural search & replace (T201): compile a `$X`/`$$X`-hole pattern
//! (`vix_structural_replace`), then either step through matches
//! interactively in the active buffer (or its selection) exactly like
//! `QueryReplace`'s own `y`/`n`/`!`/`q` session, or compute a workspace-wide
//! plan and hand it to the same preview-and-confirm step
//! `workspace_replace_all` already uses (`ReplaceConfirm`), rather than
//! building a second confirm UI.

use crossterm::event::{KeyCode, KeyEvent};

use vix_structural_replace::{Pattern, render_replacement};

use super::{App, Decision, Prompt, PromptKind, ReplaceConfirm, StructuralReplace};
use crate::editor::Tab;

impl App {
    /// **Edit → Structural Replace…** / **… in Workspace…**: open the
    /// pattern prompt; `workspace` says which flow the replacement prompt
    /// should start once the pattern's typed and compiles. No-op (buffer
    /// flow only) without an editable active buffer.
    pub(super) fn open_structural_pattern_prompt(&mut self, workspace: bool) {
        if workspace {
            self.build_file_index();
        } else if self
            .editor
            .active_tab()
            .is_none_or(|t| t.is_image() || t.read_only)
        {
            return;
        }
        self.pending_structural = (workspace, None);
        self.prompt = Some(Prompt::new(
            PromptKind::StructuralPattern,
            t!("prompt.structural_pattern").to_string(),
        ));
    }

    /// `PromptKind::StructuralPattern`'s accept handler: compile the
    /// pattern and, on success, open the replacement-template prompt.
    /// Empty input is a no-op; a pattern that fails to compile reports an
    /// error rather than opening the next prompt.
    pub(super) fn accept_structural_pattern(&mut self, input: &str) {
        if input.is_empty() {
            return;
        }
        match Pattern::compile(input) {
            Ok(pattern) => {
                self.pending_structural.1 = Some((pattern, input.to_string()));
                self.prompt = Some(Prompt::new(
                    PromptKind::StructuralReplacement,
                    t!("prompt.structural_replacement").to_string(),
                ));
            }
            Err(e) => {
                self.messages
                    .error(t!("msg.structural_pattern_invalid", error = e).to_string());
            }
        }
    }

    /// `PromptKind::StructuralReplacement`'s accept handler: dispatch to
    /// the buffer or workspace flow per what `open_structural_pattern_prompt`
    /// was called with. A no-op if the pattern prompt was somehow never
    /// answered (shouldn't happen through the normal prompt chain).
    pub(super) fn accept_structural_replacement(&mut self, input: &str) {
        let (workspace, pending) = std::mem::take(&mut self.pending_structural);
        let Some((pattern, label)) = pending else {
            return;
        };
        if workspace {
            self.begin_structural_replace_workspace(&pattern, input);
        } else {
            self.begin_structural_replace_buffer(&pattern, input, label);
        }
    }

    /// Start the interactive buffer/selection session: find the first
    /// match (scoped to the active selection, if any, otherwise the whole
    /// buffer) and highlight it, or report "no matches".
    fn begin_structural_replace_buffer(
        &mut self,
        pattern: &Pattern,
        template: &str,
        label: String,
    ) {
        let area = self.editor_view();
        let Some(t) = self.editor.active_tab_mut() else {
            return;
        };
        let bounds = t.editor.selection_span();
        let text = t.text();
        let from_byte = bounds.map_or(0, |(cs, _)| t.editor.code_ref().char_to_byte(cs));
        let bound_end_byte = bounds.map(|(_, ce)| t.editor.code_ref().char_to_byte(ce));
        let found = pattern
            .find_from(&text, from_byte)
            .filter(|m| bound_end_byte.is_none_or(|end| m.end <= end));
        let Some(m) = found else {
            self.status = t!("status.sr_no_matches").into();
            return;
        };
        let cs = t.editor.code_ref().byte_to_char(m.start);
        let ce = t.editor.code_ref().byte_to_char(m.end);
        super::highlight_match(t, cs, ce, area);
        self.structural_replace = Some(StructuralReplace {
            pattern: pattern.clone(),
            template: template.to_string(),
            current: (cs, ce),
            bounds,
            replaced: 0,
            label,
        });
        self.status = t!("status.sr_keys").into();
    }

    pub(super) fn sr_key(&mut self, key: KeyEvent) {
        let decision = match key.code {
            KeyCode::Char('y' | 'Y' | ' ') => Decision::Replace,
            KeyCode::Char('n' | 'N') | KeyCode::Delete => Decision::Skip,
            KeyCode::Char('!') => Decision::ReplaceRest,
            KeyCode::Char('q' | 'Q') | KeyCode::Esc | KeyCode::Enter => Decision::Quit,
            _ => return,
        };
        self.sr_apply(decision);
    }

    fn sr_apply(&mut self, decision: Decision) {
        let area = self.editor_view();
        let Some(sr) = self.structural_replace.as_ref() else {
            return;
        };
        let pattern = sr.pattern.clone();
        let template = sr.template.clone();
        let (cs, ce) = sr.current;
        let mut bounds = sr.bounds;
        let mut replaced = sr.replaced;
        let label = sr.label.clone();

        let next = {
            let Some(t) = self.editor.active_tab_mut() else {
                return;
            };
            match decision {
                Decision::Quit => {
                    t.editor.remove_marks();
                    t.editor.set_selection(None);
                    None
                }
                Decision::Skip => {
                    t.editor.remove_marks();
                    find_next_structural(t, &pattern, ce, bounds)
                }
                Decision::Replace => {
                    let (resume, delta) =
                        apply_one_structural_replace(t, &pattern, &template, cs, ce);
                    replaced += 1;
                    t.dirty = true;
                    t.preview = false;
                    t.editor.remove_marks();
                    adjust_bounds(&mut bounds, delta);
                    find_next_structural(t, &pattern, resume, bounds)
                }
                Decision::ReplaceRest => {
                    let (mut resume, delta) =
                        apply_one_structural_replace(t, &pattern, &template, cs, ce);
                    replaced += 1;
                    adjust_bounds(&mut bounds, delta);
                    while let Some((ns, ne)) = find_next_structural(t, &pattern, resume, bounds) {
                        let (r2, d2) = apply_one_structural_replace(t, &pattern, &template, ns, ne);
                        replaced += 1;
                        adjust_bounds(&mut bounds, d2);
                        resume = r2;
                    }
                    t.dirty = true;
                    t.preview = false;
                    t.editor.set_selection(None);
                    t.editor.remove_marks();
                    None
                }
            }
        };
        if let Some((ns, ne)) = next {
            if let Some(t) = self.editor.active_tab_mut() {
                super::highlight_match(t, ns, ne, area);
            }
            self.structural_replace = Some(StructuralReplace {
                pattern,
                template,
                current: (ns, ne),
                bounds,
                replaced,
                label,
            });
        } else {
            self.structural_replace = None;
            self.status = t!("status.sr_replaced", count = replaced).to_string();
        }
    }

    /// **Edit → Structural Replace in Workspace…**: compute a per-file
    /// replacement plan across every file in `self.file_index` and reuse
    /// `ReplaceConfirm`'s existing preview-and-confirm step (the same one
    /// `workspace_replace_all` uses) rather than writing anything yet.
    fn begin_structural_replace_workspace(&mut self, pattern: &Pattern, template: &str) {
        let mut plan: Vec<(std::path::PathBuf, String)> = Vec::new();
        let mut lines: Vec<String> = Vec::new();
        let mut replaced = 0usize;
        for path in self.file_index.clone() {
            let Some(content) = self.current_text(&path) else {
                continue;
            };
            let hits = pattern.find_all(&content);
            if hits.is_empty() {
                continue;
            }
            let mut new_content = content.clone();
            for m in hits.iter().rev() {
                let rendered = render_replacement(template, &content, m);
                new_content.replace_range(m.start..m.end, &rendered);
            }
            let rel = path.strip_prefix(&self.root).unwrap_or(&path);
            lines.push(format!("{} ({})", rel.display(), hits.len()));
            replaced += hits.len();
            plan.push((path, new_content));
        }
        if plan.is_empty() {
            self.status = t!("status.sr_no_matches").into();
            return;
        }
        self.replace_confirm = Some(ReplaceConfirm {
            plan,
            replaced,
            lines,
            scroll: 0,
        });
    }
}

/// Replace the match at char span `(cs, ce)` in `t`, re-deriving its
/// captures from `pattern` at that position (the session doesn't carry
/// captures between steps -- cheap to recompute, and keeps `StructuralReplace`
/// itself free of any borrowed/derived state). Returns the resume char
/// offset just past the inserted text, and how many chars the buffer
/// grew (positive) or shrank (negative) by.
fn apply_one_structural_replace(
    t: &mut Tab,
    pattern: &Pattern,
    template: &str,
    cs: usize,
    ce: usize,
) -> (usize, isize) {
    let source = t.text();
    let bs = t.editor.code_ref().char_to_byte(cs);
    let be = t.editor.code_ref().char_to_byte(ce);
    let captures = pattern
        .find_from(&source, bs)
        .filter(|m| m.start == bs && m.end == be)
        .map(|m| m.captures)
        .unwrap_or_default();
    let m = vix_structural_replace::Match {
        start: bs,
        end: be,
        captures,
    };
    let rendered = render_replacement(template, &source, &m);
    let old_len = ce - cs;
    let new_len = rendered.chars().count();
    super::replace_char_span(t, (cs, ce), &rendered);
    (cs + new_len, new_len.cast_signed() - old_len.cast_signed())
}

/// Grow/shrink a selection bound's end by `delta` chars (never past its
/// own start), after a replacement changed the buffer's length inside it.
fn adjust_bounds(bounds: &mut Option<(usize, usize)>, delta: isize) {
    if let Some((bs, be)) = bounds.as_mut() {
        *be = (be.cast_signed() + delta)
            .max(bs.cast_signed())
            .cast_unsigned();
    }
}

/// The next structural match at or after char offset `from`, bounded to
/// `bounds` (a selection's char span) when given. `None` past the bound or
/// the end of the buffer.
fn find_next_structural(
    t: &Tab,
    pattern: &Pattern,
    from: usize,
    bounds: Option<(usize, usize)>,
) -> Option<(usize, usize)> {
    let text = t.text();
    let from_byte = t.editor.code_ref().char_to_byte(from);
    let bound_end_byte = bounds.map(|(_, ce)| t.editor.code_ref().char_to_byte(ce));
    let m = pattern
        .find_from(&text, from_byte)
        .filter(|m| bound_end_byte.is_none_or(|end| m.end <= end))?;
    Some((
        t.editor.code_ref().byte_to_char(m.start),
        t.editor.code_ref().byte_to_char(m.end),
    ))
}
