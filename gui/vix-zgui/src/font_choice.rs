//! Which font families the window draws with.

/// Font size, in logical pixels.
pub const FONT_SIZE: f32 = 14.0;

/// Line height as a multiple of [`FONT_SIZE`].
pub const LINE_HEIGHT: f32 = 1.3;

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

/// Families to fall back to for glyphs the primary font lacks, before the
/// system's own fallback.
///
/// Vix draws file, panel and status icons from the Nerd Font private-use
/// range (a terminal user gets them from a Nerd-patched terminal font), and a
/// stock monospace font has none of them. The icons-only "Symbols Nerd Font"
/// comes first; a family that is not installed is skipped by zgui's font
/// stack, and with none installed icons show the missing-glyph box, as in a
/// non-Nerd terminal.
pub const NERD_FAMILIES: [&str; 2] = ["Symbols Nerd Font Mono", "Symbols Nerd Font"];
