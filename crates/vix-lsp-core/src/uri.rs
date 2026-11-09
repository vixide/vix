//! `file://` URIs from paths and back, the same on every platform (T772).
//!
//! Windows needs three things that a naive "prefix `file://`" misses:
//! `canonicalize` returns *verbatim* paths (`\\?\C:\…`, `\\?\UNC\server\…`),
//! the separator is `\`, and a drive path needs a `/` before the drive
//! (`file:///C:/…`). The functions here take the platform as a flag
//! (`windows`), so the Windows rules are unit-tested on every OS; the
//! `*_native` wrappers pass `cfg!(windows)`.
//!
//! On other platforms a `\` is an ordinary file-name byte, so it is
//! percent-encoded rather than treated as a separator.

use std::borrow::Cow;

/// Drop a Windows verbatim prefix: `\\?\C:\x` → `C:\x` and
/// `\\?\UNC\server\share\x` → `\\server\share\x`. Any other text is returned
/// unchanged, including a verbatim path that is not a drive or UNC path
/// (`\\?\Volume{…}\`), which has no plain form.
#[must_use]
pub fn strip_verbatim(text: &str) -> Cow<'_, str> {
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return Cow::Owned(format!(r"\\{rest}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) if is_drive(rest) => Cow::Borrowed(rest),
        _ => Cow::Borrowed(text),
    }
}

/// Whether `text` starts with a drive (`C:`) followed by a separator or the
/// end of the text.
fn is_drive(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() >= 2
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && (b.len() == 2 || b[2] == b'\\' || b[2] == b'/')
}

/// The `file://` URI for the path text `path`, percent-encoding every byte
/// but the unreserved ones, `/`, and the drive's `:`.
///
/// With `windows`, a verbatim prefix is dropped, `\` is a separator, a drive
/// path becomes `file:///C:/…` and a UNC path `file://server/share/…`.
#[must_use]
pub fn file_uri(path: &str, windows: bool) -> String {
    let mut out = String::from("file://");
    let text: Cow<'_, str> = if windows {
        Cow::Owned(strip_verbatim(path).replace('\\', "/"))
    } else {
        Cow::Borrowed(path)
    };
    // `//server/share` (UNC) keeps the server as the URI's host.
    let (host, rest) = match text.strip_prefix("//") {
        Some(unc) if windows => unc.split_once('/').map_or((unc, ""), |(h, r)| (h, r)),
        _ => ("", text.as_ref()),
    };
    out.push_str(host);
    let drive = windows && is_drive(rest);
    if !rest.starts_with('/') {
        out.push('/');
    }
    for (i, byte) in rest.bytes().enumerate() {
        let keep = byte.is_ascii_alphanumeric()
            || b"-._~/".contains(&byte)
            || (drive && i == 1 && byte == b':');
        if keep {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(byte >> 4)]));
            out.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    out
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

/// The parts of a `file://` URI: its host (empty or `localhost` for a local
/// file) and its percent-decoded path. `None` for another scheme or bad
/// percent-encoding or UTF-8.
#[must_use]
pub fn split_file_uri(uri: &str) -> Option<(&str, String)> {
    let rest = uri.strip_prefix("file://")?;
    let (host, path) = match rest.find('/') {
        Some(i) => rest.split_at(i),
        None => (rest, ""),
    };
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = path.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Some((host, String::from_utf8(out).ok()?))
}

/// The path text a `file://` URI names. `None` as for [`split_file_uri`].
///
/// With `windows`, `/C:/x` becomes `C:\x`, a host becomes a UNC path
/// (`\\server\share\x`), and `/` becomes `\`. A path with no drive (for
/// example a remote POSIX path) is left as it is.
#[must_use]
pub fn uri_path(uri: &str, windows: bool) -> Option<String> {
    let (host, path) = split_file_uri(uri)?;
    if !windows {
        return Some(path);
    }
    let local = host.is_empty() || host.eq_ignore_ascii_case("localhost");
    if !local {
        return Some(format!(r"\\{host}{}", path.replace('/', "\\")));
    }
    match path.strip_prefix('/') {
        Some(drive) if is_drive(drive) => Some(drive.replace('/', "\\")),
        _ => Some(path),
    }
}

/// [`file_uri`] for this platform's paths.
#[must_use]
pub fn file_uri_native(path: &std::path::Path) -> String {
    file_uri(&path.to_string_lossy(), cfg!(windows))
}

/// [`uri_path`] for this platform's paths.
#[must_use]
pub fn uri_path_native(uri: &str) -> Option<std::path::PathBuf> {
    uri_path(uri, cfg!(windows)).map(std::path::PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_verbatim_drive_and_unc_prefixes_only() {
        assert_eq!(strip_verbatim(r"\\?\C:\a\b.rs"), r"C:\a\b.rs");
        assert_eq!(strip_verbatim(r"\\?\UNC\srv\share\x"), r"\\srv\share\x");
        assert_eq!(strip_verbatim(r"\\?\Volume{1}\x"), r"\\?\Volume{1}\x");
        assert_eq!(strip_verbatim(r"C:\a"), r"C:\a");
        assert_eq!(strip_verbatim("/tmp/a"), "/tmp/a");
    }

    #[test]
    fn posix_paths_encode_as_before() {
        assert_eq!(
            file_uri("/tmp/my dir/f.rs", false),
            "file:///tmp/my%20dir/f.rs"
        );
        // A backslash is a file-name byte here, not a separator.
        assert_eq!(file_uri(r"/tmp/a\b", false), "file:///tmp/a%5Cb");
        assert_eq!(file_uri("/a:b", false), "file:///a%3Ab");
        assert_eq!(
            uri_path("file:///tmp/my%20dir/f.rs", false).unwrap(),
            "/tmp/my dir/f.rs"
        );
        assert_eq!(uri_path("file:///C:/x", false).unwrap(), "/C:/x");
    }

    #[test]
    fn windows_verbatim_paths_become_drive_uris() {
        let p = r"\\?\C:\Users\runner\AppData\Local\Temp\vix-1\deep\new.cfg";
        let uri = file_uri(p, true);
        assert_eq!(
            uri,
            "file:///C:/Users/runner/AppData/Local/Temp/vix-1/deep/new.cfg"
        );
        assert_eq!(
            uri_path(&uri, true).unwrap(),
            r"C:\Users\runner\AppData\Local\Temp\vix-1\deep\new.cfg"
        );
        assert_eq!(file_uri(r"C:\a b\c.rs", true), "file:///C:/a%20b/c.rs");
        // Lower-case and encoded drive colons decode the same.
        assert_eq!(uri_path("file:///c%3A/x/y", true).unwrap(), r"c:\x\y");
        assert_eq!(uri_path("file:///C:", true).unwrap(), "C:");
    }

    #[test]
    fn windows_unc_paths_keep_their_server_as_the_host() {
        let uri = file_uri(r"\\?\UNC\srv\share\a.rs", true);
        assert_eq!(uri, "file://srv/share/a.rs");
        assert_eq!(uri_path(&uri, true).unwrap(), r"\\srv\share\a.rs");
        assert_eq!(file_uri(r"\\srv\share", true), "file://srv/share");
    }

    #[test]
    fn windows_leaves_remote_posix_paths_alone() {
        assert_eq!(
            uri_path("file:///home/u/a.rs", true).unwrap(),
            "/home/u/a.rs"
        );
        assert_eq!(uri_path("file://localhost/C:/a", true).unwrap(), r"C:\a");
    }

    #[test]
    fn bad_uris_are_none() {
        assert!(split_file_uri("https://x/y").is_none());
        assert!(split_file_uri("file:///bad%zz").is_none());
        assert!(split_file_uri("file:///%FF").is_none());
    }
}
