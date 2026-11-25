//! Round-3 tests of the HL7 view and MLLP send glue. Sends go to a loopback
//! listener on an ephemeral 127.0.0.1 port; nothing leaves the machine.

#[cfg(feature = "http")]
use std::io::{Read, Write};
#[cfg(feature = "http")]
use std::net::TcpListener;

use ratatui::layout::Rect;
use vix_input::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::app::cov4::{app_at, buffer_with, temp};
#[cfg(feature = "http")]
use crate::app::cov4::{content, key};
use crate::app::{App, Focus};

const MSG: &str =
    "MSH|^~\\&|APP|FAC|DEST|FAC|20260101||ADT^A01|CTRL1|P|2.5\rPID|1||123||DOE^JOHN\r";

fn press(app: &mut App, code: KeyCode) {
    app.hl7_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn left_click(col: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

/// Accept one connection, read the MLLP frame, answer with `reply` (framed)
/// or just hang up when `None`. Returns what the client sent.
#[cfg(feature = "http")]
fn mllp_server(reply: Option<&'static str>) -> (u16, std::thread::JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut got = Vec::new();
        let mut buf = [0u8; 1024];
        while !got.ends_with(&[0x1C, 0x0D]) {
            let n = stream.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            got.extend_from_slice(&buf[..n]);
        }
        if let Some(text) = reply {
            stream
                .write_all(&vix_hl7::mllp::frame(&vix_hl7::mllp::to_wire(text)))
                .unwrap();
        }
        got
    });
    (port, handle)
}

#[cfg(feature = "http")]
fn settle(app: &mut App) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while app.hl7_running() {
        app.poll_hl7();
        assert!(std::time::Instant::now() < deadline, "no outcome");
        std::thread::yield_now();
    }
}

#[cfg(feature = "http")]
fn error_text(app: &App) -> String {
    app.messages.items.last().map_or_default(|m| m.text.clone())
}

#[test]
fn viewing_needs_an_msh_and_the_panel_keys_navigate_and_jump() {
    let mut app = app_at(&temp("hl7-view"));
    assert!(!app.run_hl7_action("tools.nothing"));
    buffer_with(&mut app, "just prose", 0);
    assert!(app.run_hl7_action("tools.hl7_view"));
    assert_eq!(app.status, t!("status.hl7_no_message").to_string());
    assert!(app.hl7_panel.is_none());

    buffer_with(&mut app, MSG, 0);
    app.run_hl7_action("tools.hl7_view");
    let total = app.hl7_panel.as_ref().expect("panel opens").len();
    assert!(total > 2);
    app.layout.hl7_panel = Rect::new(0, 0, 60, 4);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Up);
    press(&mut app, KeyCode::PageDown);
    press(&mut app, KeyCode::PageUp);
    // Filter by typing, then edit it with Backspace.
    for c in "pid".chars() {
        press(&mut app, KeyCode::Char(c));
    }
    let filtered = app.hl7_panel.as_ref().unwrap().matches().len();
    assert!(filtered > 0 && filtered < total, "{filtered} of {total}");
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Tab); // ignored
    press(&mut app, KeyCode::Enter);
    assert!(app.hl7_panel.is_none(), "Enter closes the panel");
    assert_eq!(app.focus, Focus::Editor);
    // The cursor landed on a PID line (line 2 of the message).
    assert_eq!(app.editor.cursor_line(), 1);

    app.run_hl7_action("tools.hl7_view");
    press(&mut app, KeyCode::Esc);
    assert!(app.hl7_panel.is_none());
    press(&mut app, KeyCode::Down); // no panel: ignored
}

#[test]
fn clicking_a_panel_row_jumps_but_other_mouse_events_do_not() {
    let mut app = app_at(&temp("hl7-mouse"));
    buffer_with(&mut app, MSG, 0);
    app.run_hl7_action("tools.hl7_view");
    app.layout.hl7_panel = Rect::new(2, 2, 60, 6);
    let moved = MouseEvent {
        kind: MouseEventKind::Moved,
        ..left_click(3, 3)
    };
    app.hl7_mouse(moved);
    assert!(app.hl7_panel.is_some());
    app.hl7_mouse(left_click(0, 0)); // outside the panel
    assert!(app.hl7_panel.is_some());
    app.hl7_mouse(left_click(3, 4)); // third row
    assert!(app.hl7_panel.is_none(), "a click on a row jumps and closes");
    assert_eq!(app.focus, Focus::Editor);
    app.hl7_mouse(left_click(3, 4)); // no panel any more
}

#[cfg(feature = "http")]
#[test]
fn sending_asks_for_an_endpoint_and_refuses_bad_input() {
    let mut app = app_at(&temp("hl7-prompt"));
    buffer_with(&mut app, "plain text", 0);
    app.run_hl7_action("tools.hl7_send");
    assert_eq!(app.status, t!("status.hl7_no_message").to_string());
    assert!(app.prompt.is_none());

    buffer_with(&mut app, MSG, 0);
    app.run_hl7_action("tools.hl7_send");
    assert!(app.prompt.is_some(), "the endpoint prompt opens");
    app.prompt = None;

    app.accept_hl7_send("no-port-here");
    assert_eq!(app.status, t!("status.hl7_bad_endpoint").to_string());
    buffer_with(&mut app, "plain text", 0);
    app.accept_hl7_send("127.0.0.1:9");
    assert_eq!(app.status, t!("status.hl7_no_message").to_string());
    assert!(!app.hl7_running());
}

#[cfg(feature = "http")]
#[test]
fn an_accepted_message_opens_the_reply_in_a_read_only_tab() {
    let (port, server) = mllp_server(Some(
        "MSH|^~\\&|DEST|FAC|APP|FAC|20260101||ACK|A1|P|2.5\rMSA|AA|CTRL1\r",
    ));
    let mut app = app_at(&temp("hl7-accept"));
    buffer_with(&mut app, MSG, 0);
    let endpoint = format!("127.0.0.1:{port}");
    app.accept_hl7_send(&endpoint);
    assert!(app.hl7_running());
    assert_eq!(app.hl7_endpoint, endpoint, "remembered for next time");
    assert_eq!(
        app.status,
        t!("status.hl7_sending", endpoint = endpoint.as_str()).to_string()
    );
    // A second send while one is in flight is refused.
    app.run_hl7_action("tools.hl7_send");
    assert_eq!(app.status, t!("status.hl7_busy").to_string());
    settle(&mut app);
    let sent = server.join().unwrap();
    assert_eq!(
        sent,
        vix_hl7::mllp::frame(&vix_hl7::mllp::to_wire(MSG)),
        "the message went out MLLP-framed"
    );
    let tab = app.editor.active_tab().unwrap();
    assert!(tab.read_only, "the reply tab is read-only");
    assert!(content(&mut app).contains("CTRL1"));
    assert_eq!(
        app.status,
        t!("status.hl7_reply_accept", code = "AA").to_string()
    );
}

#[cfg(feature = "http")]
#[test]
fn rejections_and_other_replies_are_worded_by_their_code() {
    for (code, key) in [
        ("AE", "status.hl7_reply_nak"),
        ("CA", "status.hl7_reply_accept"),
    ] {
        let text: &'static str = Box::leak(
            format!("MSH|^~\\&|D|F|A|F|20260101||ACK|A1|P|2.5\rMSA|{code}|CTRL1\r")
                .into_boxed_str(),
        );
        let (port, server) = mllp_server(Some(text));
        let mut app = app_at(&temp("hl7-codes"));
        buffer_with(&mut app, MSG, 0);
        app.accept_hl7_send(&format!("127.0.0.1:{port}"));
        settle(&mut app);
        server.join().unwrap();
        assert_eq!(app.status, t!(key, code = code).to_string(), "{code}");
    }
    // A reply that is not an acknowledgement at all.
    let (port, server) = mllp_server(Some("PID|1||x\r"));
    let mut app = app_at(&temp("hl7-noack"));
    buffer_with(&mut app, MSG, 0);
    app.accept_hl7_send(&format!("127.0.0.1:{port}"));
    settle(&mut app);
    server.join().unwrap();
    assert_eq!(app.status, t!("ui.hl7_ack_none").to_string());
    assert!(content(&mut app).starts_with(&t!("ui.hl7_ack_none").to_string()));
    let _ = key('x');
}

#[cfg(feature = "http")]
#[test]
fn send_failures_say_why_without_echoing_message_content() {
    // Nobody listening: connect error.
    let mut app = app_at(&temp("hl7-connect"));
    buffer_with(&mut app, MSG, 0);
    app.accept_hl7_send("127.0.0.1:1");
    settle(&mut app);
    assert!(
        error_text(&app).contains(&t!("msg.hl7_err_connect").to_string()),
        "{}",
        error_text(&app)
    );
    assert!(!error_text(&app).contains("DOE"));

    // The peer hangs up without a reply: closed.
    let (port, server) = mllp_server(None);
    let mut app = app_at(&temp("hl7-closed"));
    buffer_with(&mut app, MSG, 0);
    app.accept_hl7_send(&format!("127.0.0.1:{port}"));
    settle(&mut app);
    server.join().unwrap();
    assert!(
        error_text(&app).contains(&t!("msg.hl7_err_closed").to_string()),
        "{}",
        error_text(&app)
    );

    // The sender thread vanishing reads as closed too.
    let mut app = app_at(&temp("hl7-vanish"));
    let (tx, rx) = std::sync::mpsc::channel();
    drop(tx);
    app.hl7_rx = Some(super::Hl7Pending {
        endpoint: "x:1".into(),
        control_id: None,
        rx,
    });
    app.poll_hl7();
    assert!(!app.hl7_running());
    assert!(error_text(&app).contains(&t!("msg.hl7_err_closed").to_string()));
    app.poll_hl7(); // nothing pending: no-op
}
