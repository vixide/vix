//! Information about the active editor file and the panel's row-selection state.
//!
//! Vix's Tools menu offers a *File Information* panel: a small table of facts
//! about the file in the active tab — name, path, language, character / word /
//! line counts, on-disk size, Unix permissions, and last-modified time. The host
//! gathers the raw values (it owns the buffer and does the filesystem stat) into
//! a [`FileInfo`]; this crate formats them into [`Row`]s and tracks the selection
//! so a value can be inserted into the editor.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Raw facts the host gathers about the active file.
#[derive(Clone, Debug, Default)]
pub struct FileInfo {
    /// File name (e.g. `main.rs`), or a placeholder for an unsaved buffer.
    pub name: String,
    /// Full path, or empty if the buffer has never been saved.
    pub path: String,
    /// Editor language id (e.g. `rust`, `text`).
    pub language: String,
    /// Character count of the buffer.
    pub chars: usize,
    /// Word count (whitespace-separated) of the buffer.
    pub words: usize,
    /// Line count of the buffer.
    pub lines: usize,
    /// On-disk size in bytes, or `None` when the file is not saved.
    pub bytes: Option<u64>,
    /// Unix permission bits, or `None` (e.g. unsaved, or non-Unix).
    pub mode: Option<u32>,
    /// Last-modified time in seconds since the Unix epoch, or `None`.
    pub modified_secs: Option<i64>,
    /// Whether the buffer has unsaved changes.
    pub dirty: bool,
}

/// One row of the table: a `label` and its `value`.
#[derive(Clone, Debug)]
pub struct Row {
    /// Left-hand label.
    pub label: String,
    /// Right-hand value (empty rows insert nothing).
    pub value: String,
}

impl Row {
    fn new(label: &str, value: impl Into<String>) -> Self {
        Row { label: label.to_string(), value: value.into() }
    }
}

/// Build the display rows for `info`.
#[must_use]
pub fn rows(info: &FileInfo) -> Vec<Row> {
    let mut rows = vec![
        Row::new("Name", if info.name.is_empty() { "(unsaved)".into() } else { info.name.clone() }),
        Row::new("Path", if info.path.is_empty() { "(unsaved)".into() } else { info.path.clone() }),
        Row::new("Language", info.language.clone()),
        Row::new("Modified", if info.dirty { "yes (unsaved changes)" } else { "no" }),
        Row::new("Characters", info.chars.to_string()),
        Row::new("Words", info.words.to_string()),
        Row::new("Lines", info.lines.to_string()),
    ];
    if let Some(bytes) = info.bytes {
        rows.push(Row::new("Size", format!("{} ({bytes} bytes)", human_bytes(bytes))));
    }
    if let Some(mode) = info.mode {
        rows.push(Row::new("Permissions", format!("{} ({:o})", format_unix_mode(mode), mode & 0o7777)));
    }
    if let Some(secs) = info.modified_secs {
        rows.push(Row::new("Last modified", format_unix_time(secs)));
    }
    rows
}

/// Format a byte count as a human-readable size (`16.0 KiB`).
#[must_use]
pub fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// Format Unix permission bits as a `rwxr-xr-x`-style string (low 9 bits).
#[must_use]
pub fn format_unix_mode(mode: u32) -> String {
    let bit = |shift: u32, ch: char| if mode & (1 << shift) != 0 { ch } else { '-' };
    [
        bit(8, 'r'), bit(7, 'w'), bit(6, 'x'),
        bit(5, 'r'), bit(4, 'w'), bit(3, 'x'),
        bit(2, 'r'), bit(1, 'w'), bit(0, 'x'),
    ]
    .iter()
    .collect()
}

/// Format seconds since the Unix epoch as `YYYY-MM-DD HH:MM:SS UTC`.
#[must_use]
pub fn format_unix_time(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}:{s:02} UTC")
}

/// Convert a day count since 1970-01-01 to a `(year, month, day)` civil date.
/// Howard Hinnant's `civil_from_days` algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Selection + scroll state for the File Information overlay.
pub struct Panel {
    /// The rows, in display order.
    pub rows: Vec<Row>,
    /// Index of the highlighted row.
    pub selected: usize,
    /// First visible row, kept in sync by [`Panel::ensure_visible`].
    pub scroll: usize,
}

impl Panel {
    /// Open the panel over the rows built from `info`.
    #[must_use]
    pub fn open(info: &FileInfo) -> Self {
        Panel { rows: rows(info), selected: 0, scroll: 0 }
    }

    /// Number of rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether the table has no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Move the highlight up one row.
    pub fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Move the highlight down one row.
    pub fn down(&mut self) {
        if self.selected + 1 < self.rows.len() {
            self.selected += 1;
        }
    }

    /// Move up one page.
    pub fn page_up(&mut self, page: usize) {
        self.selected = self.selected.saturating_sub(page.max(1));
    }

    /// Move down one page.
    pub fn page_down(&mut self, page: usize) {
        if !self.rows.is_empty() {
            self.selected = (self.selected + page.max(1)).min(self.rows.len() - 1);
        }
    }

    /// Select a row directly (e.g. from a click); returns whether `idx` was real.
    pub fn select_index(&mut self, idx: usize) -> bool {
        if idx < self.rows.len() {
            self.selected = idx;
            true
        } else {
            false
        }
    }

    /// Keep the highlighted row within a window of `height` visible rows.
    pub fn ensure_visible(&mut self, height: usize) {
        let height = height.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + height {
            self.scroll = self.selected + 1 - height;
        }
        let max_scroll = self.rows.len().saturating_sub(height);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    /// The highlighted row's value (what insertion uses).
    #[must_use]
    pub fn selected_value(&self) -> String {
        self.rows.get(self.selected).map(|r| r.value.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_bits_format() {
        assert_eq!(format_unix_mode(0o644), "rw-r--r--");
        assert_eq!(format_unix_mode(0o755), "rwxr-xr-x");
        assert_eq!(format_unix_mode(0o600), "rw-------");
    }

    #[test]
    fn epoch_and_known_dates_format() {
        assert_eq!(format_unix_time(0), "1970-01-01 00:00:00 UTC");
        // 2021-01-01 00:00:00 UTC = 1609459200.
        assert_eq!(format_unix_time(1_609_459_200), "2021-01-01 00:00:00 UTC");
        // 2000-02-29 (leap day) 12:34:56 UTC = 951827696.
        assert_eq!(format_unix_time(951_827_696), "2000-02-29 12:34:56 UTC");
    }

    #[test]
    fn rows_cover_counts_and_optional_fields() {
        let info = FileInfo {
            name: "main.rs".into(),
            path: "/p/main.rs".into(),
            language: "rust".into(),
            chars: 10,
            words: 3,
            lines: 2,
            bytes: Some(2048),
            mode: Some(0o644),
            modified_secs: Some(0),
            dirty: true,
        };
        let r = rows(&info);
        assert!(r.iter().any(|x| x.label == "Characters" && x.value == "10"));
        assert!(r.iter().any(|x| x.label == "Permissions" && x.value.starts_with("rw-r--r--")));
        assert!(r.iter().any(|x| x.label == "Size" && x.value.contains("2.0 KiB")));
        assert!(r.iter().any(|x| x.label == "Modified" && x.value.starts_with("yes")));
    }

    #[test]
    fn unsaved_buffer_omits_disk_fields() {
        let info = FileInfo { language: "text".into(), lines: 1, ..Default::default() };
        let r = rows(&info);
        assert!(r.iter().any(|x| x.label == "Name" && x.value == "(unsaved)"));
        assert!(!r.iter().any(|x| x.label == "Size"));
        assert!(!r.iter().any(|x| x.label == "Permissions"));
    }

    #[test]
    fn panel_navigation_clamps() {
        let info = FileInfo { language: "text".into(), ..Default::default() };
        let mut p = Panel::open(&info);
        let last = p.len() - 1;
        p.up();
        assert_eq!(p.selected, 0);
        p.page_down(100);
        assert_eq!(p.selected, last);
        assert!(p.select_index(0));
        assert!(!p.select_index(p.len()));
    }
}
