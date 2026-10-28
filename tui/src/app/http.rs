//! HTTP client: send the active `.http`-style buffer and poll the response.

use super::{App, Focus};

impl App {
    /// Dispatch the HTTP client action. Returns `true` if handled.
    pub(super) fn run_http_action(&mut self, action: &str) -> bool {
        match action {
            "tools.http_send" => self.http_send(),
            "tools.http_copy_curl" => self.http_copy_curl(),
            _ => return false,
        }
        true
    }

    // ----- HTTP client ----------------------------------------------------

    /// Prepare the request under the cursor: pick its `###` block, expand
    /// `{{variables}}` (file variables, system variables, earlier named
    /// responses), then parse it. Returns the request and its `# @name`.
    /// On failure the reason is already in the status line or message log.
    fn http_prepare(&mut self) -> Option<(crate::http_client::Request, Option<String>)> {
        use crate::http_client::rest;
        let tab = self.editor.active_tab()?;
        let text = tab.text();
        let base = tab
            .path
            .as_ref()
            .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
            .unwrap_or_else(|| self.root.clone());
        let line = self.editor.cursor_line();
        let Some(block) = rest::block_at(&text, line) else {
            self.status = t!("status.http_no_request").to_string();
            return None;
        };
        let block_text = rest::block_text(&text, block);
        let name = rest::request_name(&block_text);
        let vars = rest::file_variables(&text);
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|d| i64::try_from(d.as_millis()).ok())
            .unwrap_or(0);
        let ctx = rest::Context {
            vars: &vars,
            named: &self.http_named,
            base_dir: Some(&base),
            now_ms,
        };
        let expanded = match rest::expand(&block_text, &ctx) {
            Ok(e) => e,
            Err(e) => {
                self.messages
                    .error(t!("msg.http_error", error = e.to_string()).to_string());
                return None;
            }
        };
        let Some(mut req) = crate::http_client::parse_request(&expanded) else {
            self.status = t!("status.http_no_request").to_string();
            return None;
        };
        if let Err(e) = rest::load_body_file(&mut req, &base) {
            self.messages
                .error(t!("msg.http_error", error = e).to_string());
            return None;
        }
        rest::encode_basic_auth(&mut req);
        Some((req, name))
    }

    /// Send the request under the cursor (the whole buffer when it has no
    /// `###` separators) on a background thread; the response opens in a new
    /// tab, and a `# @name`d request's response is kept for `{{name.response…}}`.
    pub(super) fn http_send(&mut self) {
        let Some((req, name)) = self.http_prepare() else {
            return;
        };
        self.status = t!("status.http_sending", url = req.url).to_string();
        self.http_pending_name = name;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(crate::http_client::send_response(&req).map_err(|e| e.to_string()));
        });
        self.http_rx = Some(rx);
    }

    /// Copy the request under the cursor to the clipboard as a `curl` command.
    pub(super) fn http_copy_curl(&mut self) {
        let Some((req, _)) = self.http_prepare() else {
            return;
        };
        let curl = crate::http_client::rest::to_curl(&req);
        if let Some(tab) = self.editor.active_tab_mut() {
            tab.editor.copy_to_clipboard(&curl);
        }
        self.status = t!("status.http_curl_copied").to_string();
    }

    /// Whether an HTTP request is in flight (keeps the event loop polling fast).
    #[must_use]
    pub fn http_running(&self) -> bool {
        self.http_rx.is_some() || self.fhir_rx.is_some()
    }

    /// Drain a finished HTTP response into a new tab. Called each event-loop
    /// iteration; cheap when none is running.
    pub fn poll_http(&mut self) {
        self.poll_fhir();
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
        let name = self.http_pending_name.take();
        match result {
            Ok(resp) => {
                let body = resp.format();
                if let Some(name) = name {
                    self.http_named.insert(name, resp);
                }
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
