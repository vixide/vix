//! The **Help → What's New** text (T826): the release notes bundled from
//! `CHANGELOG.md` at build time, cut to the sections that matter for the
//! running version and turned into the plain paragraphs [`crate::Panel`]
//! shows.
//!
//! The host's build script calls [`excerpt`] on the changelog and embeds
//! the result; at run time [`paragraphs`] renders it. Both are pure so the
//! rules are tested here, not in the host.

/// The part of a Keep a Changelog `changelog` worth showing for `version`:
/// every `## ` section from the first one (normally `## [Unreleased]`)
/// through the section headed `## [version]`. When no section names
/// `version`, only the first section. Text before the first `## ` heading
/// (the title and preamble) is left out; an empty string when there is no
/// `## ` heading at all.
///
/// # Examples
///
/// ```
/// use vix_welcome_panel::whats_new::excerpt;
///
/// let log = "# Changelog\n\n## [Unreleased]\n\n- New\n\n## [1.0.0] - 2026-01-01\n\n- One\n\n## [0.9.0]\n\n- Old\n";
/// assert_eq!(excerpt(log, "1.0.0"), "## [Unreleased]\n\n- New\n\n## [1.0.0] - 2026-01-01\n\n- One\n\n");
/// assert_eq!(excerpt(log, "2.0.0"), "## [Unreleased]\n\n- New\n\n");
/// ```
#[must_use]
pub fn excerpt(changelog: &str, version: &str) -> String {
    let wanted = format!("## [{version}]");
    let has_version = changelog.lines().any(|l| l.starts_with(&wanted));
    let mut out = String::new();
    let mut started = false;
    let mut in_version = false;
    for line in changelog.lines() {
        if line.starts_with("## ") {
            if in_version || (started && !has_version) {
                break;
            }
            started = true;
            in_version = line.starts_with(&wanted);
        } else if !started {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Render an [`excerpt`] as display paragraphs: a `## [x] - date` heading
/// becomes `x (date)` (`## [Unreleased]` becomes `unreleased_title`)
/// followed by a rule of `─`, a `### Kind` heading becomes `Kind`, a
/// `- ` bullet becomes `• `, an indented line continues the paragraph
/// before it, runs of blank lines collapse to one, `**` is dropped and a
/// Markdown link `[text](url)` keeps only its text.
///
/// # Examples
///
/// ```
/// use vix_welcome_panel::whats_new::paragraphs;
///
/// let text = "## [1.0.0] - 2026-01-01\n\n### Added\n\n- **Bold** [link](https://x)\n  more\n";
/// assert_eq!(
///     paragraphs(text, "Unreleased"),
///     ["1.0.0 (2026-01-01)", "──────────────────", "", "Added", "", "• Bold link more"],
/// );
/// ```
#[must_use]
pub fn paragraphs(excerpt: &str, unreleased_title: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in excerpt.lines() {
        if line.trim().is_empty() {
            if out.last().is_some_and(|l| !l.is_empty()) {
                out.push(String::new());
            }
            continue;
        }
        if let Some(heading) = line.strip_prefix("## ") {
            let title = release_title(heading, unreleased_title);
            let rule = "─".repeat(title.chars().count());
            out.push(title);
            out.push(rule);
            continue;
        }
        let text = plain(line.trim());
        if let Some(kind) = text.strip_prefix("### ") {
            out.push(kind.to_string());
        } else if let Some(item) = text.strip_prefix("- ") {
            out.push(format!("• {item}"));
        } else if line.starts_with(char::is_whitespace)
            && let Some(last) = out.last_mut().filter(|l| !l.is_empty())
        {
            last.push(' ');
            last.push_str(&text);
        } else {
            out.push(text);
        }
    }
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out
}

/// `[x] - date` as `x (date)`, `[Unreleased]` as `unreleased_title`.
fn release_title(heading: &str, unreleased_title: &str) -> String {
    let heading = heading.trim();
    let (name, date) = match heading.split_once(" - ") {
        Some((name, date)) => (name.trim(), Some(date.trim())),
        None => (heading, None),
    };
    let name = name.trim_start_matches('[').trim_end_matches(']');
    let name = if name.eq_ignore_ascii_case("unreleased") {
        unreleased_title
    } else {
        name
    };
    match date {
        Some(date) => format!("{name} ({date})"),
        None => name.to_string(),
    }
}

/// `line` without `**` and with each Markdown link `[text](url)` cut to
/// its text; a bracket that does not start a link is kept.
fn plain(line: &str) -> String {
    let line = line.replace("**", "");
    let mut out = String::new();
    let mut rest = line.as_str();
    while let Some((before, after)) = rest.split_once('[') {
        out.push_str(before);
        let found = after
            .split_once("](")
            .filter(|(text, _)| !text.contains('['))
            .and_then(|(text, tail)| tail.split_once(')').map(|(_, tail)| (text, tail)));
        if let Some((text, tail)) = found {
            out.push_str(text);
            rest = tail;
        } else {
            out.push('[');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_excerpt_stops_after_the_running_version() {
        let log =
            "# Changelog\npreamble\n## [Unreleased]\n- a\n## [2.0.0] - d\n- b\n## [1.0.0]\n- c\n";
        assert_eq!(
            excerpt(log, "2.0.0"),
            "## [Unreleased]\n- a\n## [2.0.0] - d\n- b\n"
        );
        assert_eq!(
            excerpt(log, "1.0.0"),
            "## [Unreleased]\n- a\n## [2.0.0] - d\n- b\n## [1.0.0]\n- c\n"
        );
        // "2.0" must not match "2.0.0".
        assert_eq!(excerpt(log, "2.0"), "## [Unreleased]\n- a\n");
        assert_eq!(excerpt("no headings", "1.0.0"), "");
    }

    #[test]
    fn paragraphs_render_headings_bullets_and_links() {
        let text =
            "## [Unreleased]\n\n\n- see [`docs`](docs/x.md) and [x] here\n\n## [1.0.0]\n- one\n\n";
        assert_eq!(
            paragraphs(text, "Coming next"),
            [
                "Coming next",
                "───────────",
                "",
                "• see `docs` and [x] here",
                "",
                "1.0.0",
                "─────",
                "• one",
            ]
        );
        assert_eq!(paragraphs("", "x"), Vec::<String>::new());
    }

    #[test]
    fn a_continuation_line_never_joins_a_blank_paragraph() {
        assert_eq!(paragraphs("\n  indented", "x"), ["indented"]);
    }
}
