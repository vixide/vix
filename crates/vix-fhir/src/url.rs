//! Tiny URL helpers: origin comparison, loopback detection, percent-encoding.
//!
//! Hand-rolled (rather than a URL-parsing dependency) because the crate only
//! needs three narrow, security-relevant operations: deciding whether a paging
//! link stays on the profile's server ([`origin`]), whether plain `http` is
//! tolerable ([`is_loopback`]), and building query strings ([`encode`]). All
//! helpers are `pub(crate)`; see `crates/vix-fhir/spec/index.md`.

/// `(scheme, host[:port])`, lower-cased, of an absolute `http(s)` URL; `None`
/// for any other scheme or when the authority carries `user@` credentials.
///
/// The port is kept verbatim (no default-port normalization), so
/// `https://h` and `https://h:443` compare as different origins - the safe
/// direction for a same-origin check.
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

/// Whether the URL's host is the local machine (`localhost`, `::1`, or any
/// `127.*`). Matching is on the whole host, so `localhost.evil.com` is not
/// loopback. `false` for anything [`origin`] rejects.
pub(crate) fn is_loopback(url: &str) -> bool {
    let Some((_, authority)) = origin(url) else {
        return false;
    };
    // Bracketed IPv6 (`[::1]:80`) must be unwrapped before splitting off the
    // port, since the address itself contains colons.
    let host = if let Some(stripped) = authority.strip_prefix('[') {
        stripped.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    host == "localhost" || host == "::1" || host.starts_with("127.")
}

/// Percent-encode `s`, keeping RFC 3986 unreserved characters and `keep`.
///
/// Works per UTF-8 byte, so non-ASCII text becomes multi-byte `%XX` runs;
/// `keep` is a set of extra ASCII bytes left literal (e.g. `,` in a list value).
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
