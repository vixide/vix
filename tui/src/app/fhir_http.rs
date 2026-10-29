//! FHIR client networking (T684, feature `http`): load the profile, send the
//! request on a background thread through `vix-fhir`'s network layer (which
//! rides on `vix-http-client`), and show the reply.
//!
//! Privacy: nothing here logs, caches or persists a response. Replies open in
//! unsaved tabs, the last Bundle is kept in memory only, and status/error
//! messages name the interaction and HTTP status, never a URL, body or token.

use crate::fhir::{
    BuiltRequest, Content, FhirError, OperationOutcome, Profile, Spec, classify, net, parse_bundle,
    parse_operation_outcome, pretty,
};

use super::{App, Focus};

/// A finished exchange, handed from the worker thread to the event loop.
pub(super) struct Reply {
    /// HTTP status code.
    status: u16,
    /// Response body.
    body: String,
    /// Whether this answers a Bundle paging link (replace the open viewer).
    paging: bool,
}

/// The process environment, for the profile's `*_env` secret references.
fn process_env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

impl App {
    /// Load `.vix/fhir.json`. On failure the reason is already in the status
    /// line or message log.
    fn fhir_profile(&mut self) -> Option<Profile> {
        let path = self.root.join(".vix").join("fhir.json");
        let text = match crate::authority::current().read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.status = t!("status.fhir_no_profile").to_string();
                return None;
            }
            Err(e) => {
                self.messages
                    .error(t!("msg.fhir_error", error = e.to_string()).to_string());
                return None;
            }
        };
        match Profile::parse(&text) {
            Ok(p) => Some(p),
            Err(e) => {
                self.fhir_report(&e);
                None
            }
        }
    }

    /// Put a typed FHIR error in the message log.
    fn fhir_report(&mut self, e: &FhirError) {
        self.messages
            .error(t!("msg.fhir_error", error = e.to_string()).to_string());
    }

    /// Run the line typed at the FHIR Request prompt: build the request (a
    /// Create/Update sends the active buffer as its body) and send it.
    pub(super) fn fhir_send(&mut self, input: &str) {
        let mut spec = match Spec::parse_command(input) {
            Ok(s) => s,
            Err(e) => {
                self.fhir_report(&e);
                return;
            }
        };
        if spec.needs_body() {
            let text = self.editor.active_tab().map(crate::editor::Tab::text);
            spec.body = text;
        }
        let Some(profile) = self.fhir_profile() else {
            return;
        };
        match spec.build(&profile) {
            Ok(built) => self.fhir_spawn(profile, built, false),
            Err(e) => self.fhir_report(&e),
        }
    }

    /// Fetch the open Bundle's `next` page.
    pub(super) fn fhir_follow_next(&mut self) {
        let Some(link) = self
            .fhir_bundle
            .as_ref()
            .and_then(|p| p.next_link().map(String::from))
        else {
            self.status = t!("status.fhir_no_next").to_string();
            return;
        };
        let Some(profile) = self.fhir_profile() else {
            return;
        };
        match profile.link_request(&link) {
            Ok(built) => self.fhir_spawn(profile, built, true),
            Err(e) => self.fhir_report(&e),
        }
    }

    /// Send `built` on a background thread.
    fn fhir_spawn(&mut self, profile: Profile, built: BuiltRequest, paging: bool) {
        self.status = t!("status.fhir_sending").to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = net::execute(&profile, &built, &process_env).map(|r| Reply {
                status: r.status,
                body: r.body,
                paging,
            });
            let _ = tx.send(result);
        });
        self.fhir_rx = Some(rx);
    }

    /// Drain a finished FHIR exchange. Called from `poll_http` each iteration.
    pub(super) fn poll_fhir(&mut self) {
        let result = {
            let Some(rx) = self.fhir_rx.as_ref() else {
                return;
            };
            match rx.try_recv() {
                Ok(r) => r,
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Err(FhirError::Transport),
            }
        };
        self.fhir_rx = None;
        match result {
            Ok(reply) => self.fhir_show(reply),
            Err(e) => self.fhir_report(&e),
        }
    }

    /// Present a reply: a paging reply replaces the Bundle viewer's content;
    /// anything else opens in a tab (JSON for resources and Bundles, readable
    /// issue lines first for an `OperationOutcome`).
    fn fhir_show(&mut self, reply: Reply) {
        self.focus = Focus::Editor;
        if reply.paging {
            match parse_bundle(&reply.body) {
                Ok(b) => {
                    self.fhir_last = Some(reply.body);
                    self.fhir_bundle = Some(crate::fhir::BundlePanel::open(b));
                    self.status = t!("status.fhir_done", status = reply.status).to_string();
                }
                Err(e) => self.fhir_report(&e),
            }
            return;
        }
        if reply.body.trim().is_empty() {
            self.status = t!("status.fhir_done", status = reply.status).to_string();
            return;
        }
        match classify(&reply.body) {
            Ok(Content::OperationOutcome) => {
                let text = parse_operation_outcome(&reply.body)
                    .map(|oo| outcome_text(&oo))
                    .unwrap_or_default();
                let json = pretty(&reply.body).unwrap_or_default();
                self.editor
                    .new_tab_with_content_as(&format!("{text}\n{json}\n"), Some("outcome.json"));
            }
            Ok(Content::Bundle) => {
                let count = parse_bundle(&reply.body).map_or(0, |b| b.entries.len());
                let json = pretty(&reply.body).unwrap_or_else(|_| reply.body.clone());
                self.editor
                    .new_tab_with_content_as(&json, Some("bundle.json"));
                self.fhir_last = Some(reply.body);
                self.status = t!("status.fhir_bundle", count = count).to_string();
                return;
            }
            Ok(Content::Resource(_)) => {
                let json = pretty(&reply.body).unwrap_or_else(|_| reply.body.clone());
                self.editor
                    .new_tab_with_content_as(&json, Some("resource.json"));
            }
            Err(_) => self.editor.new_tab_with_content(&reply.body),
        }
        self.status = t!("status.fhir_done", status = reply.status).to_string();
    }
}

/// An `OperationOutcome` as readable lines: `severity (code): detail`, with an
/// indented `at <expression>` line for each location.
fn outcome_text(oo: &OperationOutcome) -> String {
    let mut out = String::new();
    for issue in &oo.issues {
        out.push_str(&t!(
            "msg.fhir_issue",
            severity = issue.severity,
            code = issue.code
        ));
        if !issue.detail.is_empty() {
            out.push_str(": ");
            out.push_str(&issue.detail);
        }
        out.push('\n');
        for expr in &issue.expression {
            out.push_str("    ");
            out.push_str(&t!("msg.fhir_issue_at", expression = expr));
            out.push('\n');
        }
    }
    out
}
