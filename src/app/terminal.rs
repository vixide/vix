//! Integrated terminal: toggle, key forwarding and output polling.

#![warn(clippy::pedantic)]

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
        let area = self.layout.editor;
        let rows = area.height.max(1);
        let cols = area.width.max(1);
        let shell = std::env::var("SHELL").unwrap_or_else(|_| {
            if cfg!(windows) {
                "cmd.exe".to_string()
            } else {
                "/bin/sh".to_string()
            }
        });
        match crate::terminal::Terminal::open(&shell, &self.root, rows, cols) {
            Ok(term) => self.terminal = Some(term),
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
}
