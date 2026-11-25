//! A minimal HTTP client driven by a `.http`-style buffer, sent with the
//! pure-Rust `ureq` client.
//!
//! The buffer format (a common "REST client" shape):
//!
//! ```text
//! POST https://api.example.com/things
//! Content-Type: application/json
//! Authorization: Bearer TOKEN
//!
//! {"name": "widget"}
//! ```
//!
//! The first non-blank, non-comment line is `METHOD url` (the method is optional
//! and defaults to `GET`); following lines up to a blank line are `Header: value`;
//! everything after the blank line is the request body. Lines starting with `#`
//! or `//` are comments. Parsing is pure and unit-tested; [`send`] performs the
//! (blocking) request and is meant to be called from a background thread.
//!
//! Also parsed (see [`parse_request`]): `@name = value` variable-definition
//! lines before the request line are skipped here (substitution is handled by
//! [`mod@env`]), a trailing `HTTP/x.y` token on the request line is dropped, and
//! `?a=1` / `&b=2` continuation lines extend the URL's query.
//!
//! Submodules: [`mod@env`] (environment files and `{{var}}` substitution), [`aad`]
//! (Azure AD token acquisition) and [`rest`] (the REST-client workbench model).
//! Only `http`/`https` URLs are ever sent, see [`send_response`].
//!
//! Owning spec: `crates/vix-http-client/spec/index.md`.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

pub mod aad;
pub mod env;
pub mod rest;
mod xpath;

/// A parsed HTTP request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// HTTP method (upper-cased; defaults to `GET`).
    pub method: String,
    /// Request URL.
    pub url: String,
    /// `(name, value)` header pairs, in order.
    pub headers: Vec<(String, String)>,
    /// Request body (may be empty).
    pub body: String,
}

/// Parse a `.http`-style buffer into a [`Request`]. Returns `None` if there is no
/// request line with a URL.
///
/// The URL must be absolute (contain `://`); this keeps stray prose from being
/// mistaken for a request. Header lines without a `:` are silently ignored.
///
/// # Examples
///
/// ```
/// let req = vix_http_client::parse_request(
///     "POST https://e.com/x\nAccept: */*\n\n{\"a\":1}\n",
/// ).unwrap();
/// assert_eq!(req.method, "POST");
/// assert_eq!(req.headers, vec![("Accept".to_string(), "*/*".to_string())]);
/// assert_eq!(req.body, "{\"a\":1}");
/// // The method is optional and defaults to GET.
/// assert_eq!(vix_http_client::parse_request("https://e.com").unwrap().method, "GET");
/// ```
#[must_use]
pub fn parse_request(text: &str) -> Option<Request> {
    let mut lines = text.lines().peekable();

    // First meaningful line: `[METHOD] URL`.
    let mut request_line = None;
    for line in lines.by_ref() {
        let t = line.trim();
        // `@name = value` lines define variables; they precede the request
        // line in the `.http` convention and are not part of the request.
        if t.is_empty() || t.starts_with('#') || t.starts_with("//") || t.starts_with('@') {
            continue;
        }
        request_line = Some(t);
        break;
    }
    let request_line = request_line?;
    // An optional trailing `HTTP/1.1` is the protocol version, not part of the URL.
    let request_line = match request_line.rsplit_once(char::is_whitespace) {
        Some((head, tail)) if tail.to_ascii_uppercase().starts_with("HTTP/") => head.trim_end(),
        _ => request_line,
    };
    let mut parts = request_line.split_whitespace();
    let first = parts.next()?;
    // If the first token looks like a URL (it contains "://"), the method was
    // omitted; otherwise it's the method and the next token is the URL. (A
    // leading "/" does NOT count: only absolute URLs are accepted below.)
    let (method, mut url) = if first.contains("://") {
        ("GET".to_string(), first.to_string())
    } else {
        (first.to_ascii_uppercase(), parts.next()?.to_string())
    };
    // Require an absolute URL (with a scheme) so ordinary prose isn't mistaken for
    // a request line.
    if !url.contains("://") {
        return None;
    }

    // A query may continue on following lines that start with `?` or `&`.
    while let Some(next) = lines.peek().map(|l| l.trim()) {
        if next.starts_with('?') || next.starts_with('&') {
            url.push_str(next);
            lines.next();
        } else {
            break;
        }
    }

    // Headers until a blank line.
    let mut headers = Vec::new();
    for line in lines.by_ref() {
        let t = line.trim();
        if t.is_empty() {
            break;
        }
        if t.starts_with('#') || t.starts_with("//") {
            continue;
        }
        if let Some((name, value)) = t.split_once(':') {
            headers.push((name.trim().to_string(), value.trim().to_string()));
        }
    }

    // Everything remaining is the body, kept verbatim (including `#`/`//`
    // lines, which are comments only before the blank line).
    let body = lines.collect::<Vec<_>>().join("\n");
    Some(Request {
        method,
        url,
        headers,
        body: body.trim_end().to_string(),
    })
}

/// Why [`send`] couldn't complete a request (T673).
#[derive(Debug)]
#[non_exhaustive]
pub enum HttpError {
    /// The URL isn't `http`/`https`; holds the URL.
    UnsupportedScheme(String),
    /// The request couldn't be built (a malformed method, URL or header).
    InvalidRequest(String),
    /// The request never got a reply: DNS, connection or TLS failure.
    Transport(String),
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedScheme(url) => {
                write!(f, "unsupported URL scheme (only http/https): {url}")
            }
            Self::InvalidRequest(why) | Self::Transport(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for HttpError {}

/// A received HTTP response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    /// Status code.
    pub status: u16,
    /// Reason phrase (may be empty).
    pub reason: String,
    /// Protocol version, e.g. `HTTP/1.1`.
    pub version: String,
    /// `(name, value)` header pairs, in order.
    pub headers: Vec<(String, String)>,
    /// Body as text.
    pub body: String,
}

impl Response {
    /// The response as readable text: a status line, headers, a blank line,
    /// then the body.
    #[must_use]
    pub fn format(&self) -> String {
        use std::fmt::Write as _;
        let mut out = format!("{} {} {}\n", self.version, self.status, self.reason);
        for (name, value) in &self.headers {
            let _ = writeln!(out, "{name}: {value}");
        }
        out.push('\n');
        out.push_str(&self.body);
        out
    }
}

/// Perform `req` (blocking) and format the response as text: a status line, the
/// response headers, a blank line, then the body. On a transport error, returns
/// `Err` with the message; on an HTTP error status (4xx/5xx), returns the
/// formatted error response as `Ok` (it is still a real response to show).
///
/// Call from a background thread — this blocks on network I/O.
///
/// # Errors
/// See [`send_response`].
pub fn send(req: &Request) -> Result<String, HttpError> {
    send_response(req).map(|r| r.format())
}

/// Perform `req` (blocking) and return the structured [`Response`].
///
/// # Errors
/// [`HttpError::UnsupportedScheme`] for a non-`http(s)` URL,
/// [`HttpError::InvalidRequest`] when the request can't be built, and
/// [`HttpError::Transport`] when it can't be completed (DNS, connection, TLS).
/// An HTTP error *status* is not an error here: the error response is
/// returned as `Ok`.
pub fn send_response(req: &Request) -> Result<Response, HttpError> {
    // Only http(s) may be requested. A `.http` buffer can be an opened file, so
    // restricting the scheme keeps a crafted request from reaching other URL
    // handlers (e.g. `file://`); ureq itself doesn't implement such schemes, but
    // rejecting them explicitly is defense-in-depth and a clear error.
    if !scheme_is_http(&req.url) {
        return Err(HttpError::UnsupportedScheme(req.url.clone()));
    }
    // An HTTP error *status* carries a response worth showing, so it must not
    // surface as an `Err`.
    // No explicit timeouts are set: ureq's defaults apply, and callers run this
    // on a background thread so a slow server never blocks the UI.
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    let mut builder = ureq::http::Request::builder()
        .method(req.method.as_str())
        .uri(&req.url);
    for (name, value) in &req.headers {
        builder = builder.header(name, value);
    }
    let request = builder
        .body(req.body.clone())
        .map_err(|e| HttpError::InvalidRequest(e.to_string()))?;
    match agent.run(request) {
        Ok(resp) => Ok(read_response(resp)),
        Err(e) => Err(HttpError::Transport(e.to_string())),
    }
}

/// Whether `url` begins with a permitted (`http`/`https`) scheme, case-insensitively.
fn scheme_is_http(url: &str) -> bool {
    let lower = url.trim_start().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// Read a `ureq` response into a [`Response`]. Consumes `resp` to read its body.
fn read_response(resp: ureq::http::Response<ureq::Body>) -> Response {
    let status = resp.status();
    // `Debug` of `http::Version` renders as `HTTP/1.1`, the status-line form.
    let version = format!("{:?}", resp.version());
    let headers = resp
        .headers()
        .iter()
        // Header values that are not valid UTF-8 text are dropped rather than
        // lossily converted.
        .filter_map(|(n, v)| Some((n.to_string(), v.to_str().ok()?.to_string())))
        .collect();
    let body = match resp.into_body().read_to_string() {
        Ok(body) => body,
        // A body read failure (bad encoding, truncated stream) still yields the
        // status and headers; the failure is shown in place of the body.
        Err(e) => format!("<failed to read body: {e}>"),
    };
    Response {
        status: status.as_u16(),
        reason: status.canonical_reason().unwrap_or("").to_string(),
        version,
        headers,
        body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_method_url_headers_and_body() {
        let text = "POST https://e.com/x\nContent-Type: application/json\n\n{\"a\":1}\n";
        let req = parse_request(text).unwrap();
        assert_eq!(req.method, "POST");
        assert_eq!(req.url, "https://e.com/x");
        assert_eq!(
            req.headers,
            vec![("Content-Type".to_string(), "application/json".to_string())]
        );
        assert_eq!(req.body, "{\"a\":1}");
    }

    #[test]
    fn method_defaults_to_get_and_skips_comments() {
        let req = parse_request("# fetch it\nhttps://e.com/y\n").unwrap();
        assert_eq!(req.method, "GET");
        assert_eq!(req.url, "https://e.com/y");
        assert_eq!(
            req.headers,
            [] as [(std::string::String, std::string::String); 0]
        );
        assert_eq!(req.body, "");
    }

    #[test]
    fn empty_or_urlless_input_is_none() {
        assert!(parse_request("").is_none());
        assert!(parse_request("# only a comment\n").is_none());
        assert!(parse_request("GET\n").is_none());
    }

    #[test]
    fn strips_the_http_version_and_joins_query_continuations() {
        let req =
            parse_request("@x = 1\nGET https://e.com/s HTTP/1.1\n  ?a=1\n  &b=2\nAccept: */*\n")
                .unwrap();
        assert_eq!(req.method, "GET");
        assert_eq!(req.url, "https://e.com/s?a=1&b=2");
        assert_eq!(req.headers.len(), 1);
    }

    #[test]
    fn formats_a_response_as_status_headers_body() {
        let r = Response {
            status: 404,
            reason: "Not Found".into(),
            version: "HTTP/1.1".into(),
            headers: vec![("A".into(), "b".into())],
            body: "nope".into(),
        };
        assert_eq!(r.format(), "HTTP/1.1 404 Not Found\nA: b\n\nnope");
    }

    #[test]
    fn parses_put_with_header_and_body() {
        let req = parse_request("PUT https://e.com/z\nX-Key: 9\n\nhello").unwrap();
        assert_eq!(req.method, "PUT");
        assert_eq!(req.url, "https://e.com/z");
        assert_eq!(req.headers, vec![("X-Key".to_string(), "9".to_string())]);
        assert_eq!(req.body, "hello");
    }

    #[test]
    fn only_http_schemes_are_permitted() {
        assert!(scheme_is_http("http://e.com/x"));
        assert!(scheme_is_http("HTTPS://E.com/x"));
        assert!(!scheme_is_http("file:///etc/passwd"));
        assert!(!scheme_is_http("gopher://x/"));
        assert!(!scheme_is_http("ftp://x/"));
        // `send` refuses a non-http request without performing any I/O.
        let req = parse_request("GET file:///etc/passwd\n").unwrap();
        assert!(send(&req).is_err());
    }

    // ---- property-based ("fuzz") tests ------------------------------------

    use proptest::prelude::*;

    proptest! {
        // Parsing an arbitrary buffer never panics; any request it yields keeps
        // the "url has a scheme" invariant `send` relies on.
        #[test]
        fn parse_request_never_panics(text in ".*") {
            if let Some(req) = parse_request(&text) {
                prop_assert!(req.url.contains("://"), "url without scheme: {:?}", req.url);
            }
        }

        // A parsed request whose URL is not http(s) is always refused by `send`
        // (this runs no real I/O because the guard rejects before the request).
        #[test]
        fn non_http_urls_are_refused(scheme in "[a-z]{2,6}", rest in "[a-z0-9./]{1,10}") {
            let url = format!("{scheme}://{rest}");
            let req = Request {
                method: "GET".into(),
                url: url.clone(),
                headers: vec![],
                body: String::new(),
            };
            if !scheme_is_http(&url) {
                prop_assert!(
                    matches!(send(&req), Err(HttpError::UnsupportedScheme(_))),
                    "non-http not refused: {url}"
                );
            }
        }
    }
}

#[cfg(test)]
mod net_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Serve `reply` (raw bytes) to the first connection on a local port, then
    /// hang up. Returns the URL and the thread to join (it yields the request).
    fn serve(reply: &'static [u8]) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/path", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().unwrap();
            let mut seen = Vec::new();
            let mut buf = [0u8; 1024];
            while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = sock.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }
                seen.extend_from_slice(&buf[..n]);
            }
            sock.write_all(reply).unwrap();
            String::from_utf8_lossy(&seen).into_owned()
        });
        (url, handle)
    }

    fn get(url: &str) -> Request {
        Request {
            method: "GET".into(),
            url: url.into(),
            headers: vec![("X-Test".into(), "yes".into())],
            body: String::new(),
        }
    }

    #[test]
    fn a_local_server_reply_becomes_a_structured_response() {
        let (url, server) = serve(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello",
        );
        let resp = send_response(&get(&url)).unwrap();
        let seen = server.join().unwrap();
        assert!(seen.starts_with("GET /path HTTP/1.1"), "{seen}");
        assert!(seen.to_ascii_lowercase().contains("x-test: yes"));
        assert_eq!(resp.status, 200);
        assert_eq!(resp.reason, "OK");
        assert_eq!(resp.version, "HTTP/1.1");
        assert_eq!(resp.body, "hello");
        assert!(
            resp.headers
                .iter()
                .any(|(n, v)| n == "content-type" && v == "text/plain")
        );
    }

    #[test]
    fn an_error_status_is_a_response_not_an_error() {
        let (url, server) =
            serve(b"HTTP/1.1 404 Not Found\r\nContent-Length: 4\r\nConnection: close\r\n\r\nnope");
        let text = send(&get(&url)).unwrap();
        server.join().unwrap();
        assert!(text.starts_with("HTTP/1.1 404 Not Found\n"), "{text}");
        assert!(text.ends_with("\n\nnope"));
    }

    #[test]
    fn a_truncated_body_is_shown_in_place_of_the_body() {
        // Promises 100 bytes, delivers 3, then hangs up.
        let (url, server) =
            serve(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\nabc");
        let resp = send_response(&get(&url)).unwrap();
        server.join().unwrap();
        assert_eq!(resp.status, 200);
        assert!(
            resp.body.starts_with("<failed to read body"),
            "{}",
            resp.body
        );
    }

    #[test]
    fn a_refused_connection_is_a_transport_error() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        drop(listener);
        let err = send_response(&get(&url)).unwrap_err();
        assert!(matches!(err, HttpError::Transport(_)), "{err}");
        assert_ne!(err.to_string(), "");
    }

    #[test]
    fn unbuildable_requests_and_foreign_schemes_are_rejected_up_front() {
        let mut bad = get("http://127.0.0.1:1/");
        bad.method = "NOT A METHOD".into();
        let err = send_response(&bad).unwrap_err();
        assert!(matches!(err, HttpError::InvalidRequest(_)), "{err}");
        let file = get("file:///etc/passwd");
        let err = send_response(&file).unwrap_err();
        assert!(matches!(err, HttpError::UnsupportedScheme(_)));
        assert!(err.to_string().contains("file:///etc/passwd"));
    }

    #[test]
    fn header_lines_without_a_colon_and_comment_lines_are_ignored() {
        let req = parse_request(
            "GET https://e.com/x\n# comment\n// another\nno colon here\nAccept: */*\n\nbody",
        )
        .unwrap();
        assert_eq!(req.headers, vec![("Accept".to_string(), "*/*".to_string())]);
        assert_eq!(req.body, "body");
    }
}
