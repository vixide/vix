//! Color-depth fallback (T628): how many colors the terminal can show, and a
//! pure [`downsample`] that maps a color into that depth.
//!
//! Themes are written in RGB. A terminal that only knows the xterm 256-color
//! palette, or only the 16 ANSI colors, shows raw RGB escapes wrongly or not
//! at all, so the host downsamples every finished frame
//! ([`downsample_buffer`]) when [`detect`] finds less than truecolor. The
//! detection reads `COLORTERM`, `TERM` and `TERM_PROGRAM`, which the caller
//! passes in as data ([`TermEnv`]): nothing here reads or changes the
//! process environment, and nothing queries the terminal.
//!
//! Heuristic by design: there is no portable way to ask a terminal its depth,
//! so unknown-but-modern terminals are assumed 256-color (a safe middle: RGB
//! escapes on a 256-color terminal are the common failure) and only a short
//! allow-list is promoted to truecolor. A user can still force truecolor with
//! `COLORTERM`. Owning spec: `crates/vix-theme/spec/index.md`.

use ratatui::buffer::Buffer;
use ratatui::style::Color;

/// How many colors a terminal can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorDepth {
    /// 24-bit RGB: colors pass through unchanged.
    #[default]
    TrueColor,
    /// The xterm 256-color palette.
    Ansi256,
    /// The 16 ANSI colors.
    Ansi16,
}

impl ColorDepth {
    /// A short stable name for reports (`--doctor`): `truecolor`, `256`, `16`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            ColorDepth::TrueColor => "truecolor",
            ColorDepth::Ansi256 => "256",
            ColorDepth::Ansi16 => "16",
        }
    }
}

/// The environment variables color-depth detection looks at, passed in as
/// data so detection is pure (and testable with a fake environment).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TermEnv<'a> {
    /// `COLORTERM`, e.g. `truecolor` or `24bit`.
    pub colorterm: Option<&'a str>,
    /// `TERM`, e.g. `xterm-256color`.
    pub term: Option<&'a str>,
    /// `TERM_PROGRAM`, e.g. `Apple_Terminal` or `iTerm.app`.
    pub term_program: Option<&'a str>,
}

/// `TERM_PROGRAM` values of terminals known to speak 24-bit color.
const TRUECOLOR_PROGRAMS: &[&str] = &["iTerm.app", "WezTerm", "vscode", "ghostty", "Hyper", "zed"];

/// `TERM` values (or prefixes) of terminals known to speak 24-bit color.
const TRUECOLOR_TERMS: &[&str] = &[
    "xterm-kitty",
    "xterm-ghostty",
    "wezterm",
    "alacritty",
    "foot",
    "contour",
];

/// Decide the terminal's color depth from its environment.
///
/// # Examples
///
/// ```
/// use vix_theme::color_depth::{detect, ColorDepth, TermEnv};
/// let env = TermEnv { term: Some("xterm-256color"), ..TermEnv::default() };
/// assert_eq!(detect(&env), ColorDepth::Ansi256);
/// assert_eq!(detect(&TermEnv::default()), ColorDepth::TrueColor);
/// ```
///
/// In order: `COLORTERM=truecolor|24bit` means truecolor;
/// `TERM_PROGRAM=Apple_Terminal` means 256 colors (Terminal.app ignores RGB
/// escapes); a known truecolor `TERM_PROGRAM` or `TERM`, or a `TERM` naming
/// `truecolor`/`24bit`/`direct`, means truecolor; a `TERM` naming `256color`
/// means 256; `dumb`, `linux`, `vt*`, `ansi` and `cons*` mean 16; any other
/// `TERM` means 256. No `TERM` at all (a Windows console) means truecolor.
#[must_use]
pub fn detect(env: &TermEnv<'_>) -> ColorDepth {
    // Explicit user/terminal declaration beats every heuristic below.
    let colorterm = env.colorterm.unwrap_or("").trim().to_ascii_lowercase();
    if colorterm == "truecolor" || colorterm == "24bit" {
        return ColorDepth::TrueColor;
    }
    let program = env.term_program.unwrap_or("").trim();
    // Checked before the generic `TERM` rules because Terminal.app sets
    // `TERM=xterm-256color` yet mis-renders 24-bit escapes.
    if program == "Apple_Terminal" {
        return ColorDepth::Ansi256;
    }
    if TRUECOLOR_PROGRAMS.contains(&program) {
        return ColorDepth::TrueColor;
    }
    // No `TERM` (e.g. a Windows console, which supports VT RGB) => truecolor.
    let Some(term) = env.term.map(str::trim).filter(|t| !t.is_empty()) else {
        return ColorDepth::TrueColor;
    };
    let term = term.to_ascii_lowercase();
    if TRUECOLOR_TERMS.iter().any(|t| term.starts_with(t))
        || ["truecolor", "24bit", "direct"]
            .iter()
            .any(|s| term.contains(s))
    {
        return ColorDepth::TrueColor;
    }
    if term.contains("256color") {
        return ColorDepth::Ansi256;
    }
    if term == "dumb"
        || term == "linux"
        || term == "ansi"
        || term.starts_with("vt")
        || term.starts_with("cons")
    {
        return ColorDepth::Ansi16;
    }
    // Unknown `TERM`: assume the common 256-color baseline.
    ColorDepth::Ansi256
}

/// The xterm palette's RGB value for each of the 16 ANSI colors, in index
/// order, paired with ratatui's name for it.
const ANSI16: [(Color, (u8, u8, u8)); 16] = [
    (Color::Black, (0, 0, 0)),
    (Color::Red, (205, 0, 0)),
    (Color::Green, (0, 205, 0)),
    (Color::Yellow, (205, 205, 0)),
    (Color::Blue, (0, 0, 238)),
    (Color::Magenta, (205, 0, 205)),
    (Color::Cyan, (0, 205, 205)),
    (Color::Gray, (229, 229, 229)),
    (Color::DarkGray, (127, 127, 127)),
    (Color::LightRed, (255, 0, 0)),
    (Color::LightGreen, (0, 255, 0)),
    (Color::LightYellow, (255, 255, 0)),
    (Color::LightBlue, (92, 92, 255)),
    (Color::LightMagenta, (255, 0, 255)),
    (Color::LightCyan, (0, 255, 255)),
    (Color::White, (255, 255, 255)),
];

/// The six channel levels of the xterm 6×6×6 color cube.
const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];

/// Squared distance between two RGB colors.
fn distance2(a: (u8, u8, u8), b: (u8, u8, u8)) -> u32 {
    let d = |x: u8, y: u8| u32::from(x.abs_diff(y)).pow(2);
    d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2)
}

/// The index (0–5) of the cube level nearest `v`. The levels are uneven
/// (a 95 step after 0, then 40s), so this compares against each rather than
/// dividing.
fn cube_index(v: u8) -> u8 {
    let mut best = 0u8;
    for (i, level) in (0u8..).zip(CUBE) {
        if v.abs_diff(level) < v.abs_diff(CUBE[usize::from(best)]) {
            best = i;
        }
    }
    best
}

/// The RGB value of xterm palette entry `i`.
fn indexed_rgb(i: u8) -> (u8, u8, u8) {
    match i {
        0..=15 => ANSI16[usize::from(i)].1,
        16..=231 => {
            let n = i - 16;
            (
                CUBE[usize::from(n / 36)],
                CUBE[usize::from(n / 6 % 6)],
                CUBE[usize::from(n % 6)],
            )
        }
        _ => {
            // The 24-step gray ramp: 8, 18, ..., 238.
            let v = 8 + (i - 232) * 10;
            (v, v, v)
        }
    }
}

/// The nearest xterm 256-palette entry (cube or gray ramp) to `rgb`. The
/// first 16 entries are skipped: terminals let users redefine them.
fn nearest_256(rgb: (u8, u8, u8)) -> u8 {
    let (r, g, b) = (cube_index(rgb.0), cube_index(rgb.1), cube_index(rgb.2));
    let cube = 16 + 36 * r + 6 * g + b;
    // Compare the best cube cell with the best gray-ramp cell and keep the
    // closer: near-grays are matched far better by the 24-step ramp.
    let avg = (u16::from(rgb.0) + u16::from(rgb.1) + u16::from(rgb.2)) / 3;
    // The gray ramp is 8, 18, …, 238 (24 steps).
    let step = u8::try_from((avg.saturating_sub(3) / 10).min(23)).unwrap_or(23);
    let gray = 232 + step;
    if distance2(rgb, indexed_rgb(gray)) < distance2(rgb, indexed_rgb(cube)) {
        gray
    } else {
        cube
    }
}

/// The nearest of the 16 ANSI colors to `rgb`.
fn nearest_16(rgb: (u8, u8, u8)) -> Color {
    ANSI16
        .iter()
        .min_by_key(|(_, c)| distance2(rgb, *c))
        // `ANSI16` is never empty; `Reset` is only a total-function fallback.
        .map_or(Color::Reset, |(name, _)| *name)
}

/// Map `color` into what a terminal of `depth` can show.
///
/// Truecolor returns every color unchanged. At 256 colors an RGB color
/// becomes the nearest xterm palette entry (`Color::Indexed`). At 16 colors
/// an RGB or high indexed color becomes the nearest named ANSI color. Named
/// colors and `Reset` are never changed.
///
/// Matching is nearest-neighbour in plain RGB space (squared Euclidean
/// distance): cheap and good enough for a fallback, though not perceptually
/// uniform.
///
/// # Examples
///
/// ```
/// use ratatui::style::Color;
/// use vix_theme::{downsample, ColorDepth};
/// assert_eq!(downsample(Color::Rgb(255, 0, 0), ColorDepth::Ansi256), Color::Indexed(196));
/// assert_eq!(downsample(Color::Cyan, ColorDepth::Ansi16), Color::Cyan);
/// ```
#[must_use]
pub fn downsample(color: Color, depth: ColorDepth) -> Color {
    match (depth, color) {
        (ColorDepth::Ansi256, Color::Rgb(r, g, b)) => Color::Indexed(nearest_256((r, g, b))),
        (ColorDepth::Ansi16, Color::Rgb(r, g, b)) => nearest_16((r, g, b)),
        (ColorDepth::Ansi16, Color::Indexed(i)) if i < 16 => ANSI16[usize::from(i)].0,
        (ColorDepth::Ansi16, Color::Indexed(i)) => nearest_16(indexed_rgb(i)),
        _ => color,
    }
}

/// [`downsample`] every color of every cell in `buf` (foreground, background
/// and underline). A no-op at truecolor.
pub fn downsample_buffer(buf: &mut Buffer, depth: ColorDepth) {
    if depth == ColorDepth::TrueColor {
        return;
    }
    // Whole-frame pass after drawing, so widgets stay colour-depth agnostic.
    for cell in &mut buf.content {
        cell.fg = downsample(cell.fg, depth);
        cell.bg = downsample(cell.bg, depth);
        cell.underline_color = downsample(cell.underline_color, depth);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    fn env<'a>(
        colorterm: Option<&'a str>,
        term: Option<&'a str>,
        term_program: Option<&'a str>,
    ) -> TermEnv<'a> {
        TermEnv {
            colorterm,
            term,
            term_program,
        }
    }

    #[test]
    fn colorterm_truecolor_wins() {
        let e = env(Some("truecolor"), Some("xterm"), Some("Apple_Terminal"));
        assert_eq!(detect(&e), ColorDepth::TrueColor);
        let e = env(Some("24bit"), Some("screen"), None);
        assert_eq!(detect(&e), ColorDepth::TrueColor);
    }

    #[test]
    fn apple_terminal_is_256() {
        let e = env(None, Some("xterm-256color"), Some("Apple_Terminal"));
        assert_eq!(detect(&e), ColorDepth::Ansi256);
    }

    #[test]
    fn term_decides_without_colorterm() {
        assert_eq!(
            detect(&env(None, Some("xterm-256color"), None)),
            ColorDepth::Ansi256
        );
        assert_eq!(
            detect(&env(None, Some("xterm-kitty"), None)),
            ColorDepth::TrueColor
        );
        assert_eq!(
            detect(&env(None, Some("xterm-direct"), None)),
            ColorDepth::TrueColor
        );
        assert_eq!(detect(&env(None, Some("linux"), None)), ColorDepth::Ansi16);
        assert_eq!(detect(&env(None, Some("vt100"), None)), ColorDepth::Ansi16);
        assert_eq!(detect(&env(None, Some("dumb"), None)), ColorDepth::Ansi16);
        assert_eq!(detect(&env(None, Some("xterm"), None)), ColorDepth::Ansi256);
    }

    #[test]
    fn known_programs_and_missing_term() {
        assert_eq!(
            detect(&env(None, Some("xterm-256color"), Some("iTerm.app"))),
            ColorDepth::TrueColor
        );
        assert_eq!(detect(&TermEnv::default()), ColorDepth::TrueColor);
        assert_eq!(
            detect(&env(Some(""), Some(""), None)),
            ColorDepth::TrueColor
        );
    }

    #[test]
    fn truecolor_passes_through() {
        let c = Color::Rgb(12, 34, 56);
        assert_eq!(downsample(c, ColorDepth::TrueColor), c);
    }

    #[test]
    fn rgb_to_256_hits_cube_and_gray() {
        assert_eq!(
            downsample(Color::Rgb(255, 0, 0), ColorDepth::Ansi256),
            Color::Indexed(196)
        );
        assert_eq!(
            downsample(Color::Rgb(0, 0, 0), ColorDepth::Ansi256),
            Color::Indexed(16)
        );
        assert_eq!(
            downsample(Color::Rgb(255, 255, 255), ColorDepth::Ansi256),
            Color::Indexed(231)
        );
        // A mid gray lands on the gray ramp, not the coarser cube.
        assert_eq!(
            downsample(Color::Rgb(128, 128, 128), ColorDepth::Ansi256),
            Color::Indexed(244)
        );
        assert_eq!(
            downsample(Color::Rgb(40, 40, 40), ColorDepth::Ansi256),
            Color::Indexed(235)
        );
    }

    #[test]
    fn rgb_and_indexed_to_16() {
        assert_eq!(
            downsample(Color::Rgb(250, 10, 10), ColorDepth::Ansi16),
            Color::LightRed
        );
        assert_eq!(
            downsample(Color::Rgb(20, 20, 20), ColorDepth::Ansi16),
            Color::Black
        );
        assert_eq!(
            downsample(Color::Rgb(215, 215, 215), ColorDepth::Ansi16),
            Color::Gray
        );
        assert_eq!(
            downsample(Color::Indexed(4), ColorDepth::Ansi16),
            Color::Blue
        );
        assert_eq!(
            downsample(Color::Indexed(196), ColorDepth::Ansi16),
            Color::LightRed
        );
    }

    #[test]
    fn named_and_reset_never_change() {
        for depth in [ColorDepth::Ansi256, ColorDepth::Ansi16] {
            assert_eq!(downsample(Color::Reset, depth), Color::Reset);
            assert_eq!(downsample(Color::Cyan, depth), Color::Cyan);
        }
        assert_eq!(
            downsample(Color::Indexed(100), ColorDepth::Ansi256),
            Color::Indexed(100)
        );
    }

    #[test]
    fn buffer_pass_maps_every_cell() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 2, 1));
        buf.content[0].fg = Color::Rgb(255, 0, 0);
        buf.content[1].bg = Color::Rgb(0, 0, 0);
        downsample_buffer(&mut buf, ColorDepth::Ansi256);
        assert_eq!(buf.content[0].fg, Color::Indexed(196));
        assert_eq!(buf.content[1].bg, Color::Indexed(16));
        assert_eq!(buf.content[0].bg, Color::Reset);
    }

    #[test]
    fn names_are_stable() {
        assert_eq!(ColorDepth::TrueColor.name(), "truecolor");
        assert_eq!(ColorDepth::Ansi256.name(), "256");
        assert_eq!(ColorDepth::Ansi16.name(), "16");
    }
}
