//! The window's fonts as iced `Font`s, and the cell size measured from them.
//!
//! iced draws text through cosmic-text, whose font database already holds
//! every installed system font, and whose advanced shaping falls back per
//! glyph to any installed face that has it. So unlike vix-egui nothing is
//! loaded by hand here: the primary family (`crate::font_choice`) is named,
//! and the Nerd Font family chosen by `font_choice::nerd_fallbacks` is named
//! explicitly for private-use-area icons, so they come from the same family
//! as in the other builds rather than from whichever face cosmic-text finds
//! first.

use crate::font_choice::{self, FONT_SIZE, cell_height};
use iced::advanced::graphics::text::{Paragraph, font_system};
use iced::advanced::text::{self, Paragraph as _};
use iced::alignment::Vertical;
use iced::font::{Family, Style, Weight};
use iced::{Font, Pixels, Size};

/// The fonts a frame is drawn with, and the cell size they give.
#[derive(Clone, Copy, Debug)]
pub struct Fonts {
    /// The primary monospace family, regular weight.
    pub regular: Font,
    /// The primary family's bold face.
    pub bold: Font,
    /// The installed Nerd Font family for icons, if any.
    pub icons: Option<Font>,
    /// One cell, in logical pixels: the primary font's advance by
    /// [`cell_height`].
    pub cell: Size,
}

impl Fonts {
    /// Resolve the families and measure the cell. Call once: iced needs
    /// `'static` family names, so the names are leaked (a few bytes, once).
    #[must_use]
    pub fn load() -> Self {
        let primary: &'static str = Box::leak(font_choice::primary_family().into_boxed_str());
        let regular = Font {
            family: Family::Name(primary),
            ..Font::MONOSPACE
        };
        let icons = font_choice::nerd_fallbacks(&installed_families())
            .into_iter()
            .next()
            .map(|name| Font {
                family: Family::Name(Box::leak(name.into_boxed_str())),
                ..Font::MONOSPACE
            });
        Self {
            regular,
            bold: Font {
                weight: Weight::Bold,
                ..regular
            },
            icons,
            cell: Size::new(advance(regular), cell_height()),
        }
    }

    /// The font for a span: the icon family for a Nerd Font glyph, else the
    /// primary family in the span's weight and slant.
    #[must_use]
    pub fn for_span(&self, text: &str, bold: bool, italic: bool) -> Font {
        if let Some(icons) = self.icons
            && text.chars().any(is_private_use)
        {
            return icons;
        }
        let font = if bold { self.bold } else { self.regular };
        if italic {
            Font {
                style: Style::Italic,
                ..font
            }
        } else {
            font
        }
    }
}

/// Whether `c` is in a Unicode private-use area (where Nerd Font icons live).
#[must_use]
pub fn is_private_use(c: char) -> bool {
    matches!(c, '\u{e000}'..='\u{f8ff}' | '\u{f0000}'..='\u{ffffd}' | '\u{100000}'..='\u{10fffd}')
}

/// Every installed family name in cosmic-text's font database (the one iced
/// draws with).
fn installed_families() -> Vec<String> {
    let Ok(mut fonts) = font_system().write() else {
        return Vec::new();
    };
    let mut names: Vec<String> = fonts
        .raw()
        .db()
        .faces()
        .flat_map(|face| face.families.iter().map(|(name, _)| name.clone()))
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

/// The horizontal advance of one cell of `font` at [`FONT_SIZE`], measured
/// over a run of glyphs so rounding in the shaper does not accumulate.
fn advance(font: Font) -> f32 {
    const GLYPHS: u16 = 100;
    let sample = "M".repeat(usize::from(GLYPHS));
    let paragraph = Paragraph::with_text(text::Text {
        content: sample.as_str(),
        bounds: Size::INFINITE,
        size: Pixels(FONT_SIZE),
        line_height: text::LineHeight::Absolute(Pixels(cell_height())),
        font,
        align_x: text::Alignment::Left,
        align_y: Vertical::Top,
        shaping: text::Shaping::Basic,
        wrapping: text::Wrapping::None,
    });
    let width = paragraph.min_width() / f32::from(GLYPHS);
    if width > 0.0 { width } else { FONT_SIZE * 0.6 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nerd_icons_are_private_use() {
        assert!(is_private_use('\u{f07b}')); // nf-fa-folder
        assert!(is_private_use('\u{e7a8}')); // nf-dev-rust
        assert!(is_private_use('\u{f0219}')); // nf-md (supplementary PUA-A)
        assert!(!is_private_use('a'));
        assert!(!is_private_use('│'));
        assert!(!is_private_use('日'));
    }
}
