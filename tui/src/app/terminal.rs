//! Integrated terminal: toggle, key forwarding and output polling.

use super::{App, KeyCode, KeyEvent};

impl App {
    /// Dispatch the integrated-terminal action. Returns `true` if handled.
    pub(super) fn run_terminal_action(&mut self, action: &str) -> bool {
        match action {
            "tools.terminal" => self.toggle_terminal(),
            _ => return false,
        }
        true
    }

    // ----- Integrated terminal --------------------------------------------

    /// Toggle the integrated terminal: open a shell on a PTY, or close it if open.
    pub(super) fn toggle_terminal(&mut self) {
        if self.terminal.is_some() {
            self.terminal = None;
            return;
        }
        // Before the first frame there is no editor area yet: start at a
        // conventional 24x80 (the first draw resizes it to the overlay).
        let area = self.layout.editor;
        let (rows, cols) = if area.height == 0 || area.width == 0 {
            (24, 80)
        } else {
            (area.height, area.width)
        };
        let shell = std::env::var("SHELL").unwrap_or_else(|_| {
            if cfg!(windows) {
                "cmd.exe".to_string()
            } else {
                "/bin/sh".to_string()
            }
        });
        // T716: the authority says what runs on the local PTY — the shell
        // itself here, `ssh -t` into the remote workspace for SSH. An
        // untrusted remote workspace (T714) gets a shell in the remote home
        // directory, so nothing the workspace defines (a `cd` hook, an
        // `.envrc`) runs by itself.
        let untrusted = self.remote_untrusted();
        let cwd = (!untrusted).then_some(self.root.as_path());
        let opened = crate::authority::current()
            .terminal(&shell, cwd)
            .and_then(|spec| open_terminal(&spec, rows, cols));
        match opened {
            Ok(term) => {
                self.terminal = Some(term);
                if untrusted {
                    self.status = t!("status.remote_terminal_home").to_string();
                }
            }
            Err(e) => self
                .messages
                .error(t!("msg.command_failed", error = e).to_string()),
        }
    }

    /// Handle a key while the terminal is focused. `Ctrl+]` closes it; every other
    /// key is forwarded to the shell. A dead shell closes on the next key.
    pub(super) fn terminal_key(&mut self, key: KeyEvent) {
        let close = Self::ctrl(&key) && matches!(key.code, KeyCode::Char(']'));
        if close {
            self.terminal = None;
            return;
        }
        let Some(term) = self.terminal.as_mut() else {
            return;
        };
        if !term.alive() {
            self.terminal = None;
            return;
        }
        term.send_key(key);
    }

    /// Drain the terminal: close it once the shell has exited. Called each loop.
    pub fn poll_terminal(&mut self) {
        if self.terminal.as_ref().is_some_and(|t| !t.alive()) {
            self.terminal = None;
            self.status = t!("status.terminal_closed").to_string();
        }
    }

    /// Whether the integrated terminal is open (drives the fast poll cadence).
    #[must_use]
    pub fn terminal_running(&self) -> bool {
        self.terminal.is_some()
    }

    /// The integrated terminal's visible screen as plain text (rows joined by
    /// newlines), or `None` when it is closed. For tests and diagnostics.
    #[must_use]
    pub fn terminal_screen_text(&self) -> Option<String> {
        self.terminal.as_ref().map(|t| t.lock().screen().contents())
    }
}

/// Start `spec` (from `Authority::terminal`) on a new local PTY. A spec
/// without its own working directory (the SSH client's) starts in this
/// process's directory, else the temp directory: either exists here, which
/// the remote workspace root need not.
fn open_terminal(
    spec: &crate::authority::ProcessSpec,
    rows: u16,
    cols: u16,
) -> std::io::Result<crate::terminal::Terminal> {
    let args: Vec<&std::ffi::OsStr> = spec
        .get_args()
        .iter()
        .filter_map(|arg| match arg {
            crate::authority::Arg::Plain(a) | crate::authority::Arg::Raw(a) => Some(a.as_os_str()),
            _ => None,
        })
        .collect();
    let dir = spec.cwd().map_or_else(
        || std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir()),
        std::path::Path::to_path_buf,
    );
    crate::terminal::Terminal::open_command(spec.program(), &args, &dir, rows, cols)
}
