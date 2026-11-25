//! Vim modelines and Emacs file variables: per-file settings written in the
//! file itself.
//!
//! [`parse`] reads a file's first and last lines and returns the handful of
//! settings Vix applies from them ([`Modeline`]). Only harmless, bounded
//! options are recognized (indentation, tab width, wrapping, the final
//! newline); everything else, including any option that could run code or
//! change what Vix does beyond one buffer's layout, is ignored. Spec:
//! `crates/vix-editorconfig/spec/index.md`.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

/// Largest tab width or indent size a modeline may set.
const MAX_WIDTH: usize = 16;
/// Largest wrap column a modeline may set.
const MAX_COLUMN: usize = 1000;
/// How many trailing lines the host should hand [`parse`] as `tail`, so an
/// Emacs `Local Variables:` block fits (Emacs itself looks at the last 3,000
/// characters).
pub const TAIL_LINES: usize = 64;

/// `a - b`, or the declared `fallback` when it would go below zero. The
/// fallback is a parameter so each call names the declaration it comes from;
/// written inline, `checked_sub(..).unwrap_or(0)` is rewritten by Clippy to
/// `saturating_sub`, which the checked-arithmetic spec forbids.
fn sub_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_sub(b).unwrap_or(fallback)
}

/// The settings a file's modelines ask for. `None` means "not set here".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Modeline {
    /// Indent with a tab (`true`) or spaces (`false`).
    pub indent_is_tab: Option<bool>,
    /// Columns per indent level.
    pub indent_size: Option<usize>,
    /// Columns a tab character spans.
    pub tab_width: Option<usize>,
    /// Hard-wrap (fill) column.
    pub wrap_column: Option<usize>,
    /// Soft-wrap long lines on screen.
    pub soft_wrap: Option<bool>,
    /// End the file with a newline on save.
    pub insert_final_newline: Option<bool>,
}

impl Modeline {
    /// Whether no setting was found.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Modeline::default()
    }

    /// The string the Tab key inserts, if the modelines say: a tab, or the
    /// indent size (else the tab width, else 4) in spaces. A size with no
    /// style means spaces.
    #[must_use]
    pub fn indent_string(&self) -> Option<String> {
        match self.indent_is_tab {
            Some(true) => Some("\t".to_string()),
            Some(false) => Some(" ".repeat(self.indent_size.or(self.tab_width).unwrap_or(4))),
            None => self.indent_size.map(|n| " ".repeat(n)),
        }
    }

    /// `self`, with each unset field taken from `other`.
    fn or(self, other: Modeline) -> Modeline {
        Modeline {
            indent_is_tab: self.indent_is_tab.or(other.indent_is_tab),
            indent_size: self.indent_size.or(other.indent_size),
            tab_width: self.tab_width.or(other.tab_width),
            wrap_column: self.wrap_column.or(other.wrap_column),
            soft_wrap: self.soft_wrap.or(other.soft_wrap),
            insert_final_newline: self.insert_final_newline.or(other.insert_final_newline),
        }
    }
}

/// The modelines in a file, given its first lines (`head`) and its last
/// [`TAIL_LINES`] lines (`tail`; the two may overlap for a short file).
///
/// Vim modelines are looked for in the first and last `lines` lines, an Emacs
/// `-*- … -*-` line on the first line (the second after a `#!` line), and an
/// Emacs `Local Variables:` block in `tail`. Emacs settings win over Vim
/// ones. Vim modelines apply in file order, as in Vim, so a later one wins;
/// the `-*-` line wins over a `Local Variables:` block. `lines == 0` turns
/// modelines off.
#[must_use]
pub fn parse(head: &str, tail: &str, lines: usize) -> Modeline {
    if lines == 0 {
        return Modeline::default();
    }
    let head_lines: Vec<&str> = head.lines().take(lines).collect();
    let tail_all: Vec<&str> = tail.lines().collect();
    // The last `lines` lines; a shorter file is all of it.
    let tail_lines = tail_all.iter().skip(sub_or(tail_all.len(), lines, 0));

    let vim_top = head_lines
        .iter()
        .fold(Modeline::default(), |acc, l| vim_line(l).or(acc));
    let vim_bottom = tail_lines.fold(Modeline::default(), |acc, l| vim_line(l).or(acc));
    let emacs_top = emacs_first_line(head);
    let emacs_bottom = emacs_local_variables(&tail_all);

    // Vim applies modelines in file order, so a later one wins.
    emacs_top.or(emacs_bottom).or(vim_bottom).or(vim_top)
}

/// A width in `1..=max`, from text.
fn bounded(value: &str, max: usize) -> Option<usize> {
    value
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|n| (1..=max).contains(n))
}

/// The settings in one line's Vim modeline, if it has one: `vim:`, `vi:` or
/// `ex:` at the start of the line or after a blank, then either
/// `set opt opt …:` or options separated by blanks or colons.
fn vim_line(line: &str) -> Modeline {
    let Some(rest) = vim_options(line) else {
        return Modeline::default();
    };
    let rest = rest.trim_start();
    // The `set` form ends at the first `:`; the other ends with the line.
    let opts: Vec<&str> = match rest
        .strip_prefix("set ")
        .or_else(|| rest.strip_prefix("se "))
    {
        Some(set) => set
            .split(':')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .collect(),
        None => rest
            .split(|c: char| c == ':' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .collect(),
    };
    let mut m = Modeline::default();
    for opt in opts {
        let (name, value) = opt.split_once('=').unwrap_or((opt, ""));
        match name {
            "ts" | "tabstop" => m.tab_width = bounded(value, MAX_WIDTH).or(m.tab_width),
            "sw" | "shiftwidth" => m.indent_size = bounded(value, MAX_WIDTH).or(m.indent_size),
            "tw" | "textwidth" => m.wrap_column = bounded(value, MAX_COLUMN).or(m.wrap_column),
            "et" | "expandtab" => m.indent_is_tab = Some(false),
            "noet" | "noexpandtab" => m.indent_is_tab = Some(true),
            "wrap" => m.soft_wrap = Some(true),
            "nowrap" => m.soft_wrap = Some(false),
            "eol" | "endofline" | "fixeol" | "fixendofline" => {
                m.insert_final_newline = Some(true);
            }
            "noeol" | "noendofline" | "nofixeol" | "nofixendofline" => {
                m.insert_final_newline = Some(false);
            }
            _ => {}
        }
    }
    m
}

/// The text after a line's `vim:`/`vi:`/`ex:` marker, when the marker starts
/// the line or follows a blank.
fn vim_options(line: &str) -> Option<&str> {
    ["vim:", "Vim:", "vi:", "ex:"].iter().find_map(|marker| {
        line.match_indices(marker).find_map(|(at, _)| {
            let (before, rest) = line.split_at_checked(at)?;
            if before.is_empty() || before.ends_with(char::is_whitespace) {
                rest.strip_prefix(marker)
            } else {
                None
            }
        })
    })
}

/// The settings in an Emacs `-*- var: value; … -*-` line: the first line, or
/// the second when the first is a `#!` line. A bare `-*- mode -*-` sets
/// nothing Vix applies.
fn emacs_first_line(head: &str) -> Modeline {
    let mut lines = head.lines();
    let first = lines.next().unwrap_or("");
    let line = if first.starts_with("#!") {
        lines.next().unwrap_or("")
    } else {
        first
    };
    let Some((_, after)) = line.split_once("-*-") else {
        return Modeline::default();
    };
    let Some((vars, _)) = after.split_once("-*-") else {
        return Modeline::default();
    };
    vars.split(';')
        .filter_map(|pair| pair.split_once(':'))
        .fold(Modeline::default(), |acc, (k, v)| {
            emacs_var(k.trim(), v.trim()).or(acc)
        })
}

/// The settings in an Emacs `Local Variables:` … `End:` block. Each line in
/// it carries the same prefix and suffix as the `Local Variables:` line (a
/// comment marker, usually), which are stripped.
fn emacs_local_variables(tail: &[&str]) -> Modeline {
    let Some(start) = tail.iter().rposition(|l| l.contains("Local Variables:")) else {
        return Modeline::default();
    };
    let mut rest = tail.iter().skip(start);
    let Some((prefix, after)) = rest
        .next()
        .and_then(|opener| opener.split_once("Local Variables:"))
    else {
        return Modeline::default();
    };
    let suffix = after.trim();
    let mut m = Modeline::default();
    for line in rest {
        let body = line.strip_prefix(prefix).unwrap_or(line);
        let body = body.trim_end();
        let body = body.strip_suffix(suffix).unwrap_or(body).trim();
        if body == "End:" {
            break;
        }
        if let Some((k, v)) = body.split_once(':') {
            // A later line wins, as in Emacs.
            m = emacs_var(k.trim(), v.trim()).or(m);
        }
    }
    m
}

/// One Emacs file variable, if Vix applies it.
fn emacs_var(name: &str, value: &str) -> Modeline {
    let flag = match value {
        "t" => Some(true),
        "nil" => Some(false),
        _ => None,
    };
    let mut m = Modeline::default();
    match name {
        "tab-width" => m.tab_width = bounded(value, MAX_WIDTH),
        "indent-tabs-mode" => m.indent_is_tab = flag,
        "fill-column" => m.wrap_column = bounded(value, MAX_COLUMN),
        "truncate-lines" => m.soft_wrap = flag.map(|truncate| !truncate),
        "require-final-newline" => m.insert_final_newline = flag,
        // The per-mode indent offsets: `c-basic-offset`,
        // `python-indent-offset`, `js-indent-level`, `sh-basic-offset`, ….
        _ if name.ends_with("-basic-offset")
            || name.ends_with("-indent-offset")
            || name.ends_with("-indent-level") =>
        {
            m.indent_size = bounded(value, MAX_WIDTH);
        }
        _ => {}
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(text: &str) -> Modeline {
        parse(text, text, 5)
    }

    #[test]
    fn vim_set_form() {
        let m = one("# vim: set ts=4 sw=2 et tw=72 :\nbody\n");
        assert_eq!(m.tab_width, Some(4));
        assert_eq!(m.indent_size, Some(2));
        assert_eq!(m.indent_is_tab, Some(false));
        assert_eq!(m.wrap_column, Some(72));
        assert_eq!(m.indent_string().as_deref(), Some("  "));
    }

    #[test]
    fn vim_set_form_stops_at_the_colon() {
        let m = one("/* vim: set noet: ts=8 */\n");
        assert_eq!(m.indent_is_tab, Some(true));
        assert_eq!(m.tab_width, None);
    }

    #[test]
    fn vim_plain_form_splits_on_blanks_and_colons() {
        let m = one("// vi:noet:ts=8 nowrap noeol\n");
        assert_eq!(m.indent_is_tab, Some(true));
        assert_eq!(m.tab_width, Some(8));
        assert_eq!(m.soft_wrap, Some(false));
        assert_eq!(m.insert_final_newline, Some(false));
        assert_eq!(m.indent_string().as_deref(), Some("\t"));
        let m = one("ex: wrap eol\n");
        assert_eq!(m.soft_wrap, Some(true));
        assert_eq!(m.insert_final_newline, Some(true));
    }

    #[test]
    fn vim_marker_needs_a_blank_or_the_line_start() {
        assert!(one("let kvim: set ts=2:\n").is_empty());
        assert!(one("nothing here\n").is_empty());
        assert_eq!(one("Vim: set ts=3:\n").tab_width, Some(3));
    }

    #[test]
    fn long_names_and_fix_eol() {
        let m = one("# vim: set tabstop=6 shiftwidth=3 expandtab textwidth=90 fixendofline:\n");
        assert_eq!(m.tab_width, Some(6));
        assert_eq!(m.indent_size, Some(3));
        assert_eq!(m.indent_is_tab, Some(false));
        assert_eq!(m.wrap_column, Some(90));
        assert_eq!(m.insert_final_newline, Some(true));
        let m = one("# vim: set noexpandtab noendofline nofixeol:\n");
        assert_eq!(m.indent_is_tab, Some(true));
        assert_eq!(m.insert_final_newline, Some(false));
    }

    #[test]
    fn unknown_and_out_of_range_options_are_ignored() {
        let m = one("# vim: set ts=0 sw=99 tw=0 modeline foldmethod=marker:\n");
        assert!(m.is_empty(), "{m:?}");
        assert!(one("# vim: set ts=x:\n").is_empty());
    }

    #[test]
    fn only_the_first_and_last_lines_are_read() {
        let mut text = String::from("a\n");
        for _ in 0..20 {
            text.push_str("filler\n");
        }
        let middle = format!("{text}# vim: set ts=2:\n{text}");
        assert!(one(&middle).is_empty());
        assert_eq!(one(&format!("{text}# vim: set ts=2:\n")).tab_width, Some(2));
        assert_eq!(parse("# vim: set ts=2:\n", "", 1).tab_width, Some(2));
        assert!(parse("x\n# vim: set ts=2:\n", "", 1).is_empty());
    }

    #[test]
    fn zero_lines_turns_modelines_off() {
        assert!(parse("# vim: set ts=2:\n", "# vim: set ts=2:\n", 0).is_empty());
    }

    #[test]
    fn a_later_vim_modeline_wins_as_in_vim() {
        let mut text = String::from("# vim: set ts=2 tw=60:\n");
        for _ in 0..20 {
            text.push_str("filler\n");
        }
        text.push_str("# vim: set ts=8 sw=4:\n");
        let m = one(&text);
        assert_eq!(m.tab_width, Some(8));
        assert_eq!(m.indent_size, Some(4));
        assert_eq!(m.wrap_column, Some(60));
    }

    #[test]
    fn the_emacs_first_line_wins_over_local_variables() {
        let head = "-*- tab-width: 2 -*-\n";
        let tail = "Local Variables:\ntab-width: 6\nfill-column: 50\nEnd:\n";
        let m = parse(head, tail, 5);
        assert_eq!(m.tab_width, Some(2));
        assert_eq!(m.wrap_column, Some(50));
    }

    #[test]
    fn a_later_vim_line_in_the_head_wins_over_an_earlier_one() {
        let m = one("# vim: set ts=2:\n# vim: set ts=6:\n");
        assert_eq!(m.tab_width, Some(6));
    }

    #[test]
    fn emacs_first_line() {
        let m =
            one("# -*- mode: python; tab-width: 4; indent-tabs-mode: nil; fill-column: 79 -*-\n");
        assert_eq!(m.tab_width, Some(4));
        assert_eq!(m.indent_is_tab, Some(false));
        assert_eq!(m.wrap_column, Some(79));
        assert_eq!(m.indent_string().as_deref(), Some("    "));
    }

    #[test]
    fn emacs_line_after_a_shebang() {
        let m = one("#!/usr/bin/env python\n# -*- indent-tabs-mode: t -*-\n");
        assert_eq!(m.indent_is_tab, Some(true));
        assert!(
            one("x\n# -*- indent-tabs-mode: t -*-\n")
                .indent_is_tab
                .is_none()
        );
    }

    #[test]
    fn emacs_bare_mode_line_sets_nothing() {
        assert!(one("/* -*- c -*- */\n").is_empty());
        assert!(one("# -*- tab-width: 4\n").is_empty());
    }

    #[test]
    fn emacs_flags_and_mode_offsets() {
        let m = one("-*- truncate-lines: t; require-final-newline: nil; c-basic-offset: 8 -*-\n");
        assert_eq!(m.soft_wrap, Some(false));
        assert_eq!(m.insert_final_newline, Some(false));
        assert_eq!(m.indent_size, Some(8));
        let m = one("-*- truncate-lines: nil; python-indent-offset: 2; js-indent-level: 3 -*-\n");
        assert_eq!(m.soft_wrap, Some(true));
        assert_eq!(m.indent_size, Some(3));
        assert_eq!(one("-*- indent-tabs-mode: maybe -*-\n").indent_is_tab, None);
    }

    #[test]
    fn emacs_local_variables_block() {
        let tail = "code\n;; Local Variables:\n;; tab-width: 3\n;; fill-column: 70\n;; End:\n";
        let m = parse("code\n", tail, 5);
        assert_eq!(m.tab_width, Some(3));
        assert_eq!(m.wrap_column, Some(70));
    }

    #[test]
    fn local_variables_strip_prefix_and_suffix_and_stop_at_end() {
        let tail =
            "/* Local Variables: */\n/* indent-tabs-mode: t */\n/* End: */\n/* tab-width: 9 */\n";
        let m = parse("", tail, 5);
        assert_eq!(m.indent_is_tab, Some(true));
        assert_eq!(m.tab_width, None);
    }

    #[test]
    fn emacs_wins_over_vim() {
        let text = "# -*- tab-width: 4 -*-\n# vim: set ts=8 sw=2:\n";
        let m = one(text);
        assert_eq!(m.tab_width, Some(4));
        assert_eq!(m.indent_size, Some(2));
    }

    #[test]
    fn indent_string_without_a_style() {
        assert_eq!(one("# vim: set ts=4:\n").indent_string(), None);
        assert_eq!(
            one("# vim: set sw=3:\n").indent_string().as_deref(),
            Some("   ")
        );
        let tabs = Modeline {
            indent_is_tab: Some(false),
            ..Modeline::default()
        };
        assert_eq!(tabs.indent_string().as_deref(), Some("    "));
    }
}
