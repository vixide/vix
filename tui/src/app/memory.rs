//! Memory bookkeeping for a long session (T830): per-file state the App keeps
//! beside its tabs is dropped once the file's tab is gone, so opening and
//! closing files for hours does not grow it.

use super::App;
use std::collections::HashSet;
use std::path::PathBuf;

impl App {
    /// Forget what the App keeps per file for files no tab shows any more
    /// (T830): the last-seen disk mtimes, the change gutter's HEAD and base
    /// blobs, and the language-server sync state (sending `didClose`, which
    /// also drops the client's copy of the text). Every close path ends up
    /// here, including Close All, a preview tab the next preview replaces and
    /// a buffer whose file was deleted, so none of them can leak. Run from the
    /// once-a-second disk poll; costs one pass over the open tabs.
    pub(super) fn prune_closed_file_state(&mut self) {
        let open: HashSet<PathBuf> = self
            .editor
            .tabs
            .iter()
            .filter_map(|t| t.path.clone())
            .collect();
        self.disk_mtimes.retain(|p, _| open.contains(p));
        #[cfg(feature = "git")]
        {
            self.git_head_cache.retain(|p, _| open.contains(p));
            self.git_gutter_base_cache.retain(|p, _| open.contains(p));
        }
        let closed: Vec<PathBuf> = self
            .lsp_synced
            .keys()
            .filter(|p| !open.contains(*p))
            .cloned()
            .collect();
        for path in closed {
            self.lsp_close(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::app::cov4::{app_at, temp};

    /// How many files the long-session exercise opens and closes.
    const FILES: usize = 1_000;

    /// One in this many visited files also builds its change gutter (each
    /// build runs `git show`, about 10 ms in a debug build).
    #[cfg(feature = "git")]
    const GUTTER_EVERY: usize = 10;

    /// The sizes of every per-file or per-jump collection the App keeps.
    fn report(app: &App, label: &str) -> String {
        #[cfg(feature = "git")]
        let git = format!(
            " git_head_cache={} git_gutter_base_cache={}",
            app.git_head_cache.len(),
            app.git_gutter_base_cache.len()
        );
        #[cfg(not(feature = "git"))]
        let git = String::new();
        format!(
            "{label}: tabs={} disk_mtimes={} lsp_synced={} nav_history={} closed_tabs={} \
             recent_files={} messages={} clipboard_ring={}{git}",
            app.editor.tabs.len(),
            app.disk_mtimes.len(),
            app.lsp_synced.len(),
            app.nav_history.len(),
            app.closed_tabs.len(),
            app.settings.recent_files.len(),
            app.messages.items.len(),
            app.clipboard_ring.len(),
        )
    }

    /// What `lsp_sync_active` records when a server handles the active file.
    fn mark_synced(app: &mut App, path: &Path) {
        if let Some(rev) = app.editor.active_tab().map(|t| t.editor.revision()) {
            app.lsp_synced.insert(path.to_path_buf(), rev);
        }
    }

    /// Run the once-a-second disk poll now.
    fn poll(app: &mut App) {
        app.last_disk_poll = None;
        app.poll_file_changes();
    }

    /// Open file `i` the way a session does (a jump, or an explorer preview),
    /// let a tick see it, then close it one of three ways.
    fn visit(app: &mut App, path: &Path, i: usize) {
        if i.is_multiple_of(3) {
            app.open_path(path, true);
        } else {
            app.with_jump(|s| s.open_path(path, false));
        }
        mark_synced(app, path);
        #[cfg(feature = "git")]
        if i.is_multiple_of(GUTTER_EVERY) {
            app.refresh_git_gutter();
        }
        poll(app);
        match i % 3 {
            1 => app.do_close_active(),
            2 if i % 30 == 2 => {
                app.run_action("file.close_all");
            }
            _ => {}
        }
    }

    /// A workspace of `n` small files (a committed repository with `git`).
    fn workspace(tag: &str, n: usize) -> (PathBuf, Vec<PathBuf>) {
        let dir = temp(tag);
        let paths: Vec<PathBuf> = (0..n)
            .map(|i| {
                let p = dir.join(format!("f{i:04}.txt"));
                std::fs::write(&p, format!("file {i}\n")).unwrap();
                p
            })
            .collect();
        #[cfg(feature = "git")]
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "Test"],
            &["config", "user.email", "test@example.invalid"],
            &["config", "commit.gpgsign", "false"],
            &["config", "core.hooksPath", "/dev/null"],
            &["add", "-A"],
            &["commit", "-q", "-m", "seed"],
        ] {
            crate::git::run(&dir, args).unwrap();
        }
        (dir, paths)
    }

    /// An App on `dir` that never writes the real settings file.
    fn app_on(dir: &Path) -> App {
        let mut app = app_at(dir);
        app.settings_path = Some(dir.join("vix-test-config.toml"));
        app.refresh_git();
        app
    }

    /// The paths of the open tabs.
    fn open_paths(app: &App) -> std::collections::HashSet<PathBuf> {
        app.editor
            .tabs
            .iter()
            .filter_map(|t| t.path.clone())
            .collect()
    }

    #[test]
    fn opening_and_closing_a_thousand_files_keeps_per_file_state_bounded() {
        let (dir, paths) = workspace("mem-1000", FILES);
        let mut app = app_on(&dir);
        let started = std::time::Instant::now();
        for (i, p) in paths.iter().enumerate() {
            visit(&mut app, p, i);
        }
        poll(&mut app);
        eprintln!(
            "{} ({:.1?})",
            report(&app, "after 1,000 files"),
            started.elapsed()
        );
        let open = open_paths(&app);
        assert!(app.disk_mtimes.keys().all(|p| open.contains(p)));
        assert!(app.lsp_synced.keys().all(|p| open.contains(p)));
        #[cfg(feature = "git")]
        {
            assert!(app.git_head_cache.keys().all(|p| open.contains(p)));
            assert!(app.git_gutter_base_cache.keys().all(|p| open.contains(p)));
        }
        assert!(app.nav_history.len() <= NAV_HISTORY_CAP);
        assert!(app.nav_idx < app.nav_history.len());
        assert!(app.closed_tabs.len() <= 20);
        assert!(app.settings.recent_files.len() <= app.settings.recent_files_max);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn close_all_and_a_replaced_preview_drop_their_files_state() {
        let (dir, paths) = workspace("mem-close", 3);
        let mut app = app_on(&dir);
        // A preview the next preview replaces never went through a close.
        app.open_path(&paths[0], true);
        mark_synced(&mut app, &paths[0]);
        poll(&mut app);
        app.open_path(&paths[1], true);
        mark_synced(&mut app, &paths[1]);
        poll(&mut app);
        assert!(!app.lsp_synced.contains_key(&paths[0]));
        assert!(!app.disk_mtimes.contains_key(&paths[0]));
        assert!(app.lsp_synced.contains_key(&paths[1]), "the open one stays");
        assert!(app.disk_mtimes.contains_key(&paths[1]));
        // Close All forgets every file at the next poll.
        app.open_path(&paths[2], false);
        mark_synced(&mut app, &paths[2]);
        app.run_action("file.close_all");
        poll(&mut app);
        assert!(app.lsp_synced.is_empty());
        assert!(app.disk_mtimes.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_reopened_file_rebuilds_what_was_dropped() {
        let (dir, paths) = workspace("mem-reopen", 1);
        let p = &paths[0];
        let mut app = app_on(&dir);
        app.open_path(p, false);
        poll(&mut app);
        app.do_close_active();
        poll(&mut app);
        assert!(!app.disk_mtimes.contains_key(p));
        // Changed on disk while closed: reopening shows the new text and the
        // first poll only records the mtime, with no reload warning.
        std::fs::write(p, "changed\n").unwrap();
        app.open_path(p, false);
        let messages = app.messages.items.len();
        poll(&mut app);
        assert!(app.disk_mtimes.contains_key(p));
        assert_eq!(app.messages.items.len(), messages);
        assert_eq!(app.editor.active_tab().unwrap().text(), "changed\n");
        // The language server hears a fresh didOpen: the sync state is gone.
        assert!(!app.lsp_synced.contains_key(p));
        #[cfg(feature = "git")]
        {
            // The HEAD blob is fetched again and the gutter marks the change.
            app.refresh_git_gutter();
            assert!(app.git_head_cache.contains_key(p));
            let marks = app
                .editor
                .active_tab()
                .unwrap()
                .editor
                .gutter_marks()
                .cloned();
            assert!(
                marks.is_some_and(|m| !m.is_empty()),
                "the edit shows in the gutter"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn position_history_keeps_the_newest_hundred_jumps() {
        let (dir, paths) = workspace("mem-nav", 2);
        let mut app = app_on(&dir);
        app.open_path(&paths[0], false);
        for i in 0..NAV_HISTORY_CAP * 2 {
            app.with_jump(|s| s.open_path(&paths[i % 2], false));
        }
        assert_eq!(app.nav_history.len(), NAV_HISTORY_CAP);
        assert_eq!(app.nav_idx, NAV_HISTORY_CAP - 1);
        // Back still walks the kept entries.
        app.nav_back();
        assert_eq!(app.nav_idx, NAV_HISTORY_CAP - 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
