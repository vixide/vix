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

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

use crate::geo::Geo;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
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

/// A request to the backup worker thread (T859).
enum WorkerMsg {
    /// Write a snapshot of a buffer.
    Write(WriteJob),
    /// Delete a backup, after every write queued before it.
    Remove {
        /// Where the backup lives.
        store: BackupStore,
        /// Which backup.
        key: OwnedKey,
        /// The backup file, as `store.path_of(key)`.
        path: PathBuf,
        /// This request's number.
        seq: u64,
    },
    /// Answer once every earlier request is done.
    Sync(Sender<()>),
}

/// One buffer's snapshot, written by the worker.
struct WriteJob {
    /// Where the backup goes.
    store: BackupStore,
    /// Which backup.
    key: OwnedKey,
    /// The backup file, as `store.path_of(key)`.
    path: PathBuf,
    /// This request's number.
    seq: u64,
    /// The text: an O(1) clone of the buffer's rope, sharing its chunks.
    rope: ropey::Rope,
    /// Set when a newer request for the same backup supersedes this one.
    cancel: Arc<AtomicBool>,
}

/// What the worker reports back for a write or a removal.
struct Done {
    /// The backup file.
    path: PathBuf,
    /// The request's number.
    seq: u64,
    /// `Err` with the message of a failed write; a superseded write is
    /// not a failure.
    result: Result<(), String>,
}

/// The backup worker thread's handle: started on the first backup of a
/// buffer over [`LARGE_BYTES`], ending when the app is dropped.
#[derive(Debug)]
struct BackupWorker {
    /// Requests, handled in order.
    requests: Sender<WorkerMsg>,
    /// Outcomes, read on the UI thread by [`App::drain_backup_results`].
    done: Receiver<Done>,
}

/// The worker loop: handle each request in order until the app hangs up.
fn backup_worker_loop(requests: &Receiver<WorkerMsg>, done: &Sender<Done>) {
    while let Ok(msg) = requests.recv() {
        let report = match msg {
            WorkerMsg::Write(job) => {
                let result =
                    job.store
                        .write_chunks(job.key.as_key(), job.rope.chunks(), &job.cancel);
                Done {
                    path: job.path,
                    seq: job.seq,
                    result: match result {
                        Err(e) if e.kind() != std::io::ErrorKind::Interrupted => Err(e.to_string()),
                        _ => Ok(()),
                    },
                }
            }
            WorkerMsg::Remove {
                store,
                key,
                path,
                seq,
            } => {
                store.remove(key.as_key());
                Done {
                    path,
                    seq,
                    result: Ok(()),
                }
            }
            WorkerMsg::Sync(reply) => {
                let _ = reply.send(());
                continue;
            }
        };
        // The app is gone: nothing reads outcomes, but finish the queue.
        let _ = done.send(report);
    }
}

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
    /// A copy of a file from before an in-place save that never finished
    /// (T663, T865): the recovery directory's record.
    InterruptedSave(vix_fileops::Recovery),
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
    /// The worker thread that writes large buffers' backups (T859).
    worker: Option<BackupWorker>,
    /// Backup files with a request still queued or running on the worker:
    /// the newest request's number, and its cancel flag (set when a newer
    /// one supersedes it). A backup listed here is only touched through the
    /// worker, so the requests for it land in the order they were made.
    in_flight: HashMap<PathBuf, (u64, Arc<AtomicBool>)>,
    /// The number of the last worker request.
    seq: u64,
}

impl HotExitState {
    /// Make the next idle pass due at once, as if [`PASS_INTERVAL`] had
    /// already elapsed: a test settles the pass this way instead of sleeping
    /// or depending on how slow its frames happen to be (T773).
    #[cfg(test)]
    pub(crate) fn make_pass_due(&mut self) {
        self.last_pass = None;
    }
}

/// A buffer's backup key, owned.
#[derive(Debug, Clone)]
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
        self.drain_backup_results();
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

    /// The backup worker's request channel, starting the thread on first
    /// use; `None` when no thread can be started.
    fn backup_worker(&mut self) -> Option<Sender<WorkerMsg>> {
        if self.hot_exit.worker.is_none() {
            let (requests, inbox) = std::sync::mpsc::channel();
            let (outbox, done) = std::sync::mpsc::channel();
            std::thread::Builder::new()
                .name("vix-hot-exit".to_string())
                .spawn(move || backup_worker_loop(&inbox, &outbox))
                .ok()?;
            self.hot_exit.worker = Some(BackupWorker { requests, done });
        }
        self.hot_exit.worker.as_ref().map(|w| w.requests.clone())
    }

    /// The next worker request number, and `path` marked in flight under
    /// it with a fresh cancel flag; an older request for `path` is
    /// cancelled (T859: a newer snapshot supersedes it).
    fn next_backup_request(&mut self, path: &Path) -> (u64, Arc<AtomicBool>) {
        let seq = self.hot_exit.seq.wrapping_add(1);
        self.hot_exit.seq = seq;
        let cancel = Arc::new(AtomicBool::new(false));
        if let Some((_, older)) = self
            .hot_exit
            .in_flight
            .insert(path.to_path_buf(), (seq, Arc::clone(&cancel)))
        {
            older.store(true, Ordering::Relaxed);
        }
        (seq, cancel)
    }

    /// The worker hung up (it can only have panicked): forget it, so the
    /// next backup starts a new one, and say whether `msg` was sent.
    fn send_to_backup_worker(&mut self, requests: &Sender<WorkerMsg>, msg: WorkerMsg) -> bool {
        if requests.send(msg).is_ok() {
            return true;
        }
        self.hot_exit.worker = None;
        self.hot_exit.in_flight.clear();
        false
    }

    /// Hand a snapshot of a buffer to the worker. `false` when there is
    /// no worker, so the caller writes it here instead.
    fn queue_backup_write(
        &mut self,
        store: &BackupStore,
        key: OwnedKey,
        path: PathBuf,
        rope: ropey::Rope,
    ) -> bool {
        let Some(requests) = self.backup_worker() else {
            return false;
        };
        let (seq, cancel) = self.next_backup_request(&path);
        let job = WriteJob {
            store: store.clone(),
            key,
            path,
            seq,
            rope,
            cancel,
        };
        self.send_to_backup_worker(&requests, WorkerMsg::Write(job))
    }

    /// Delete the backup `key`: through the worker when it still has a
    /// request for it (cancelling that write and running after it), so a
    /// write in flight cannot bring a deleted backup back; else at once.
    fn remove_backup(&mut self, store: &BackupStore, key: OwnedKey) {
        let path = store.path_of(key.as_key());
        self.drain_backup_results();
        if self.hot_exit.in_flight.contains_key(&path)
            && let Some(requests) = self.backup_worker()
        {
            let (seq, _) = self.next_backup_request(&path);
            let msg = WorkerMsg::Remove {
                store: store.clone(),
                key,
                path: path.clone(),
                seq,
            };
            if !self.send_to_backup_worker(&requests, msg) {
                // No worker left to race with: delete it here, as
                // `BackupStore::remove` does.
                let _ = std::fs::remove_file(&path);
            }
            return;
        }
        store.remove(key.as_key());
    }

    /// Wait until the worker has handled every request so far, then take
    /// its outcomes. Free when nothing is in flight.
    fn settle_backups(&mut self) {
        self.drain_backup_results();
        if self.hot_exit.in_flight.is_empty() {
            return;
        }
        if let Some(requests) = self.hot_exit.worker.as_ref().map(|w| w.requests.clone()) {
            let (reply, answered) = std::sync::mpsc::channel();
            if self.send_to_backup_worker(&requests, WorkerMsg::Sync(reply)) {
                let _ = answered.recv();
            }
        }
        self.drain_backup_results();
    }

    /// [`App::settle_backups`] when `path` has a request in flight: a read
    /// of that backup must see the newest write.
    fn settle_backup(&mut self, path: &Path) {
        self.drain_backup_results();
        if self.hot_exit.in_flight.contains_key(path) {
            self.settle_backups();
        }
    }

    /// Take the worker's outcomes: a request that was the newest for its
    /// backup is no longer in flight, and a failed newest write is reported
    /// (once a session) and leaves its buffer due for another backup.
    fn drain_backup_results(&mut self) {
        let Some(worker) = &self.hot_exit.worker else {
            return;
        };
        let mut failed = Vec::new();
        while let Ok(done) = worker.done.try_recv() {
            let newest = self
                .hot_exit
                .in_flight
                .get(&done.path)
                .is_some_and(|(seq, _)| *seq == done.seq);
            if !newest {
                continue;
            }
            self.hot_exit.in_flight.remove(&done.path);
            if let Err(e) = done.result {
                failed.push((done.path, e));
            }
        }
        if failed.is_empty() {
            return;
        }
        let store = self.backup_store();
        let workspace = self.session_key();
        for (path, error) in failed {
            self.backup_failed(&error);
            let Some(store) = &store else {
                continue;
            };
            for idx in 0..self.editor.tabs.len() {
                let Some(key) = self.tab_backup_key(idx, &workspace, false) else {
                    continue;
                };
                if store.path_of(key.as_key()) == path
                    && let Some(tab) = self.editor.tabs.get_mut(idx)
                {
                    tab.backup.revision = None;
                }
            }
        }
    }

    /// Report a failed backup write, once a session.
    fn backup_failed(&mut self, error: &dyn std::fmt::Display) {
        if !self.hot_exit.warned {
            self.hot_exit.warned = true;
            self.messages
                .warn(t!("msg.hot_exit_failed", error = error).to_string());
        }
    }

    /// Back up every eligible buffer whose text changed since its last
    /// backup. `force` (on quit) ignores the large-buffer cadence. A buffer
    /// over [`LARGE_BYTES`] (or one whose backup the worker is still
    /// writing) is snapshotted, an O(1) rope clone, and written by the
    /// worker thread (T859); a smaller one is written here.
    fn backup_dirty_tabs(&mut self, force: bool) {
        self.drain_backup_results();
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
            let code = tab.editor.code_ref();
            let path = store.path_of(key.as_key());
            let snapshot = (code.len_bytes() > LARGE_BYTES
                || self.hot_exit.in_flight.contains_key(&path))
            .then(|| code.rope().clone());
            let queued = snapshot
                .is_some_and(|rope| self.queue_backup_write(&store, key.clone(), path, rope));
            let written = if queued {
                Ok(())
            } else {
                // Copying the text is the cost T749 cares about; it happens
                // only for a buffer that changed since its last backup.
                match self.editor.tabs.get(idx) {
                    Some(tab) => store.write(key.as_key(), &tab.text()),
                    None => continue,
                }
            };
            match written {
                Ok(()) => {
                    if let Some(tab) = self.editor.tabs.get_mut(idx) {
                        tab.backup.revision = Some(revision);
                        tab.backup.written_at = Some(Instant::now());
                    }
                }
                Err(e) => self.backup_failed(&e),
            }
        }
    }

    /// On quit: back up whatever is still unsaved (a forced quit, or a
    /// front end that exits without asking), and wait until the worker has
    /// written every backup it was given.
    pub(super) fn flush_hot_exit(&mut self) {
        if self.settings.startup.hot_exit {
            self.backup_dirty_tabs(true);
        }
        self.settle_backups();
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
            self.remove_backup(&store, key);
        }
        if let Some(prior) = prior {
            let identity = self.backup_identity(&prior);
            self.remove_backup(&store, OwnedKey::File(identity));
        }
        if let Some(tab) = self.editor.tabs.get_mut(idx) {
            let id = tab.backup.session_id.take();
            tab.backup = BackupMark::default();
            if let Some(id) = id {
                self.remove_backup(&store, OwnedKey::Untitled(workspace, id));
            }
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
        self.settle_backup(&store.path_of(key));
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
            _ => self.remove_backup(&store, OwnedKey::File(identity)),
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

    /// Show the front recovery offer when nothing else is asking. Also run
    /// when the diff its `d` opened closes, so the offer is back at once
    /// (T868) rather than on the next tick.
    pub(super) fn show_next_recovery(&mut self) {
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
                Recovery::InterruptedSave(rec) => {
                    // Gone or no longer readable (another Vix acted on it).
                    let Ok(record) = rec.read() else {
                        self.hot_exit.pending.pop_front();
                        continue;
                    };
                    match &record.original {
                        Some(path) => t!(
                            "confirm.interrupted_save",
                            path = printable(&path.display().to_string())
                        )
                        .to_string(),
                        None => t!(
                            "confirm.interrupted_save_unknown",
                            path = printable(&record.recorded)
                        )
                        .to_string(),
                    }
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
            KeyCode::Char('d' | 'D') => match &front {
                Recovery::File { path, identity } => {
                    self.confirm = None;
                    self.show_backup_diff(path, identity);
                }
                Recovery::InterruptedSave(rec) => {
                    self.confirm = None;
                    self.show_interrupted_save_diff(rec);
                }
                Recovery::Untitled(_) => {}
            },
            KeyCode::Char('x' | 'X') => {
                self.confirm = None;
                self.hot_exit.pending.pop_front();
                self.discard_backup(&front);
                if !matches!(front, Recovery::InterruptedSave(_)) {
                    self.status = t!("status.hot_exit_discarded").to_string();
                }
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
        if let Recovery::InterruptedSave(rec) = &item {
            self.restore_interrupted_save(rec);
            return;
        }
        let Some(store) = self.backup_store() else {
            return;
        };
        match item {
            Recovery::File { path, identity } => {
                self.settle_backup(&store.path_of(BackupKey::File(&identity)));
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
                    self.settle_backup(&store.path_of(key));
                    let Some(backup) = store.read(key) else {
                        continue;
                    };
                    self.editor.new_tab_with_content(&backup.text);
                    if let Some(tab) = self.editor.active_tab_mut() {
                        tab.backup.revision = Some(tab.editor.revision());
                        tab.backup.written_at = Some(Instant::now());
                        tab.backup.session_id = Some(id);
                    }
                    count = count.plus(1);
                }
                self.status = t!("status.hot_exit_recovered_untitled", count = count).to_string();
            }
            Recovery::InterruptedSave(_) => {}
        }
    }

    /// Delete the backups behind `item`.
    fn discard_backup(&mut self, item: &Recovery) {
        if let Recovery::InterruptedSave(rec) = item {
            self.status = match rec.discard() {
                Ok(()) => t!("status.interrupted_save_discarded").to_string(),
                Err(e) => t!("status.interrupted_save_failed", error = e.to_string()).to_string(),
            };
            return;
        }
        let Some(store) = self.backup_store() else {
            return;
        };
        match item {
            Recovery::File { identity, .. } => {
                self.remove_backup(&store, OwnedKey::File(identity.clone()));
            }
            Recovery::Untitled(ids) => {
                let workspace = self.session_key();
                for id in ids {
                    self.remove_backup(&store, OwnedKey::Untitled(workspace.clone(), id.clone()));
                }
            }
            Recovery::InterruptedSave(_) => {}
        }
    }

    /// Queue the interrupted saves recorded in `dir` for the recovery
    /// prompt (T865), each once.
    pub(super) fn offer_interrupted_saves(&mut self, dir: &Path) {
        for rec in vix_fileops::pending_recoveries(dir) {
            let item = Recovery::InterruptedSave(rec);
            if !self.hot_exit.pending.contains(&item) {
                self.hot_exit.pending.push_back(item);
            }
        }
        self.show_next_recovery();
    }

    /// Restore an interrupted save's copy. A record that names its file
    /// exactly is written back over it, byte for byte, and clean tabs reload.
    /// One that cannot (a legacy record whose path is ambiguous, or one from
    /// another platform) is never written to a guessed path: its text opens
    /// in a new, unsaved buffer, so the person chooses where it goes with
    /// Save As. Contents that are not UTF-8 cannot become a buffer, so that
    /// record stays in the recovery directory.
    fn restore_interrupted_save(&mut self, rec: &vix_fileops::Recovery) {
        let record = match rec.read() {
            Ok(record) => record,
            Err(e) => {
                self.status =
                    t!("status.interrupted_save_failed", error = e.to_string()).to_string();
                return;
            }
        };
        if let Some(path) = &record.original {
            self.status = match rec.restore() {
                Ok(()) => {
                    self.editor.reload_clean_from_disk();
                    t!(
                        "status.interrupted_save_restored",
                        path = printable(&path.display().to_string())
                    )
                    .to_string()
                }
                Err(e) => t!("status.interrupted_save_failed", error = e.to_string()).to_string(),
            };
            return;
        }
        let Ok(text) = String::from_utf8(record.contents) else {
            let dir = rec
                .copy
                .parent()
                .map_or_else(String::new, |d| d.display().to_string());
            self.status = t!("status.interrupted_save_not_text", dir = dir).to_string();
            return;
        };
        self.editor.new_tab_with_content(&text);
        if let Some(tab) = self.editor.active_tab_mut() {
            tab.dirty = true;
        }
        let _ = rec.discard();
        self.status = t!("status.interrupted_save_opened").to_string();
    }

    /// Show the file on disk against an interrupted save's copy in the diff
    /// overlay (an empty left side when the file cannot be read).
    fn show_interrupted_save_diff(&mut self, rec: &vix_fileops::Recovery) {
        let Ok(record) = rec.read() else {
            return;
        };
        let current = record
            .original
            .as_deref()
            .and_then(|p| std::fs::read(p).ok())
            .map_or_default(|b| String::from_utf8_lossy(&b).into_owned());
        let copy = String::from_utf8_lossy(&record.contents);
        let lines = crate::diff_view::build(&current, &copy);
        let name = record
            .original
            .as_deref()
            .and_then(Path::file_name)
            .map_or_else(
                || record.recorded.clone(),
                |n| n.to_string_lossy().into_owned(),
            );
        self.diff_view = Some(super::DiffViewState {
            title: t!("ui.interrupted_save_diff", name = printable(&name)).to_string(),
            lines,
            scroll: 0,
        });
    }

    /// Show the file as opened against its backup in the diff overlay.
    fn show_backup_diff(&mut self, path: &Path, identity: &str) {
        if let Some(store) = self.backup_store() {
            self.settle_backup(&store.path_of(BackupKey::File(identity)));
        }
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

/// `text` with control characters (a newline in a file name) escaped, so a
/// prompt or status line stays on one line.
fn printable(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            let escaped: Vec<char> = if c.is_control() {
                c.escape_default().collect()
            } else {
                vec![c]
            };
            escaped
        })
        .collect()
}

#[cfg(test)]
#[path = "tests_r3_hot_exit.rs"]
mod r3_tests;
