//! Explorer → Edit Directory (T636): an Oil/wdired-style editable listing of
//! one directory. The listing is a real, editable buffer carrying a synthetic
//! `path` (the same pattern as T211's editable search results), so saving it
//! is intercepted in [`App::save`]: the buffer is diffed back against the
//! listing it was rendered from into renames, creates and deletes
//! ([`vix_fileops::dir_edit`]), previewed in a confirm step, and only then
//! applied, deletes honoring the `explorer_delete` setting.

use std::path::{Path, PathBuf};

use vix_fileops::dir_edit::{self, DiffError, Listed, Plan, Step};
use vix_input::{KeyCode, KeyEvent};

use super::{App, Focus};

/// The open directory listing: which directory, and the entries the buffer
/// was rendered from (their positions are the lines' ids).
pub(crate) struct DirEditSession {
    /// The directory being edited.
    dir: PathBuf,
    /// The entries, in listing order.
    entries: Vec<Listed>,
}

/// A diffed listing awaiting confirmation.
pub(crate) struct DirEditConfirm {
    /// The directory, as shown in the title.
    pub(crate) dir_label: String,
    /// How many changes will be made.
    pub(crate) count: usize,
    /// One preview row per change.
    pub(crate) lines: Vec<String>,
    /// First visible preview row.
    pub(crate) scroll: usize,
    /// The ordered filesystem steps to run on confirm.
    steps: Vec<Step>,
}

impl App {
    /// The synthetic `path` the directory-listing buffer carries -- never a
    /// real file, so [`App::save`] can recognize the buffer by it.
    pub(super) const DIR_EDIT_PATH: &str = "*directory*";

    /// Whether the active tab is the open directory-listing buffer.
    pub(super) fn active_tab_is_dir_edit(&self) -> bool {
        self.dir_edit.is_some()
            && self.editor.active_tab().and_then(|t| t.path.as_deref())
                == Some(Path::new(Self::DIR_EDIT_PATH))
    }

    /// The directory `explorer.edit_directory` lists: the one already being
    /// edited (to refresh it), else the explorer's highlighted directory (or
    /// a highlighted file's parent) when the explorer has focus, else the
    /// active file's directory, else the explorer root.
    fn dir_edit_target(&self) -> PathBuf {
        if self.active_tab_is_dir_edit()
            && let Some(s) = self.dir_edit.as_ref()
        {
            return s.dir.clone();
        }
        if self.focus == Focus::Explorer
            && let Some(n) = self.explorer.selected_node()
        {
            if n.is_dir {
                return n.path.clone();
            }
            if let Some(parent) = n.path.parent() {
                return parent.to_path_buf();
            }
        }
        let auth = crate::authority::current();
        if let Some(p) = self.active_path()
            && auth.is_file(&p)
            && let Some(parent) = p.parent()
        {
            return parent.to_path_buf();
        }
        self.explorer.root.clone()
    }

    /// `dir`'s entries, directories first and then by name. A name that
    /// isn't valid Unicode is left out, so it can't be renamed or deleted
    /// under a mangled spelling.
    fn list_dir_for_edit(dir: &Path) -> std::io::Result<Vec<Listed>> {
        let mut entries: Vec<Listed> = crate::authority::current()
            .read_dir(dir)?
            .into_iter()
            .filter_map(|e| {
                let name = e.path.file_name()?.to_str()?.to_string();
                Some(Listed::new(name, e.is_dir))
            })
            .collect();
        entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.cmp(&b.name)));
        Ok(entries)
    }

    /// The directory relative to the workspace root, for messages.
    fn dir_edit_label(&self, dir: &Path) -> String {
        match dir.strip_prefix(&self.root) {
            Ok(rel) if rel.as_os_str().is_empty() => ".".to_string(),
            Ok(rel) => rel.display().to_string(),
            Err(_) => dir.display().to_string(),
        }
    }

    /// `explorer.edit_directory`: open (or refresh) the editable listing of
    /// [`App::dir_edit_target`], one `/<id> <name>` line per entry.
    pub(super) fn open_dir_edit(&mut self) {
        let dir = self.dir_edit_target();
        let label = self.dir_edit_label(&dir);
        let entries = match Self::list_dir_for_edit(&dir) {
            Ok(e) => e,
            Err(e) => {
                self.messages
                    .error(t!("msg.dir_edit_list_failed", path = label, error = e).to_string());
                return;
            }
        };
        let content = dir_edit::render(&entries);
        let count = entries.len();
        let existing = self
            .editor
            .tabs
            .iter()
            .position(|t| t.path.as_deref() == Some(Path::new(Self::DIR_EDIT_PATH)));
        if let Some(i) = existing {
            self.editor.active = i;
        } else {
            self.editor.new_tab_with_content(&content);
        }
        if let Some(t) = self.editor.active_tab_mut() {
            t.path = Some(PathBuf::from(Self::DIR_EDIT_PATH));
            t.editor.set_content(&content);
            t.editor.set_cursor(0);
            t.dirty = false;
        }
        self.focus = Focus::Editor;
        self.dir_edit = Some(DirEditSession { dir, entries });
        self.dir_edit_confirm = None;
        self.status = t!("status.dir_edit_opened", path = label, count = count).to_string();
    }

    /// `Ctrl+S` on the listing: diff it and, when anything changed, open the
    /// confirm step; otherwise say why nothing happens.
    pub(super) fn prepare_dir_edit_confirm(&mut self) {
        let Some(session) = self.dir_edit.as_ref() else {
            return;
        };
        let Some(text) = self.editor.active_tab().map(crate::editor::Tab::text) else {
            return;
        };
        let plan = match dir_edit::diff(&session.entries, &text) {
            Ok(p) => p,
            Err(e) => {
                self.status = diff_error_text(&e);
                return;
            }
        };
        if plan.is_empty() {
            self.status = t!("status.dir_edit_no_changes").to_string();
            return;
        }
        let tag = format!("vix-dir-edit-{}", std::process::id());
        let steps = dir_edit::schedule(&session.entries, &plan, &tag);
        let dir_label = self.dir_edit_label(&session.dir);
        let lines = self.dir_edit_preview(&plan);
        self.dir_edit_confirm = Some(DirEditConfirm {
            dir_label,
            count: plan.len(),
            lines,
            scroll: 0,
            steps,
        });
    }

    /// One preview row per change: renames, then creates, then deletes
    /// (worded as trash or permanent delete, per `explorer_delete`).
    fn dir_edit_preview(&self, plan: &Plan) -> Vec<String> {
        let slash = |dir: bool| if dir { "/" } else { "" };
        let delete_key = if self.explorer_delete_uses_trash() {
            "ui.dir_edit_trash"
        } else {
            "ui.dir_edit_delete"
        };
        let renames = plan
            .renames
            .iter()
            .map(|(from, to)| t!("ui.dir_edit_rename", from = from, to = to).to_string());
        let creates = plan.creates.iter().map(|(name, dir)| {
            let name = format!("{name}{}", slash(*dir));
            t!("ui.dir_edit_create", name = name).to_string()
        });
        let deletes = plan.deletes.iter().map(|e| {
            let name = format!("{}{}", e.name, slash(e.is_dir));
            t!(delete_key, name = name).to_string()
        });
        renames.chain(creates).chain(deletes).collect()
    }

    /// Run a confirmed plan's steps in order, stopping at the first failure,
    /// keep open buffers following renamed files (and close deleted ones),
    /// then re-list the directory into the buffer.
    fn apply_dir_edit_confirm(&mut self) {
        let Some(confirm) = self.dir_edit_confirm.take() else {
            return;
        };
        let Some(dir) = self.dir_edit.as_ref().map(|s| s.dir.clone()) else {
            return;
        };
        let auth = crate::authority::current();
        let canon_dir = auth.canonicalize(&dir).unwrap_or_else(|_| dir.clone());
        let mut done = 0usize;
        for step in &confirm.steps {
            if let Err((path, e)) = self.run_dir_edit_step(&dir, &canon_dir, step) {
                self.messages
                    .error(t!("msg.dir_edit_failed", path = path.display(), error = e).to_string());
                break;
            }
            done += 1;
        }
        let complete = done == confirm.steps.len();
        self.explorer.rebuild();
        self.open_dir_edit_refresh(&dir);
        self.status = if complete {
            t!("status.dir_edit_applied", count = confirm.count).to_string()
        } else {
            t!("status.dir_edit_stopped").to_string()
        };
    }

    /// Re-list `dir` into the listing buffer after applying, so its ids match
    /// the directory again and saving once more does nothing.
    fn open_dir_edit_refresh(&mut self, dir: &Path) {
        let Ok(entries) = Self::list_dir_for_edit(dir) else {
            return;
        };
        let content = dir_edit::render(&entries);
        if let Some(t) = self
            .editor
            .tabs
            .iter_mut()
            .find(|t| t.path.as_deref() == Some(Path::new(Self::DIR_EDIT_PATH)))
        {
            let cursor = t.editor.get_cursor();
            t.editor.set_content(&content);
            t.editor.set_cursor(cursor.min(content.chars().count()));
            t.dirty = false;
        }
        self.dir_edit = Some(DirEditSession {
            dir: dir.to_path_buf(),
            entries,
        });
    }

    /// Run one step under `dir`. Never overwrites: a rename or create whose
    /// target already exists fails instead (a rename that only changes the
    /// case of a name on a case-insensitive filesystem is allowed).
    fn run_dir_edit_step(
        &mut self,
        dir: &Path,
        canon_dir: &Path,
        step: &Step,
    ) -> Result<(), (PathBuf, std::io::Error)> {
        let auth = crate::authority::current();
        let exists_err = |p: &Path| {
            (
                p.to_path_buf(),
                std::io::Error::from(std::io::ErrorKind::AlreadyExists),
            )
        };
        match step {
            Step::Delete { name, .. } => {
                let path = dir.join(name);
                let result = if self.explorer_delete_uses_trash() {
                    auth.trash(&path)
                } else {
                    auth.remove_path(&path)
                };
                result.map_err(|e| (path, e))?;
                self.close_buffers_under(&canon_dir.join(name));
            }
            Step::Rename { from, to } => {
                let src = dir.join(from);
                let dst = dir.join(to);
                if auth.symlink_metadata(&dst).is_ok() {
                    let same = auth.canonicalize(&src).ok() == auth.canonicalize(&dst).ok();
                    if !same {
                        return Err(exists_err(&dst));
                    }
                }
                auth.rename(&src, &dst).map_err(|e| (src, e))?;
                self.relocate_buffers(&canon_dir.join(from), &canon_dir.join(to));
            }
            Step::Create { name, is_dir } => {
                let path = dir.join(name);
                if auth.symlink_metadata(&path).is_ok() {
                    return Err(exists_err(&path));
                }
                let result = if *is_dir {
                    auth.create_dir_all(&path)
                } else {
                    auth.write(&path, b"")
                };
                result.map_err(|e| (path, e))?;
            }
        }
        Ok(())
    }

    /// Keys in the confirm step: `y`/Enter applies, `n`/Esc cancels, arrows
    /// scroll the preview.
    pub(super) fn dir_edit_confirm_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y' | 'Y') | KeyCode::Enter => self.apply_dir_edit_confirm(),
            KeyCode::Char('n' | 'N') | KeyCode::Esc => {
                self.dir_edit_confirm = None;
                self.status = t!("status.dir_edit_cancelled").to_string();
            }
            KeyCode::Up => {
                if let Some(c) = self.dir_edit_confirm.as_mut() {
                    c.scroll = c.scroll.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if let Some(c) = self.dir_edit_confirm.as_mut() {
                    c.scroll = (c.scroll + 1).min(c.lines.len().saturating_sub(1));
                }
            }
            _ => {}
        }
    }
}

/// The status line for a listing that can't be applied.
fn diff_error_text(e: &DiffError) -> String {
    match e {
        DiffError::DuplicateId { id, line } => {
            t!("status.dir_edit_duplicate_id", id = id, line = line).to_string()
        }
        DiffError::UnknownId { id, line } => {
            t!("status.dir_edit_unknown_id", id = id, line = line).to_string()
        }
        DiffError::InvalidName { name, line } => {
            t!("status.dir_edit_invalid_name", name = name, line = line).to_string()
        }
        DiffError::DuplicateName { name } => {
            t!("status.dir_edit_duplicate_name", name = name).to_string()
        }
        _ => e.to_string(),
    }
}
