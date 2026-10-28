// Shared fixtures/helpers live in `common.rs`.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use crate::integration::common::*;

const BUNDLE: &str = r#"{
  "resourceType": "Bundle", "type": "searchset", "total": 2,
  "link": [{"relation": "next", "url": "NEXT"}],
  "entry": [
    {"resource": {"resourceType": "Patient", "id": "a", "name": [{"family": "Smith", "given": ["Ann"]}]}},
    {"resource": {"resourceType": "Patient", "id": "b", "name": [{"family": "Jones"}]}}
  ]
}"#;

fn active_text(app: &App) -> String {
    app.editor.active_tab().unwrap().text()
}

#[test]
fn bundle_viewer_lists_entries_and_opens_one_in_a_tab() {
    let mut app = app_at(Path::new("."));
    buffer_with(&mut app, &BUNDLE.replace("NEXT", "https://x/next"), 0);
    app.run_action("tools.http_fhir_bundle");
    let panel = app
        .fhir_bundle
        .as_ref()
        .expect("viewer opens on the buffer");
    assert_eq!(panel.len(), 2);
    assert_eq!(panel.bundle.total, Some(2));

    app.on_key(keycode(KeyCode::Down));
    assert_eq!(app.fhir_bundle.as_ref().unwrap().selected, 1);
    let tabs_before = app.editor.tabs.len();
    app.on_key(keycode(KeyCode::Enter));
    assert!(
        app.fhir_bundle.is_none(),
        "Enter opens the entry and closes"
    );
    assert_eq!(app.editor.tabs.len(), tabs_before + 1);
    let text = active_text(&app);
    assert!(
        text.contains("\"id\": \"b\""),
        "pretty-printed resource: {text}"
    );
    assert!(
        app.editor.active_tab().unwrap().path.is_none(),
        "never a file"
    );

    // The Bundle is remembered (in memory) so the viewer reopens from the
    // entry tab.
    app.run_action("tools.http_fhir_bundle");
    assert!(app.fhir_bundle.is_some());
    app.on_key(esc());
    assert!(app.fhir_bundle.is_none());
}

#[test]
fn bundle_viewer_without_a_bundle_says_so() {
    let mut app = app_at(Path::new("."));
    buffer_with(&mut app, "just prose", 0);
    app.run_action("tools.http_fhir_bundle");
    assert!(app.fhir_bundle.is_none());
    assert!(!app.status.is_empty(), "explains why nothing opened");
}

#[test]
fn request_without_a_profile_reports_it_and_sends_nothing() {
    let dir = unique_dir("fhir-noprofile");
    let mut app = app_at(&dir);
    app.run_action("tools.http_fhir_request");
    type_str(&mut app, "read Patient/1\n");
    assert!(!app.http_running());
    assert!(app.status.contains("fhir.json"), "{}", app.status);
}

#[test]
fn a_bad_command_is_refused_before_any_network_use() {
    let dir = unique_dir("fhir-badcmd");
    fs::create_dir_all(dir.join(".vix")).unwrap();
    fs::write(
        dir.join(".vix/fhir.json"),
        r#"{"base_url":"http://127.0.0.1:9"}"#,
    )
    .unwrap();
    let mut app = app_at(&dir);
    app.run_action("tools.http_fhir_request");
    type_str(&mut app, "frobnicate Patient\n");
    assert!(!app.http_running());
}

mod net {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    use super::*;

    /// A loopback server answering each request in turn; returns the base URL
    /// and a handle yielding the request lines it saw.
    fn serve(replies: Vec<(u16, String)>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let mut lines = Vec::new();
            for (status, body) in replies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0_u8; 8192];
                let n = stream.read(&mut buf).unwrap();
                let text = String::from_utf8_lossy(&buf[..n]).to_string();
                lines.push(text.lines().next().unwrap_or("").to_string());
                write!(
                    stream,
                    "HTTP/1.1 {status} X\r\nContent-Type: application/fhir+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
            lines
        });
        (base, handle)
    }

    fn workspace(base: &str) -> PathBuf {
        // The port makes the directory unique per test (tests run in parallel).
        let tag: String = base.chars().filter(char::is_ascii_digit).collect();
        let dir = unique_dir(&format!("fhir-net-{tag}"));
        fs::create_dir_all(dir.join(".vix")).unwrap();
        fs::write(
            dir.join(".vix/fhir.json"),
            format!(r#"{{"base_url":"{base}/fhir"}}"#),
        )
        .unwrap();
        dir
    }

    fn finish(app: &mut App) {
        wait_until(
            app,
            App::poll_http,
            |a| !a.http_running(),
            Duration::from_secs(10),
        );
    }

    #[test]
    fn search_shows_json_then_the_viewer_pages_through_next() {
        let (base, server) = serve_pages();
        let dir = workspace(&base);
        let mut app = app_at(&dir);
        app.run_action("tools.http_fhir_request");
        type_str(&mut app, "search Patient name=smith\n");
        assert!(app.http_running());
        finish(&mut app);

        let text = active_text(&app);
        assert!(text.contains("\"resourceType\": \"Bundle\""), "{text}");
        assert!(app.status.contains('2'), "count in status: {}", app.status);

        app.run_action("tools.http_fhir_bundle");
        assert_eq!(app.fhir_bundle.as_ref().unwrap().len(), 2);
        app.on_key(key('n'));
        assert!(app.http_running());
        finish(&mut app);
        let panel = app.fhir_bundle.as_ref().expect("viewer shows page two");
        assert_eq!(panel.len(), 1);
        assert!(panel.next_link().is_none());

        let lines = server.join().unwrap();
        assert!(
            lines[0].starts_with("GET /fhir/Patient?name=smith HTTP/1.1"),
            "{lines:?}"
        );
        assert!(lines[1].contains("_getpages=p2"), "{lines:?}");
    }

    /// Two pages served from one listener whose port the first page's `next`
    /// link must name.
    fn serve_pages() -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let next = format!("{base}/fhir?_getpages=p2");
        let pages = vec![
            BUNDLE.replace("NEXT", &next),
            r#"{"resourceType":"Bundle","type":"searchset","entry":[{"resource":{"resourceType":"Patient","id":"c"}}]}"#
                .to_string(),
        ];
        let handle = std::thread::spawn(move || {
            let mut lines = Vec::new();
            for body in pages {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0_u8; 8192];
                let n = stream.read(&mut buf).unwrap();
                let text = String::from_utf8_lossy(&buf[..n]).to_string();
                lines.push(text.lines().next().unwrap_or("").to_string());
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
            lines
        });
        (base, handle)
    }

    #[test]
    fn an_operation_outcome_is_rendered_as_readable_lines() {
        let oo = r#"{"resourceType":"OperationOutcome","issue":[{"severity":"error","code":"not-found","diagnostics":"Resource Patient/zz is not known"}]}"#;
        let (base, server) = serve(vec![(404, oo.to_string())]);
        let dir = workspace(&base);
        let mut app = app_at(&dir);
        app.run_action("tools.http_fhir_request");
        type_str(&mut app, "read Patient/zz\n");
        finish(&mut app);
        let text = active_text(&app);
        assert!(
            text.contains("error (not-found): Resource Patient/zz is not known"),
            "{text}"
        );
        assert!(
            text.contains("\"OperationOutcome\""),
            "JSON follows the lines"
        );
        server.join().unwrap();
    }
}
