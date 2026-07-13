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

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

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
#[must_use]
pub fn parse_request(text: &str) -> Option<Request> {
    let mut lines = text.lines().peekable();

    // First meaningful line: `[METHOD] URL`.
    let mut request_line = None;
    for line in lines.by_ref() {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') || t.starts_with("//") {
            continue;
        }
        request_line = Some(t);
        break;
    }
    let request_line = request_line?;
    let mut parts = request_line.split_whitespace();
    let first = parts.next()?;
    // If the first token looks like a URL (has "://" or starts with "/"), the
    // method was omitted; otherwise it's the method and the next token is the URL.
    let (method, url) = if first.contains("://") {
        ("GET".to_string(), first.to_string())
    } else {
        (first.to_ascii_uppercase(), parts.next()?.to_string())
    };
    // Require an absolute URL (with a scheme) so ordinary prose isn't mistaken for
    // a request line.
    if !url.contains("://") {
        return None;
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

    // Everything remaining is the body.
    let body = lines.collect::<Vec<_>>().join("\n");
    Some(Request {
        method,
        url,
        headers,
        body: body.trim_end().to_string(),
    })
}

/// Perform `req` (blocking) and format the response as text: a status line, the
/// response headers, a blank line, then the body. On a transport error, returns
/// `Err` with the message; on an HTTP error status (4xx/5xx), returns the
/// formatted error response as `Ok` (it is still a real response to show).
///
/// Call from a background thread — this blocks on network I/O.
///
/// # Errors
/// Returns `Err` with the transport error message when the request cannot be
/// completed (DNS, connection, TLS). An HTTP error *status* is not an error here:
/// the formatted error response is returned as `Ok`.
pub fn send(req: &Request) -> Result<String, String> {
    // Only http(s) may be requested. A `.http` buffer can be an opened file, so
    // restricting the scheme keeps a crafted request from reaching other URL
    // handlers (e.g. `file://`); ureq itself doesn't implement such schemes, but
    // rejecting them explicitly is defense-in-depth and a clear error.
    if !scheme_is_http(&req.url) {
        return Err(format!("unsupported URL scheme (only http/https): {}", req.url));
    }
    let mut r = ureq::request(&req.method, &req.url);
    for (name, value) in &req.headers {
        r = r.set(name, value);
    }
    let result = if req.body.is_empty() {
        r.call()
    } else {
        r.send_string(&req.body)
    };
    match result {
        // A success or an HTTP status error both carry a response worth showing.
        Ok(resp) | Err(ureq::Error::Status(_, resp)) => Ok(format_response(resp)),
        Err(e) => Err(e.to_string()),
    }
}

/// Whether `url` begins with a permitted (`http`/`https`) scheme, case-insensitively.
fn scheme_is_http(url: &str) -> bool {
    let lower = url.trim_start().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// Format a `ureq` response into a readable text block (status, headers, blank
/// line, body). Consumes `resp` to read its body.
fn format_response(resp: ureq::Response) -> String {
    use std::fmt::Write as _;
    let mut out = format!(
        "{} {} {}\n",
        resp.http_version(),
        resp.status(),
        resp.status_text()
    );
    for name in resp.headers_names() {
        if let Some(value) = resp.header(&name) {
            let _ = writeln!(out, "{name}: {value}");
        }
    }
    out.push('\n');
    match resp.into_string() {
        Ok(body) => out.push_str(&body),
        Err(e) => {
            let _ = write!(out, "<failed to read body: {e}>");
        }
    }
    out
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
        assert!(req.headers.is_empty());
        assert!(req.body.is_empty());
    }

    #[test]
    fn empty_or_urlless_input_is_none() {
        assert!(parse_request("").is_none());
        assert!(parse_request("# only a comment\n").is_none());
        assert!(parse_request("GET\n").is_none());
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
                prop_assert!(send(&req).is_err(), "non-http not refused: {url}");
            }
        }
    }
}
