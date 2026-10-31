//! Integrated terminals (T631): several PTY shells as tabs of one overlay,
//! with scrollback, mouse selection and copy, and mouse forwarding.

use super::{App, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use crate::terminal::{Selection, Terminal};

/// Lines one mouse-wheel notch scrolls the terminal's scrollback.
const WHEEL_LINES: isize = 3;

impl App {
    /// Dispatch the integrated-terminal actions. Returns `true` if handled.
    pub(super) fn run_terminal_action(&mut self, action: &str) -> bool {
        match action {
            "tools.terminal" => self.toggle_terminal(),
            "terminal.new" => self.open_new_terminal(),
            "terminal.close" => self.close_terminal(),
            "terminal.next" => self.switch_terminal(true),
            "terminal.prev" => self.switch_terminal(false),
            "terminal.copy" => self.terminal_copy(),
            _ => return false,
        }
        true
    }

    // ----- Integrated terminals -------------------------------------------

    /// Show or hide the terminal overlay; with no terminal yet, open one.
    /// Hiding never stops a shell (`terminal.close` does).
    pub(super) fn toggle_terminal(&mut self) {
        if self.terminals.is_visible() {
            self.terminals.set_shown(false);
        } else if self.terminals.is_empty() {
            self.open_new_terminal();
        } else {
            self.terminals.set_shown(true);
        }
    }

    /// Open another terminal as a new tab, make it active and show the
    /// overlay. What runs comes from the workspace authority (T716).
    pub(super) fn open_new_terminal(&mut self) {
        let (rows, cols) = self.terminal_start_size();
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
        let scrollback = self.settings.terminal_scrollback;
        let opened = crate::authority::current()
            .terminal(&shell, cwd)
            .and_then(|spec| open_terminal(&spec, rows, cols, scrollback));
        match opened {
            Ok(term) => {
                self.terminals.push(term);
                if untrusted {
                    self.status = t!("status.remote_terminal_home").to_string();
                }
            }
            Err(e) => self
                .messages
                .error(t!("msg.command_failed", error = e).to_string()),
        }
    }

    /// The size a new terminal starts at: the editor area, or a conventional
    /// 24x80 before the first frame (the first draw resizes it).
    pub(super) fn terminal_start_size(&self) -> (u16, u16) {
        let area = self.layout.editor;
        if area.height == 0 || area.width == 0 {
            (24, 80)
        } else {
            (area.height, area.width)
        }
    }

    /// Close (kill) the active terminal; the next one becomes active, and
    /// the overlay goes with the last.
    fn close_terminal(&mut self) {
        if self.terminals.close_active().is_none() {
            self.status = t!("status.terminal_none").to_string();
        }
    }

    /// Activate the next (or previous) terminal and show the overlay.
    fn switch_terminal(&mut self, forward: bool) {
        if self.terminals.is_empty() {
            self.status = t!("status.terminal_none").to_string();
            return;
        }
        if forward {
            self.terminals.next();
        } else {
            self.terminals.prev();
        }
        self.terminals.set_shown(true);
    }

    /// Copy the active terminal's selection, or its whole visible view, to
    /// the clipboard (the editor's fallback-aware copy when a buffer is open).
    fn terminal_copy(&mut self) {
        let Some(text) = self.terminals.active().map(Terminal::copy_text) else {
            self.status = t!("status.terminal_none").to_string();
            return;
        };
        if let Some(tab) = self.editor.active_tab_mut() {
            tab.editor.copy_to_clipboard(&text);
        } else if crate::clipboard::set(&text).is_err() {
            self.messages
                .warn(t!("msg.terminal_copy_failed").to_string());
            return;
        }
        self.status = t!("status.terminal_copied").to_string();
    }

    /// Handle a key while the terminal overlay is shown. `Ctrl+]` hides it;
    /// Shift+PgUp/PgDn page the scrollback and Shift+Home/End jump to its
    /// top or back to the live screen; Ctrl+PgUp/PgDn switch terminals.
    /// Every other key returns to the live screen and goes to the active
    /// shell. A dead shell closes on the next key.
    pub(super) fn terminal_key(&mut self, key: KeyEvent) {
        let ctrl = Self::ctrl(&key);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        if ctrl && matches!(key.code, KeyCode::Char(']')) {
            self.terminals.set_shown(false);
            return;
        }
        if ctrl && matches!(key.code, KeyCode::PageUp | KeyCode::PageDown) {
            self.switch_terminal(key.code == KeyCode::PageDown);
            return;
        }
        let Some(term) = self.terminals.active_mut() else {
            return;
        };
        if !term.alive() {
            self.terminals.close_active();
            return;
        }
        let page = isize::try_from(term.size().0.saturating_sub(1).max(1)).unwrap_or(1);
        match key.code {
            KeyCode::PageUp if shift => {
                term.scroll_by(page);
            }
            KeyCode::PageDown if shift => {
                term.scroll_by(-page);
            }
            KeyCode::Home if shift => {
                term.scroll_by(isize::MAX);
            }
            KeyCode::End if shift => term.scroll_to_bottom(),
            _ => {
                term.scroll_to_bottom();
                term.set_selection(None);
                term.send_key(key);
            }
        }
    }

    /// A mouse event while the terminal overlay is shown; `true` when it
    /// landed on the terminal's screen. When the child asked for mouse
    /// reports the event goes to it (hold Shift to select here instead, as
    /// in xterm); otherwise the wheel scrolls the scrollback and a left drag
    /// selects text, copied when the button comes up.
    pub(super) fn terminal_mouse(&mut self, mouse: MouseEvent) -> bool {
        let area = self.layout.terminal;
        if !super::rect_contains(area, mouse.column, mouse.row) {
            return false;
        }
        let cell = (mouse.row - area.y, mouse.column - area.x);
        let Some(term) = self.terminals.active_mut() else {
            return false;
        };
        let shift = mouse.modifiers.contains(KeyModifiers::SHIFT);
        if !shift && term.wants_mouse() {
            term.send_mouse(mouse.kind, cell.1, cell.0, mouse.modifiers);
            return true;
        }
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                term.scroll_by(WHEEL_LINES);
            }
            MouseEventKind::ScrollDown => {
                term.scroll_by(-WHEEL_LINES);
            }
            MouseEventKind::Down(MouseButton::Left) => {
                term.set_selection(Some(Selection::at(cell)));
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(mut sel) = term.selection() {
                    sel.head = cell;
                    term.set_selection(Some(sel));
                }
            }
            MouseEventKind::Up(MouseButton::Left)
                if term.selection().is_some_and(|s| !s.is_empty()) =>
            {
                self.terminal_copy();
            }
            _ => {}
        }
        true
    }

    /// Drain the terminals: close each whose shell has exited. Called each
    /// loop.
    pub fn poll_terminal(&mut self) {
        if !self.terminals.retain(Terminal::alive).is_empty() {
            self.status = t!("status.terminal_closed").to_string();
        }
    }

    /// Whether any terminal is running, shown or not (drives the fast poll
    /// cadence).
    #[must_use]
    pub fn terminal_running(&self) -> bool {
        !self.terminals.is_empty()
    }

    /// Whether the terminal overlay is shown (it then takes every key).
    #[must_use]
    pub fn terminal_visible(&self) -> bool {
        self.terminals.is_visible()
    }

    /// How many terminals are open.
    #[must_use]
    pub fn terminal_count(&self) -> usize {
        self.terminals.len()
    }

    /// The active terminal's tab index (0 with none).
    #[must_use]
    pub fn terminal_active(&self) -> usize {
        self.terminals.active_index()
    }

    /// How far the active terminal's view is scrolled back (0 with none, or
    /// when following the shell).
    #[must_use]
    pub fn terminal_scroll_offset(&self) -> usize {
        self.terminals.active().map_or(0, Terminal::scroll_offset)
    }

    /// The active terminal's view as plain text (rows joined by newlines;
    /// scrolled back when it is), or `None` with no terminal. For tests and
    /// diagnostics.
    #[must_use]
    pub fn terminal_screen_text(&self) -> Option<String> {
        self.terminals
            .active()
            .map(|t| t.lock().screen().contents())
    }

    /// The text of the terminal at tab `index`'s view, like
    /// [`Self::terminal_screen_text`].
    #[must_use]
    pub fn terminal_screen_text_at(&self, index: usize) -> Option<String> {
        self.terminals
            .iter()
            .nth(index)
            .map(|(_, t)| t.lock().screen().contents())
    }
}

/// Start `spec` (from `Authority::terminal`) on a new local PTY keeping
/// `scrollback` lines. A spec without its own working directory (the SSH
/// client's) starts in this process's directory, else the temp directory:
/// either exists here, which the remote workspace root need not.
fn open_terminal(
    spec: &crate::authority::ProcessSpec,
    rows: u16,
    cols: u16,
    scrollback: usize,
) -> std::io::Result<Terminal> {
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
    Terminal::open_command(spec.program(), &args, &dir, rows, cols, scrollback)
}
