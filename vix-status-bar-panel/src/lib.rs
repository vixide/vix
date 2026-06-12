//! Formatting for the bottom status bar's two segments.
//!
//! Pure string logic — the host (the `vix` app) gathers the live data (cursor,
//! path, dirty flag, keyway mode, language, line ending, selection) and the Nerd
//! Font glyphs, calls these builders, and renders the resulting strings.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

/// The left segment: `" {mode}{path}{dirty}  —  {status}"`.
///
/// `mode` is the keyway mode indicator (with trailing spacing) or empty; `dirty`
/// is the unsaved-buffer glyph (with leading space) or empty.
#[must_use]
pub fn left_segment(mode: &str, path: &str, dirty: &str, status: &str) -> String {
    format!(" {mode}{path}{dirty}  \u{2014}  {status}")
}

/// The editor-info prefix for the right segment:
/// `"{language}  {line_ending}  UTF-8   {selection}"`.
///
/// `language` is `None` for a non-text tab (e.g. an image), giving an empty
/// string. `selection` is `(chars, lines)` when text is selected, rendered as
/// `"Sel {chars} ({lines}L)   "`.
#[must_use]
pub fn info_segment(
    language: Option<&str>,
    line_ending: &str,
    selection: Option<(usize, usize)>,
) -> String {
    let Some(language) = language else {
        return String::new();
    };
    let sel = selection.map_or(String::new(), |(chars, lines)| {
        format!("Sel {chars} ({lines}L)   ")
    });
    format!("{language}  {line_ending}  UTF-8   {sel}")
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
    fn git_segment_shows_branch_and_dirty_dot() {
        assert_eq!(git_segment(Some("main"), "\u{e0a0}", false), "\u{e0a0} main   ");
        assert_eq!(git_segment(Some("main"), "\u{e0a0}", true), "\u{e0a0} main \u{2022}   ");
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
        assert_eq!(info_segment(None, "LF", None), "");
    }

    #[test]
    fn info_includes_selection_when_present() {
        assert_eq!(
            info_segment(Some("rust"), "LF", Some((12, 3))),
            "rust  LF  UTF-8   Sel 12 (3L)   "
        );
        assert_eq!(info_segment(Some("rust"), "CRLF", None), "rust  CRLF  UTF-8   ");
    }

    #[test]
    fn right_appends_position_and_glyph() {
        assert_eq!(right_segment("rust  LF  UTF-8   ", 2, 5, "C"), "rust  LF  UTF-8   Ln 2:Col 5   C ");
    }
}
