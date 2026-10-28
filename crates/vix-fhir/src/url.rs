//! Tiny URL helpers: origin comparison, loopback detection, percent-encoding.

/// `(scheme, host[:port])`, lower-cased, of an absolute `http(s)` URL; `None`
/// for any other scheme or when the authority carries `user@` credentials.
pub(crate) fn origin(url: &str) -> Option<(String, String)> {
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    Some((scheme, authority.to_ascii_lowercase()))
}

/// Whether the URL's host is the local machine.
pub(crate) fn is_loopback(url: &str) -> bool {
    let Some((_, authority)) = origin(url) else {
        return false;
    };
    let host = if let Some(stripped) = authority.strip_prefix('[') {
        stripped.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    host == "localhost" || host == "::1" || host.starts_with("127.")
}

/// Percent-encode `s`, keeping RFC 3986 unreserved characters and `keep`.
pub(crate) fn encode(s: &str, keep: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) || keep.as_bytes().contains(&b) {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins() {
        assert_eq!(
            origin("HTTPS://Fhir.Example.com/base/x?y"),
            Some(("https".into(), "fhir.example.com".into()))
        );
        assert_eq!(origin("https://u:p@evil.com/"), None);
        assert_eq!(origin("ftp://x/"), None);
        assert_eq!(origin("file:///etc"), None);
    }

    #[test]
    fn loopback() {
        assert!(is_loopback("http://localhost:8080/fhir"));
        assert!(is_loopback("http://127.0.0.1:1/"));
        assert!(is_loopback("http://[::1]:80/"));
        assert!(!is_loopback("http://example.com/"));
        assert!(!is_loopback("http://localhost.evil.com/"));
    }

    #[test]
    fn encoding() {
        assert_eq!(encode("John Smith", ""), "John%20Smith");
        assert_eq!(encode("a,b", ","), "a,b");
        assert_eq!(
            encode("http://loinc.org|1", ""),
            "http%3A%2F%2Floinc.org%7C1"
        );
        assert_eq!(encode("é", ""), "%C3%A9");
    }
}
