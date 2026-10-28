//! Tools → HL7 View / HL7 Send: the structured view of the active HL7 v2
//! message, and an MLLP sender whose acknowledgement opens in a read-only tab.
//!
//! The parser, view model and framing live in `vix-hl7`; this module is the
//! App glue. Sending is an outward network action: it is gated by the `http`
//! capability, goes only to a `host:port` the user types, runs on a
//! background thread (a channel polled each event-loop tick, like the HTTP
//! client), and keeps nothing -- message text and replies are never logged or
//! written to disk, and the last endpoint is remembered in memory only.

use vix_input::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};

use super::{App, Focus, rect_contains};

/// An in-flight HL7 send.
#[cfg(feature = "http")]
pub(super) struct Hl7Pending {
    /// The `host:port` shown to the user.
    endpoint: String,
    /// The background thread's outcome.
    rx: std::sync::mpsc::Receiver<Result<Vec<u8>, crate::hl7::net::SendError>>,
}

impl App {
    /// Dispatch the HL7 actions. Returns `true` if handled.
    pub(super) fn run_hl7_action(&mut self, action: &str) -> bool {
        match action {
            "tools.hl7_view" => self.open_hl7_view(),
            #[cfg(feature = "http")]
            "tools.hl7_send" => self.begin_hl7_send(),
            _ => return false,
        }
        true
    }

    // ----- View -----------------------------------------------------------

    /// Parse the active buffer and open the numbered segment/field view on it.
    fn open_hl7_view(&mut self) {
        let Some(text) = self.editor.active_tab().map(crate::editor::Tab::text) else {
            return;
        };
        let msg = crate::hl7::Message::parse(&text);
        if !msg.has_msh() {
            self.status = t!("status.hl7_no_message").to_string();
            return;
        }
        self.hl7_panel = Some(crate::hl7::Panel::open(&msg, self.editor.cursor_line()));
    }

    pub(super) fn hl7_key(&mut self, key: KeyEvent) {
        let page = (self.layout.hl7_panel.height as usize).max(1);
        let Some(p) = self.hl7_panel.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Up => p.up(),
            KeyCode::Down => p.down(),
            KeyCode::PageUp => p.page_up(page),
            KeyCode::PageDown => p.page_down(page),
            KeyCode::Backspace => p.backspace(),
            KeyCode::Char(c) => p.push(c),
            KeyCode::Enter => self.jump_to_hl7_row(),
            KeyCode::Esc => self.hl7_panel = None,
            _ => {}
        }
    }

    pub(super) fn hl7_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let rect = self.layout.hl7_panel;
        if !rect_contains(rect, mouse.column, mouse.row) {
            return;
        }
        let Some(p) = self.hl7_panel.as_mut() else {
            return;
        };
        let idx = p.scroll + (mouse.row - rect.y) as usize;
        if p.select_index(idx) {
            self.jump_to_hl7_row();
        }
    }

    /// Close the view and move the cursor to the highlighted segment/field.
    fn jump_to_hl7_row(&mut self) {
        let Some((line, col)) = self
            .hl7_panel
            .as_ref()
            .and_then(crate::hl7::Panel::selected_row)
            .map(|r| (r.line, r.col))
        else {
            return;
        };
        self.hl7_panel = None;
        let area = self.layout.editor;
        self.editor.goto(line + 1, Some(col + 1), area);
        self.focus = Focus::Editor;
    }
}

#[cfg(feature = "http")]
impl App {
    // ----- Send -----------------------------------------------------------

    /// Ask for the `host:port` to send the active message to.
    fn begin_hl7_send(&mut self) {
        if self.hl7_rx.is_some() {
            self.status = t!("status.hl7_busy").to_string();
            return;
        }
        let Some(text) = self.editor.active_tab().map(crate::editor::Tab::text) else {
            return;
        };
        if !crate::hl7::Message::parse(&text).has_msh() {
            self.status = t!("status.hl7_no_message").to_string();
            return;
        }
        self.prompt = Some(
            super::Prompt::new(
                super::PromptKind::Hl7Send,
                t!("prompt.hl7_send").to_string(),
            )
            .with_input(self.hl7_endpoint.clone()),
        );
    }

    /// The prompt's accept handler: validate the endpoint (the port is
    /// required) and start the send on a background thread.
    pub(super) fn accept_hl7_send(&mut self, input: &str) {
        let Ok(ep) = crate::hl7::net::Endpoint::parse(input) else {
            self.status = t!("status.hl7_bad_endpoint").to_string();
            return;
        };
        let Some(text) = self.editor.active_tab().map(crate::editor::Tab::text) else {
            return;
        };
        if !crate::hl7::Message::parse(&text).has_msh() {
            self.status = t!("status.hl7_no_message").to_string();
            return;
        }
        self.hl7_endpoint = ep.to_string();
        self.status = t!("status.hl7_sending", endpoint = ep.to_string()).to_string();
        let payload = crate::hl7::mllp::to_wire(&text);
        let (tx, rx) = std::sync::mpsc::channel();
        let target = ep.clone();
        std::thread::spawn(move || {
            let opts = crate::hl7::net::SendOptions::default();
            let _ = tx.send(crate::hl7::net::send(&target, &payload, &opts));
        });
        self.hl7_rx = Some(Hl7Pending {
            endpoint: ep.to_string(),
            rx,
        });
    }

    /// Whether an HL7 send is in flight (keeps the event loop polling fast).
    #[must_use]
    pub fn hl7_running(&self) -> bool {
        self.hl7_rx.is_some()
    }

    /// Drain a finished HL7 send: the reply opens in a read-only tab. Called
    /// each event-loop iteration; cheap when none is running.
    pub fn poll_hl7(&mut self) {
        let outcome = {
            let Some(pending) = self.hl7_rx.as_ref() else {
                return;
            };
            match pending.rx.try_recv() {
                Ok(r) => r,
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Err(crate::hl7::net::SendError::Closed)
                }
            }
        };
        let Some(pending) = self.hl7_rx.take() else {
            return;
        };
        match outcome {
            Ok(bytes) => self.show_hl7_reply(&pending.endpoint, &bytes),
            Err(e) => {
                let why = match e.code() {
                    "resolve" => t!("msg.hl7_err_resolve"),
                    "connect" => t!("msg.hl7_err_connect"),
                    "timeout" => t!("msg.hl7_err_timeout"),
                    "closed" => t!("msg.hl7_err_closed"),
                    "framing" => t!("msg.hl7_err_framing"),
                    _ => t!("msg.hl7_err_io"),
                };
                self.messages
                    .error(t!("msg.hl7_error", error = why).to_string());
            }
        }
    }

    /// Open the reply as a read-only tab: an acknowledgement summary line,
    /// then the numbered field view of the reply.
    fn show_hl7_reply(&mut self, endpoint: &str, bytes: &[u8]) {
        let reply = String::from_utf8_lossy(bytes).into_owned();
        let msg = crate::hl7::Message::parse(&reply);
        let ack = crate::hl7::Ack::from_message(&msg);
        let summary = match &ack {
            Some(a) => t!(
                "ui.hl7_ack_summary",
                code = a.code_text.as_str(),
                id = a.control_id.as_str()
            )
            .to_string(),
            None => t!("ui.hl7_ack_none").to_string(),
        };
        let content = format!("{summary}\n\n{}\n", crate::hl7::pretty(&msg));
        let title = t!("ui.hl7_ack_title", endpoint = endpoint).to_string();
        self.open_readonly_text_tab(&title, &content);
        self.status = match ack {
            Some(a) if a.code.is_accept() => {
                t!("status.hl7_reply_accept", code = a.code_text).to_string()
            }
            Some(a) if a.code.is_nak() => {
                t!("status.hl7_reply_nak", code = a.code_text).to_string()
            }
            Some(a) => t!("status.hl7_reply_other", code = a.code_text).to_string(),
            None => t!("ui.hl7_ack_none").to_string(),
        };
    }
}

#[cfg(not(feature = "http"))]
impl App {
    /// Never running: this build has no HL7 sender.
    #[must_use]
    pub fn hl7_running(&self) -> bool {
        false
    }

    /// No reply to drain.
    pub fn poll_hl7(&mut self) {
        debug_assert!(self.layout.hl7_panel.width < u16::MAX);
    }
}
