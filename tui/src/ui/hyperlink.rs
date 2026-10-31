//! OSC 8 hyperlinks (T628): make the URLs shown in the markdown preview and
//! the messages dock clickable in terminals that support it.
//!
//! Panes that show URLs record their area and the full URLs that may appear
//! there in `Layout::hyperlink_regions` while drawing. At the end of the
//! frame [`apply`] finds each visible URL in those areas and wraps every one
//! of its cells in `ESC ] 8 ; ; URL ESC \ … ESC ] 8 ; ; ESC \`, marking the
//! cell's width as 1 so ratatui's diff doesn't count the escape as text.
//! Only a URL from the recorded list is ever linked, and only one made of
//! printable ASCII, so text in a message can't smuggle an escape sequence.

use std::num::NonZeroU16;
use std::ops::Range;

use ratatui::Frame;
use ratatui::buffer::{Buffer, CellDiffOption};
use ratatui::layout::Rect;

use crate::app::App;

/// The URL schemes that are linked.
const SCHEMES: &[&str] = &["https://", "http://", "file://"];

/// Whether `c` can be part of a linked URL: printable ASCII other than space,
/// quotes, angle brackets, and the few characters RFC 3986 never allows.
fn is_url_char(c: char) -> bool {
    c.is_ascii_graphic()
        && !matches!(
            c,
            '"' | '\'' | '<' | '>' | '`' | '{' | '}' | '|' | '\\' | '^'
        )
}

/// The byte ranges of the `http://`, `https://` and `file://` URLs in
/// `text`. Trailing sentence punctuation and an unbalanced closing `)` or
/// `]` (a URL written in parentheses) are left out.
pub(super) fn find_urls(text: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(start) = SCHEMES
        .iter()
        .filter_map(|s| text[from..].find(s).map(|i| from + i))
        .min()
    {
        let len = text[start..]
            .find(|c: char| !is_url_char(c))
            .unwrap_or(text.len() - start);
        let mut end = start + len;
        loop {
            let url = &text[start..end];
            let Some(last) = url.chars().last() else {
                break;
            };
            let unbalanced = |open: char, close: char| {
                last == close && url.matches(close).count() > url.matches(open).count()
            };
            if matches!(last, '.' | ',' | ';' | ':' | '!' | '?')
                || unbalanced('(', ')')
                || unbalanced('[', ']')
            {
                end -= 1;
            } else {
                break;
            }
        }
        // A bare scheme (`https://` and nothing after) is not a link.
        let url = &text[start..end];
        if SCHEMES
            .iter()
            .any(|s| url.starts_with(s) && url.len() > s.len())
        {
            out.push(start..end);
        }
        from = (start + len).max(start + 1);
    }
    out
}

/// Every distinct URL in `texts`, in order of first appearance.
pub(super) fn urls_in<'a>(texts: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut urls: Vec<String> = Vec::new();
    for text in texts {
        for range in find_urls(text) {
            let url = &text[range];
            if !urls.iter().any(|u| u == url) {
                urls.push(url.to_string());
            }
        }
    }
    urls
}

/// `symbol` wrapped in an OSC 8 hyperlink to `url`.
fn wrap(symbol: &str, url: &str) -> String {
    format!("\x1b]8;;{url}\x1b\\{symbol}\x1b]8;;\x1b\\")
}

/// Link the URLs visible in `area` of `buf` to their entry in `urls`. A
/// visible URL that the pane edge cut short links to the full URL it starts.
pub(super) fn link_area(buf: &mut Buffer, area: Rect, urls: &[String]) {
    let area = area.intersection(buf.area);
    if urls.is_empty() || area.is_empty() {
        return;
    }
    let one = CellDiffOption::ForcedWidth(NonZeroU16::MIN);
    for y in area.top()..area.bottom() {
        // One byte per cell: an ASCII cell keeps its character, anything
        // else (wide glyphs, already-linked cells) becomes a space.
        let row: String = (area.left()..area.right())
            .map(|x| {
                let mut chars = buf[(x, y)].symbol().chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) if c.is_ascii_graphic() => c,
                    _ => ' ',
                }
            })
            .collect();
        for range in find_urls(&row) {
            let shown = &row[range.clone()];
            let Some(url) = urls
                .iter()
                .find(|u| u.starts_with(shown) && u.chars().all(is_url_char))
            else {
                continue;
            };
            for offset in range {
                let Ok(dx) = u16::try_from(offset) else {
                    break;
                };
                let cell = &mut buf[(area.left() + dx, y)];
                let symbol = wrap(cell.symbol(), url);
                cell.set_symbol(&symbol).set_diff_option(one);
            }
        }
    }
}

/// The end-of-frame pass: link every recorded region, when the terminal
/// gets hyperlinks at all.
pub(super) fn apply(app: &App, frame: &mut Frame) {
    if !app.terminal_caps.hyperlinks {
        return;
    }
    let buf = frame.buffer_mut();
    for (area, urls) in &app.layout.hyperlink_regions {
        link_area(buf, *area, urls);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urls(text: &str) -> Vec<&str> {
        find_urls(text).into_iter().map(|r| &text[r]).collect()
    }

    #[test]
    fn finds_urls_and_trims_punctuation() {
        assert_eq!(
            urls("see https://example.com/a?b=1. and http://x.org"),
            ["https://example.com/a?b=1", "http://x.org"]
        );
        assert_eq!(
            urls("Docs (https://example.com/docs)"),
            ["https://example.com/docs"]
        );
        assert_eq!(
            urls("wiki https://en.wikipedia.org/wiki/Rust_(language)"),
            ["https://en.wikipedia.org/wiki/Rust_(language)"]
        );
        assert_eq!(urls("file:///tmp/x.txt"), ["file:///tmp/x.txt"]);
        assert_eq!(urls("no links here, just https://"), [] as [&str; 0]);
        assert_eq!(urls(""), [] as [&str; 0]);
    }

    #[test]
    fn stops_at_control_and_non_ascii_characters() {
        assert_eq!(
            urls("https://a.example/\x1b]8;;evil"),
            ["https://a.example/"]
        );
        assert_eq!(urls("https://a.example/é"), ["https://a.example/"]);
    }

    #[test]
    fn links_each_url_cell_with_a_forced_width() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 1));
        buf.set_string(0, 0, "go https://a.b now", ratatui::style::Style::default());
        let list = vec!["https://a.b".to_string()];
        let all = buf.area;
        link_area(&mut buf, all, &list);
        assert_eq!(buf[(0, 0)].symbol(), "g");
        assert_eq!(
            buf[(3, 0)].symbol(),
            "\x1b]8;;https://a.b\x1b\\h\x1b]8;;\x1b\\"
        );
        assert_eq!(
            buf[(13, 0)].symbol(),
            "\x1b]8;;https://a.b\x1b\\b\x1b]8;;\x1b\\"
        );
        assert_eq!(
            buf[(3, 0)].diff_option,
            CellDiffOption::ForcedWidth(NonZeroU16::MIN)
        );
        assert_eq!(buf[(14, 0)].symbol(), " ");
    }

    #[test]
    fn a_cut_off_url_links_to_the_full_one_and_unknown_urls_stay_plain() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 12, 1));
        buf.set_string(0, 0, "https://exam", ratatui::style::Style::default());
        let list = vec!["https://example.com/long".to_string()];
        let all = buf.area;
        link_area(&mut buf, all, &list);
        assert!(buf[(0, 0)].symbol().contains("https://example.com/long"));

        let mut buf = Buffer::empty(Rect::new(0, 0, 12, 1));
        buf.set_string(0, 0, "https://a.b", ratatui::style::Style::default());
        let all = buf.area;
        link_area(&mut buf, all, &["https://c.d".to_string()]);
        assert_eq!(buf[(0, 0)].symbol(), "h");
    }

    #[test]
    fn urls_in_dedupes_in_order() {
        assert_eq!(
            urls_in(["a https://x.y", "https://z.w and https://x.y"]),
            ["https://x.y", "https://z.w"]
        );
    }
}
