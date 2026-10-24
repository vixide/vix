//! The window's fonts: the primary monospace family from
//! `crate::font_choice`, and a Nerd Font for Vix's icons.
//!
//! Slint's text stack (parley over fontique) takes one family name per
//! `Text` and falls back by *script* to the system's sans-serif and UI
//! fonts, which covers CJK and emoji but not the Nerd Font icons in the
//! private-use area, which belong to no script. So the icon family is chosen
//! here, from the families in Slint's own font collection, and each icon span
//! names it directly (`render::row_text` gives every non-ASCII cell its own
//! span).

use crate::font_choice;

/// The families text is drawn with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Families {
    /// The primary monospace family.
    pub primary: String,
    /// The installed Nerd Font for icons, if any.
    pub icons: Option<String>,
}

impl Families {
    /// The primary family, and the best installed Nerd Font from Slint's
    /// font collection. Call once Slint's platform is set up (after the
    /// window exists).
    #[must_use]
    pub fn load() -> Self {
        let mut collection = slint::fontique_011::shared_collection();
        let installed: Vec<String> = collection.family_names().map(String::from).collect();
        Self {
            primary: font_choice::primary_family(),
            icons: font_choice::nerd_fallbacks(&installed).into_iter().next(),
        }
    }

    /// The family for a span of `text`: the icon family for a Nerd Font
    /// icon (when one is installed), else the primary family.
    #[must_use]
    pub fn for_text(&self, text: &str) -> &str {
        match &self.icons {
            Some(icons) if text.chars().next().is_some_and(is_icon) => icons,
            _ => &self.primary,
        }
    }
}

/// Whether `c` is in a private-use area, where Nerd Fonts put their icons.
#[must_use]
pub fn is_icon(c: char) -> bool {
    matches!(c, '\u{E000}'..='\u{F8FF}' | '\u{F0000}'..='\u{FFFFD}' | '\u{100000}'..='\u{10FFFD}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_use_the_nerd_family_when_installed() {
        let mut f = Families {
            primary: "Menlo".into(),
            icons: Some("Symbols Nerd Font Mono".into()),
        };
        assert_eq!(f.for_text("\u{f07b}"), "Symbols Nerd Font Mono");
        assert_eq!(f.for_text("\u{f0219}"), "Symbols Nerd Font Mono");
        assert_eq!(f.for_text("abc"), "Menlo");
        assert_eq!(f.for_text("─"), "Menlo");
        f.icons = None;
        assert_eq!(f.for_text("\u{f07b}"), "Menlo");
    }
}
