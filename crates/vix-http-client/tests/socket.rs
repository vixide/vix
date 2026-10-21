//! `send` against a real local socket: an HTTP error status is a formatted
//! response (not an `Err`), and the request's method, headers and body arrive.

use std::io::{Read, Write};
use std::net::TcpListener;

use vix_http_client::{Request, send};

#[test]
fn an_error_status_is_a_formatted_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/thing", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0_u8; 4096];
        let n = stream.read(&mut buf).unwrap();
        let request = String::from_utf8_lossy(&buf[..n]).to_string();
        let body = "nope";
        write!(
            stream,
            "HTTP/1.1 404 Not Found\r\nX-Test: yes\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        request
    });
    let req = Request {
        method: "GET".into(),
        url,
        headers: vec![("X-Probe".into(), "1".into())],
        body: String::new(),
    };
    let out = send(&req).expect("a 404 is still a response");
    let request = server.join().unwrap();
    assert!(out.starts_with("HTTP/1.1 404 Not Found\n"), "{out}");
    assert!(out.contains("x-test: yes"), "{out}");
    assert!(out.ends_with("\nnope"), "{out}");
    assert!(request.starts_with("GET /thing HTTP/1.1"), "{request}");
    assert!(request.to_ascii_lowercase().contains("x-probe: 1"));
}
