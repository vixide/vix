//! End-to-end smoke test: `vix_ai_core::complete` against a real (local) HTTP
//! server, not just the pure per-provider request/response functions the
//! unit tests already cover in `src/*.rs` -- this is the one place that
//! actually exercises `ureq` over a socket: headers really sent, a JSON body
//! really posted, and a non-2xx status still parsed as a real response
//! rather than treated as a transport failure.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

use vix_ai_core::Provider;

/// Accept exactly one connection, read one HTTP request off it (headers via
/// `\r\n\r\n`, body via `Content-Length`), reply with `status_line`/`body`,
/// and return the request's body text.
fn serve_one(listener: &TcpListener, status_line: &str, body: &str) -> String {
    let (mut stream, _) = listener.accept().expect("a connection");
    let request = read_request(&mut stream);
    let response = format!(
        "{status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .expect("write response");
    request
}

/// Read one HTTP/1.1 request's body off `stream`: headers up to the blank
/// line, then exactly `Content-Length` more bytes.
fn read_request(stream: &mut TcpStream) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let n = stream.read(&mut chunk).expect("read headers");
        assert_ne!(n, 0, "connection closed before headers completed");
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
        let n = stream.read(&mut chunk).expect("read body");
        assert_ne!(n, 0, "connection closed before body completed");
        buf.extend_from_slice(&chunk[..n]);
    }
    String::from_utf8_lossy(&buf[header_end..header_end + content_length]).to_string()
}

#[test]
fn a_successful_response_round_trips_over_a_real_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let endpoint = format!("http://{}/api/generate", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        serve_one(
            &listener,
            "HTTP/1.1 200 OK",
            r#"{"response":"mock summary","done":true}"#,
        )
    });

    let reply = vix_ai_core::complete(
        Provider::Ollama,
        &endpoint,
        "llama3.2",
        "",
        "Summarize this.",
        "some input text",
    );

    let request_body = server.join().expect("server thread");
    assert_eq!(reply, Ok("mock summary".to_string()));
    // The request really carried the instruction/input this call was given.
    assert!(request_body.contains("\"system\":\"Summarize this.\""));
    assert!(request_body.contains("\"prompt\":\"some input text\""));
}

#[test]
fn a_non_2xx_status_is_still_parsed_as_a_real_error_message() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let endpoint = format!("http://{}/api/generate", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        serve_one(
            &listener,
            "HTTP/1.1 404 Not Found",
            r#"{"error":"model 'llama3.2' not found"}"#,
        )
    });

    let reply = vix_ai_core::complete(
        Provider::Ollama,
        &endpoint,
        "llama3.2",
        "",
        "Summarize this.",
        "some input text",
    );

    server.join().expect("server thread");
    assert_eq!(reply, Err("model 'llama3.2' not found".to_string()));
}

#[test]
fn headers_carry_the_resolved_api_key() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().unwrap();
    let endpoint = format!("http://{addr}/v1/chat/completions");
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("a connection");
        let mut buf = Vec::new();
        let mut chunk = [0_u8; 4096];
        // Just enough to capture the header block for the assertion below.
        let n = stream.read(&mut chunk).expect("read");
        buf.extend_from_slice(&chunk[..n]);
        let body = r#"{"choices":[{"message":{"content":"ok"}}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes());
        String::from_utf8_lossy(&buf).to_string()
    });

    let _ = vix_ai_core::complete(
        Provider::OpenAi,
        &endpoint,
        "gpt-4o-mini",
        "sk-test-key",
        "Be terse.",
        "hi",
    );

    let request_text = server.join().expect("server thread");
    assert!(
        request_text
            .to_ascii_lowercase()
            .contains("authorization: bearer sk-test-key"),
        "{request_text}"
    );
}
