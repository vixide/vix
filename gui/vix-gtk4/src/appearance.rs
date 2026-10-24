//! Whether the desktop asks for a dark appearance, from GTK's settings.
//!
//! GTK 4.20 added `gtk-interface-color-scheme`, which follows the desktop's
//! dark/light preference; it is looked up by name at run time, since this
//! build targets the GTK 4.0 API. Older GTKs fall back to
//! `gtk-application-prefer-dark-theme`, then to a theme name ending in
//! `-dark` (`Adwaita-dark`).

use gtk4::glib::{self, prelude::*};

/// Whether GTK's settings ask for a dark appearance (`false` with no
/// display, so no settings).
#[must_use]
pub fn dark() -> bool {
    let Some(settings) = gtk4::Settings::default() else {
        return false;
    };
    let scheme = settings
        .find_property("gtk-interface-color-scheme")
        .and_then(|_| {
            let value = settings.property_value("gtk-interface-color-scheme");
            glib::EnumValue::from_value(&value).map(|(_, v)| v.nick().to_string())
        });
    let prefer_dark = settings.is_gtk_application_prefer_dark_theme();
    let theme = settings.gtk_theme_name();
    is_dark(scheme.as_deref(), prefer_dark, theme.as_deref())
}

/// The decision itself: an explicit `dark`/`light` color scheme wins;
/// otherwise prefer-dark, otherwise a `-dark` theme name.
#[must_use]
pub fn is_dark(scheme: Option<&str>, prefer_dark: bool, theme: Option<&str>) -> bool {
    match scheme {
        Some("dark") => true,
        Some("light") => false,
        _ => prefer_dark || theme.is_some_and(|t| t.to_ascii_lowercase().ends_with("-dark")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_scheme_wins_then_prefer_dark_then_theme_name() {
        assert!(is_dark(Some("dark"), false, Some("Adwaita")));
        assert!(!is_dark(Some("light"), true, Some("Adwaita-dark")));
        // "default" / "unsupported" defer to the older settings.
        assert!(is_dark(Some("default"), true, None));
        assert!(is_dark(None, false, Some("Adwaita-dark")));
        assert!(!is_dark(None, false, Some("Adwaita")));
        assert!(!is_dark(None, false, None));
    }
}
