//! T848: the client's own timeouts. A server that accepts the connection and
//! then never answers fails the request within the read timeout, and the
//! defaults are bounded.

use std::net::TcpListener;
use std::time::{Duration, Instant};

use vix_http_client::{
    DEFAULT_CONNECT_TIMEOUT, DEFAULT_READ_TIMEOUT, HttpError, Request, Timeouts, send_response,
    send_response_with,
};

fn get(url: String) -> Request {
    Request {
        method: "GET".into(),
        url,
        headers: Vec::new(),
        body: String::new(),
    }
}

#[test]
fn a_server_that_accepts_and_never_answers_times_out() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/slow", listener.local_addr().unwrap());
    // Accept, then hold the connection open without ever writing.
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        std::thread::sleep(Duration::from_secs(2));
        drop(stream);
    });
    let timeouts = Timeouts::new(Duration::from_secs(2), Duration::from_millis(300));
    let started = Instant::now();
    let result = send_response_with(&get(url), timeouts);
    let took = started.elapsed();
    assert!(matches!(result, Err(HttpError::Timeout(_))), "{result:?}");
    assert!(took < Duration::from_secs(3), "failed after {took:?}");
    server.join().unwrap();
}

#[test]
fn the_defaults_are_bounded_and_documented() {
    assert_eq!(Timeouts::DEFAULT, Timeouts::default());
    assert_eq!(Timeouts::DEFAULT.connect, DEFAULT_CONNECT_TIMEOUT);
    assert_eq!(Timeouts::DEFAULT.read, DEFAULT_READ_TIMEOUT);
    assert_eq!(DEFAULT_CONNECT_TIMEOUT, Duration::from_secs(10));
    assert_eq!(DEFAULT_READ_TIMEOUT, Duration::from_secs(60));
}

#[test]
fn send_response_still_reaches_a_live_server_with_the_defaults() {
    use std::io::{Read, Write};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0_u8; 1024];
        let _ = stream.read(&mut buf).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
            .unwrap();
    });
    let resp = send_response(&get(url)).unwrap();
    assert_eq!((resp.status, resp.body.as_str()), (200, "ok"));
    server.join().unwrap();
}

#[test]
fn a_timeout_error_says_so() {
    let text = HttpError::Timeout("timeout: Connect".into()).to_string();
    assert!(text.starts_with("timed out"), "{text}");
}
