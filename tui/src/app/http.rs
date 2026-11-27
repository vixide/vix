//! HTTP client: send the active `.http`-style buffer and poll the response.

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

use super::{App, Confirm, ConfirmKind, Focus, HttpConsentKey, HttpEnvChooser};
use crate::app::chooser::ChooserSlot;
use crate::geo::Geo;
use vix_input::{KeyCode, KeyEvent, MouseEvent};

/// The local zone's offset from UTC, in minutes, at `now_ms`.
fn local_offset_minutes(now_ms: i64) -> i32 {
    jiff::Timestamp::from_millisecond(now_ms)
        .ok()
        .map_or(0, |ts| {
            jiff::tz::TimeZone::system()
                .to_offset(ts)
                .seconds()
                .quot(60)
        })
}

/// A request prepared for sending or for its preview (T848).
struct HttpPrepared {
    /// The parsed, expanded request.
    request: crate::http_client::Request,
    /// Its `# @name`, if any.
    name: Option<String>,
    /// Its timeouts (defaults, or its `# @timeout` lines).
    timeouts: crate::http_client::Timeouts,
    /// The secret variables it uses (only filled for a masked preview).
    secrets: Vec<String>,
}

/// `scheme://host[:port]` of `url`, lower-cased: what a consent covers.
fn url_origin(url: &str) -> String {
    let (scheme, rest) = url.split_once("://").unwrap_or(("", url));
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    // Credentials before an `@` are not part of the origin.
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    format!("{scheme}://{host}").to_ascii_lowercase()
}

impl App {
    /// Dispatch the HTTP client action. Returns `true` if handled.
    pub(super) fn run_http_action(&mut self, action: &str) -> bool {
        match action {
            "tools.http_send" => self.http_send(),
            "tools.http_copy_curl" => self.http_copy_curl(),
            "tools.http_environment" => self.open_http_env_chooser(),
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
        self.http_prepare_with(false).map(|p| (p.request, p.name))
    }

    /// [`App::http_prepare`], plus the block's timeouts and the secret
    /// variables it uses (T848). With `masked`, no secret is read: each is
    /// [`rest::MASK`](crate::http_client::rest::MASK) in the request, and a
    /// `< file` body is left unread.
    fn http_prepare_with(&mut self, masked: bool) -> Option<HttpPrepared> {
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
        let timeouts = match rest::timeouts(&block_text) {
            Ok(t) => t,
            Err(e) => {
                self.messages
                    .error(t!("msg.http_error", error = e.to_string()).to_string());
                return None;
            }
        };
        let file_vars = rest::file_variables(&text);
        let vars = self.http_variables(&file_vars)?;
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
            local_offset_min: local_offset_minutes(now_ms),
        };
        let expanded = if masked {
            rest::expand_masked(&block_text, &ctx).map(|m| (m.text, m.secrets))
        } else {
            rest::expand(&block_text, &ctx).map(|e| (e, Vec::new()))
        };
        let (expanded, secrets) = match expanded {
            Ok(e) => e,
            Err(rest::VarError::AzureCliMissing) => {
                self.messages.error(t!("msg.http_az_missing").to_string());
                return None;
            }
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
        if !masked && let Err(e) = rest::load_body_file(&mut req, &base) {
            self.messages
                .error(t!("msg.http_error", error = e.to_string()).to_string());
            return None;
        }
        rest::encode_basic_auth(&mut req);
        Some(HttpPrepared {
            request: req,
            name,
            timeouts,
            secrets,
        })
    }

    /// The variables for a request: `$shared`, then the file's own, then the
    /// active environment's (last definition wins). With no environments file
    /// this is just the file's variables. A malformed file is reported and
    /// stops the send, rather than quietly sending with the wrong values.
    fn http_variables(&mut self, file: &[(String, String)]) -> Option<Vec<(String, String)>> {
        use crate::http_client::env;
        match env::load(&self.root) {
            Ok(Some(envs)) => {
                let active = env::read_active(&self.root);
                Some(envs.variables(active.as_deref(), file))
            }
            Ok(None) => Some(file.to_vec()),
            Err(e) => {
                self.messages
                    .error(t!("msg.http_error", error = e.to_string()).to_string());
                None
            }
        }
    }

    /// Open the environment chooser, highlighting the active environment.
    fn open_http_env_chooser(&mut self) {
        use crate::http_client::env;
        let envs = match env::load(&self.root) {
            Ok(Some(envs)) => envs,
            Ok(None) => {
                self.status = t!("status.http_env_missing").to_string();
                return;
            }
            Err(e) => {
                self.messages
                    .error(t!("msg.http_error", error = e.to_string()).to_string());
                return;
            }
        };
        let names: Vec<String> = envs.names().into_iter().map(String::from).collect();
        let selected = env::read_active(&self.root)
            .and_then(|a| names.iter().position(|n| *n == a))
            .map_or(0, |i| i.plus(1));
        self.chooser
            .open_http_env(HttpEnvChooser { names, selected });
    }

    /// Handle a key in the HTTP-environment chooser: Up/Down move, Enter applies the
    /// environment, Esc closes. Index `names.len()` is the extra "no environment" row.
    pub(super) fn http_env_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up => {
                if let Some(c) = self.chooser.http_env_mut() {
                    c.selected = c.selected.minus(1);
                }
            }
            KeyCode::Down => {
                if let Some(c) = self.chooser.http_env_mut() {
                    c.selected = c.selected.plus(1).min(c.names.len());
                }
            }
            KeyCode::Enter => self.apply_http_env(),
            KeyCode::Esc => self.chooser = None,
            _ => {}
        }
    }

    /// Handle a mouse event in the HTTP-environment chooser: clicking a row applies it.
    pub(super) fn http_env_mouse(&mut self, mouse: MouseEvent) {
        if let Some(idx) = self.chooser_row(mouse)
            && let Some(c) = self.chooser.http_env_mut()
            && idx <= c.names.len()
        {
            c.selected = idx;
            self.apply_http_env();
        }
    }

    /// Make the highlighted row the workspace's active environment (row 0
    /// clears it) and remember it in `.vix/http-env.active`.
    fn apply_http_env(&mut self) {
        let Some(c) = self.chooser.take_http_env() else {
            return;
        };
        let name = c.selected.checked_sub(1).and_then(|i| c.names.get(i));
        match crate::http_client::env::write_active(&self.root, name.map(String::as_str)) {
            Ok(()) => {
                self.status = match name {
                    Some(n) => t!("status.http_env_set", name = n).to_string(),
                    None => t!("status.http_env_cleared").to_string(),
                };
            }
            Err(e) => self
                .messages
                .error(t!("msg.http_error", error = e.to_string()).to_string()),
        }
    }

    /// Send the request under the cursor (the whole buffer when it has no
    /// `###` separators) on a background thread; the response opens in a new
    /// tab, and a `# @name`d request's response is kept for `{{name.response…}}`.
    ///
    /// A request that uses a secret variable (`$processEnv`, `$dotenv`,
    /// `$aadToken`) is first shown masked in a confirmation (T848), before
    /// any secret is read: every time in an untrusted workspace, and once per
    /// file and origin per session in a trusted one.
    pub(super) fn http_send(&mut self) {
        let Some(preview) = self.http_prepare_with(true) else {
            return;
        };
        if !preview.secrets.is_empty() {
            let key = HttpConsentKey {
                file: self.active_path(),
                origin: url_origin(&preview.request.url),
            };
            let once = self.http_consent_once.take();
            let approved = once.as_ref() == Some(&key)
                || (self.workspace_trusted() && self.http_consented.contains(&key));
            if !approved {
                self.http_confirm_secrets(&preview, key);
                return;
            }
        }
        let Some(prepared) = self.http_prepare_with(false) else {
            return;
        };
        let HttpPrepared {
            request: req,
            name,
            timeouts,
            ..
        } = prepared;
        self.status = t!("status.http_sending", url = req.url).to_string();
        self.http_pending_name = name;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(
                crate::http_client::send_response_with(&req, timeouts).map_err(|e| e.to_string()),
            );
        });
        self.http_rx = Some(rx);
    }

    /// Ask before sending a request that uses secret variables (T848),
    /// showing the masked request: method, URL and which secrets fill it.
    fn http_confirm_secrets(&mut self, preview: &HttpPrepared, key: HttpConsentKey) {
        let request = format!("{} {}", preview.request.method, preview.request.url);
        let variables = preview.secrets.join(", ");
        let message = if self.workspace_trusted() {
            t!(
                "confirm.http_secrets",
                request = request,
                variables = variables
            )
        } else {
            t!(
                "confirm.http_secrets_untrusted",
                request = request,
                variables = variables
            )
        };
        self.confirm = Some(Confirm {
            message: message.to_string(),
            paths: key.file.iter().cloned().collect(),
            kind: ConfirmKind::HttpSendSecrets,
        });
        self.http_consent_pending = Some(key);
    }

    /// Keys for the secret-variable confirmation (T848): `y` sends, `n`/Esc
    /// cancels. Returns whether the confirmation was open.
    pub(super) fn http_confirm_key(&mut self, key: KeyEvent) -> bool {
        if self.confirm.as_ref().map(|c| c.kind) != Some(ConfirmKind::HttpSendSecrets) {
            return false;
        }
        match key.code {
            KeyCode::Char('y' | 'Y') => {
                self.confirm = None;
                if let Some(consent) = self.http_consent_pending.take() {
                    // A trusted workspace remembers the answer for this file
                    // and origin; an untrusted one asks again next time.
                    if self.workspace_trusted() {
                        self.http_consented.insert(consent.clone());
                    }
                    self.http_consent_once = Some(consent);
                }
                self.http_send();
            }
            KeyCode::Char('n' | 'N') | KeyCode::Esc => {
                self.confirm = None;
                self.http_consent_pending = None;
                self.status = t!("status.http_send_cancelled").to_string();
            }
            _ => {}
        }
        true
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

#[cfg(test)]
#[path = "tests_r3_http.rs"]
mod r3_tests;
