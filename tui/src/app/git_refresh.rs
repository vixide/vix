//! The first git refresh, run in the background (T830).
//!
//! [`App::refresh_git`] shells out to `git` three times (repo?, branch,
//! status), which costs tens of milliseconds and grows with the repository.
//! [`App::start`] does not pay that on the UI thread: it calls
//! [`App::start_git_refresh`], which runs the same three commands on a worker
//! thread and posts a [`GitSnapshot`] on a channel; [`App::poll_git_refresh`]
//! (from [`App::tick`]) installs it, so the status-bar branch and the change
//! gutter fill in a frame or two after the first one. Anything that needs git
//! state at once (opening the Git panel, a save, a git action) still calls
//! [`App::refresh_git`], which supersedes a background refresh in flight.
//! Owning spec: `crates/vix-git/spec/index.md` ("When git state is read").

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

use std::path::PathBuf;
use std::sync::mpsc::{self, TryRecvError};

use super::App;

/// What one git refresh found: the state [`App::refresh_git`] keeps.
#[cfg_attr(
    not(feature = "git"),
    allow(dead_code, reason = "only the git capability takes snapshots")
)]
pub(crate) struct GitSnapshot {
    /// Whether the workspace root is inside a git working tree.
    pub(crate) repo: bool,
    /// The current branch (or a detached HEAD's short hash).
    pub(crate) branch: Option<String>,
    /// The changed files, as `git status --porcelain` reports them.
    pub(crate) status: Vec<crate::git::FileStatus>,
}

/// A background git refresh in flight.
pub(crate) struct GitRefreshJob {
    /// The snapshot, sent once.
    rx: mpsc::Receiver<GitSnapshot>,
    /// The workspace root it reads; a snapshot of another root is dropped.
    root: PathBuf,
}

impl App {
    /// Read the git state on a worker thread (T830); [`App::poll_git_refresh`]
    /// installs it when it lands. A refresh already in flight is replaced.
    /// Without the `git` capability this does nothing.
    pub fn start_git_refresh(&mut self) {
        #[cfg(feature = "git")]
        {
            let root = self.root.clone();
            let worker_root = root.clone();
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                // The App may have dropped the job (a newer refresh): nobody
                // is waiting for this one then.
                let _ = tx.send(GitSnapshot::take(&worker_root));
            });
            self.git_refresh_job = Some(GitRefreshJob { rx, root });
        }
        #[cfg(not(feature = "git"))]
        {
            self.git_refresh_job = None;
        }
    }

    /// Install a finished background git refresh. Called once per event-loop
    /// iteration; cheap when none is running. A snapshot of a workspace root
    /// the App has since left is dropped.
    pub fn poll_git_refresh(&mut self) {
        let snapshot = {
            let Some(job) = self.git_refresh_job.as_ref() else {
                return;
            };
            match job.rx.try_recv() {
                Ok(snapshot) => snapshot,
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    // The worker died; the next save or git action refreshes.
                    self.git_refresh_job = None;
                    return;
                }
            }
        };
        let Some(job) = self.git_refresh_job.take() else {
            return;
        };
        if job.root == self.root {
            self.apply_git_snapshot(snapshot);
        }
    }

    /// Whether a background git refresh is running.
    #[must_use]
    pub fn git_refresh_busy(&self) -> bool {
        self.git_refresh_job.is_some()
    }
}

#[cfg(all(test, feature = "git"))]
mod tests {
    use crate::app::git::tests::{git, repo};
    use crate::app::{App, AppFlags};
    use crate::settings::Settings;
    use std::path::PathBuf;

    /// Tick until the background refresh lands.
    fn wait(app: &mut App) {
        let start = std::time::Instant::now();
        while app.git_refresh_busy() {
            assert!(
                start.elapsed().as_secs() < 60,
                "background git refresh never landed"
            );
            std::thread::sleep(std::time::Duration::from_millis(2));
            app.tick();
        }
    }

    #[test]
    fn a_new_app_knows_nothing_until_the_background_refresh_lands() {
        let d = repo("bg-refresh", &[("a.txt", "one\n")]);
        std::fs::write(d.join("b.txt"), "new\n").unwrap();
        let mut app = App::new(d.clone(), Settings::default());
        assert_eq!(app.git_branch, None, "nothing read on the UI thread");
        assert!(!app.flags.contains(AppFlags::GIT_REPO));
        app.start_git_refresh();
        assert!(app.git_refresh_busy());
        assert!(app.poll_interval() <= std::time::Duration::from_millis(50));
        wait(&mut app);
        assert!(app.flags.contains(AppFlags::GIT_REPO));
        assert_eq!(app.git_branch.as_deref(), Some("main"));
        assert!(app.git_dirty(), "the untracked file counts");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_synchronous_refresh_supersedes_the_background_one() {
        let d = repo("bg-start", &[("a.txt", "one\n")]);
        let mut app = App::new(d.clone(), Settings::default());
        app.start_git_refresh();
        assert!(app.git_refresh_busy());
        // Anything that needs git now reads it now; the older answer is dropped.
        git(&d, &["checkout", "-q", "-b", "topic"]);
        app.refresh_git();
        assert!(!app.git_refresh_busy());
        assert_eq!(app.git_branch.as_deref(), Some("topic"));
        app.tick();
        assert_eq!(app.git_branch.as_deref(), Some("topic"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_snapshot_of_a_root_the_app_left_is_dropped() {
        let d = repo("bg-reroot", &[("a.txt", "one\n")]);
        let mut app = App::new(d.clone(), Settings::default());
        app.start_git_refresh();
        app.root = PathBuf::from("/nonexistent-git-refresh");
        wait(&mut app);
        assert_eq!(app.git_branch, None);
        assert!(!app.flags.contains(AppFlags::GIT_REPO));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn outside_a_repository_the_refresh_lands_as_no_repo() {
        let d = std::env::temp_dir().join(format!("vix-bg-norepo-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let mut app = App::new(d.clone(), Settings::default());
        app.start_git_refresh();
        wait(&mut app);
        assert!(!app.flags.contains(AppFlags::GIT_REPO));
        assert_eq!(app.git_branch, None);
        let _ = std::fs::remove_dir_all(&d);
    }
}
