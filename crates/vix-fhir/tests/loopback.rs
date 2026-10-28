//! The network layer against a loopback `TcpListener` stand-in server. No real
//! FHIR server is ever contacted.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "benches, examples and tests fail loudly on a broken fixture"
)]

use std::io::{Read, Write};
use std::net::TcpListener;

use vix_fhir::net::{execute, fetch_token};
use vix_fhir::{Profile, Spec, parse_bundle};

/// Read one full HTTP request (headers, then `Content-Length` body bytes).
fn read_request(stream: &mut std::net::TcpStream) -> String {
    let mut data = Vec::new();
    let mut buf = [0_u8; 4096];
    loop {
        let n = stream.read(&mut buf).unwrap();
        data.extend_from_slice(&buf[..n]);
        let text = String::from_utf8_lossy(&data).to_string();
        if let Some(end) = text.find("\r\n\r\n") {
            let len = text
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                })
                .unwrap_or(0);
            if data.len() >= end + 4 + len || n == 0 {
                return text;
            }
        } else if n == 0 {
            return text;
        }
    }
}

/// Serve each `(status line, body)` in turn on a loopback port; returns the
/// base URL and a handle yielding the raw requests received.
fn serve(replies: Vec<(&'static str, String)>) -> (String, std::thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for (status, body) in replies {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            seen.push(request);
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: application/fhir+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
        seen
    });
    (base, handle)
}

fn no_env(_: &str) -> Option<String> {
    None
}

#[test]
fn search_returns_a_parseable_bundle_with_fhir_accept() {
    let bundle = r#"{"resourceType":"Bundle","type":"searchset","entry":[{"resource":{"resourceType":"Patient","id":"1"}}]}"#;
    let (base, server) = serve(vec![("200 OK", bundle.to_string())]);
    let profile = Profile::parse(&format!(r#"{{"base_url":"{base}/fhir"}}"#)).unwrap();
    let built = Spec::parse_command("search Patient name=Smith")
        .unwrap()
        .build(&profile)
        .unwrap();
    let resp = execute(&profile, &built, &no_env).unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(parse_bundle(&resp.body).unwrap().entries.len(), 1);
    let seen = server.join().unwrap();
    assert!(
        seen[0].starts_with("GET /fhir/Patient?name=Smith HTTP/1.1"),
        "{}",
        seen[0]
    );
    assert!(
        seen[0]
            .to_ascii_lowercase()
            .contains("accept: application/fhir+json")
    );
    assert!(!seen[0].to_ascii_lowercase().contains("authorization"));
}

#[test]
fn error_status_is_a_response_not_an_error() {
    let oo =
        r#"{"resourceType":"OperationOutcome","issue":[{"severity":"error","code":"not-found"}]}"#;
    let (base, server) = serve(vec![("404 Not Found", oo.to_string())]);
    let profile = Profile::parse(&format!(r#"{{"base_url":"{base}"}}"#)).unwrap();
    let built = Spec::parse_command("read Patient/nope")
        .unwrap()
        .build(&profile)
        .unwrap();
    let resp = execute(&profile, &built, &no_env).unwrap();
    assert_eq!(resp.status, 404);
    server.join().unwrap();
}

#[test]
fn bearer_token_is_sent_from_the_environment() {
    let (base, server) = serve(vec![(
        "200 OK",
        r#"{"resourceType":"Patient"}"#.to_string(),
    )]);
    let profile = Profile::parse(&format!(
        r#"{{"base_url":"{base}","auth":{{"type":"bearer","token_env":"FHIR_TOKEN"}}}}"#
    ))
    .unwrap();
    let built = Spec::parse_command("read Patient/1")
        .unwrap()
        .build(&profile)
        .unwrap();
    let env = |n: &str| (n == "FHIR_TOKEN").then(|| "s3cret".to_string());
    execute(&profile, &built, &env).unwrap();
    let seen = server.join().unwrap();
    assert!(
        seen[0].contains("uthorization: Bearer s3cret"),
        "{}",
        seen[0]
    );
}

#[test]
fn smart_backend_fetches_a_token_then_uses_it() {
    let (base, server) = serve(vec![
        (
            "200 OK",
            r#"{"access_token":"tok123","token_type":"bearer"}"#.to_string(),
        ),
        ("200 OK", r#"{"resourceType":"Patient"}"#.to_string()),
    ]);
    let profile = Profile::parse(&format!(
        r#"{{"base_url":"{base}/fhir","auth":{{"type":"smart_backend","token_url":"{base}/token",
        "client_id_env":"FHIR_ID","client_secret_env":"FHIR_SECRET","scope":"system/*.read"}}}}"#
    ))
    .unwrap();
    let env = |n: &str| match n {
        "FHIR_ID" => Some("cid".to_string()),
        "FHIR_SECRET" => Some("csec".to_string()),
        _ => None,
    };
    let built = Spec::parse_command("read Patient/1")
        .unwrap()
        .build(&profile)
        .unwrap();
    execute(&profile, &built, &env).unwrap();
    let seen = server.join().unwrap();
    assert!(seen[0].starts_with("POST /token"), "{}", seen[0]);
    assert!(seen[0].contains("grant_type=client_credentials"));
    // Basic base64("cid:csec") = Y2lkOmNzZWM=
    assert!(seen[0].contains("Basic Y2lkOmNzZWM="), "{}", seen[0]);
    assert!(seen[1].contains("Bearer tok123"), "{}", seen[1]);
}

#[test]
fn token_failure_surfaces_as_a_typed_error() {
    let (base, server) = serve(vec![(
        "401 Unauthorized",
        r#"{"error":"invalid_client"}"#.to_string(),
    )]);
    let profile = Profile::parse(&format!(
        r#"{{"base_url":"{base}","auth":{{"type":"smart_backend","token_url":"{base}/token",
        "client_id_env":"FHIR_ID","client_secret_env":"FHIR_SECRET"}}}}"#
    ))
    .unwrap();
    let env = |_: &str| Some("x".to_string());
    assert_eq!(
        fetch_token(&profile, &env),
        Err(vix_fhir::FhirError::BadTokenResponse)
    );
    server.join().unwrap();
}

#[test]
fn unreachable_server_is_a_transport_error() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let profile = Profile::parse(&format!(r#"{{"base_url":"{base}"}}"#)).unwrap();
    let built = Spec::parse_command("capabilities")
        .unwrap()
        .build(&profile)
        .unwrap();
    assert_eq!(
        execute(&profile, &built, &no_env).unwrap_err(),
        vix_fhir::FhirError::Transport
    );
}
