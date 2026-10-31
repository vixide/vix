//! The picker preview pane's data (T630): a bounded window of a file around
//! one line, built without ever holding more than [`PREVIEW_BYTES`] of it.

/// Narrowest screen (in columns) that shows a picker's preview pane.
pub const MIN_PREVIEW_WIDTH: u16 = 100;

/// Most bytes read from disk for one preview, so a huge file costs one
/// bounded read.
pub const PREVIEW_BYTES: u64 = 256 * 1024;

/// Lines kept on either side of the target line.
pub const PREVIEW_RADIUS: usize = 40;

/// Longest a single preview line is kept, in characters.
const LINE_CHARS: usize = 400;

/// A window of a file around one target line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    /// 1-based line number of `lines[0]`.
    pub first_line: usize,
    /// The window's lines, tabs expanded and each clipped.
    pub lines: Vec<String>,
    /// 1-based target line (the one highlighted).
    pub target: usize,
    /// Whether the target line lies past the bytes that were read, so the
    /// window shows only the file's start.
    pub beyond: bool,
}

impl Preview {
    /// The 0-based index into `lines` of the target line, if it is inside the
    /// window.
    #[must_use]
    pub fn target_index(&self) -> Option<usize> {
        let i = self.target.checked_sub(self.first_line)?;
        (i < self.lines.len()).then_some(i)
    }

    /// The rows a pane `height` rows tall shows: `(first 1-based line number,
    /// lines)`, with the target as near the middle as the window allows.
    #[must_use]
    pub fn visible(&self, height: usize) -> (usize, &[String]) {
        if height == 0 || self.lines.is_empty() {
            return (self.first_line, &[]);
        }
        let len = self.lines.len();
        let center = self.target_index().unwrap_or(0);
        let max_start = len.saturating_sub(height);
        let start = center.saturating_sub(height / 2).min(max_start);
        let end = (start + height).min(len);
        (self.first_line + start, &self.lines[start..end])
    }
}

/// Clip and tab-expand one line for display.
fn clean(line: &str) -> String {
    line.trim_end_matches(['\r', '\n'])
        .replace('\t', "    ")
        .chars()
        .take(LINE_CHARS)
        .collect()
}

/// Build the preview of `bytes` (the start of a file, at most
/// [`PREVIEW_BYTES`] of it) around 1-based `line`. `None` for binary content
/// (a NUL byte).
///
/// When `bytes` was cut at the read limit, its last (possibly partial) line
/// is dropped, and a target past the lines that remain marks the preview
/// [`Preview::beyond`] and shows the window at the file's start instead.
#[must_use]
pub fn preview_from_bytes(bytes: &[u8], line: usize, truncated: bool) -> Option<Preview> {
    if bytes.contains(&0) {
        return None;
    }
    let text = String::from_utf8_lossy(bytes);
    let mut all: Vec<&str> = text.lines().collect();
    if truncated && !text.ends_with('\n') {
        all.pop();
    }
    let target = line.max(1);
    let beyond = target > all.len() && truncated;
    let center = if beyond { 1 } else { target };
    let first = center.saturating_sub(PREVIEW_RADIUS).max(1);
    let last = (center + PREVIEW_RADIUS).min(all.len());
    let lines = all
        .get(first - 1..last)
        .unwrap_or_default()
        .iter()
        .map(|l| clean(l))
        .collect();
    Some(Preview {
        first_line: first,
        lines,
        target,
        beyond,
    })
}

/// Build the preview of a buffer with `total` lines around 1-based `line`,
/// fetching only the window's lines through `get` (0-based index), so an
/// open buffer is never copied whole.
#[must_use]
pub fn preview_from_lines(total: usize, line: usize, get: impl Fn(usize) -> String) -> Preview {
    let target = line.clamp(1, total.max(1));
    let first = target.saturating_sub(PREVIEW_RADIUS).max(1);
    let last = (target + PREVIEW_RADIUS).min(total);
    let lines = (first..=last).map(|n| clean(&get(n - 1))).collect();
    Preview {
        first_line: first,
        lines,
        target,
        beyond: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbered(n: usize) -> String {
        (1..=n).fold(String::new(), |mut acc, i| {
            acc.push('L');
            acc.push_str(&i.to_string());
            acc.push('\n');
            acc
        })
    }

    #[test]
    fn window_is_centered_on_the_target() {
        let p = preview_from_bytes(numbered(200).as_bytes(), 100, false).unwrap();
        assert_eq!(p.first_line, 100 - PREVIEW_RADIUS);
        assert_eq!(p.lines.len(), 2 * PREVIEW_RADIUS + 1);
        assert_eq!(p.lines[p.target_index().unwrap()], "L100");
        let (first, rows) = p.visible(5);
        assert_eq!(first, 98);
        assert_eq!(rows, ["L98", "L99", "L100", "L101", "L102"]);
    }

    #[test]
    fn window_clamps_at_the_file_edges() {
        let p = preview_from_bytes(numbered(10).as_bytes(), 2, false).unwrap();
        assert_eq!(p.first_line, 1);
        assert_eq!(p.lines.len(), 10);
        let (first, rows) = p.visible(4);
        assert_eq!(first, 1);
        assert_eq!(rows.len(), 4);
        let (first, rows) = p.visible(50);
        assert_eq!((first, rows.len()), (1, 10));
        let end = preview_from_bytes(numbered(10).as_bytes(), 10, false).unwrap();
        let (first, rows) = end.visible(4);
        assert_eq!((first, rows.last().map(String::as_str)), (7, Some("L10")));
    }

    #[test]
    fn binary_content_has_no_preview() {
        assert!(preview_from_bytes(b"abc\0def", 1, false).is_none());
    }

    #[test]
    fn a_target_past_a_truncated_read_is_marked_beyond() {
        let bytes = numbered(100);
        let cut = &bytes.as_bytes()[..bytes.len() - 2]; // "L100" now partial
        let p = preview_from_bytes(cut, 500, true).unwrap();
        assert!(p.beyond);
        assert_eq!(p.first_line, 1);
        assert!(p.target_index().is_none());
        assert!(
            !p.lines.iter().any(|l| l == "L100"),
            "the partial line is dropped"
        );
    }

    #[test]
    fn tabs_are_expanded_and_long_lines_clipped() {
        let long = "x".repeat(1000);
        let p = preview_from_bytes(format!("\tA\n{long}\n").as_bytes(), 1, false).unwrap();
        assert_eq!(p.lines[0], "    A");
        assert_eq!(p.lines[1].chars().count(), LINE_CHARS);
    }

    #[test]
    fn lines_fetches_only_the_window() {
        let fetched = std::cell::RefCell::new(Vec::new());
        let p = preview_from_lines(10_000, 5_000, |i| {
            fetched.borrow_mut().push(i);
            format!("L{}", i + 1)
        });
        assert_eq!(fetched.borrow().len(), 2 * PREVIEW_RADIUS + 1);
        assert_eq!(p.lines[p.target_index().unwrap()], "L5000");
    }
}
