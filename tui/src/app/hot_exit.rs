//! Hot exit (T629): back up unsaved buffers while idle and on quit, and offer
//! a backup back — with a diff, never applied on its own — when its file is
//! next opened (or, for untitled buffers, when the workspace next starts).
//!
//! The store and its file format live in `vix_undo_store::hot_exit`; this
//! module decides *when* to write (an idle pass every [`PASS_INTERVAL`] that
//! writes only buffers whose text changed since their last backup, large
//! buffers at most every [`LARGE_INTERVAL`]), *what* key a buffer has (its
//! local path, its `ssh://` name in a remote workspace, or an untitled
//! buffer's session id), and the recovery prompt.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use vix_input::{KeyCode, KeyEvent};
use vix_undo_store::hot_exit::{self, BackupKey, BackupStore};

use super::{App, Confirm, ConfirmKind};
use crate::editor::{BackupMark, Tab};

/// How often the idle pass looks for buffers to back up.
const PASS_INTERVAL: Duration = Duration::from_secs(2);

/// Buffers larger than this many bytes are backed up at most every
/// [`LARGE_INTERVAL`] (T749: a 100 MB buffer is not copied every pass).
const LARGE_BYTES: usize = 4 * 1024 * 1024;

/// The slowest backup cadence, for buffers over [`LARGE_BYTES`].
const LARGE_INTERVAL: Duration = Duration::from_secs(60);

/// One backup waiting to be offered back.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Recovery {
    /// A backup of the open file at `path`, stored under `identity`.
    File {
        /// The tab's path.
        path: PathBuf,
        /// The backup's identity.
        identity: String,
    },
    /// The workspace's untitled backups, by session id.
    Untitled(Vec<String>),
}

/// Hot-exit state held by [`App`].
#[derive(Debug, Default)]
pub(crate) struct HotExitState {
    /// Where backups go; `None` derives it (see [`App::backup_store`]).
    dir: Option<PathBuf>,
    /// When the last idle pass ran.
    last_pass: Option<Instant>,
    /// Backups still to offer, the front one shown first.
    pending: VecDeque<Recovery>,
    /// A failed write was already reported this session.
    warned: bool,
}

/// A buffer's backup key, owned.
enum OwnedKey {
    /// A buffer with a file: its identity.
    File(String),
    /// An untitled buffer: workspace key and session id.
    Untitled(String, String),
}

impl OwnedKey {
    /// The borrowed form the store takes.
    fn as_key(&self) -> BackupKey<'_> {
        match self {
            Self::File(identity) => BackupKey::File(identity),
            Self::Untitled(workspace, id) => BackupKey::Untitled { workspace, id },
        }
    }
}

impl App {
    /// Keep this app's hot-exit backups in `dir` instead of the user's
    /// config directory (tests, embedders).
    #[must_use]
    pub fn with_backup_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.hot_exit.dir = Some(dir.into());
        self
    }

    /// The backup store: the explicit directory, else `backup/` beside an
    /// overridden settings file, else beside an overridden session file,
    /// else `<config>/backup/`. Deriving it from the overrides keeps a run
    /// pointed at another config (every test) out of the user's.
    pub(super) fn backup_store(&self) -> Option<BackupStore> {
        if let Some(dir) = &self.hot_exit.dir {
            return Some(BackupStore::at(dir.clone()));
        }
        if let Some(parent) = self.settings_path.as_ref().and_then(|p| p.parent()) {
            return Some(BackupStore::at(parent.join("backup")));
        }
        if let Some(session) = &self.session_path {
            return Some(BackupStore::at(session.with_extension("backup")));
        }
        BackupStore::default_location()
    }

    /// The identity a file's backup is keyed by: its `ssh://` name in a
    /// remote workspace (backups always stay local), else its canonical path.
    fn backup_identity(&self, path: &Path) -> String {
        self.remote_identity(path)
            .unwrap_or_else(|| hot_exit::local_identity(path))
    }

    /// Whether `tab` holds unsaved text hot exit should keep: dirty, text,
    /// owned here (not a Cowork guest's buffer) and editable.
    fn backup_eligible(tab: &Tab) -> bool {
        tab.dirty
            && !tab.is_image()
            && !tab.is_remote()
            && !tab.read_only
            && tab.path.as_deref() != Some(Path::new(Self::WGREP_RESULTS_PATH))
    }

    /// The key of the tab at `idx`, giving an untitled tab its session id
    /// when it has none yet (`assign`).
    fn tab_backup_key(&mut self, idx: usize, workspace: &str, assign: bool) -> Option<OwnedKey> {
        let path = self.editor.tabs.get(idx)?.path.clone();
        if let Some(path) = path {
            return Some(OwnedKey::File(self.backup_identity(&path)));
        }
        let tab = self.editor.tabs.get_mut(idx)?;
        if tab.backup.session_id.is_none() && assign {
            tab.backup.session_id = Some(hot_exit::new_session_id());
        }
        let id = tab.backup.session_id.clone()?;
        Some(OwnedKey::Untitled(workspace.to_owned(), id))
    }

    /// The idle half of hot exit, run every tick: back up changed buffers
    /// (at most every 2 seconds) and show the next recovery offer.
    pub fn poll_hot_exit(&mut self) {
        self.show_next_recovery();
        if !self.settings.startup.hot_exit {
            return;
        }
        if self
            .hot_exit
            .last_pass
            .is_some_and(|t| t.elapsed() < PASS_INTERVAL)
        {
            return;
        }
        self.hot_exit.last_pass = Some(Instant::now());
        self.backup_dirty_tabs(false);
    }

    /// Back up every eligible buffer whose text changed since its last
    /// backup. `force` (on quit) ignores the large-buffer cadence.
    fn backup_dirty_tabs(&mut self, force: bool) {
        let due: Vec<usize> = self
            .editor
            .tabs
            .iter()
            .enumerate()
            .filter(|(_, t)| Self::backup_eligible(t))
            .filter(|(_, t)| t.backup.revision != Some(t.editor.revision()))
            .filter(|(_, t)| {
                force
                    || t.editor.code_ref().len_bytes() <= LARGE_BYTES
                    || t.backup
                        .written_at
                        .is_none_or(|at| at.elapsed() >= LARGE_INTERVAL)
            })
            .map(|(i, _)| i)
            .collect();
        if due.is_empty() {
            return;
        }
        let Some(store) = self.backup_store() else {
            return;
        };
        let workspace = self.session_key();
        for idx in due {
            let Some(key) = self.tab_backup_key(idx, &workspace, true) else {
                continue;
            };
            let Some(tab) = self.editor.tabs.get(idx) else {
                continue;
            };
            let revision = tab.editor.revision();
            // Copying the text is the cost T749 cares about; it happens only
            // for a buffer that changed since its last backup.
            match store.write(key.as_key(), &tab.text()) {
                Ok(()) => {
                    if let Some(tab) = self.editor.tabs.get_mut(idx) {
                        tab.backup.revision = Some(revision);
                        tab.backup.written_at = Some(Instant::now());
                    }
                }
                Err(e) => {
                    if !self.hot_exit.warned {
                        self.hot_exit.warned = true;
                        self.messages
                            .warn(t!("msg.hot_exit_failed", error = e).to_string());
                    }
                }
            }
        }
    }

    /// On quit: back up whatever is still unsaved (a forced quit, or a
    /// front end that exits without asking).
    pub(super) fn flush_hot_exit(&mut self) {
        if self.settings.startup.hot_exit {
            self.backup_dirty_tabs(true);
        }
    }

    /// Delete the active tab's backup, under `prior` (its key before a Save
    /// As) as well as its current key, and forget the mark: the text is safe
    /// on disk now, or the user chose to throw it away.
    pub(super) fn drop_active_backup(&mut self, prior: Option<PathBuf>) {
        let Some(store) = self.backup_store() else {
            return;
        };
        let workspace = self.session_key();
        let idx = self.editor.active;
        if let Some(key) = self.tab_backup_key(idx, &workspace, false) {
            store.remove(key.as_key());
        }
        if let Some(prior) = prior {
            store.remove(BackupKey::File(&self.backup_identity(&prior)));
        }
        if let Some(tab) = self.editor.tabs.get_mut(idx) {
            if let Some(id) = tab.backup.session_id.take() {
                store.remove(BackupKey::Untitled {
                    workspace: &workspace,
                    id: &id,
                });
            }
            tab.backup = BackupMark::default();
        }
    }

    /// After `path` opened in a new tab: queue its backup for the recovery
    /// prompt when one exists and differs from the file. One that matches
    /// the file is stale (the edits were saved some other way) and is
    /// deleted. The buffer's text is copied only when a backup exists.
    pub(super) fn check_backup_on_open(&mut self, path: &Path) {
        if !self.settings.startup.hot_exit {
            return;
        }
        let Some(store) = self.backup_store() else {
            return;
        };
        let identity = self.backup_identity(path);
        let key = BackupKey::File(&identity);
        if !store.exists(key) {
            return;
        }
        let Some(tab) = self
            .editor
            .tabs
            .iter()
            .find(|t| t.path.as_deref() == Some(path))
        else {
            return;
        };
        match store.read(key) {
            Some(backup) if backup.text != tab.text() => {
                let item = Recovery::File {
                    path: path.to_path_buf(),
                    identity,
                };
                if !self.hot_exit.pending.contains(&item) {
                    self.hot_exit.pending.push_back(item);
                }
                self.show_next_recovery();
            }
            _ => store.remove(key),
        }
    }

    /// At startup: queue this workspace's untitled backups for one prompt.
    pub fn offer_untitled_recovery(&mut self) {
        if !self.settings.startup.hot_exit {
            return;
        }
        let Some(store) = self.backup_store() else {
            return;
        };
        // An untitled buffer already open (recovered earlier) is not offered.
        let open: Vec<&str> = self
            .editor
            .tabs
            .iter()
            .filter_map(|t| t.backup.session_id.as_deref())
            .collect();
        let ids: Vec<String> = store
            .untitled_ids(&self.session_key())
            .into_iter()
            .filter(|id| !open.contains(&id.as_str()))
            .collect();
        if !ids.is_empty() {
            self.hot_exit.pending.push_back(Recovery::Untitled(ids));
            self.show_next_recovery();
        }
    }

    /// Show the front recovery offer when nothing else is asking.
    fn show_next_recovery(&mut self) {
        if self.confirm.is_some()
            || self.diff_view.is_some()
            || self.unsaved.is_some()
            || self.script_trust.is_some()
        {
            return;
        }
        while let Some(front) = self.hot_exit.pending.front() {
            let message = match front {
                Recovery::File { path, .. } => {
                    // The tab was closed before the offer came up: offer it
                    // again on the next open instead.
                    if !self
                        .editor
                        .tabs
                        .iter()
                        .any(|t| t.path.as_deref() == Some(path.as_path()))
                    {
                        self.hot_exit.pending.pop_front();
                        continue;
                    }
                    t!("confirm.hot_exit_file", path = path.display()).to_string()
                }
                Recovery::Untitled(ids) => {
                    t!("confirm.hot_exit_untitled", count = ids.len()).to_string()
                }
            };
            self.confirm = Some(Confirm {
                message,
                paths: Vec::new(),
                kind: ConfirmKind::RecoverBackup,
            });
            return;
        }
    }

    /// Keys for the recovery prompt: `r` recovers, `d` shows the diff (the
    /// prompt comes back when it closes), `x` discards the backup, `n`/Esc
    /// decides later (the backup stays). Returns whether the prompt was open.
    pub(super) fn hot_exit_confirm_key(&mut self, key: KeyEvent) -> bool {
        if self.confirm.as_ref().map(|c| c.kind) != Some(ConfirmKind::RecoverBackup) {
            return false;
        }
        let Some(front) = self.hot_exit.pending.front().cloned() else {
            self.confirm = None;
            return true;
        };
        match key.code {
            KeyCode::Char('r' | 'R') => {
                self.confirm = None;
                self.hot_exit.pending.pop_front();
                self.recover(front);
            }
            KeyCode::Char('d' | 'D') => {
                if let Recovery::File { path, identity } = &front {
                    self.confirm = None;
                    self.show_backup_diff(path, identity);
                }
            }
            KeyCode::Char('x' | 'X') => {
                self.confirm = None;
                self.hot_exit.pending.pop_front();
                self.discard_backup(&front);
                self.status = t!("status.hot_exit_discarded").to_string();
            }
            KeyCode::Char('n' | 'N') | KeyCode::Esc => {
                self.confirm = None;
                self.hot_exit.pending.pop_front();
                self.status = t!("status.hot_exit_later").to_string();
            }
            _ => {}
        }
        self.show_next_recovery();
        true
    }

    /// Put a backup back: a file's text into its tab, untitled buffers into
    /// new tabs. Either way the buffer is left dirty — nothing is saved.
    fn recover(&mut self, item: Recovery) {
        let Some(store) = self.backup_store() else {
            return;
        };
        match item {
            Recovery::File { path, identity } => {
                let Some(backup) = store.read(BackupKey::File(&identity)) else {
                    return;
                };
                let Some(i) = self
                    .editor
                    .tabs
                    .iter()
                    .position(|t| t.path.as_deref() == Some(path.as_path()))
                else {
                    return;
                };
                self.editor.active = i;
                if let Some(tab) = self.editor.tabs.get_mut(i) {
                    let caret = tab.editor.get_cursor().min(backup.text.chars().count());
                    tab.editor.set_content(&backup.text);
                    tab.editor.set_selection(None);
                    tab.editor.set_cursor(caret);
                    tab.dirty = true;
                    tab.preview = false;
                    tab.backup.revision = Some(tab.editor.revision());
                    tab.backup.written_at = Some(Instant::now());
                }
                self.status = t!("status.hot_exit_recovered").to_string();
            }
            Recovery::Untitled(ids) => {
                let workspace = self.session_key();
                let mut count = 0usize;
                for id in ids {
                    let key = BackupKey::Untitled {
                        workspace: &workspace,
                        id: &id,
                    };
                    let Some(backup) = store.read(key) else {
                        continue;
                    };
                    self.editor.new_tab_with_content(&backup.text);
                    if let Some(tab) = self.editor.active_tab_mut() {
                        tab.backup.revision = Some(tab.editor.revision());
                        tab.backup.written_at = Some(Instant::now());
                        tab.backup.session_id = Some(id);
                    }
                    count += 1;
                }
                self.status = t!("status.hot_exit_recovered_untitled", count = count).to_string();
            }
        }
    }

    /// Delete the backups behind `item`.
    fn discard_backup(&self, item: &Recovery) {
        let Some(store) = self.backup_store() else {
            return;
        };
        match item {
            Recovery::File { identity, .. } => store.remove(BackupKey::File(identity)),
            Recovery::Untitled(ids) => {
                let workspace = self.session_key();
                for id in ids {
                    store.remove(BackupKey::Untitled {
                        workspace: &workspace,
                        id,
                    });
                }
            }
        }
    }

    /// Show the file as opened against its backup in the diff overlay.
    fn show_backup_diff(&mut self, path: &Path, identity: &str) {
        let Some(backup) = self
            .backup_store()
            .and_then(|s| s.read(BackupKey::File(identity)))
        else {
            return;
        };
        let Some(current) = self
            .editor
            .tabs
            .iter()
            .find(|t| t.path.as_deref() == Some(path))
            .map(Tab::text)
        else {
            return;
        };
        let lines = crate::diff_view::build(&current, &backup.text);
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        self.diff_view = Some(super::DiffViewState {
            title: t!("ui.hot_exit_diff", name = name).to_string(),
            lines,
            scroll: 0,
        });
    }

    /// Whether a recovery offer is open or waiting (tests, front ends).
    #[must_use]
    pub fn recovery_pending(&self) -> bool {
        !self.hot_exit.pending.is_empty()
    }
}
