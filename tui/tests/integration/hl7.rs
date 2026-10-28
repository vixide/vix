// Tools → HL7 View / HL7 Send (T685).
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use crate::integration::common::*;

const MESSAGE: &str = "MSH|^~\\&|SEND|FAC|RECV|FAC|202401011200||ADT^A01|MSG0001|P|2.5\nPID|1||12345^^^HOSP^MR||DOE^JANE\nPV1|1|I|ICU^101";

fn enter() -> KeyEvent {
    KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
}

#[test]
fn hl7_view_opens_filters_and_jumps_to_the_field() {
    let mut app = app_at(Path::new("."));
    app.editor.new_tab_with_content(MESSAGE);
    app.run_action("tools.hl7_view");
    assert!(
        app.hl7_panel.is_some(),
        "Tools → HL7 View opens the overlay"
    );

    for c in "doe".chars() {
        app.on_key(key(c));
    }
    app.on_key(enter());
    assert!(app.hl7_panel.is_none(), "Enter jumps and closes");
    assert_eq!(
        app.editor.cursor_line(),
        1,
        "the cursor lands on the PID segment"
    );

    // Esc closes the overlay without moving.
    app.run_action("tools.hl7_view");
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(app.hl7_panel.is_none());
}

#[test]
fn hl7_view_refuses_a_buffer_without_msh() {
    let mut app = app_at(Path::new("."));
    type_str(&mut app, "just some prose\n");
    app.run_action("tools.hl7_view");
    assert!(app.hl7_panel.is_none());
    assert!(app.status.contains("MSH"), "{}", app.status);
}

#[cfg(feature = "http")]
mod send {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn hl7_send_requires_a_port() {
        let mut app = app_at(Path::new("."));
        app.editor.new_tab_with_content(MESSAGE);
        app.run_action("tools.hl7_send");
        assert!(app.prompt.is_some(), "the endpoint is asked for");
        for c in "localhost".chars() {
            app.on_key(key(c));
        }
        app.on_key(enter());
        assert!(!app.hl7_running(), "nothing was sent without a port");
        assert!(app.status.contains("host:port"), "{}", app.status);
    }

    #[test]
    fn hl7_send_refuses_a_non_hl7_buffer() {
        let mut app = app_at(Path::new("."));
        type_str(&mut app, "not hl7\n");
        app.run_action("tools.hl7_send");
        assert!(app.prompt.is_none());
        assert!(!app.hl7_running());
    }

    #[test]
    fn hl7_send_shows_the_ack_in_a_read_only_tab() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut deframer = vix::hl7::mllp::Deframer::new(1 << 16);
            let mut buf = [0u8; 1024];
            let payload = loop {
                let n = conn.read(&mut buf).unwrap();
                assert!(n > 0);
                if let Some(p) = deframer.push(&buf[..n]).unwrap().into_iter().next() {
                    break p;
                }
            };
            conn.write_all(&vix::hl7::mllp::frame(
                b"MSH|^~\\&|RECV|FAC|SEND|FAC|2024||ACK|9|P|2.5\rMSA|AA|MSG0001",
            ))
            .unwrap();
            payload
        });

        let mut app = app_at(Path::new("."));
        app.editor.new_tab_with_content(MESSAGE);
        let tabs_before = app.editor.tabs.len();
        app.run_action("tools.hl7_send");
        for c in format!("127.0.0.1:{port}").chars() {
            app.on_key(key(c));
        }
        app.on_key(enter());
        assert!(app.hl7_running(), "the send runs in the background");

        wait_until(
            &mut app,
            App::poll_hl7,
            |a| !a.hl7_running(),
            Duration::from_secs(10),
        );
        let sent = server.join().unwrap();
        assert!(
            sent.starts_with(b"MSH|^~\\&|SEND"),
            "segments go out CR-separated"
        );
        assert!(!sent.contains(&b'\n'));

        assert_eq!(
            app.editor.tabs.len(),
            tabs_before + 1,
            "the reply opens a tab"
        );
        let tab = app.editor.active_tab().unwrap();
        assert!(tab.read_only, "the ACK tab is read-only");
        let text = tab.text();
        assert!(text.contains("AA") && text.contains("MSG0001"), "{text}");
        assert!(
            text.contains("MSA-1"),
            "the reply is shown field by field: {text}"
        );
        assert!(app.status.contains("AA"), "{}", app.status);
    }

    #[test]
    fn hl7_send_reports_a_refused_connection() {
        let port = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let mut app = app_at(Path::new("."));
        app.editor.new_tab_with_content(MESSAGE);
        let tabs_before = app.editor.tabs.len();
        app.run_action("tools.hl7_send");
        for c in format!("127.0.0.1:{port}").chars() {
            app.on_key(key(c));
        }
        app.on_key(enter());
        wait_until(
            &mut app,
            App::poll_hl7,
            |a| !a.hl7_running(),
            Duration::from_secs(10),
        );
        assert_eq!(
            app.editor.tabs.len(),
            tabs_before,
            "no reply tab on failure"
        );
    }
}
