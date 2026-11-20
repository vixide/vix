//! Formatting for the bottom status bar's two segments.
//!
//! Pure string logic — the host (the `vix` app) gathers the live data (cursor,
//! path, dirty flag, keymap mode, language, line ending, selection) and the Nerd
//! Font glyphs, calls these builders, and renders the resulting strings.

/// The left segment: `" {mode}{path}{dirty}  —  {status}"`.
///
/// `mode` is the keymap mode indicator (with trailing spacing) or empty; `dirty`
/// is the unsaved-buffer glyph (with leading space) or empty.
#[must_use]
pub fn left_segment(mode: &str, path: &str, dirty: &str, status: &str) -> String {
    format!(" {mode}{path}{dirty}  \u{2014}  {status}")
}

/// [`left_segment`] fitted into `width` columns (counted in chars, as the
/// host lays the bar out) so the status message stays readable: the full
/// segment if it fits; else with `path` cut to its file name; else without
/// the path; and if even that is too long, cut with `…`. An empty `status`
/// never drops the path, only cuts it.
#[must_use]
pub fn fit_left_segment(mode: &str, path: &str, dirty: &str, status: &str, width: usize) -> String {
    let fits = |s: &String| s.chars().count() <= width;
    let full = left_segment(mode, path, dirty, status);
    if fits(&full) {
        return full;
    }
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let short = left_segment(mode, name, dirty, status);
    if fits(&short) || status.is_empty() {
        return cut(&short, width);
    }
    cut(&format!(" {mode}{status}"), width)
}

/// `s` cut to `width` chars, ending in `…` when anything was cut.
fn cut(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        return s.to_string();
    }
    let mut out: String = s.chars().take(width.saturating_sub(1)).collect();
    if width > 0 {
        out.push('\u{2026}');
    }
    out
}

/// The editor-info prefix for the right segment:
/// `"{language}  {line_ending}  {encoding}   {selection}"`, where `encoding`
/// is the tab's file encoding, such as `UTF-8` or `Shift_JIS` (T661).
///
/// `language` is `None` for a non-text tab (e.g. an image), giving an empty
/// string. `selection` is `(chars, lines)` when text is selected, rendered as
/// `"Sel {chars} ({lines}L)   "`.
#[must_use]
pub fn info_segment(
    language: Option<&str>,
    line_ending: &str,
    encoding: &str,
    selection: Option<(usize, usize)>,
) -> String {
    let Some(language) = language else {
        return String::new();
    };
    let sel = selection.map_or(String::new(), |(chars, lines)| {
        format!("Sel {chars} ({lines}L)   ")
    });
    format!("{language}  {line_ending}  {encoding}   {sel}")
}

/// The right segment: `"{info}Ln {line}:Col {col}   {calendar} "`, where `info`
/// is [`info_segment`]'s output and `calendar` is the calendar glyph.
#[must_use]
pub fn right_segment(info: &str, line: usize, col: usize, calendar: &str) -> String {
    format!("{info}Ln {line}:Col {col}   {calendar} ")
}

/// The git indicator for the right segment: `"{glyph} {branch}{dirty}   "`, where
/// `glyph` is the branch icon and `dirty` is a bullet (`•`) when the working tree
/// has changes. Empty when `branch` is `None` (not a repo / detached with no
/// name), so it cleanly disappears.
#[must_use]
pub fn git_segment(branch: Option<&str>, glyph: &str, dirty: bool) -> String {
    match branch {
        Some(b) => {
            let dot = if dirty { " \u{2022}" } else { "" };
            format!("{glyph} {b}{dot}   ")
        }
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_narrow_bar_keeps_the_status_message_readable() {
        let full = left_segment("", "src/app/main.rs", "", "Saved");
        assert_eq!(
            fit_left_segment("", "src/app/main.rs", "", "Saved", 80),
            full
        );
        // The path shrinks to its file name first…
        assert_eq!(
            fit_left_segment("", "src/app/main.rs", "", "Saved", 20),
            " main.rs  \u{2014}  Saved"
        );
        // …then goes, and the message is cut only when nothing else is left.
        assert_eq!(
            fit_left_segment("NORMAL ", "src/main.rs", "", "Saved", 16),
            " NORMAL Saved"
        );
        assert_eq!(
            fit_left_segment("", "a.rs", "", "a long status message", 10),
            " a long s\u{2026}"
        );
        // No message: the path is only ever cut.
        assert_eq!(
            fit_left_segment("", "src/app/main.rs", "", "", 9),
            " main.rs\u{2026}"
        );
        assert_eq!(fit_left_segment("", "x", "", "y", 0), "");
    }

    #[test]
    fn git_segment_shows_branch_and_dirty_dot() {
        assert_eq!(
            git_segment(Some("main"), "\u{e0a0}", false),
            "\u{e0a0} main   "
        );
        assert_eq!(
            git_segment(Some("main"), "\u{e0a0}", true),
            "\u{e0a0} main \u{2022}   "
        );
        assert_eq!(git_segment(None, "\u{e0a0}", true), "");
    }

    #[test]
    fn left_joins_pieces_with_an_em_dash() {
        assert_eq!(
            left_segment("", "src/main.rs", " *", "Saved"),
            " src/main.rs *  \u{2014}  Saved"
        );
    }

    #[test]
    fn info_is_empty_for_a_non_text_tab() {
        assert_eq!(info_segment(None, "LF", "UTF-8", None), "");
    }

    #[test]
    fn info_includes_selection_when_present() {
        assert_eq!(
            info_segment(Some("rust"), "LF", "UTF-8", Some((12, 3))),
            "rust  LF  UTF-8   Sel 12 (3L)   "
        );
        assert_eq!(
            info_segment(Some("rust"), "CRLF", "Shift_JIS", None),
            "rust  CRLF  Shift_JIS   "
        );
    }

    #[test]
    fn right_appends_position_and_glyph() {
        assert_eq!(
            right_segment("rust  LF  UTF-8   ", 2, 5, "C"),
            "rust  LF  UTF-8   Ln 2:Col 5   C "
        );
    }
}
