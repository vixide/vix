//! Round-3 tests of the HTTP client glue: environment errors, the chooser's
//! keys and mouse, and a full send against a loopback server on an
//! ephemeral 127.0.0.1 port.

use std::io::{Read, Write};
use std::net::TcpListener;

use ratatui::layout::Rect;
use vix_input::{KeyCode, KeyEvent, KeyModifiers};

use super::HttpEnvChooser;
use crate::app::cov4::{app_at, buffer_with, click, code, content, last_message, temp};
use crate::app::{App, ChooserSlot, Focus};

fn envs_dir(tag: &str, json: &str) -> std::path::PathBuf {
    let dir = temp(tag);
    std::fs::create_dir_all(dir.join(".vix")).unwrap();
    std::fs::write(dir.join(".vix/http-env.json"), json).unwrap();
    dir
}

/// A one-shot loopback HTTP server: answers the first request with `body`
/// and reports the request text it read.
fn serve_once(body: &'static str) -> (u16, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut seen = Vec::new();
        let mut buf = [0u8; 1024];
        loop {
            let n = stream.read(&mut buf).unwrap();
            seen.extend_from_slice(&buf[..n]);
            if n == 0 || seen.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        String::from_utf8_lossy(&seen).into_owned()
    });
    (port, handle)
}

fn wait_for_response(app: &mut App) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while app.http_running() {
        app.poll_http();
        assert!(
            std::time::Instant::now() < deadline,
            "response never arrived"
        );
        std::thread::yield_now();
    }
}

#[test]
fn run_http_action_only_claims_its_own_actions() {
    let mut app = app_at(&temp("http-claim"));
    assert!(!app.run_http_action("tools.not_http"));
    assert!(app.run_http_action("tools.http_environment"));
}

#[test]
fn a_malformed_environments_file_stops_send_copy_and_the_chooser() {
    let dir = envs_dir("http-bad-env", "{ not json");
    let mut app = app_at(&dir);
    buffer_with(&mut app, "GET http://127.0.0.1:1/x\n", 0);
    app.messages.items.clear();
    app.http_copy_curl();
    assert!(last_message(&app).is_some(), "the problem is reported");
    assert!(!app.status.contains("curl"));
    app.messages.items.clear();
    app.http_send();
    assert!(
        !app.http_running(),
        "a broken environments file never sends"
    );
    assert!(last_message(&app).is_some());
    app.messages.items.clear();
    app.open_http_env_chooser();
    assert!(app.chooser.http_env().is_none());
    assert!(last_message(&app).is_some());
}

#[test]
fn an_unreadable_block_or_body_file_is_reported() {
    let dir = temp("http-body");
    let mut app = app_at(&dir);
    // A body file that does not exist.
    buffer_with(
        &mut app,
        "POST http://127.0.0.1:1/x\n\n< ./missing.json\n",
        0,
    );
    app.messages.items.clear();
    app.http_copy_curl();
    assert!(
        last_message(&app).is_some(),
        "the missing body file is reported"
    );
    assert!(!app.status.contains("curl"));
    // An unterminated variable.
    buffer_with(&mut app, "GET http://127.0.0.1:1/{{oops\n", 0);
    app.messages.items.clear();
    app.http_copy_curl();
    assert!(last_message(&app).is_some());
}

#[test]
fn environment_chooser_keys_move_clamp_apply_and_close() {
    let dir = envs_dir("http-chooser", r#"{"dev": {"h": "x"}, "prod": {"h": "y"}}"#);
    let mut app = app_at(&dir);
    app.open_http_env_chooser();
    let c = app.chooser.http_env().expect("chooser opens");
    assert_eq!(
        (c.names.clone(), c.selected),
        (vec!["dev".to_string(), "prod".to_string()], 0)
    );

    let press = |app: &mut App, k: KeyCode| app.http_env_key(KeyEvent::new(k, KeyModifiers::NONE));
    press(&mut app, KeyCode::Up);
    assert_eq!(
        app.chooser.http_env().unwrap().selected,
        0,
        "stops at the top"
    );
    for _ in 0..5 {
        press(&mut app, KeyCode::Down);
    }
    assert_eq!(
        app.chooser.http_env().unwrap().selected,
        2,
        "last row is the last environment"
    );
    press(&mut app, KeyCode::Char('x'));
    assert_eq!(
        app.chooser.http_env().unwrap().selected,
        2,
        "other keys do nothing"
    );
    press(&mut app, KeyCode::Enter);
    assert!(app.chooser.http_env().is_none());
    assert_eq!(
        std::fs::read_to_string(dir.join(".vix/http-env.active")).unwrap(),
        "prod"
    );
    assert_eq!(
        app.status,
        t!("status.http_env_set", name = "prod").to_string()
    );

    // The active one is highlighted when reopened; row 0 clears it.
    app.open_http_env_chooser();
    assert_eq!(app.chooser.http_env().unwrap().selected, 2);
    press(&mut app, KeyCode::Up);
    press(&mut app, KeyCode::Up);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.status, t!("status.http_env_cleared").to_string());

    app.open_http_env_chooser();
    press(&mut app, KeyCode::Esc);
    assert!(app.chooser.http_env().is_none());
}

#[test]
fn clicking_a_chooser_row_applies_that_environment() {
    let dir = envs_dir("http-click", r#"{"dev": {"h": "x"}}"#);
    let mut app = app_at(&dir);
    app.open_http_env_chooser();
    app.layout.chooser = Rect::new(0, 0, 40, 10);
    // Out of range (below the "no environment" + dev rows) does nothing.
    app.http_env_mouse(click(2, 8));
    assert!(app.chooser.http_env().is_some(), "{:?}", app.layout.chooser);
    app.http_env_mouse(click(2, 1));
    // Row 0 is "no environment", so row 1 is `dev`.
    assert!(app.chooser.http_env().is_none());
    assert_eq!(
        app.status,
        t!("status.http_env_set", name = "dev").to_string()
    );
    // A click with no chooser open is ignored.
    app.http_env_mouse(click(2, 1));
}

#[test]
fn applying_an_environment_reports_a_write_failure() {
    let dir = envs_dir("http-write-fail", r#"{"dev": {"h": "x"}}"#);
    let mut app = app_at(&dir);
    app.chooser.open_http_env(HttpEnvChooser {
        names: vec!["dev".into()],
        selected: 1,
    });
    // `.vix/http-env.active` as a directory makes the write fail.
    std::fs::create_dir_all(dir.join(".vix/http-env.active")).unwrap();
    app.messages.items.clear();
    app.http_env_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        last_message(&app).is_some(),
        "the failure is in the message log"
    );
    assert!(app.chooser.http_env().is_none());
}

#[test]
fn a_request_is_sent_over_loopback_and_the_named_response_is_kept() {
    let (port, server) = serve_once("hello from the loopback");
    let dir = temp("http-send");
    let mut app = app_at(&dir);
    buffer_with(
        &mut app,
        &format!("# @name first\nGET http://127.0.0.1:{port}/greet\nAccept: text/plain\n"),
        0,
    );
    let tabs_before = app.editor.tabs.len();
    app.http_send();
    assert!(app.http_running());
    assert_eq!(
        app.status,
        t!(
            "status.http_sending",
            url = format!("http://127.0.0.1:{port}/greet")
        )
        .to_string()
    );
    wait_for_response(&mut app);
    let request = server.join().unwrap();
    assert!(request.starts_with("GET /greet HTTP/1.1"), "{request}");
    assert_eq!(app.status, t!("status.http_done").to_string());
    assert_eq!(
        app.editor.tabs.len(),
        tabs_before + 1,
        "the response opens in a new tab"
    );
    assert_eq!(app.focus, Focus::Editor);
    assert!(content(&mut app).contains("hello from the loopback"));
    assert!(
        app.http_named.contains_key("first"),
        "a named response is kept"
    );
}

#[test]
fn a_failed_send_is_reported_in_the_message_log() {
    // Port 1 on loopback is never listening.
    let mut app = app_at(&temp("http-fail"));
    buffer_with(&mut app, "GET http://127.0.0.1:1/never\n", 0);
    app.messages.items.clear();
    app.http_send();
    wait_for_response(&mut app);
    let msg = last_message(&app).expect("the failure is reported");
    assert_ne!(msg, String::new());
    assert!(
        app.editor
            .tabs
            .iter()
            .all(|t| !t.text().contains("HTTP/1.1 200"))
    );
    // A dropped sender (thread died) reads as a failure too.
    let (tx, rx) = std::sync::mpsc::channel();
    drop(tx);
    app.http_rx = Some(rx);
    app.messages.items.clear();
    app.poll_http();
    assert!(!app.http_running());
    assert!(
        last_message(&app)
            .unwrap()
            .contains(&t!("status.http_failed").to_string())
    );
    let _ = code(KeyCode::Esc);
}
