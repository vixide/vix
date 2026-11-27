//! The fuzzy file finder's index, completed in the background (T845).
//!
//! [`App::build_file_index`] walks the first [`FILE_INDEX_FIRST_PASS`] files
//! on the spot, as the finder always has, so Quick Open lists results at
//! once. When that pass was full, a worker thread walks the whole workspace
//! (up to [`FILE_INDEX_CAP`]) through the same authority slot and posts the
//! list on a channel; [`App::poll_file_index`] (from [`App::tick`]) swaps it
//! in and re-filters an open palette. Owning spec:
//! `crates/vix-palette/spec/index.md` ("File index").

use std::path::PathBuf;
use std::sync::mpsc::{self, TryRecvError};

use super::App;
use crate::authority::{Authority, WalkCancel, WalkOptions};
use crate::geo::Geo;

/// Files walked synchronously before Quick Open shows anything: the old
/// whole-index cap, so first results cost what they did before T845.
pub(crate) const FILE_INDEX_FIRST_PASS: usize = 5_000;

/// The background index's safety cap. A keystroke re-filters every indexed
/// file, measured at about 0.1 ms per 1,000 files when every file matches
/// (`palette/quick_open/keystroke`, `docs/performance/index.md`), so this
/// keeps the worst keystroke inside the 16 ms frame budget; past it the
/// picker says the index was cut short.
pub(crate) const FILE_INDEX_CAP: usize = 150_000;

/// How complete `App::file_index` is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FileIndexState {
    /// Every file the walk options allow is indexed.
    #[default]
    Complete,
    /// Only the first pass is in; a background walk is finishing the rest.
    Building,
    /// The walk stopped at [`FILE_INDEX_CAP`] (or its worker died).
    CutShort,
}

/// A background walk in flight.
pub(crate) struct FileIndexJob {
    /// The finished list, sent once.
    rx: mpsc::Receiver<Vec<PathBuf>>,
    /// Cancels the walk when the job is replaced or dropped.
    cancel: WalkCancel,
    /// The workspace folders it walks.
    folders: Vec<PathBuf>,
}

impl Drop for FileIndexJob {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// Walk every folder in `folders` through `auth`, at most `limit` files in
/// all, stopping early once `cancel` is cancelled.
fn walk_folders(
    auth: &dyn Authority,
    folders: &[PathBuf],
    limit: usize,
    cancel: Option<&WalkCancel>,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for folder in folders {
        if out.len() >= limit || cancel.is_some_and(WalkCancel::is_cancelled) {
            break;
        }
        let mut opts = WalkOptions::file_index(limit.minus(out.len()));
        if let Some(c) = cancel {
            opts = opts.with_cancel(c.clone());
        }
        out.extend(auth.walk_files(folder, &opts));
    }
    out
}

impl App {
    /// Rebuild the project file index used by the fuzzy file finder.
    ///
    /// Walks through the authority slot (T710) with
    /// [`WalkOptions::file_index`]: `.gitignore`, `.ignore`, git's global
    /// excludes and hidden-file rules honored (the `ignore` crate, ripgrep's
    /// engine, locally), `target`/`node_modules` pruned, depth bounded.
    ///
    /// The first [`FILE_INDEX_FIRST_PASS`] files are walked now. If there
    /// may be more, a background walk finishes the index (T845); a later
    /// call while it runs for the same folders keeps it, and a call for
    /// other folders cancels it. When the index already holds a finished
    /// walk of these folders larger than the first pass, that list stays in
    /// place while the background walk refreshes it.
    pub(super) fn build_file_index(&mut self) {
        let folders = self.workspace_folders.clone();
        if self
            .file_index_job
            .as_ref()
            .is_some_and(|j| j.folders == folders)
        {
            return;
        }
        self.file_index_job = None; // cancels a walk of other folders
        let auth = crate::authority::current();
        let have_full = self.file_index_folders == folders
            && self.file_index.len() > FILE_INDEX_FIRST_PASS
            && self.file_index_state != FileIndexState::Building;
        if !have_full {
            self.file_index = walk_folders(auth.as_ref(), &folders, FILE_INDEX_FIRST_PASS, None);
            self.file_index_folders.clone_from(&folders);
            if self.file_index.len() < FILE_INDEX_FIRST_PASS {
                self.file_index_state = FileIndexState::Complete;
                return;
            }
            self.file_index_state = FileIndexState::Building;
        }
        let cancel = WalkCancel::new();
        let (tx, rx) = mpsc::channel();
        let (worker_cancel, worker_folders) = (cancel.clone(), folders.clone());
        std::thread::spawn(move || {
            let files = walk_folders(
                auth.as_ref(),
                &worker_folders,
                FILE_INDEX_CAP,
                Some(&worker_cancel),
            );
            if !worker_cancel.is_cancelled() {
                // The App dropped the job: nobody wants this list.
                let _ = tx.send(files);
            }
        });
        self.file_index_job = Some(FileIndexJob {
            rx,
            cancel,
            folders,
        });
    }

    /// Install a finished background index (T845). Called once per
    /// event-loop iteration; cheap when no walk is running. A list for
    /// folders the workspace no longer has is dropped.
    pub fn poll_file_index(&mut self) {
        let files = {
            let Some(job) = self.file_index_job.as_ref() else {
                return;
            };
            match job.rx.try_recv() {
                Ok(files) => files,
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    // The worker died: what the first pass found is all there is.
                    self.file_index_job = None;
                    if self.file_index_state == FileIndexState::Building {
                        self.file_index_state = FileIndexState::CutShort;
                    }
                    return;
                }
            }
        };
        let Some(job) = self.file_index_job.take() else {
            return;
        };
        if job.folders != self.workspace_folders {
            return;
        }
        self.file_index_state = if files.len() >= FILE_INDEX_CAP {
            FileIndexState::CutShort
        } else {
            FileIndexState::Complete
        };
        self.file_index = files;
        self.file_index_folders.clone_from(&job.folders);
        if self.palette.is_some() {
            self.recompute_palette();
        }
        // An open project search ran over the first pass only: search again
        // now that every file is listed, or its hits stay silently partial.
        if self.workspace_search.is_some() {
            self.run_workspace_search();
        }
    }

    /// How many files Quick Open can find right now.
    #[must_use]
    pub fn file_index_len(&self) -> usize {
        self.file_index.len()
    }

    /// Whether a background file-index walk is running.
    #[must_use]
    pub fn file_index_busy(&self) -> bool {
        self.file_index_job.is_some()
    }

    /// Forget the index's folders and cancel a walk in flight, so the next
    /// [`App::build_file_index`] walks afresh (a re-rooted workspace, whose
    /// authority may differ even where a path is the same).
    pub(super) fn reset_file_index(&mut self) {
        self.file_index_job = None;
        self.file_index_folders.clear();
        self.file_index_state = FileIndexState::Complete;
    }

    /// The file finder's note on an incomplete index (T845): still building,
    /// or cut short at the cap. `None` when the index is complete, or the
    /// palette is not listing the project's files.
    #[must_use]
    pub(crate) fn file_index_notice(&self) -> Option<String> {
        let files_mode = self
            .palette
            .as_ref()
            .is_some_and(|p| p.mode() == crate::palette::Mode::Files);
        if !files_mode || self.palette_files_override.is_some() {
            return None;
        }
        let n = self.file_index.len();
        match self.file_index_state {
            FileIndexState::Complete => None,
            FileIndexState::Building => Some(t!("ui.palette_indexing", n = n).to_string()),
            FileIndexState::CutShort => Some(t!("ui.palette_index_cut_short", n = n).to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FILE_INDEX_CAP, FILE_INDEX_FIRST_PASS, FileIndexJob, FileIndexState};
    use crate::app::App;
    use crate::authority::WalkCancel;
    use crate::settings::Settings;
    use crate::workspace_search::WorkspaceSearch;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;

    /// A fresh directory with `n` empty files spread over subdirectories.
    fn tree(tag: &str, n: usize) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vix-file-index-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for i in 0..n {
            let sub = dir.join(format!("d{}", i / 500));
            if i % 500 == 0 {
                std::fs::create_dir_all(&sub).unwrap();
            }
            std::fs::write(sub.join(format!("f{i}.txt")), "").unwrap();
        }
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn wait(app: &mut App) {
        let start = std::time::Instant::now();
        while app.file_index_busy() {
            assert!(
                start.elapsed().as_secs() < 60,
                "background index never landed"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
            app.tick();
        }
    }

    fn type_query(app: &mut App, q: &str) {
        for c in q.chars() {
            app.on_key(vix_input::KeyEvent::new(
                vix_input::KeyCode::Char(c),
                vix_input::KeyModifiers::NONE,
            ));
        }
    }

    /// A job whose list is `files` (already sent) for `folders`.
    fn job_with(files: Option<Vec<PathBuf>>, folders: Vec<PathBuf>) -> FileIndexJob {
        let (tx, rx) = mpsc::channel();
        if let Some(f) = files {
            tx.send(f).unwrap();
        }
        FileIndexJob {
            rx,
            cancel: WalkCancel::new(),
            folders,
        }
    }

    #[test]
    fn a_small_tree_is_indexed_at_once_with_no_worker() {
        let dir = tree("small", 12);
        let mut app = App::new(dir.clone(), Settings::default());
        app.run_action("tools.palette");
        assert_eq!(app.file_index_len(), 12);
        assert!(!app.file_index_busy());
        assert_eq!(app.file_index_state, FileIndexState::Complete);
        assert_eq!(app.file_index_notice(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_large_tree_lists_the_first_pass_then_every_file() {
        let total = FILE_INDEX_FIRST_PASS + 300;
        let dir = tree("large", total);
        let mut app = App::new(dir.clone(), Settings::default());
        app.run_action("tools.palette");
        assert_eq!(
            app.file_index_len(),
            FILE_INDEX_FIRST_PASS,
            "first results at once"
        );
        assert!(!app.palette.as_ref().unwrap().entries.is_empty());
        assert!(app.file_index_busy());
        let note = app.file_index_notice().unwrap();
        assert!(note.contains(&FILE_INDEX_FIRST_PASS.to_string()), "{note}");
        // Reopening while the walk runs keeps the same walk.
        app.build_file_index();
        assert!(app.file_index_busy());
        wait(&mut app);
        assert_eq!(app.file_index_len(), total, "every file indexed");
        assert_eq!(app.file_index_state, FileIndexState::Complete);
        assert_eq!(app.file_index_notice(), None);
        // Every file is findable, wherever the walk visited it.
        for probe in [0, total / 2, total - 1] {
            app.run_action("tools.palette");
            let name = format!("f{probe}.txt");
            type_query(&mut app, &name);
            let entries = &app.palette.as_ref().unwrap().entries;
            assert!(
                entries
                    .iter()
                    .any(|e| e.label.ends_with(&format!("/{name}"))),
                "{name}"
            );
            app.palette = None;
        }
        // A rebuild keeps the full list on screen while it refreshes.
        app.build_file_index();
        assert_eq!(app.file_index_len(), total);
        assert_eq!(app.file_index_notice(), None);
        wait(&mut app);
        assert_eq!(app.file_index_len(), total);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_open_project_search_runs_again_when_the_full_index_lands() {
        let dir = tree("search-refresh", 3);
        std::fs::write(dir.join("first.txt"), "a needle here\n").unwrap();
        std::fs::write(dir.join("later.txt"), "another needle\n").unwrap();
        let mut app = App::new(dir.clone(), Settings::default());
        // As if the first pass had listed only `first.txt` and the walk
        // were still running.
        app.file_index = vec![dir.join("first.txt")];
        app.file_index_folders.clone_from(&app.workspace_folders);
        let mut panel = WorkspaceSearch::new(false);
        panel.query = "needle".to_string();
        app.workspace_search = Some(panel);
        app.run_workspace_search();
        assert_eq!(app.workspace_search_hits().len(), 1, "first pass only");
        let all = vec![dir.join("first.txt"), dir.join("later.txt")];
        app.file_index_job = Some(job_with(Some(all), app.workspace_folders.clone()));
        app.poll_file_index();
        let hits: Vec<&str> = app
            .workspace_search_hits()
            .iter()
            .map(|h| h.rel.as_str())
            .collect();
        assert_eq!(hits.len(), 2, "{hits:?}");
        assert!(hits.iter().any(|r| r.ends_with("later.txt")), "{hits:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn changing_folders_cancels_the_walk_in_flight() {
        let mut app = App::new(PathBuf::from("/nonexistent-a"), Settings::default());
        let old = job_with(None, vec![PathBuf::from("/nonexistent-a")]);
        let cancel = old.cancel.clone();
        app.file_index_job = Some(old);
        app.workspace_folders = vec![PathBuf::from("/nonexistent-b")];
        app.build_file_index();
        assert!(cancel.is_cancelled(), "the old walk was told to stop");
        assert!(!app.file_index_busy(), "an empty folder needs no worker");
    }

    #[test]
    fn a_list_for_old_folders_is_dropped() {
        let mut app = App::new(PathBuf::from("/nonexistent-c"), Settings::default());
        app.file_index_job = Some(job_with(
            Some(vec![PathBuf::from("/elsewhere/x")]),
            vec![PathBuf::from("/elsewhere")],
        ));
        app.poll_file_index();
        assert!(!app.file_index_busy());
        assert_eq!(app.file_index_len(), 0);
    }

    #[test]
    fn hitting_the_cap_or_losing_the_worker_says_cut_short() {
        let root = PathBuf::from("/nonexistent-d");
        let mut app = App::new(root.clone(), Settings::default());
        app.run_action("tools.palette");
        app.file_index_state = FileIndexState::Building;
        app.file_index_job = Some(job_with(
            Some(vec![
                Path::new("/nonexistent-d/x").to_path_buf();
                FILE_INDEX_CAP
            ]),
            vec![root.clone()],
        ));
        app.poll_file_index();
        assert_eq!(app.file_index_state, FileIndexState::CutShort);
        let note = app.file_index_notice().unwrap();
        assert!(note.contains(&FILE_INDEX_CAP.to_string()), "{note}");

        app.file_index_state = FileIndexState::Building;
        app.file_index_job = Some(job_with(None, vec![root])); // sender dropped
        app.poll_file_index();
        assert!(!app.file_index_busy());
        assert_eq!(app.file_index_state, FileIndexState::CutShort);
    }

    #[test]
    fn rerooting_forgets_the_index_and_its_walk() {
        let mut app = App::new(PathBuf::from("/nonexistent-e"), Settings::default());
        let job = job_with(None, vec![PathBuf::from("/nonexistent-e")]);
        let cancel = job.cancel.clone();
        app.file_index_job = Some(job);
        app.file_index_folders = vec![PathBuf::from("/nonexistent-e")];
        app.reset_file_index();
        assert!(cancel.is_cancelled());
        assert_eq!(app.file_index_folders, Vec::<PathBuf>::new());
        assert!(!app.file_index_busy());
    }

    #[test]
    fn the_notice_is_only_for_the_file_finder() {
        let mut app = App::new(PathBuf::from("/nonexistent-f"), Settings::default());
        app.file_index_state = FileIndexState::CutShort;
        assert_eq!(app.file_index_notice(), None, "no palette open");
        app.run_action("tools.palette");
        app.file_index_state = FileIndexState::CutShort;
        assert!(app.file_index_notice().is_some());
        type_query(&mut app, ">");
        assert_eq!(app.file_index_notice(), None, "commands mode");
    }
}
