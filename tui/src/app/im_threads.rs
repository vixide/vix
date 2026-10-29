//! Anchored threads in the editor (T753, `spec/instant-message`): a thread of
//! messages attached to a line range, stored as `.vix/im/threads/<id>.json`
//! under the workspace root so it can be shared through Git.
//!
//! The pure model -- capturing the anchor and its context, re-finding it
//! after edits, the JSON format -- is `vix_im::thread`. This module is the
//! App side and needs no network, so it is in every build: loading the
//! workspace's threads, the gutter marks (`Editor::set_comment_marks`), the
//! **Comment on Line…** prompt, the **Threads…** list, next/previous and
//! resolve/reopen, and writing a thread back when it changes or when its
//! file is saved at a new place.
//!
//! Re-anchoring runs when the active buffer's revision changes: each thread
//! on the file is looked for at its current line first, then by
//! `vix_im::reanchor`, and its range follows in memory. A thread whose text
//! and context are both gone is *outdated*: it has no gutter mark, and the
//! list says so. Moved line numbers reach the thread files when the file is
//! saved, so a thread file never points at lines of an unsaved buffer.
//!
//! Like the rest of IM, nothing here opens or focuses anything on its own: a
//! thread someone else committed shows only as a gutter mark.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use vix_editor_core::editor::CommentMark;
use vix_im::{Anchor, Message, Thread, reanchor};
use vix_input::{KeyCode, KeyEvent};

use super::im::{ImDialog, now_ms};
use super::{App, Prompt, PromptKind};
use crate::editor::Tab;

/// Where thread files live, relative to the workspace root.
pub(crate) const THREADS_DIR: &str = ".vix/im/threads";

/// What the open **Comment on Line…** prompt will do with its answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PendingComment {
    /// Reply to this thread.
    Reply(String),
    /// Start a thread on these 1-based lines of this root-relative file.
    New {
        /// The file, relative to the root, `/`-separated.
        path: String,
        /// The first line.
        start: u32,
        /// The last line.
        end: u32,
    },
}

/// The workspace's anchored threads, loaded on first use.
#[derive(Debug, Default)]
pub(crate) struct ThreadStore {
    /// The root they were loaded from; `None` until the first load. A new
    /// root (the workspace changed) loads again.
    root: Option<PathBuf>,
    /// Every thread, sorted by path and line.
    pub(crate) threads: Vec<Thread>,
    /// Ids of threads whose anchor could not be found in the open buffer.
    pub(crate) outdated: HashSet<String>,
    /// Ids whose range moved in memory since the file was last written.
    moved: HashSet<String>,
    /// Bumped on every change, so the gutter knows to redraw.
    generation: u64,
    /// `(tab index, buffer revision, generation)` the gutter marks were last
    /// computed for.
    marks_key: Option<(usize, u64, u64)>,
    /// The open prompt's target.
    pub(crate) pending: Option<PendingComment>,
}

impl ThreadStore {
    /// The thread with this id.
    pub(crate) fn get(&self, id: &str) -> Option<&Thread> {
        self.threads.iter().find(|t| t.id == id)
    }

    fn get_mut(&mut self, id: &str) -> Option<&mut Thread> {
        self.threads.iter_mut().find(|t| t.id == id)
    }

    fn sort(&mut self) {
        self.threads
            .sort_by(|a, b| (&a.path, a.start_line, &a.id).cmp(&(&b.path, b.start_line, &b.id)));
    }

    /// Note a change, so the gutter is recomputed.
    fn touch(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }
}

/// Read every `<id>.json` in `dir`. A file that is not a thread, whose id is
/// unsafe or does not match its name, or whose path is absolute or climbs
/// out with `..`, is skipped.
fn load_dir(dir: &Path) -> Vec<Thread> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let path = e.path();
            let stem = path.file_stem()?.to_str()?.to_string();
            if path.extension()? != "json" {
                return None;
            }
            let text = std::fs::read_to_string(&path).ok()?;
            let thread = Thread::from_json(&text).ok()?;
            (thread.id == stem && thread.file_name().is_some() && is_relative_path(&thread.path))
                .then_some(thread)
        })
        .collect()
}

/// Whether a stored thread path is relative and stays inside the root.
fn is_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !Path::new(path).has_root()
        && path
            .split('/')
            .all(|part| part != ".." && !part.contains(':'))
}

/// A buffer's text as lines, for anchoring.
fn buffer_lines(text: &str) -> Vec<&str> {
    text.split('\n').collect()
}

/// Find each thread in `lines`, cheaply first: an anchor still at its
/// current line is exact without a search. Moves ranges in memory and
/// returns the ids that are outdated and those that moved.
fn anchor_all<'a>(
    threads: impl Iterator<Item = &'a mut Thread>,
    lines: &[&str],
) -> (Vec<String>, Vec<String>) {
    let (mut outdated, mut moved) = (Vec::new(), Vec::new());
    for thread in threads {
        let start = usize::try_from(thread.start_line.max(1) - 1).unwrap_or(usize::MAX);
        let anchor_lines: Vec<&str> = thread.anchor_text.split('\n').collect();
        let here = lines
            .get(start..start.saturating_add(anchor_lines.len()))
            .is_some_and(|slice| {
                slice
                    .iter()
                    .zip(&anchor_lines)
                    .all(|(a, b)| a.trim_end() == b.trim_end())
            });
        if here {
            continue;
        }
        let anchor = reanchor(thread, lines);
        if anchor == Anchor::Outdated {
            outdated.push(thread.id.clone());
            continue;
        }
        let before = thread.start_line;
        thread.apply(anchor);
        if thread.start_line != before {
            moved.push(thread.id.clone());
        }
    }
    (outdated, moved)
}

impl App {
    /// Dispatch the thread actions. Returns `true` if `action` was one.
    pub(super) fn run_im_thread_action(&mut self, action: &str) -> bool {
        match action {
            "im.comment_line" => self.im_ask_comment(),
            "im.threads" => self.im_open_threads(),
            "im.next_thread" => self.im_step_thread(true),
            "im.prev_thread" => self.im_step_thread(false),
            "im.resolve_thread" => self.im_resolve_at_cursor(),
            _ => return false,
        }
        true
    }

    /// The directory thread files live in.
    fn im_threads_dir(&self) -> PathBuf {
        self.root.join(THREADS_DIR)
    }

    /// Load the workspace's threads unless they are loaded for this root.
    fn im_threads_load_once(&mut self) {
        if self.im.threads.root.as_deref() != Some(self.root.as_path()) {
            self.im_threads_reload();
        }
    }

    /// Read the thread files again (another person's may have arrived
    /// through Git). Ranges moved in memory are found again on the next
    /// refresh.
    fn im_threads_reload(&mut self) {
        let threads = load_dir(&self.im_threads_dir());
        let store = &mut self.im.threads;
        store.root = Some(self.root.clone());
        store.threads = threads;
        store.outdated.clear();
        store.moved.clear();
        store.sort();
        store.touch();
    }

    /// A file's path relative to the workspace root, `/`-separated, or
    /// `None` when it is outside the root.
    fn im_root_relative(&self, path: &Path) -> Option<String> {
        let relative = path
            .strip_prefix(&self.root)
            .ok()
            .map(Path::to_path_buf)
            .or_else(|| {
                let (file, root) = (path.canonicalize().ok()?, self.root.canonicalize().ok()?);
                file.strip_prefix(root).ok().map(Path::to_path_buf)
            })?;
        let text = relative.to_string_lossy().replace('\\', "/");
        is_relative_path(&text).then_some(text)
    }

    /// The active tab's root-relative path, when it is a file in the root.
    fn im_active_relative(&self) -> Option<String> {
        let tab = self.editor.active_tab().filter(|t| !t.is_image())?;
        self.im_root_relative(tab.path.as_deref()?)
    }

    /// Find the active file's threads in its buffer now (each moves with its
    /// text), and set the gutter marks.
    fn im_threads_refresh_active(&mut self) {
        self.im_threads_load_once();
        // Most files have no thread: skip the path check and the buffer copy
        // (this runs whenever the active buffer changes).
        let rel = if self.im.threads.threads.is_empty() {
            None
        } else {
            self.im_active_relative()
                .filter(|rel| self.im.threads.threads.iter().any(|t| t.path == *rel))
        };
        let Some(rel) = rel else {
            if let Some(t) = self.editor.active_tab_mut() {
                t.editor.clear_comment_marks();
            }
            return;
        };
        let text = self.editor.active_tab().map(Tab::text).unwrap_or_default();
        let lines = buffer_lines(&text);
        let store = &mut self.im.threads;
        let on_file: Vec<&mut Thread> =
            store.threads.iter_mut().filter(|t| t.path == rel).collect();
        let ids: Vec<String> = on_file.iter().map(|t| t.id.clone()).collect();
        let (outdated, moved) = anchor_all(on_file.into_iter(), &lines);
        for id in &ids {
            store.outdated.remove(id);
        }
        store.outdated.extend(outdated);
        store.moved.extend(moved);
        store.sort();
        let marks: Vec<(usize, CommentMark)> = store
            .threads
            .iter()
            .filter(|t| t.path == rel && !store.outdated.contains(&t.id))
            .map(|t| {
                let mark = if t.resolved {
                    CommentMark::Resolved
                } else {
                    CommentMark::Open
                };
                let line = usize::try_from(t.start_line.saturating_sub(1)).unwrap_or(usize::MAX);
                (line, mark)
            })
            .collect();
        if let Some(t) = self.editor.active_tab_mut() {
            t.editor.set_comment_marks(marks);
        }
    }

    /// The per-frame refresh from `ui::draw`: recompute the active tab's
    /// thread marks only when the tab, its text or the threads changed.
    pub fn refresh_thread_marks_if_stale(&mut self) {
        let key = self
            .editor
            .active_tab()
            .map(|t| (self.editor.active, t.editor.revision()));
        let store_changed = self.im.threads.root.as_deref() != Some(self.root.as_path());
        let key = key.map(|(tab, rev)| (tab, rev, self.im.threads.generation));
        if !store_changed && key == self.im.threads.marks_key {
            return;
        }
        self.im_threads_refresh_active();
        // Loading may have bumped the generation; key on the settled value.
        self.im.threads.marks_key = self.editor.active_tab().map(|t| {
            (
                self.editor.active,
                t.editor.revision(),
                self.im.threads.generation,
            )
        });
    }

    /// The active file's threads that are not outdated, as `(id, start
    /// line)`, after a fresh re-anchor.
    fn im_threads_here(&mut self) -> Vec<(String, u32)> {
        self.im_threads_refresh_active();
        let Some(rel) = self.im_active_relative() else {
            return Vec::new();
        };
        let store = &self.im.threads;
        store
            .threads
            .iter()
            .filter(|t| t.path == rel && !store.outdated.contains(&t.id))
            .map(|t| (t.id.clone(), t.start_line))
            .collect()
    }

    /// The thread covering the cursor line in the active file, an open one
    /// before a resolved one.
    fn im_thread_at_cursor(&mut self) -> Option<String> {
        let here = self.im_threads_here();
        let line = u32::try_from(self.editor.cursor_1based().0).unwrap_or(u32::MAX);
        let store = &self.im.threads;
        let mut covering: Vec<&Thread> = here
            .iter()
            .filter_map(|(id, _)| store.get(id))
            .filter(|t| t.covers(line))
            .collect();
        covering.sort_by_key(|t| t.resolved);
        covering.first().map(|t| t.id.clone())
    }

    /// Write one thread's file.
    fn im_write_thread(&mut self, id: &str) {
        let dir = self.im_threads_dir();
        let Some(thread) = self.im.threads.get(id) else {
            return;
        };
        let (Some(name), Ok(json)) = (thread.file_name(), thread.to_json()) else {
            return;
        };
        let written = std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::write(dir.join(name), format!("{json}\n")));
        match written {
            Ok(()) => {
                self.im.threads.moved.remove(id);
            }
            Err(e) => {
                self.messages
                    .error(t!("msg.im_thread_write_failed", error = e).to_string());
            }
        }
    }

    /// **IM → Comment on Line…**: ask for the comment. On a line with a
    /// thread it is a reply; otherwise it starts a thread on the selected
    /// lines, or the cursor's line.
    fn im_ask_comment(&mut self) {
        let Some(rel) = self.im_active_relative() else {
            self.status = t!("status.im_thread_no_file").to_string();
            return;
        };
        if let Some(id) = self.im_thread_at_cursor() {
            let line = self.im.threads.get(&id).map_or(0, |t| t.start_line);
            self.im.threads.pending = Some(PendingComment::Reply(id));
            self.prompt = Some(Prompt::new(
                PromptKind::ImComment,
                t!("prompt.im_reply", line = line).to_string(),
            ));
            return;
        }
        let (start, end) = self
            .editor
            .active_tab()
            .and_then(|t| {
                let (s, e) = t.editor.selection_span()?;
                if s == e {
                    return None;
                }
                let code = t.editor.code_ref();
                Some((
                    code.char_to_line(s) + 1,
                    code.char_to_line(e.saturating_sub(1).max(s)) + 1,
                ))
            })
            .unwrap_or_else(|| {
                let line = self.editor.cursor_1based().0;
                (line, line)
            });
        let (start, end) = (
            u32::try_from(start).unwrap_or(u32::MAX),
            u32::try_from(end).unwrap_or(u32::MAX),
        );
        self.im.threads.pending = Some(PendingComment::New {
            path: rel,
            start,
            end,
        });
        let label = if end > start {
            t!("prompt.im_comment_lines", start = start, end = end)
        } else {
            t!("prompt.im_comment", line = start)
        };
        self.prompt = Some(Prompt::new(PromptKind::ImComment, label.to_string()));
    }

    /// The **Comment on Line…** prompt's answer: reply, or start the thread,
    /// and write it.
    pub(super) fn im_comment(&mut self, input: &str) {
        let Some(pending) = self.im.threads.pending.take() else {
            return;
        };
        let body = input.trim();
        if body.is_empty() {
            return;
        }
        let message = Message::text(self.im.inbox.nick(), now_ms(), body);
        let id = match pending {
            PendingComment::Reply(id) => {
                let Some(thread) = self.im.threads.get_mut(&id) else {
                    return;
                };
                thread.reply(message);
                self.status = t!("status.im_thread_replied", line = thread.start_line).to_string();
                id
            }
            PendingComment::New { path, start, end } => {
                let text = self.editor.active_tab().map(Tab::text).unwrap_or_default();
                let dir = self.im_threads_dir();
                let store = &self.im.threads;
                let id = vix_im::thread::new_id(now_ms(), |id| {
                    store.get(id).is_some() || dir.join(format!("{id}.json")).exists()
                });
                let mut thread = Thread::new(id.clone(), path, &buffer_lines(&text), start, end);
                thread.reply(message);
                self.status = t!("status.im_thread_started", line = thread.start_line).to_string();
                self.im.threads.threads.push(thread);
                self.im.threads.sort();
                id
            }
        };
        self.im.threads.touch();
        self.im_write_thread(&id);
    }

    /// **IM → Threads…**: read the thread files again and list them, with
    /// the thread at the cursor (if any) highlighted.
    fn im_open_threads(&mut self) {
        self.im_threads_reload();
        let at_cursor = self.im_thread_at_cursor();
        let ids: Vec<String> = self
            .im
            .threads
            .threads
            .iter()
            .map(|t| t.id.clone())
            .collect();
        if ids.is_empty() {
            self.status = t!("status.im_no_threads").to_string();
            return;
        }
        let selected = at_cursor
            .and_then(|id| ids.iter().position(|i| *i == id))
            .unwrap_or(0);
        self.im_dialog = Some(ImDialog::Threads { ids, selected });
    }

    /// A key in the **Threads…** list: Up/Down move, Enter jumps, `r`
    /// resolves or reopens, Esc closes. Returns the dialog to keep open.
    pub(super) fn im_threads_key(
        &mut self,
        key: KeyEvent,
        ids: Vec<String>,
        selected: usize,
    ) -> Option<ImDialog> {
        match key.code {
            KeyCode::Up => Some(ImDialog::Threads {
                selected: selected.saturating_sub(1),
                ids,
            }),
            KeyCode::Down => {
                let selected = (selected + 1).min(ids.len().saturating_sub(1));
                Some(ImDialog::Threads { ids, selected })
            }
            KeyCode::Enter => {
                if let Some(id) = ids.get(selected) {
                    self.im_jump_to_thread(&id.clone());
                }
                None
            }
            KeyCode::Char('r' | 'R') => {
                if let Some(id) = ids.get(selected) {
                    self.im_toggle_resolved(&id.clone());
                }
                Some(ImDialog::Threads { ids, selected })
            }
            KeyCode::Esc => None,
            _ => Some(ImDialog::Threads { ids, selected }),
        }
    }

    /// Open a thread's file at the thread's line as it is now. A path
    /// outside the root (a thread file from elsewhere) opens nothing.
    fn im_jump_to_thread(&mut self, id: &str) {
        let Some(path) = self.im.threads.get(id).map(|t| t.path.clone()) else {
            return;
        };
        let Some(file) = self.im_inside_root(&path) else {
            self.status = t!("status.im_ref_outside", reference = path).to_string();
            return;
        };
        self.with_jump(|s| {
            s.open_path(&file, false);
            s.im_threads_refresh_active();
            let line = s.im.threads.get(id).map_or(1, |t| t.start_line);
            let area = s.editor_view();
            s.editor.goto(line as usize, None, area);
            s.focus = super::Focus::Editor;
        });
        if self.im.threads.outdated.contains(id) {
            self.status = t!("status.im_thread_outdated").to_string();
        } else {
            self.im_thread_status(id);
        }
    }

    /// Say which thread the cursor is on, with its first message, in the
    /// status line (the user asked: a jump or next/previous).
    fn im_thread_status(&mut self, id: &str) {
        let Some(thread) = self.im.threads.get(id) else {
            return;
        };
        let first = thread
            .messages
            .first()
            .map(|m| format!("{}: {}", m.author, m.body))
            .unwrap_or_default();
        let key = if thread.resolved {
            "status.im_thread_here_resolved"
        } else {
            "status.im_thread_here"
        };
        self.status = t!(
            key,
            line = thread.start_line,
            count = thread.messages.len(),
            text = first
        )
        .to_string();
    }

    /// **IM → Next Thread** / **Previous Thread**: move to the next thread in
    /// the active file, wrapping around.
    fn im_step_thread(&mut self, forward: bool) {
        let here = self.im_threads_here();
        let lines: Vec<u32> = here.iter().map(|&(_, l)| l).collect();
        let from = u32::try_from(self.editor.cursor_1based().0).unwrap_or(u32::MAX);
        let Some(line) = vix_im::thread::step(&lines, from, forward) else {
            self.status = t!("status.im_no_threads_here").to_string();
            return;
        };
        let id = here
            .iter()
            .find(|&&(_, l)| l == line)
            .map(|(id, _)| id.clone());
        self.with_jump(|s| {
            let area = s.editor_view();
            s.editor.goto(line as usize, None, area);
        });
        if let Some(id) = id {
            self.im_thread_status(&id);
        }
    }

    /// **IM → Resolve or Reopen Thread** on the cursor's line.
    fn im_resolve_at_cursor(&mut self) {
        match self.im_thread_at_cursor() {
            Some(id) => self.im_toggle_resolved(&id),
            None => self.status = t!("status.im_no_thread_at_line").to_string(),
        }
    }

    /// Resolve an open thread or reopen a resolved one, and write it.
    fn im_toggle_resolved(&mut self, id: &str) {
        let Some(thread) = self.im.threads.get_mut(id) else {
            return;
        };
        thread.resolved = !thread.resolved;
        let key = if thread.resolved {
            "status.im_thread_resolved"
        } else {
            "status.im_thread_reopened"
        };
        self.status = t!(key, line = thread.start_line).to_string();
        self.im.threads.touch();
        self.im_write_thread(id);
    }

    /// After a file is saved: its threads' ranges as found in the saved text
    /// go to their files, so the stored lines match the file on disk.
    pub(super) fn im_threads_saved(&mut self, saved: &Path) {
        if self.im.threads.root.is_none() {
            return;
        }
        let Some(rel) = self.im_root_relative(saved) else {
            return;
        };
        self.im_threads_refresh_active();
        let ids: Vec<String> = self
            .im
            .threads
            .threads
            .iter()
            .filter(|t| t.path == rel && self.im.threads.moved.contains(&t.id))
            .map(|t| t.id.clone())
            .collect();
        for id in ids {
            self.im_write_thread(&id);
        }
    }

    /// The file a root-relative path names, only when it is an existing file
    /// inside the workspace root (after resolving links).
    pub(super) fn im_inside_root(&self, path: &str) -> Option<PathBuf> {
        let reference = vix_im::CodeRef {
            path: path.to_string(),
            line: 1,
            column: None,
            end_line: None,
        };
        vix_im::resolve_in_root(&self.root, &reference).filter(|p| {
            p.is_file()
                && match (p.canonicalize(), self.root.canonicalize()) {
                    (Ok(file), Ok(root)) => file.starts_with(root),
                    _ => false,
                }
        })
    }

    /// The threads in the list, by id, for drawing: the thread, and whether
    /// it is outdated.
    pub(crate) fn im_thread_rows<'a>(&'a self, ids: &'a [String]) -> Vec<(&'a Thread, bool)> {
        ids.iter()
            .filter_map(|id| {
                self.im
                    .threads
                    .get(id)
                    .map(|t| (t, self.im.threads.outdated.contains(id)))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{anchor_all, buffer_lines, is_relative_path};
    use vix_im::Thread;

    #[test]
    fn stored_paths_must_stay_inside_the_root() {
        assert!(is_relative_path("src/a.rs"));
        assert!(is_relative_path("a.rs"));
        for bad in ["", "/etc/hosts", "../x", "src/../../x", "C:/x", "a\\b"] {
            assert!(!is_relative_path(bad), "{bad}");
        }
    }

    #[test]
    fn anchoring_moves_and_marks_outdated() {
        let old = buffer_lines("a\nb\nc\nd\ne\n");
        let mut threads = [
            Thread::new("t1", "f", &old, 3, 3),
            Thread::new("t2", "f", &old, 5, 5),
        ];
        let none: [String; 0] = [];
        // Unchanged: nothing moves.
        let (outdated, moved) = anchor_all(threads.iter_mut(), &old);
        assert_eq!((outdated, moved), (none.to_vec(), none.to_vec()));
        // Two lines added above: both move down.
        let new = buffer_lines("x\ny\na\nb\nc\nd\ne\n");
        let (outdated, moved) = anchor_all(threads.iter_mut(), &new);
        assert_eq!(outdated, none);
        assert_eq!(moved, ["t1", "t2"]);
        assert_eq!((threads[0].start_line, threads[1].start_line), (5, 7));
        // Everything replaced: outdated, and the stored line stays.
        let gone = buffer_lines("1\n2\n");
        let (outdated, _) = anchor_all(threads.iter_mut(), &gone);
        assert_eq!(outdated, ["t1", "t2"]);
        assert_eq!(threads[0].start_line, 5);
    }
}
