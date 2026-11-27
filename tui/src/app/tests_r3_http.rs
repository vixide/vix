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

// ----- T848: secret variables wait for a masked preview ---------------------

/// Write `name` in `dir` as a request using `$processEnv HOME` in a
/// header, and open it.
fn open_secret_request(app: &mut App, dir: &std::path::Path, name: &str, port: u16) {
    let path = dir.join(name);
    std::fs::write(
        &path,
        format!("GET http://127.0.0.1:{port}/s\nX-Home: {{{{$processEnv HOME}}}}\n"),
    )
    .unwrap();
    app.open_path(&path, false);
}

fn press(app: &mut App, c: char) {
    app.confirm_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
}

fn asking(app: &App) -> bool {
    app.confirm.as_ref().map(|c| c.kind) == Some(super::ConfirmKind::HttpSendSecrets)
}

#[test]
fn an_untrusted_request_with_process_env_is_not_sent_until_confirmed() {
    let home = std::env::var("HOME").unwrap();
    let (port, server) = serve_once("ok");
    let dir = temp("http-secret-untrusted");
    let mut app = app_at(&dir);
    open_secret_request(&mut app, &dir, "api.http", port);
    app.http_send();
    assert!(!app.http_running(), "nothing is sent before the answer");
    assert!(asking(&app), "the preview is shown");
    let message = app.confirm.as_ref().unwrap().message.clone();
    assert_eq!(
        message,
        t!(
            "confirm.http_secrets_untrusted",
            request = format!("GET http://127.0.0.1:{port}/s"),
            variables = "$processEnv HOME"
        )
        .to_string()
    );
    assert!(!message.contains(&home), "the value stays hidden");
    // No cancels.
    press(&mut app, 'n');
    assert!(app.confirm.is_none() && !app.http_running());
    assert_eq!(app.status, t!("status.http_send_cancelled").to_string());
    // Yes sends, with the real value.
    app.http_send();
    press(&mut app, 'y');
    assert!(app.http_running());
    wait_for_response(&mut app);
    let request = server.join().unwrap().to_ascii_lowercase();
    assert!(
        request.contains(&format!("x-home: {}", home.to_ascii_lowercase())),
        "{request}"
    );
    // An untrusted workspace asks again next time, even for the same file.
    app.open_path(&dir.join("api.http"), false);
    app.http_send();
    assert!(!app.http_running());
    assert!(asking(&app));
}

#[test]
fn a_trusted_workspace_asks_once_per_file_and_origin() {
    let dir = temp("http-secret-trusted");
    let mut app = app_at(&dir);
    let mut store = app.load_session();
    store.set_scripts_trusted(&app.session_key(), Some(true));
    app.store_session(&store).unwrap();
    let (port, server) = serve_once("ok");
    open_secret_request(&mut app, &dir, "a.http", port);
    app.http_send();
    assert!(asking(&app), "a newly opened file still shows the preview");
    assert_eq!(
        app.confirm.as_ref().unwrap().message,
        t!(
            "confirm.http_secrets",
            request = format!("GET http://127.0.0.1:{port}/s"),
            variables = "$processEnv HOME"
        )
        .to_string()
    );
    press(&mut app, 'y');
    wait_for_response(&mut app);
    server.join().unwrap();
    // The same file and origin is sent straight away (its server is gone
    // now, so the send itself fails; only the question matters here).
    app.open_path(&dir.join("a.http"), false);
    app.http_send();
    assert!(app.confirm.is_none() && app.http_running());
    wait_for_response(&mut app);
    // Another file, same origin: asked again.
    open_secret_request(&mut app, &dir, "b.http", port);
    app.http_send();
    assert!(asking(&app), "another file asks again");
    press(&mut app, 'n');
    // The first file aimed at another origin: asked again too.
    app.open_path(&dir.join("a.http"), false);
    let other = port.wrapping_add(1);
    buffer_with(
        &mut app,
        &format!("GET http://127.0.0.1:{other}/s\nX-Home: {{{{$processEnv HOME}}}}\n"),
        0,
    );
    app.http_send();
    assert!(asking(&app), "another origin asks again");
    press(&mut app, 'n');
}

#[test]
fn a_request_without_secret_variables_needs_no_confirmation() {
    let (port, server) = serve_once("ok");
    let mut app = app_at(&temp("http-no-secret"));
    buffer_with(
        &mut app,
        &format!("GET http://127.0.0.1:{port}/p\nX-Id: {{{{$guid}}}}\n"),
        0,
    );
    app.http_send();
    assert!(app.confirm.is_none());
    assert!(app.http_running());
    wait_for_response(&mut app);
    server.join().unwrap();
}

#[test]
fn a_bad_timeout_line_stops_the_send() {
    let mut app = app_at(&temp("http-bad-timeout"));
    buffer_with(&mut app, "# @timeout soon\nGET http://127.0.0.1:1/x\n", 0);
    app.messages.items.clear();
    app.http_send();
    assert!(!app.http_running());
    assert!(last_message(&app).is_some_and(|m| m.contains("@timeout soon")));
}
