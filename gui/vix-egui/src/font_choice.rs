//! Which font families the window draws with.

/// Font size, in logical pixels.
pub const FONT_SIZE: f32 = 14.0;

/// Line height as a multiple of [`FONT_SIZE`].
pub const LINE_HEIGHT: f32 = 1.3;

/// The cell height for [`FONT_SIZE`], in logical pixels.
#[must_use]
pub fn cell_height() -> f32 {
    (FONT_SIZE * LINE_HEIGHT).round()
}

/// The monospace family for this platform, unless `VIX_GUI_FONT` names one.
#[must_use]
pub fn primary_family() -> String {
    std::env::var("VIX_GUI_FONT").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "Menlo".into()
        } else if cfg!(target_os = "windows") {
            "Consolas".into()
        } else {
            "DejaVu Sans Mono".into()
        }
    })
}

/// Installed families to fall back to for glyphs the primary font lacks.
///
/// Vix draws file, panel and status icons from the Nerd Font private-use
/// range (a terminal user gets them from a Nerd-patched terminal font), and a
/// stock monospace font has none of them. Prefer the icons-only "Symbols Nerd
/// Font", then any installed "… Nerd Font Mono" family; with none installed,
/// icons show the missing-glyph box, as in a non-Nerd terminal.
#[must_use]
pub fn nerd_fallbacks<S: AsRef<str>>(installed: &[S]) -> Vec<String> {
    let mut out: Vec<String> = ["Symbols Nerd Font Mono", "Symbols Nerd Font"]
        .into_iter()
        .filter(|f| installed.iter().any(|i| i.as_ref() == *f))
        .map(String::from)
        .collect();
    if let Some(mono) = installed
        .iter()
        .map(AsRef::as_ref)
        .find(|f| f.ends_with("Nerd Font Mono") && !out.iter().any(|o| o == f))
    {
        out.push(mono.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_symbols_then_any_nerd_mono() {
        let installed = ["Menlo", "Hack Nerd Font Mono", "Symbols Nerd Font Mono"];
        assert_eq!(
            nerd_fallbacks(&installed),
            ["Symbols Nerd Font Mono", "Hack Nerd Font Mono"]
        );
        assert_eq!(nerd_fallbacks(&["Menlo"]), Vec::<String>::new());
        // Symbols Mono is not listed twice when it is the only Nerd family.
        assert_eq!(
            nerd_fallbacks(&["Symbols Nerd Font Mono"]),
            ["Symbols Nerd Font Mono"]
        );
    }
}
