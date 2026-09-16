//! End-to-end smoke test for T124's direct HTTP AI providers: drives a real
//! `App` through the public `ai.summarize` action against a mock HTTP
//! server, proving the whole path -- `Settings::ai_provider` dispatch,
//! `App::spawn_ai_http`, `vix_ai_core::complete`, a real socket, and the
//! reply landing in a new editor tab -- not just the pure per-provider
//! functions `crates/vix-ai-core`'s own unit tests already cover, or
//! `crates/vix-ai-core/tests/http_smoke.rs`'s direct (App-free) exercise of
//! `complete` alone.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use vix::app::App;
use vix::settings::Settings;

/// Accept one connection, drain its request (headers via `\r\n\r\n`, body via
/// `Content-Length` -- same minimal parser as `vix-ai-core`'s own HTTP smoke
/// test), and reply with a canned Ollama-shaped `/api/generate` success body.
fn serve_one_ollama_reply(listener: TcpListener) {
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        read_request(&mut stream);
        let body = r#"{"response":"mock summary","done":true}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
    });
}

fn read_request(stream: &mut TcpStream) {
    let mut buf = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let Ok(n) = stream.read(&mut chunk) else {
            return;
        };
        if n == 0 {
            return;
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
    };
    let header_text = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let content_length: usize = header_text
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().unwrap_or(0))
        })
        .unwrap_or(0);
    while buf.len() < header_end + content_length {
        let Ok(n) = stream.read(&mut chunk) else {
            return;
        };
        if n == 0 {
            return;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

#[test]
fn ai_summarize_via_a_configured_http_provider_lands_in_a_new_tab() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let endpoint = format!("http://{}/api/generate", listener.local_addr().unwrap());
    serve_one_ollama_reply(listener);

    let root = std::env::temp_dir().join(format!("vix-ai-provider-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("notes.txt");
    std::fs::write(&file, "Some text to summarize.\n").unwrap();

    let settings = Settings {
        ai_provider: "ollama".to_string(),
        ai_endpoint: endpoint,
        ..Settings::default()
    };

    let session_path = std::env::temp_dir().join(format!(
        "vix-ai-provider-smoke-session-{}.toml",
        std::process::id()
    ));
    let mut app = App::new(root.clone(), settings).with_session_path(session_path);
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app.open_initial(&file);
    let tabs_before = app.editor.tabs.len();

    app.run_action("ai.summarize");

    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && app.editor.tabs.len() == tabs_before {
        app.poll_ai_replace();
        std::thread::sleep(Duration::from_millis(20));
    }

    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(
        app.editor.tabs.len(),
        tabs_before + 1,
        "the reply should have opened in a new tab"
    );
    assert_eq!(
        app.editor.active_tab().map(|t| t.editor.get_content()),
        Some("mock summary".to_string())
    );
}
