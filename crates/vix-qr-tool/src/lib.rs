//! QR code generation for Tools → QR Code.
//!
//! Encodes text (a selection, URL, or line) into a QR code rendered with Unicode
//! half-block characters, suitable for display in a TUI overlay and scanning from
//! the screen. The host shows the rendered art read-only.
//!
//! Two vertical modules share one terminal row via `Dense1x2` half-blocks, so
//! the code stays roughly square in a typical 1:2 character cell. The quiet
//! zone (blank border) is kept because scanners need it.
//!
//! Owning spec: `crates/vix-qr-tool/spec/index.md`.

use qrcode::QrCode;
use qrcode::render::unicode;

/// Render `text` as a QR code drawn with Unicode half-block characters, or
/// `None` when the text is empty or cannot be encoded (e.g. too long).
///
/// The error detail is dropped on purpose: the host only needs "can't show a
/// QR" (QR capacity tops out at a few thousand bytes).
///
/// # Examples
///
/// ```
/// let art = vix_qr_tool::render("https://example.com").unwrap();
/// assert!(art.lines().count() > 5);
/// assert!(vix_qr_tool::render("").is_none());
/// ```
#[must_use]
pub fn render(text: &str) -> Option<String> {
    if text.is_empty() {
        return None;
    }
    // Byte mode is chosen automatically by the `qrcode` crate; the error
    // correction level is its default (medium).
    let code = QrCode::new(text.as_bytes()).ok()?;
    let art = code
        .render::<unicode::Dense1x2>()
        .dark_color(unicode::Dense1x2::Dark)
        .light_color(unicode::Dense1x2::Light)
        .quiet_zone(true)
        .build();
    Some(art)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_multi_row_qr() {
        let art = render("https://example.com").expect("encodes");
        assert!(art.lines().count() > 5, "QR spans several rows");
        assert!(
            art.contains('█') || art.contains('▀') || art.contains('▄'),
            "uses block glyphs"
        );
    }

    #[test]
    fn empty_text_is_none() {
        assert!(render("").is_none());
    }

    proptest::proptest! {
        // Rendering arbitrary text never panics: too-long input (past QR
        // capacity) returns None rather than crashing.
        #[test]
        fn render_never_panics(s in ".*") {
            let _ = render(&s);
        }
    }
}
