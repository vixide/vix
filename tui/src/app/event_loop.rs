//! The front-end-neutral half of startup and of the event loop. Both front
//! ends — the terminal loop in `src/main.rs` and the desktop GUI (`gui/`) —
//! call [`App::start`] once after constructing the `App`, then, per loop
//! iteration, [`App::tick`] before drawing and wait at most
//! [`App::poll_interval`] for the next input event. So startup and background
//! work (streamed commands, LSP/DAP traffic, async reparses, file watching,
//! auto-save, DB queries, …) behave identically in both; only reading input
//! and presenting a frame differ.

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use super::App;
use crate::cli::Cli;

impl App {
    /// Everything between [`App::new`] and the first frame that both front
    /// ends share: scripts and their trust prompt, key overrides, the
    /// first-run welcome screen, then whatever the command line asked to open
    /// (the tutor, file arguments, `-` for stdin, `--diff`) — or, with no file
    /// given, the previous session for this workspace.
    ///
    /// Git state is deliberately *not* refreshed here: `refresh_git` shells
    /// out to `git` (T122: ~82ms of a ~96ms cold start), so callers draw a
    /// first frame and only then call [`App::refresh_git`]. `App`'s git fields
    /// default to "no repo", which renders correctly in the meantime.
    ///
    /// # Errors
    /// Returns an error only when `-` was given and stdin could not be read.
    pub fn start(&mut self, cli: &Cli) -> io::Result<()> {
        self.load_scripts();
        self.maybe_prompt_script_trust();
        self.resolve_key_overrides();
        // First-run welcome screen (no-op after it has been seen once).
        self.maybe_show_welcome();

        // Optional file argument(s): open each, focusing the last. A single `-`
        // reads stdin into a scratch buffer instead of opening a file named
        // "-". With no file given, reopen the previous session for this
        // workspace (if enabled).
        if cli.tutor {
            self.open_tutor();
        } else if cli.files.is_empty() {
            self.restore_session();
        } else if cli.files == [PathBuf::from("-")] {
            let mut content = String::new();
            io::Read::read_to_string(&mut io::stdin(), &mut content)?;
            self.open_stdin_buffer(&content);
        } else {
            for path in &cli.files {
                self.open_initial(path);
            }
        }

        // `--diff OLD NEW`: an editable side-by-side comparison (T660) — the
        // shape a `git difftool` driver invokes with. `num_args = 2` on the arg
        // guarantees exactly two paths here.
        if let Some(paths) = &cli.diff
            && let [old, new] = paths.as_slice()
        {
            self.open_compare_files(old, new);
        }
        Ok(())
    }

    /// Drain every background source into `App` state, ready for the next
    /// frame. Call once per loop iteration, before drawing.
    pub fn tick(&mut self) {
        // Drain any streamed output from a running command into the bottom dock,
        // and any finished dashboard metrics into the dashboard panel.
        self.poll_command();
        self.poll_editor_drag();
        self.poll_ai_replace();
        self.poll_dashboard();
        self.poll_pomodoro();
        self.poll_terminal();
        self.refresh_inline_blame();
        self.refresh_outline_dock();
        self.refresh_word_highlight();
        self.refresh_backlinks_follow();
        // Drain language-server messages (diagnostics, hover/definition/completion
        // responses) and sync the active document.
        self.poll_lsp();
        self.poll_dap();
        // Install any completed background reparse (large-file async highlighting).
        self.poll_parse();
        // Reload files changed on disk by another process (throttled internally).
        self.poll_file_changes();
        // Periodically save the active buffer when auto-save is on.
        self.poll_auto_save();
        // Drain a finished HTTP response into a new tab.
        self.poll_http();
        // Drain a finished asynchronous DB query into the workbench.
        self.poll_db_query();
        // Drain a finished asynchronous DB connect/reconnect (Run H, T531).
        self.poll_db_connect();
        self.poll_db_reconnect();
    }

    /// How long the loop may wait for input before ticking again: short
    /// while something is streaming or in flight (so its output appears
    /// promptly), otherwise just often enough to keep the clock current.
    #[must_use]
    pub fn poll_interval(&self) -> Duration {
        if self.command_running()
            || self.ai_replace_running()
            || self.dashboard_loading()
            || self.lsp_busy()
            || self.pomodoro_running()
            || self.terminal_running()
            || self.dap_busy()
            || self.parse_busy()
            || self.http_running()
            || self.db_query_running()
            || self.db_connect_running()
            || self.db_reconnect_running()
            || self.editor_drag_active()
        {
            Duration::from_millis(50)
        } else {
            Duration::from_millis(500)
        }
    }
}
