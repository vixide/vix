//! Round-4 behavior tests for `app/fhir_http.rs` (test-only; a child module so
//! it can build replies and feed the worker channel directly, with no
//! network).

use super::*;
use crate::app::cov4::{app_at, buffer_with, temp};
use crate::messages::Level;

const BUNDLE: &str = r#"{"resourceType":"Bundle","type":"searchset","entry":[
    {"resource":{"resourceType":"Patient","id":"a"}},
    {"resource":{"resourceType":"Patient","id":"b"}}],
  "link":[{"relation":"next","url":"http://127.0.0.1:9/fhir/Patient?page=2"}]}"#;

const OUTCOME: &str = r#"{"resourceType":"OperationOutcome","issue":[
    {"severity":"error","code":"invalid","diagnostics":"bad thing","expression":["Patient.name","Patient.id"]},
    {"severity":"warning","code":"informational"}]}"#;

fn reply(status: u16, body: &str, paging: bool) -> Reply {
    Reply {
        status,
        body: body.to_string(),
        paging,
    }
}

/// A workspace whose `.vix/fhir.json` holds `profile`.
fn with_profile(tag: &str, profile: &str) -> App {
    let dir = temp(tag);
    std::fs::create_dir_all(dir.join(".vix")).unwrap();
    std::fs::write(dir.join(".vix/fhir.json"), profile).unwrap();
    app_at(&dir)
}

fn errors(app: &App) -> usize {
    app.messages
        .items
        .iter()
        .filter(|m| m.level == Level::Error)
        .count()
}

fn error_text(app: &App, needle: &str) -> bool {
    app.messages
        .items
        .iter()
        .any(|m| m.level == Level::Error && m.text.contains(needle))
}

#[test]
fn a_missing_profile_is_a_status_not_an_error() {
    let mut a = app_at(&temp("r4-fhir-noprofile"));
    let logged = a.messages.items.len();
    assert!(a.fhir_profile().is_none());
    assert_eq!(a.status, t!("status.fhir_no_profile").to_string());
    assert_eq!(a.messages.items.len(), logged);
}

#[test]
fn an_unreadable_or_malformed_profile_is_logged() {
    // A directory where the file should be: readable path, unreadable file.
    let dir = temp("r4-fhir-unreadable");
    std::fs::create_dir_all(dir.join(".vix").join("fhir.json")).unwrap();
    let mut a = app_at(&dir);
    a.status.clear();
    let logged = a.messages.items.len();
    assert!(a.fhir_profile().is_none());
    assert_eq!(a.messages.items.len(), logged + 1, "an error was logged");
    assert!(a.status.is_empty(), "not reported as merely missing");

    let mut bad = with_profile("r4-fhir-malformed", "{not json");
    let before = errors(&bad);
    assert!(bad.fhir_profile().is_none());
    assert_eq!(errors(&bad), before + 1);
    let mut insecure = with_profile(
        "r4-fhir-insecure",
        r#"{"base_url":"http://example.org/fhir","auth":{"type":"bearer","token_env":"FHIR_TOKEN"}}"#,
    );
    let before = errors(&insecure);
    assert!(
        insecure.fhir_profile().is_none(),
        "bearer over plain http is refused"
    );
    assert_eq!(errors(&insecure), before + 1);
}

#[test]
fn a_good_profile_loads() {
    let mut a = with_profile(
        "r4-fhir-good",
        r#"{"base_url":"https://example.org/fhir/"}"#,
    );
    let profile = a.fhir_profile().expect("loads");
    assert_eq!(profile.base_url, "https://example.org/fhir");
}

#[test]
fn bad_commands_and_profiles_stop_before_any_request() {
    let mut a = with_profile(
        "r4-fhir-badcmd",
        r#"{"base_url":"https://example.org/fhir"}"#,
    );
    let before = errors(&a);
    a.fhir_send("frobnicate Patient");
    assert_eq!(errors(&a), before + 1);
    assert!(error_text(&a, "frobnicate"));
    assert!(a.fhir_rx.is_none(), "nothing was sent");

    // No profile at all: the command parses but nothing is sent.
    let mut none = app_at(&temp("r4-fhir-nosend"));
    none.fhir_send("read Patient/1");
    assert_eq!(none.status, t!("status.fhir_no_profile").to_string());
    assert!(none.fhir_rx.is_none());

    // A create takes the active buffer as its body, but still needs a profile.
    let mut create = app_at(&temp("r4-fhir-create"));
    buffer_with(&mut create, r#"{"resourceType":"Patient"}"#, 0);
    create.fhir_send("create Patient");
    assert_eq!(create.status, t!("status.fhir_no_profile").to_string());
}

#[test]
fn following_next_needs_a_bundle_with_a_next_link_and_a_profile() {
    let mut a = app_at(&temp("r4-fhir-next"));
    a.fhir_follow_next();
    assert_eq!(a.status, t!("status.fhir_no_next").to_string());

    // With a bundle that has a next link but no profile: the profile status.
    a.fhir_show(reply(200, BUNDLE, true));
    assert!(a.fhir_bundle.is_some());
    a.status.clear();
    a.fhir_follow_next();
    assert_eq!(a.status, t!("status.fhir_no_profile").to_string());

    // A profile on another host refuses the link instead of sending it.
    let mut other = with_profile(
        "r4-fhir-next-other",
        r#"{"base_url":"https://example.org/fhir"}"#,
    );
    other.fhir_show(reply(200, BUNDLE, true));
    let before = errors(&other);
    other.fhir_follow_next();
    assert!(
        other.fhir_rx.is_none(),
        "a link off the profile's origin is not followed"
    );
    assert_eq!(errors(&other), before + 1);
}

#[test]
fn polling_reports_a_dropped_worker_and_shows_a_finished_one() {
    let mut a = app_at(&temp("r4-fhir-poll"));
    a.poll_fhir(); // nothing in flight

    // Still running: left in place.
    let (tx, rx) = std::sync::mpsc::channel();
    a.fhir_rx = Some(rx);
    a.poll_fhir();
    assert!(a.fhir_rx.is_some());

    // A finished exchange is shown and the receiver dropped.
    tx.send(Ok(reply(
        200,
        r#"{"resourceType":"Patient","id":"x"}"#,
        false,
    )))
    .unwrap();
    a.poll_fhir();
    assert!(a.fhir_rx.is_none());
    assert_eq!(a.status, t!("status.fhir_done", status = 200).to_string());

    // A worker that vanished without answering is a transport error.
    let (tx, rx) = std::sync::mpsc::channel::<Result<Reply, FhirError>>();
    a.fhir_rx = Some(rx);
    drop(tx);
    a.poll_fhir();
    assert!(a.fhir_rx.is_none());
    assert!(error_text(&a, &FhirError::Transport.to_string()));

    // A failure from the worker is logged too.
    let (tx, rx) = std::sync::mpsc::channel();
    a.fhir_rx = Some(rx);
    tx.send(Err(FhirError::Transport)).unwrap();
    let before = a.messages.items.len();
    a.poll_fhir();
    assert_eq!(a.messages.items.len(), before + 1);
}

#[test]
fn replies_open_in_the_right_kind_of_tab() {
    let mut a = app_at(&temp("r4-fhir-show"));
    let tabs = a.editor.tabs.len();

    // A resource: pretty JSON in an unsaved tab.
    a.fhir_show(reply(200, r#"{"resourceType":"Patient","id":"p1"}"#, false));
    assert_eq!(a.editor.tabs.len(), tabs + 1);
    let text = a.editor.active_tab().unwrap().text();
    assert!(text.contains("\"id\": \"p1\""), "{text}");
    assert!(a.editor.active_tab().unwrap().path.is_none());
    assert_eq!(a.status, t!("status.fhir_done", status = 200).to_string());

    // A Bundle: the count in the status, kept for the viewer.
    a.fhir_show(reply(200, BUNDLE, false));
    assert_eq!(a.status, t!("status.fhir_bundle", count = 2).to_string());
    assert_eq!(a.fhir_last.as_deref(), Some(BUNDLE));

    // An OperationOutcome: readable issue lines first, then the JSON.
    a.fhir_show(reply(422, OUTCOME, false));
    let text = a.editor.active_tab().unwrap().text();
    let first = text.lines().next().unwrap();
    assert!(
        first.contains("error") && first.contains("invalid"),
        "{first}"
    );
    assert!(text.contains(&t!("msg.fhir_issue_at", expression = "Patient.name").to_string()));
    assert!(text.contains(&t!("msg.fhir_issue_at", expression = "Patient.id").to_string()));
    assert!(text.contains("\"resourceType\": \"OperationOutcome\""));
    assert_eq!(a.status, t!("status.fhir_done", status = 422).to_string());

    // Not JSON: opened as it came.
    a.fhir_show(reply(500, "plain failure text", false));
    assert_eq!(a.editor.active_tab().unwrap().text(), "plain failure text");

    // An empty body (a successful delete) opens nothing.
    let count = a.editor.tabs.len();
    a.fhir_show(reply(204, "  \n", false));
    assert_eq!(a.editor.tabs.len(), count);
    assert_eq!(a.status, t!("status.fhir_done", status = 204).to_string());
    assert_eq!(a.focus, Focus::Editor);
}

#[test]
fn a_paging_reply_replaces_the_viewer_or_reports_a_bad_bundle() {
    let mut a = app_at(&temp("r4-fhir-paging"));
    let tabs = a.editor.tabs.len();
    a.fhir_show(reply(200, BUNDLE, true));
    assert_eq!(
        a.fhir_bundle.as_ref().map(crate::fhir::BundlePanel::len),
        Some(2)
    );
    assert_eq!(a.editor.tabs.len(), tabs, "paging opens no tab");
    assert_eq!(a.status, t!("status.fhir_done", status = 200).to_string());

    a.fhir_bundle = None;
    let before = errors(&a);
    a.fhir_show(reply(200, "not a bundle", true));
    assert!(a.fhir_bundle.is_none());
    assert_eq!(errors(&a), before + 1);
}

#[test]
fn outcome_text_lists_issues_with_their_locations() {
    let oo = crate::fhir::parse_operation_outcome(OUTCOME).unwrap();
    let text = outcome_text(&oo);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 4, "{text}");
    assert!(lines[0].ends_with(": bad thing"), "{}", lines[0]);
    assert!(lines[1].starts_with("    ") && lines[2].starts_with("    "));
    assert!(
        !lines[3].starts_with(' ') && !lines[3].contains(": "),
        "no detail, no colon"
    );
}
