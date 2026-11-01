//! Integrated terminals (T631): several PTY shells as tabs of one overlay,
//! with scrollback, mouse selection and copy, and mouse forwarding.

use super::{App, Focus, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
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
            "terminal.send_selection" => self.terminal_send_selection(),
            "terminal.restart" => self.terminal_restart(),
            "terminal.toggle_capture" => self.terminal_toggle_capture(),
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

    /// Handle a key while the terminal overlay is shown; `false` when it is
    /// left for the editor (capture is off, T666, and the key is a Ctrl or
    /// Alt chord). `Ctrl+]` hides the overlay and `Ctrl+\` toggles whether
    /// chords go to the shell; Shift+PgUp/PgDn page the scrollback and
    /// Shift+Home/End jump to its top or back to the live screen;
    /// Ctrl+PgUp/PgDn switch terminals. Every other key returns to the live
    /// screen and goes to the active shell. After the shell has exited,
    /// Enter restarts it and any other key closes the tab.
    pub(super) fn terminal_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = Self::ctrl(&key);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);
        if ctrl && matches!(key.code, KeyCode::Char(']')) {
            self.terminals.set_shown(false);
            return true;
        }
        if ctrl && matches!(key.code, KeyCode::Char('\\')) {
            self.terminal_toggle_capture();
            return true;
        }
        if ctrl && matches!(key.code, KeyCode::PageUp | KeyCode::PageDown) {
            self.switch_terminal(key.code == KeyCode::PageDown);
            return true;
        }
        let Some(term) = self.terminals.active_mut() else {
            return true;
        };
        if !term.capture()
            && key
                .modifiers
                .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return false;
        }
        if !term.alive() {
            if key.code == KeyCode::Enter {
                self.terminal_restart();
            } else {
                self.terminals.close_active();
                self.status = t!("status.terminal_closed").to_string();
            }
            return true;
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
        true
    }

    /// Restart the active terminal's shell in place, keeping its transcript
    /// (T666).
    fn terminal_restart(&mut self) {
        let marker = t!("ui.terminal_restarted").to_string();
        let Some(term) = self.terminals.active_mut() else {
            self.status = t!("status.terminal_none").to_string();
            return;
        };
        match term.restart(&marker) {
            Ok(()) => self.status = t!("status.terminal_restarted").to_string(),
            Err(e) => self
                .messages
                .error(t!("msg.command_failed", error = e).to_string()),
        }
    }

    /// Choose whether editor chords go to the active terminal's shell or to
    /// the editor (T666).
    fn terminal_toggle_capture(&mut self) {
        let Some(term) = self.terminals.active_mut() else {
            self.status = t!("status.terminal_none").to_string();
            return;
        };
        let capture = !term.capture();
        term.set_capture(capture);
        self.status = if capture {
            t!("status.terminal_capture_on")
        } else {
            t!("status.terminal_capture_off")
        }
        .to_string();
    }

    /// Send the editor's selection, or its current line, to the active
    /// terminal as a command and press Enter (T666); opens a terminal first
    /// if none is open.
    fn terminal_send_selection(&mut self) {
        let text = self.editor.active_tab_mut().and_then(|t| {
            t.editor
                .get_selection_text()
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    let line = t.editor.cursor_line();
                    t.text().lines().nth(line).map(str::to_string)
                })
        });
        let Some(text) = text.filter(|s| !s.trim().is_empty()) else {
            self.status = t!("status.terminal_nothing_to_send").to_string();
            return;
        };
        if self.terminals.is_empty() {
            self.open_new_terminal();
        }
        let Some(term) = self.terminals.active_mut() else {
            return;
        };
        term.send_text(&text);
        self.terminals.set_shown(true);
    }

    /// The file and position a click on cell (`row`, `col`) of the active
    /// terminal's view names: the clicked word read as `path[:line[:col]]`,
    /// else a location the problem matchers find in the whole row (T632's
    /// parser), resolved against the shell's working directory (OSC 7).
    pub(super) fn terminal_location_at(
        &self,
        row: u16,
        col: u16,
    ) -> Option<(std::path::PathBuf, Option<(usize, usize)>)> {
        let term = self.terminals.active()?;
        let cwd = term.cwd();
        let auth = crate::authority::current();
        if let Some(word) = term.word_at(row, col) {
            let (file, target) = crate::palette::parse_path_target(word.trim_end_matches(':'));
            if !file.is_empty() {
                for base in [cwd.as_path(), self.root.as_path()] {
                    let path = vix_tasks::matcher::resolve_path(base, &file);
                    if auth.is_file(&path) {
                        return Some((path, target));
                    }
                }
            }
        }
        let (path, line, col) = self.output_location(&term.row_text(row), &cwd)?;
        Some((path, Some((line, col.unwrap_or(1)))))
    }

    /// Ctrl+click on terminal output: open the `path:line` under the pointer
    /// and hide the overlay. `false` when there is nothing to open there.
    fn terminal_open_at(&mut self, row: u16, col: u16) -> bool {
        let Some((path, target)) = self.terminal_location_at(row, col) else {
            return false;
        };
        self.terminals.set_shown(false);
        self.with_jump(|s| {
            s.open_path(&path, false);
            if let Some((line, col)) = target {
                let area = s.editor_view();
                s.editor.goto(line, Some(col), area);
            }
            s.focus = Focus::Editor;
        });
        true
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
        let shift = mouse.modifiers.contains(KeyModifiers::SHIFT);
        if mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && mouse.modifiers.contains(KeyModifiers::CONTROL)
            && self.terminal_open_at(cell.0, cell.1)
        {
            return true;
        }
        let Some(term) = self.terminals.active_mut() else {
            return false;
        };
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

    /// Note each terminal whose shell has exited. Called each loop.
    pub fn poll_terminal(&mut self) {
        // An exited shell keeps its tab (and transcript) so it can be
        // restarted in place (T666); it is reported once.
        let mut exited = false;
        for (_, term) in self.terminals.iter_mut() {
            exited |= term.take_exit_notice();
        }
        if exited {
            self.status = t!("status.terminal_exited").to_string();
        }
    }

    /// Whether any terminal is running, shown or not (drives the fast poll
    /// cadence).
    #[must_use]
    pub fn terminal_running(&self) -> bool {
        !self.terminals.is_empty()
    }

    /// Whether the active terminal's shell is still running (`false` with
    /// none open, or after it exited and before it is restarted or closed).
    #[must_use]
    pub fn terminal_alive(&self) -> bool {
        self.terminals.active().is_some_and(Terminal::alive)
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
