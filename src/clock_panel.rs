//! Vix Clock: the date/time strings shown in Vix's clock box, plus a small
//! selectable row model. Moved out of the calendar panel so the clock (live
//! times) and the calendar (a navigable month grid) are independent.
//!
//! This crate is pure logic over [`jiff`] and [`crate::time_zone_model`] — it
//! computes strings but does no rendering, so the host draws it and the logic
//! stays unit-testable without a terminal. The strings cover the system-local
//! time, the UTC instant, the ISO 8601 commercial (week) date, and the wall
//! clock in the application-wide active time zone.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use jiff::tz::{Offset, TimeZone};
use jiff::Zoned;

/// Current time in the system's local time zone.
#[must_use]
pub fn now_local() -> Zoned {
    Zoned::now()
}

/// `HH:MM:SS` in the local zone.
#[must_use]
pub fn local_clock(now: &Zoned) -> String {
    now.strftime("%H:%M:%S").to_string()
}

/// Local date and time with seconds precision: `YYYY-MM-DD HH:MM:SS` in the
/// system zone (as opposed to the UTC instant).
#[must_use]
pub fn local_datetime(now: &Zoned) -> String {
    now.strftime("%Y-%m-%d %H:%M:%S").to_string()
}

/// ISO 8601 instant in UTC: `YYYY-MM-DDTHH:MM:SSZ`.
#[must_use]
pub fn utc_iso(now: &Zoned) -> String {
    now.timestamp().strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// ISO 8601 commercial date: `YYYY-Www-D` (week-numbering year, week 01..53,
/// day 1=Monday..7=Sunday).
#[must_use]
pub fn iso_week_date(now: &Zoned) -> String {
    let iso = now.date().iso_week_date();
    format!(
        "{:04}-W{:02}-{}",
        iso.year(),
        iso.week(),
        iso.weekday().to_monday_one_offset()
    )
}

/// The wall clock `YYYY-MM-DD HH:MM:SS` at a fixed UTC `offset_minutes`, derived
/// from the same instant as `now`. Used to show the active time zone's time.
/// Offsets are clamped to jiff's valid range.
#[must_use]
pub fn datetime_at_offset(now: &Zoned, offset_minutes: i32) -> String {
    let secs = (offset_minutes * 60).clamp(-93_599, 93_599);
    let offset = Offset::from_seconds(secs).unwrap_or(Offset::UTC);
    now.timestamp().to_zoned(TimeZone::fixed(offset)).strftime("%Y-%m-%d %H:%M:%S").to_string()
}

/// The active zone's current wall clock (`YYYY-MM-DD HH:MM:SS`), using its
/// standard (non-DST) offset from [`crate::time_zone_model`].
#[must_use]
pub fn active_zone_datetime(now: &Zoned) -> String {
    datetime_at_offset(now, crate::time_zone_model::active_offset_minutes())
}

/// One labeled clock row: a translation-key-free *value* the host pairs with a
/// localized label, and which a click/Enter inserts into the editor.
pub struct Row {
    /// Stable key identifying the row (`"local"`, `"utc"`, `"iso_week"`,
    /// `"zone"`), so the host can localize the label.
    pub key: &'static str,
    /// The formatted value (e.g. `2026-06-14 09:30:00`).
    pub value: String,
}

/// The clock box: which row is selected. The values themselves are recomputed
/// from the live clock on each render, so this holds only the cursor.
pub struct Clock {
    /// Index of the highlighted row.
    pub selected: usize,
}

impl Default for Clock {
    fn default() -> Self {
        Clock::new()
    }
}

impl Clock {
    /// A clock with the first row selected.
    #[must_use]
    pub fn new() -> Self {
        Clock { selected: 0 }
    }

    /// The rows to display, in order: local date-time, UTC, ISO commercial week
    /// date, and the active time zone's time.
    #[must_use]
    pub fn rows(&self, now: &Zoned) -> Vec<Row> {
        vec![
            Row { key: "local", value: local_datetime(now) },
            Row { key: "utc", value: utc_iso(now) },
            Row { key: "iso_week", value: iso_week_date(now) },
            Row { key: "zone", value: active_zone_datetime(now) },
        ]
    }

    /// Number of rows (constant).
    #[must_use]
    pub fn row_count(&self) -> usize {
        4
    }

    /// Highlight the previous row (clamped).
    pub fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Highlight the next row (clamped).
    pub fn down(&mut self) {
        self.selected = (self.selected + 1).min(self.row_count() - 1);
    }

    /// Set the highlight from an absolute row index (e.g. a mouse click); ignored
    /// if out of range.
    pub fn select(&mut self, row: usize) {
        if row < self.row_count() {
            self.selected = row;
        }
    }

    /// The highlighted row's value at instant `now` (for insertion).
    #[must_use]
    pub fn selected_value(&self, now: &Zoned) -> Option<String> {
        self.rows(now).into_iter().nth(self.selected).map(|r| r.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_have_expected_shapes() {
        let now = now_local();
        assert_eq!(local_clock(&now).len(), 8); // HH:MM:SS
        assert_eq!(local_datetime(&now).len(), 19); // YYYY-MM-DD HH:MM:SS
        assert!(utc_iso(&now).ends_with('Z'));
        let iso = iso_week_date(&now);
        assert!(iso.contains("-W"), "commercial date: {iso}");
    }

    #[test]
    fn offset_conversion_differs_by_offset() {
        let now = now_local();
        let utc = datetime_at_offset(&now, 0);
        let plus = datetime_at_offset(&now, 60);
        // Same length, and (almost always) a different wall-clock string.
        assert_eq!(utc.len(), plus.len());
    }

    #[test]
    fn clock_rows_and_navigation() {
        let now = now_local();
        let mut c = Clock::new();
        assert_eq!(c.rows(&now).len(), 4);
        c.up();
        assert_eq!(c.selected, 0);
        c.down();
        c.down();
        c.down();
        c.down();
        assert_eq!(c.selected, 3);
        assert!(c.selected_value(&now).is_some());
    }
}
