//! Which font families the window draws with, and the cell metrics to use
//! when none of them can be found.
//!
//! With a monospace family (`crate::fonts`) the cell is measured from it. If
//! none is installed BUI's embedded proportional font, Inter, draws instead:
//! then the cell is sized for Inter's widest common glyphs, with [`nudge`]
//! centering each character in it.

/// Font size, in logical pixels.
pub const FONT_SIZE: f32 = 14.0;

/// Line height as a multiple of [`FONT_SIZE`].
pub const LINE_HEIGHT: f32 = 1.3;

/// The horizontal distance between two cell columns with the embedded font,
/// in logical pixels.
///
/// Inter's average advance is about 0.56 em; 8 px at 14 px leaves a letter
/// pair a hair of air without a visible gap between neighbours.
pub const CELL_WIDTH: f32 = 8.0;

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

/// Broad-coverage system families per platform, for scripts and symbols a
/// monospace font lacks. BUI has no fallback of its own, unlike the toolkits
/// of the other front ends, so `crate::fonts` walks this list. Every writing
/// system should find a home here, not only CJK.
const SYSTEM_FALLBACKS: &[&str] = if cfg!(target_os = "macos") {
    &[
        "Apple Symbols",
        "Hiragino Sans",
        "PingFang SC",
        "Apple SD Gothic Neo",
        "Geeza Pro",
        "Arial Unicode MS",
        "Thonburi",
        "Kohinoor Devanagari",
    ]
} else if cfg!(target_os = "windows") {
    &[
        "Segoe UI Symbol",
        "Segoe UI Emoji",
        "Yu Gothic",
        "Microsoft YaHei",
        "Malgun Gothic",
        "Segoe UI",
        "Leelawadee UI",
        "Nirmala UI",
    ]
} else {
    &[
        "Noto Sans CJK JP",
        "Noto Sans CJK SC",
        "Noto Sans Symbols2",
        "Noto Sans Symbols",
        "Noto Emoji",
        "Noto Sans",
        "DejaVu Sans",
        "WenQuanYi Micro Hei",
    ]
};

/// Every installed family to try for a glyph the primary font lacks, in
/// order: Nerd Fonts first, for Vix's icons, then the system's broad ones.
#[must_use]
pub fn fallbacks<S: AsRef<str>>(installed: &[S]) -> Vec<String> {
    let mut out = nerd_fallbacks(installed);
    out.extend(
        SYSTEM_FALLBACKS
            .iter()
            .filter(|f| installed.iter().any(|i| i.as_ref() == **f))
            .map(ToString::to_string),
    );
    out
}

/// How far to move a character right (negative: left) inside its cell so a
/// proportional glyph reads as if centered: Inter's `i`, `l` and `.` are far
/// narrower than a cell and its `m` and `W` wider. A rough, by-eye table,
/// since BUI exposes no text measurement.
#[must_use]
pub fn nudge(ch: char) -> f32 {
    match ch {
        'i' | 'l' | '.' | ',' | ':' | ';' | '!' | '|' | '\'' | '`' => 2.0,
        'j' | 't' | 'f' | 'I' | 'r' | '1' | '(' | ')' | '[' | ']' | '{' | '}' => 1.0,
        'm' | 'M' => -2.0,
        'w' | 'W' | '@' => -1.5,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrow_glyphs_move_right_and_wide_ones_left() {
        assert!(nudge('l') > nudge('t'));
        assert!(nudge('t') > nudge('e'));
        assert!(nudge('m') < 0.0);
        assert!(nudge('e').abs() < f32::EPSILON);
        // No nudge pushes a glyph out of its cell.
        for c in ' '..='~' {
            assert!(nudge(c).abs() <= CELL_WIDTH / 3.0, "{c}");
        }
    }

    /// The dedicated Symbols Nerd Font is preferred over other installed Nerd Font Mono families.
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

    #[test]
    fn fallbacks_put_nerd_fonts_first_and_skip_what_is_not_installed() {
        let system = SYSTEM_FALLBACKS[0];
        let installed = [system, "Not A Font", "Hack Nerd Font Mono"];
        assert_eq!(fallbacks(&installed), ["Hack Nerd Font Mono", system]);
        assert_eq!(fallbacks(&["Not A Font"]), Vec::<String>::new());
    }

    #[test]
    fn cell_is_taller_than_wide_and_whole_pixels() {
        assert!(cell_height() > CELL_WIDTH);
        assert!((cell_height() - cell_height().round()).abs() < f32::EPSILON);
        assert!((CELL_WIDTH - CELL_WIDTH.round()).abs() < f32::EPSILON);
    }
}
