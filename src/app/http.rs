//! HTTP client: send the active `.http`-style buffer and poll the response.

use super::{App, Focus};

impl App {
    /// Dispatch the HTTP client action. Returns `true` if handled.
    pub(super) fn run_http_action(&mut self, action: &str) -> bool {
        match action {
            "tools.http_send" => self.http_send(),
            _ => return false,
        }
        true
    }

    // ----- HTTP client ----------------------------------------------------

    /// Send the HTTP request described by the active buffer (a `.http`-style
    /// document) on a background thread; the response opens in a new tab.
    pub(super) fn http_send(&mut self) {
        let Some(text) = self.editor.active_tab().map(crate::editor::Tab::text) else {
            return;
        };
        let Some(req) = crate::http_client::parse_request(&text) else {
            self.status = t!("status.http_no_request").to_string();
            return;
        };
        self.status = t!("status.http_sending", url = req.url).to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(crate::http_client::send(&req));
        });
        self.http_rx = Some(rx);
    }

    /// Whether an HTTP request is in flight (keeps the event loop polling fast).
    #[must_use]
    pub fn http_running(&self) -> bool {
        self.http_rx.is_some()
    }

    /// Drain a finished HTTP response into a new tab. Called each event-loop
    /// iteration; cheap when none is running.
    pub fn poll_http(&mut self) {
        let result = {
            let Some(rx) = self.http_rx.as_ref() else {
                return;
            };
            match rx.try_recv() {
                Ok(r) => r,
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Err(t!("status.http_failed").to_string())
                }
            }
        };
        self.http_rx = None;
        match result {
            Ok(body) => {
                self.editor.new_tab_with_content(&body);
                self.focus = Focus::Editor;
                self.status = t!("status.http_done").to_string();
            }
            Err(e) => self
                .messages
                .error(t!("msg.http_error", error = e).to_string()),
        }
    }
}
