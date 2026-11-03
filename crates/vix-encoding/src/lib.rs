//! Text-file encodings (T661): detect a file's encoding and BOM, decode it to
//! text, and encode it back byte for byte.
//!
//! Decoding goes through [`encoding_rs`]; a file with neither a BOM nor valid
//! UTF-8 is guessed with [`chardetng`]. A guess is only accepted when it
//! decodes without errors *and* re-encodes to the exact same bytes, so saving
//! an untouched buffer can never change the file.
//!
//! # How the pieces fit
//!
//! * [`TextEncoding`] names an encoding plus a BOM flag; it is what a tab
//!   remembers so a save writes the file back in its original encoding.
//! * [`detect`] is the open path (BOM, then UTF-8, then a guess);
//!   [`decode_exact`] / [`decode_lossy`] serve the explicit "Reopen with
//!   Encoding" chooser; [`encode`] is the save path.
//! * The lossless invariant (decode, then re-encode, equals the input) is
//!   enforced in exactly one place, [`decode_exact`], and everything else is
//!   built on it.
//!
//! The crate is pure (bytes in, text out); the host does all file I/O.
//!
//! Spec: `crates/vix-encoding/spec/index.md`.
//!
//! # Examples
//! ```
//! use vix_encoding::{detect, encode, TextEncoding};
//!
//! let enc = TextEncoding::from_label("UTF-16LE").unwrap();
//! let bytes = encode("hi\n", enc).unwrap();
//! assert_eq!(&bytes[..2], b"\xFF\xFE"); // BOM first
//! let (text, found) = detect(&bytes).unwrap();
//! assert_eq!((text.as_str(), found), ("hi\n", enc));
//! ```

use std::borrow::Cow;
use std::fmt;

use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE};

/// A file's encoding plus whether it starts with a byte-order mark.
///
/// The fields are private so the only constructors are [`TextEncoding::UTF8`],
/// [`TextEncoding::from_label`] and [`sniff_bom`], all of which guarantee an
/// encoding that [`encode`] can round-trip (no `replacement`, no stateful
/// ISO-2022-JP) and that UTF-16 always carries a BOM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextEncoding {
    encoding: &'static Encoding,
    bom: bool,
}

/// The default is plain UTF-8 without a BOM, matching new files.
impl Default for TextEncoding {
    fn default() -> Self {
        Self::UTF8
    }
}

/// Renders as the canonical name, plus a trailing ` BOM` when applicable
/// (`UTF-8`, `UTF-16LE BOM`); this is the status-bar and chooser text.
impl fmt::Display for TextEncoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())?;
        if self.bom {
            f.write_str(" BOM")?;
        }
        Ok(())
    }
}

/// The encodings the Reopen/Save with Encoding choosers offer, by label.
/// `ISO-8859-1` is decoded as `windows-1252`, as every browser does; the
/// two agree on every byte outside 0x80–0x9F.
///
/// Every entry is accepted by [`TextEncoding::from_label`] (pinned by a test).
/// Labels here are a curated subset: [`TextEncoding::from_label`] accepts any
/// WHATWG label, but only these are shown in the UI.
pub const CHOICES: &[&str] = &[
    "UTF-8",
    "UTF-8 BOM",
    "UTF-16LE BOM",
    "UTF-16BE BOM",
    "windows-1252",
    "windows-1250",
    "windows-1251",
    "ISO-8859-2",
    "ISO-8859-15",
    "KOI8-R",
    "GBK",
    "gb18030",
    "Big5",
    "Shift_JIS",
    "EUC-JP",
    "EUC-KR",
];

impl TextEncoding {
    /// UTF-8 without a BOM: what new files and plain UTF-8 files use.
    pub const UTF8: Self = Self {
        encoding: UTF_8,
        bom: false,
    };

    /// The encoding a label names (`"UTF-8"`, `"Shift_JIS"`, `"latin1"`, …,
    /// any [WHATWG label](https://encoding.spec.whatwg.org/#names-and-labels)),
    /// with a BOM when the label ends in ` BOM`. UTF-16 always has a BOM,
    /// since without one it can't be told apart from binary. `None` for an
    /// unknown label, or one (like `replacement`) that can't round-trip.
    ///
    /// # Examples
    /// ```
    /// use vix_encoding::TextEncoding;
    ///
    /// assert_eq!(TextEncoding::from_label("latin1").unwrap().name(), "windows-1252");
    /// assert!(TextEncoding::from_label("UTF-8 BOM").unwrap().has_bom());
    /// assert!(TextEncoding::from_label("replacement").is_none());
    /// ```
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        let (name, bom) = match label.trim().strip_suffix(" BOM") {
            Some(name) => (name, true),
            None => (label.trim(), false),
        };
        let encoding = Encoding::for_label(name.as_bytes())?;
        // `output_encoding()` is the encoding encoding_rs actually *writes* for
        // this one; it differs for `replacement` and `x-user-defined`, which
        // would silently re-encode as UTF-8. UTF-16 is exempt because it is
        // also "output as UTF-8" per WHATWG, but `encode` hand-writes it.
        if encoding.output_encoding() != encoding && !is_utf16(encoding) {
            return None; // e.g. `replacement`, `x-user-defined`
        }
        if encoding == encoding_rs::ISO_2022_JP {
            return None; // stateful; not offered
        }
        Some(Self {
            encoding,
            bom: bom || is_utf16(encoding),
        })
    }

    /// The canonical name, such as `UTF-8`, `UTF-16LE` or `Shift_JIS`.
    #[must_use]
    pub fn name(&self) -> &'static str {
        self.encoding.name()
    }

    /// Whether the file starts with a byte-order mark.
    #[must_use]
    pub fn has_bom(&self) -> bool {
        self.bom
    }

    /// Whether this is plain UTF-8 with no BOM (the default).
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::UTF8
    }

    /// The BOM bytes this encoding writes first (empty when it has none).
    ///
    /// A BOM-carrying legacy (non-Unicode) encoding cannot be built through
    /// [`TextEncoding::from_label`] except as `"<label> BOM"`; the final arm
    /// then writes the UTF-8 BOM, which is only meaningful for UTF-8.
    fn bom_bytes(&self) -> &'static [u8] {
        match (self.bom, self.encoding) {
            (false, _) => &[],
            (true, e) if e == UTF_16LE => b"\xFF\xFE",
            (true, e) if e == UTF_16BE => b"\xFE\xFF",
            (true, _) => b"\xEF\xBB\xBF",
        }
    }
}

/// Whether `encoding` is one of the two UTF-16 flavours, which `encoding_rs`
/// can decode but not encode, so [`encode`] special-cases them.
fn is_utf16(encoding: &'static Encoding) -> bool {
    encoding == UTF_16LE || encoding == UTF_16BE
}

/// The encoding a byte-order mark at the start of `bytes` names, if any.
///
/// Recognises the UTF-8, UTF-16LE and UTF-16BE marks. The returned encoding
/// always has its BOM flag set.
///
/// # Examples
/// ```
/// let enc = vix_encoding::sniff_bom(b"\xEF\xBB\xBFhello").unwrap();
/// assert_eq!(enc.to_string(), "UTF-8 BOM");
/// assert!(vix_encoding::sniff_bom(b"hello").is_none());
/// ```
#[must_use]
pub fn sniff_bom(bytes: &[u8]) -> Option<TextEncoding> {
    let (encoding, _) = Encoding::for_bom(bytes)?;
    Some(TextEncoding {
        encoding,
        bom: true,
    })
}

/// Decode a file's bytes, choosing the encoding: a BOM wins, then valid
/// UTF-8, then a [`chardetng`] guess that decodes cleanly and round-trips
/// byte for byte. `None` when nothing qualifies; the caller then falls back
/// to a lossy, read-only open.
///
/// The returned text never includes the BOM; the BOM is remembered in the
/// returned [`TextEncoding`] and re-emitted by [`encode`].
///
/// # Examples
/// ```
/// use vix_encoding::{detect, TextEncoding};
///
/// assert_eq!(detect(b"plain\n"), Some(("plain\n".to_string(), TextEncoding::UTF8)));
/// ```
#[must_use]
pub fn detect(bytes: &[u8]) -> Option<(String, TextEncoding)> {
    if let Some(enc) = sniff_bom(bytes) {
        return decode_exact(bytes, enc).map(|text| (text, enc));
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Some((text.to_string(), TextEncoding::UTF8));
    }
    // Not a BOM and not valid UTF-8: ask chardetng. Deny ISO-2022-JP (stateful,
    // never offered) and deny UTF-8 (we already know it isn't valid UTF-8).
    // `true` marks this as the last chunk, so the detector can finalize.
    let mut detector = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
    detector.feed(bytes, true);
    let encoding = detector.guess(None, chardetng::Utf8Detection::Deny);
    // A guess is only a candidate: `from_label` may reject it, and
    // `decode_exact` rejects it unless it round-trips byte for byte.
    let enc = TextEncoding::from_label(encoding.name())?;
    decode_exact(bytes, enc).map(|text| (text, enc))
}

/// Decode `bytes` as `enc`, only if that is lossless: no malformed sequences,
/// and [`encode`] gives back exactly `bytes`. A BOM must be there when `enc`
/// has one, and is never part of the text.
///
/// Returns `None` (rather than an error) because callers only need to know
/// "not losslessly decodable as `enc`", then fall back to [`decode_lossy`].
///
/// # Examples
/// ```
/// use vix_encoding::{decode_exact, TextEncoding};
///
/// let latin = TextEncoding::from_label("windows-1252").unwrap();
/// assert_eq!(decode_exact(b"caf\xE9", latin).as_deref(), Some("caf\u{e9}"));
/// // Invalid UTF-8 is not an exact UTF-8 decode.
/// assert!(decode_exact(b"caf\xE9", TextEncoding::UTF8).is_none());
/// ```
#[must_use]
pub fn decode_exact(bytes: &[u8], enc: TextEncoding) -> Option<String> {
    // `strip_prefix` fails (`None`) when a BOM is required but absent; with an
    // empty BOM it is a no-op.
    let body = bytes.strip_prefix(enc.bom_bytes())?;
    let text = enc
        .encoding
        .decode_without_bom_handling_and_without_replacement(body)?
        .into_owned();
    // Round-trip check: catches decoders that map distinct byte sequences to
    // the same text (e.g. non-canonical or overlong forms in some CJK sets),
    // where saving would otherwise silently rewrite the file.
    (encode(&text, enc).ok()? == bytes).then_some(text)
}

/// Decode `bytes` as `enc` even if it isn't lossless (Reopen with Encoding):
/// malformed sequences become U+FFFD. The flag is `true` when anything was
/// replaced, so the caller can open it read-only.
///
/// # Examples
/// ```
/// use vix_encoding::{decode_lossy, TextEncoding};
///
/// let (text, lossy) = decode_lossy(b"caf\xE9", TextEncoding::UTF8);
/// assert_eq!(text, "caf\u{FFFD}");
/// assert!(lossy);
/// ```
#[must_use]
pub fn decode_lossy(bytes: &[u8], enc: TextEncoding) -> (String, bool) {
    if let Some(text) = decode_exact(bytes, enc) {
        return (text, false);
    }
    // Unlike `decode_exact`, a missing BOM is tolerated here: this path exists
    // to open files that do not match, so it decodes whatever is there.
    let body = bytes.strip_prefix(enc.bom_bytes()).unwrap_or(bytes);
    let (text, _) = enc.encoding.decode_without_bom_handling(body);
    (text.into_owned(), true)
}

/// A character the target encoding has no bytes for, so the text can't be
/// saved in it without losing data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
///
/// Returned by [`encode`]; the host shows it in the save-failed message
/// instead of writing a lossy `?`.
pub struct Unmappable {
    /// The first (in text order) character that doesn't fit. `U+FFFD` if the
    /// culprit could not be isolated (never expected).
    pub ch: char,
    /// The encoding that can't hold it.
    pub encoding: TextEncoding,
}

impl fmt::Display for Unmappable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} can't encode {:?} (U+{:04X})",
            self.encoding,
            self.ch,
            u32::from(self.ch)
        )
    }
}

impl std::error::Error for Unmappable {}

/// Encode `text` as `enc`, BOM first when it has one.
///
/// # Errors
///
/// [`Unmappable`] when `text` holds a character `enc` can't represent;
/// nothing is ever replaced with `?` or an HTML entity.
///
/// # Examples
/// ```
/// use vix_encoding::{encode, TextEncoding};
///
/// let latin = TextEncoding::from_label("windows-1252").unwrap();
/// assert_eq!(encode("caf\u{e9}", latin).unwrap(), b"caf\xE9");
/// let err = encode("snow \u{2603}", latin).unwrap_err();
/// assert_eq!(err.ch, '\u{2603}');
/// ```
pub fn encode(text: &str, enc: TextEncoding) -> Result<Vec<u8>, Unmappable> {
    let mut out = enc.bom_bytes().to_vec();
    if enc.encoding == UTF_8 {
        out.extend_from_slice(text.as_bytes());
    } else if is_utf16(enc.encoding) {
        // encoding_rs's encoders never emit UTF-16 (per the WHATWG spec they
        // fall back to UTF-8), so these two are written by hand.
        let big = enc.encoding == UTF_16BE;
        for unit in text.encode_utf16() {
            out.extend_from_slice(&if big {
                unit.to_be_bytes()
            } else {
                unit.to_le_bytes()
            });
        }
    } else {
        // `encode` substitutes numeric character references for unmappable
        // characters and reports `had_errors`; we treat that as a hard failure
        // so a save can never lose data.
        let (bytes, _, had_errors) = enc.encoding.encode(text);
        if had_errors {
            let ch = first_unmappable(text, enc.encoding).unwrap_or('\u{FFFD}');
            return Err(Unmappable { ch, encoding: enc });
        }
        out.extend_from_slice(&bytes);
    }
    Ok(out)
}

/// Find the first character of `text` that `encoding` cannot represent, by
/// encoding each char on its own. Only run on the (rare) error path, so the
/// per-char cost does not matter.
fn first_unmappable(text: &str, encoding: &'static Encoding) -> Option<char> {
    let mut buf = [0u8; 4];
    text.chars().find(|c| {
        let (_, _, bad): (Cow<'_, [u8]>, _, bool) = encoding.encode(c.encode_utf8(&mut buf));
        bad
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(label: &str) -> TextEncoding {
        TextEncoding::from_label(label).expect(label)
    }

    /// Every chooser entry resolves, and encoding then decoding sample text
    /// in it gives back the same bytes and text.
    #[test]
    fn every_choice_round_trips_byte_for_byte() {
        let samples: &[(&str, &str)] = &[
            ("UTF-8", "héllo → 世界\r\n"),
            ("UTF-8 BOM", "héllo → 世界\n"),
            ("UTF-16LE BOM", "héllo 𝄞 世界\n"),
            ("UTF-16BE BOM", "héllo 𝄞 世界\n"),
            ("windows-1252", "café – “quoted” €\n"),
            ("windows-1250", "Łódź, Žluťoučký\n"),
            ("windows-1251", "Привет, мир\n"),
            ("ISO-8859-2", "Łódź\n"),
            ("ISO-8859-15", "€ café\n"),
            ("KOI8-R", "Привет\n"),
            ("GBK", "你好，世界\n"),
            ("gb18030", "你好 😀\n"),
            ("Big5", "你好，世界\n"),
            ("Shift_JIS", "こんにちは、世界\n"),
            ("EUC-JP", "こんにちは\n"),
            ("EUC-KR", "안녕하세요\n"),
        ];
        assert_eq!(samples.len(), CHOICES.len());
        for (label, text) in samples {
            let e = enc(label);
            let bytes = encode(text, e).unwrap();
            assert_eq!(decode_exact(&bytes, e).as_deref(), Some(*text), "{label}");
        }
        for label in CHOICES {
            enc(label);
        }
    }

    /// A BOM picks the encoding and is kept for the save, never put in the text.
    #[test]
    fn detect_honors_and_keeps_a_bom() {
        for label in ["UTF-8 BOM", "UTF-16LE BOM", "UTF-16BE BOM"] {
            let e = enc(label);
            let bytes = encode("a\nb\n", e).unwrap();
            let (text, found) = detect(&bytes).unwrap();
            assert_eq!((text.as_str(), found), ("a\nb\n", e), "{label}");
            assert!(found.has_bom());
        }
        assert_eq!(
            detect(b"plain\n"),
            Some(("plain\n".into(), TextEncoding::UTF8))
        );
    }

    /// Legacy text with no BOM is guessed and still saves back unchanged.
    #[test]
    fn detect_guesses_legacy_encodings() {
        for (label, text) in [
            (
                "Shift_JIS",
                "日本語のテキストファイルです。改行もあります。\n",
            ),
            (
                "windows-1251",
                "Это обычный текстовый файл на русском языке.\n",
            ),
            ("GBK", "这是一个普通的中文文本文件，用于测试编码检测。\n"),
            ("EUC-KR", "이것은 한국어로 된 일반 텍스트 파일입니다.\n"),
        ] {
            let bytes = encode(text, enc(label)).unwrap();
            let (decoded, found) = detect(&bytes).unwrap_or_else(|| panic!("{label}"));
            assert_eq!(decoded, text, "{label} guessed as {found}");
            assert_eq!(encode(&decoded, found).unwrap(), bytes);
        }
    }

    /// Saving a character the encoding lacks fails instead of writing `?`.
    #[test]
    fn unmappable_characters_refuse_to_encode() {
        let err = encode("snow ☃", enc("windows-1252")).unwrap_err();
        assert_eq!(err.ch, '☃');
        assert!(encode("☃", enc("UTF-16LE BOM")).is_ok());
    }

    /// A BOM the encoding expects but the bytes lack fails the exact decode;
    /// the lossy one still opens it, flagged.
    #[test]
    fn lossy_decode_flags_replacements() {
        assert!(decode_exact(b"abc", enc("UTF-8 BOM")).is_none());
        let (text, lossy) = decode_lossy(b"caf\xE9", TextEncoding::UTF8);
        assert_eq!((text.as_str(), lossy), ("caf\u{FFFD}", true));
        let (text, lossy) = decode_lossy(b"caf\xE9", enc("windows-1252"));
        assert_eq!((text.as_str(), lossy), ("café", false));
    }

    #[test]
    fn labels_and_display() {
        assert_eq!(enc("latin1").name(), "windows-1252");
        assert_eq!(enc("UTF-16LE").to_string(), "UTF-16LE BOM");
        assert_eq!(TextEncoding::UTF8.to_string(), "UTF-8");
        assert!(TextEncoding::from_label("replacement").is_none());
        assert!(TextEncoding::from_label("nope").is_none());
    }
}
