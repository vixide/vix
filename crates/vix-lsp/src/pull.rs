//! Per-document pull diagnostics (T619): `textDocument/diagnostic` after
//! `didOpen`, after a debounced `didChange`, and on
//! `workspace/diagnostic/refresh`, merged into the same per-(file, server)
//! map push diagnostics fill. See `spec/index.md` § Pull diagnostics.

use std::time::{Duration, Instant};

use serde_json::Value;
use vix_lsp_core::message::{self, DiagnosticReport};

use crate::{Lsp, LspEvent, Pending, Target, canonical, uri_to_path};

/// How long after the last `didChange` to a document its pull is sent, so a
/// burst of keystrokes sends one request.
pub(crate) const PULL_DEBOUNCE: Duration = Duration::from_millis(250);

impl Lsp {
    /// Ask server `lang` for `uri`'s diagnostics now, with the last report's
    /// `resultId` as `previousResultId`. Nothing is sent when the document is
    /// not open on that server.
    pub(crate) fn pull_diagnostics(&mut self, lang: &str, uri: &str) {
        self.pull_due.remove(&(lang.to_string(), uri.to_string()));
        let Some(server) = self.servers.get_mut(lang) else {
            return;
        };
        let Some(&version) = server.docs.get(uri) else {
            return;
        };
        let previous = server.diagnostic_result_ids.get(uri).map(String::as_str);
        let params = message::document_diagnostic_params(uri, previous);
        let id = server.alloc_id();
        server.pending.insert(id, Pending::DocumentDiagnostics);
        server.targets.insert(
            id,
            Target {
                path: uri_to_path(uri),
                uri: uri.to_string(),
                version,
            },
        );
        server.send(message::request(id, "textDocument/diagnostic", &params));
    }

    /// Pull `uri`'s diagnostics from `lang` once [`PULL_DEBOUNCE`] has passed
    /// without a further change.
    pub(crate) fn schedule_pull(&mut self, lang: &str, uri: &str) {
        self.pull_due.insert(
            (lang.to_string(), uri.to_string()),
            Instant::now() + PULL_DEBOUNCE,
        );
    }

    /// Pull every document open on `lang` (after `initialize`, and on
    /// `workspace/diagnostic/refresh`).
    pub(crate) fn pull_all_open(&mut self, lang: &str) {
        let Some(server) = self.servers.get(lang) else {
            return;
        };
        if !server.diagnostic_provider {
            return;
        }
        let mut uris: Vec<String> = server.docs.keys().cloned().collect();
        uris.sort();
        for uri in uris {
            self.pull_diagnostics(lang, &uri);
        }
    }

    /// Send every debounced pull whose time has come (called by [`Lsp::poll`]).
    pub(crate) fn send_due_pulls(&mut self) {
        if self.pull_due.is_empty() {
            return;
        }
        let now = Instant::now();
        let mut due: Vec<(String, String)> = self
            .pull_due
            .iter()
            .filter(|(_, at)| **at <= now)
            .map(|(key, _)| key.clone())
            .collect();
        due.sort();
        for (lang, uri) in due {
            self.pull_diagnostics(&lang, &uri);
        }
    }

    /// Apply a current `textDocument/diagnostic` reply for `uri` from `lang`:
    /// a full report replaces that server's diagnostics for the file, an
    /// unchanged one keeps them; either way its `resultId` is kept for the
    /// next pull. Related documents' reports merge the same way.
    pub(crate) fn apply_pull_reply(
        &mut self,
        lang: &str,
        uri: &str,
        result: &Value,
        events: &mut Vec<LspEvent>,
    ) {
        let Some((main, related)) = message::parse_document_diagnostic_report(result) else {
            return;
        };
        self.apply_report(lang, uri, main, events);
        for (other, report) in related {
            self.apply_report(lang, &other, report, events);
        }
    }

    /// Apply one document's report (see [`Lsp::apply_pull_reply`]).
    fn apply_report(
        &mut self,
        lang: &str,
        uri: &str,
        report: DiagnosticReport,
        events: &mut Vec<LspEvent>,
    ) {
        let result_id = match report {
            DiagnosticReport::Full { result_id, items } => {
                let path = canonical(&uri_to_path(uri));
                self.set_diagnostics(lang, &path, items);
                events.push(LspEvent::Diagnostics(path));
                result_id
            }
            DiagnosticReport::Unchanged { result_id } => Some(result_id),
            // `DiagnosticReport` is `#[non_exhaustive]`.
            _ => None,
        };
        let Some(server) = self.servers.get_mut(lang) else {
            return;
        };
        match result_id {
            Some(id) => {
                server.diagnostic_result_ids.insert(uri.to_string(), id);
            }
            None => {
                server.diagnostic_result_ids.remove(uri);
            }
        }
    }
}
