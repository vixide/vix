//! QR code generation for Tools → QR Code.
//!
//! Encodes text (a selection, URL, or line) into a QR code rendered with Unicode
//! half-block characters, suitable for display in a TUI overlay and scanning from
//! the screen. The host shows the rendered art read-only.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use qrcode::QrCode;
use qrcode::render::unicode;

/// Render `text` as a QR code drawn with Unicode half-block characters, or
/// `None` when the text is empty or cannot be encoded (e.g. too long).
#[must_use]
pub fn render(text: &str) -> Option<String> {
    if text.is_empty() {
        return None;
    }
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
}
