//! The on-disk layout of an interrupted-save recovery record (T865).
//!
//! **Framed (version 2).** Lengths frame both the path and the bytes, so any
//! native path (a newline in a Unix file name, non-UTF-8 bytes, a Windows
//! name with unpaired surrogates) and any contents (empty, binary, no final
//! newline) round-trip exactly:
//!
//! ```text
//! VIXRECOVER 2\n
//! path <kind> <length>\n<length bytes of path>\n
//! data <length>\n<length bytes of contents>\n
//! end\n
//! ```
//!
//! `<kind>` is `unix` (the path's raw bytes), `windows` (its UTF-16 code units,
//! little-endian) or `utf8`. Lengths are decimal with no sign and no leading
//! zero. Anything else, a short read or bytes after `end` make the record
//! invalid, and an invalid record is never offered.
//!
//! **Legacy (before T865).** `VIXRECOVER\n`, the path as `Path::display`, a
//! newline, then the bytes. Ambiguous: a newline in the path moves the split,
//! and `display` replaces non-UTF-8 bytes. See [`legacy_identity`].

use std::path::{Path, PathBuf};

/// The framed format's first line.
pub(crate) const FRAMED_MAGIC: &[u8] = b"VIXRECOVER 2";

/// The legacy format's first line.
pub(crate) const LEGACY_MAGIC: &[u8] = b"VIXRECOVER";

/// How a framed record stores its path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathKind {
    /// Raw Unix path bytes.
    Unix,
    /// UTF-16 code units, little-endian.
    Windows,
    /// UTF-8 text (any other platform).
    Utf8,
}

impl PathKind {
    /// The tag written in the `path` line.
    const fn tag(self) -> &'static str {
        match self {
            Self::Unix => "unix",
            Self::Windows => "windows",
            Self::Utf8 => "utf8",
        }
    }

    /// The kind a tag names.
    fn from_tag(tag: &[u8]) -> Option<Self> {
        match tag {
            b"unix" => Some(Self::Unix),
            b"windows" => Some(Self::Windows),
            b"utf8" => Some(Self::Utf8),
            _ => None,
        }
    }
}

/// A decoded framed record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Framed {
    /// How `path` is encoded.
    pub(crate) kind: PathKind,
    /// The encoded path.
    pub(crate) path: Vec<u8>,
    /// The original contents.
    pub(crate) data: Vec<u8>,
}

/// `path` in this platform's native framing.
#[cfg_attr(
    any(unix, windows),
    expect(
        clippy::unnecessary_wraps,
        reason = "`None` on platforms whose paths need not be UTF-8 and have no native framing"
    )
)]
pub(crate) fn native_path_field(path: &Path) -> Option<(PathKind, Vec<u8>)> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;
        Some((PathKind::Unix, path.as_os_str().as_bytes().to_vec()))
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt as _;
        Some((
            PathKind::Windows,
            path.as_os_str()
                .encode_wide()
                .flat_map(u16::to_le_bytes)
                .collect(),
        ))
    }
    #[cfg(not(any(unix, windows)))]
    {
        path.to_str()
            .map(|s| (PathKind::Utf8, s.as_bytes().to_vec()))
    }
}

/// The framed record for `kind`/`path` and `data`.
pub(crate) fn encode(kind: PathKind, path: &[u8], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(FRAMED_MAGIC);
    out.push(b'\n');
    out.extend_from_slice(format!("path {} {}\n", kind.tag(), path.len()).as_bytes());
    out.extend_from_slice(path);
    out.extend_from_slice(format!("\ndata {}\n", data.len()).as_bytes());
    out.extend_from_slice(data);
    out.extend_from_slice(b"\nend\n");
    out
}

/// A reader over a record's bytes that never indexes out of bounds.
struct Reader<'a> {
    /// What is left to read.
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    /// The bytes up to the next newline, consuming the newline.
    fn line(&mut self) -> Option<&'a [u8]> {
        let at = self.rest.iter().position(|b| *b == b'\n')?;
        let (line, rest) = self.rest.split_at_checked(at)?;
        self.rest = rest.get(1..)?;
        Some(line)
    }

    /// The next `n` bytes, then a newline.
    fn field(&mut self, n: usize) -> Option<&'a [u8]> {
        let (field, rest) = self.rest.split_at_checked(n)?;
        let (newline, rest) = rest.split_first()?;
        if *newline != b'\n' {
            return None;
        }
        self.rest = rest;
        Some(field)
    }
}

/// A decimal length: digits only, no leading zero but `0` itself.
fn length(text: &[u8]) -> Option<usize> {
    let ok = !text.is_empty()
        && text.iter().all(u8::is_ascii_digit)
        && (text == b"0" || text.first() != Some(&b'0'));
    if !ok {
        return None;
    }
    std::str::from_utf8(text).ok()?.parse().ok()
}

/// Decode a framed record; `None` for anything incomplete or malformed.
pub(crate) fn decode(raw: &[u8]) -> Option<Framed> {
    let mut r = Reader { rest: raw };
    if r.line()? != FRAMED_MAGIC {
        return None;
    }
    let mut head = r.line()?.split(|b| *b == b' ');
    let (Some(b"path"), Some(kind), Some(len), None) =
        (head.next(), head.next(), head.next(), head.next())
    else {
        return None;
    };
    let kind = PathKind::from_tag(kind)?;
    let path = r.field(length(len)?)?.to_vec();
    let mut head = r.line()?.split(|b| *b == b' ');
    let (Some(b"data"), Some(len), None) = (head.next(), head.next(), head.next()) else {
        return None;
    };
    let data = r.field(length(len)?)?.to_vec();
    if r.line()? != b"end" || !r.rest.is_empty() {
        return None;
    }
    if kind == PathKind::Windows && !path.as_chunks::<2>().1.is_empty() {
        return None;
    }
    Some(Framed { kind, path, data })
}

/// The UTF-16 code units of a `windows` path field.
fn wide_units(bytes: &[u8]) -> Vec<u16> {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect()
}

/// The recorded path as this platform's path, when it is exactly
/// representable here; `None` when the record came from another platform
/// and its path cannot be named without loss.
pub(crate) fn native_path(kind: PathKind, bytes: &[u8]) -> Option<PathBuf> {
    match kind {
        #[cfg(unix)]
        PathKind::Unix => {
            use std::os::unix::ffi::OsStringExt as _;
            Some(PathBuf::from(std::ffi::OsString::from_vec(bytes.to_vec())))
        }
        #[cfg(windows)]
        PathKind::Windows => {
            use std::os::windows::ffi::OsStringExt as _;
            Some(PathBuf::from(std::ffi::OsString::from_wide(&wide_units(
                bytes,
            ))))
        }
        PathKind::Utf8 => std::str::from_utf8(bytes).ok().map(PathBuf::from),
        // A path from another platform.
        _ => None,
    }
}

/// The recorded path for people to read, lossily where it must be.
pub(crate) fn display_path(kind: PathKind, bytes: &[u8]) -> String {
    match kind {
        PathKind::Unix | PathKind::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
        PathKind::Windows => String::from_utf16_lossy(&wide_units(bytes)),
    }
}

/// A decoded legacy record: the path line and the bytes after it.
pub(crate) fn decode_legacy(raw: &[u8]) -> Option<(String, Vec<u8>)> {
    let mut parts = raw.splitn(3, |b| *b == b'\n');
    if parts.next()? != LEGACY_MAGIC {
        return None;
    }
    let path = String::from_utf8(parts.next()?.to_vec()).ok()?;
    let data = parts.next()?.to_vec();
    Some((path, data))
}

/// Whether a legacy record's path line names its file without doubt.
///
/// The legacy writer named the record `<pid>-<seq>-<file name>.vixrecover`
/// (the name as UTF-8, else `file`) and wrote the path with
/// `Path::display`. The path is taken as established only when the line has
/// no U+FFFD (`display` writes one for bytes that are not UTF-8), is
/// absolute, names an existing regular file, and its file name equals the
/// one in the record's own name: a newline in the file name or in a
/// directory name shows up as a mismatch there. What remains (a directory
/// literally named `<x>\n` beside a file `<x>` whose name repeats the real
/// file's) cannot be told apart, and is accepted.
pub(crate) fn legacy_identity(copy: &Path, path_line: &str) -> Option<PathBuf> {
    if path_line.contains('\u{FFFD}') {
        return None;
    }
    let path = PathBuf::from(path_line);
    if !path.is_absolute() || !path.is_file() {
        return None;
    }
    let recorded = copy.file_name()?.to_str()?.strip_suffix(".vixrecover")?;
    let mut fields = recorded.splitn(3, '-');
    let pid = fields.next()?;
    let seq = fields.next()?;
    let name = fields.next()?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(pid) || !digits(seq) {
        return None;
    }
    (path.file_name()?.to_str()? == name).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framed_records_round_trip_and_reject_anything_short_or_extra() {
        for (path, data) in [
            (b"/tmp/a\nb".as_slice(), b"x\ny".as_slice()),
            (b"/tmp/\xff\xfe".as_slice(), b"".as_slice()),
            (b"".as_slice(), b"\0\nend\n".as_slice()),
        ] {
            let raw = encode(PathKind::Unix, path, data);
            let back = decode(&raw).unwrap();
            assert_eq!((back.path.as_slice(), back.data.as_slice()), (path, data));
            for cut in 0..raw.len() {
                assert_eq!(decode(raw.get(..cut).unwrap()), None, "cut at {cut}");
            }
            let mut extra = raw;
            extra.push(b'x');
            assert_eq!(decode(&extra), None);
        }
        assert_eq!(
            decode(b"VIXRECOVER 2\npath unix 01\na\ndata 0\n\nend\n"),
            None
        );
        assert_eq!(
            decode(b"VIXRECOVER 2\npath mac 1\na\ndata 0\n\nend\n"),
            None
        );
        assert_eq!(
            decode(b"VIXRECOVER 2\npath windows 1\na\ndata 0\n\nend\n"),
            None
        );
    }

    #[test]
    fn a_windows_path_keeps_its_code_units() {
        let units: Vec<u16> = "C:\\Users\\Zoë\\日記.txt".encode_utf16().collect();
        let bytes: Vec<u8> = units.iter().flat_map(|u| u.to_le_bytes()).collect();
        let back = decode(&encode(PathKind::Windows, &bytes, b"x")).unwrap();
        assert_eq!(wide_units(&back.path), units);
        assert_eq!(
            display_path(back.kind, &back.path),
            "C:\\Users\\Zoë\\日記.txt"
        );
        // Only Windows can name it exactly.
        assert_eq!(native_path(back.kind, &back.path).is_some(), cfg!(windows));
    }

    #[test]
    fn legacy_lines_split_on_the_first_two_newlines() {
        assert_eq!(
            decode_legacy(b"VIXRECOVER\n/a/b\nline\nmore"),
            Some(("/a/b".into(), b"line\nmore".to_vec()))
        );
        assert_eq!(decode_legacy(b"VIXRECOVER 2\n/a\nx"), None);
        assert_eq!(decode_legacy(b"VIXRECOVER"), None);
    }
}
