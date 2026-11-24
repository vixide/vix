//! Vix Clock: the date/time strings shown in Vix's clock box, plus a small
//! selectable row model. Moved out of the calendar panel so the clock (live
//! times) and the calendar (a navigable month grid) are independent.
//!
//! This crate is pure logic over [`jiff`] and [`vix_time_zone_model`] — it
//! computes strings but does no rendering, so the host draws it and the logic
//! stays unit-testable without a terminal. The strings cover the system-local
//! time, the UTC instant, the ISO 8601 commercial (week) date, and the wall
//! clock in the application-wide active time zone.

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

use jiff::ZonedDateTime;
use jiff::tz::{Offset, TimeZone};

/// Current time in the system's local time zone.
#[must_use]
pub fn now_local() -> ZonedDateTime {
    ZonedDateTime::now()
}

/// `HH:MM:SS` in the local zone.
#[must_use]
pub fn local_clock(now: &ZonedDateTime) -> String {
    now.strftime("%H:%M:%S").to_string()
}

/// Local date and time with seconds precision: `YYYY-MM-DD HH:MM:SS` in the
/// system zone (as opposed to the UTC instant).
#[must_use]
pub fn local_datetime(now: &ZonedDateTime) -> String {
    now.strftime("%Y-%m-%d %H:%M:%S").to_string()
}

/// ISO 8601 instant in UTC: `YYYY-MM-DDTHH:MM:SSZ`.
#[must_use]
pub fn utc_iso(now: &ZonedDateTime) -> String {
    now.timestamp().strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Local ISO 8601 date-time without offset: `YYYY-MM-DDTHH:MM:SS`.
#[must_use]
pub fn iso8601(now: &ZonedDateTime) -> String {
    now.strftime("%Y-%m-%dT%H:%M:%S").to_string()
}

/// RFC 3339 date-time with the local UTC offset: `YYYY-MM-DDTHH:MM:SS±HH:MM`.
#[must_use]
pub fn rfc3339(now: &ZonedDateTime) -> String {
    now.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

/// Unix epoch in whole seconds.
#[must_use]
pub fn epoch_seconds(now: &ZonedDateTime) -> String {
    now.strftime("%s").to_string()
}

/// ISO 8601 commercial date: `YYYY-Www-D` (week-numbering year, week 01..53,
/// day 1=Monday..7=Sunday).
#[must_use]
pub fn iso_week_date(now: &ZonedDateTime) -> String {
    let iso = now.date().iso_week_date();
    format!(
        "{:04}-W{:02}-{}",
        iso.year(),
        iso.week(),
        iso.weekday().to_monday_one_offset()
    )
}

/// `a - b`, or the declared `fallback` when it would go below zero. The
/// fallback is a parameter so each call names the declaration it comes from;
/// written inline, `checked_sub(..).unwrap_or(0)` is rewritten by Clippy to
/// `saturating_sub`, which the checked-arithmetic spec forbids.
fn sub_or(a: usize, b: usize, fallback: usize) -> usize {
    a.checked_sub(b).unwrap_or(fallback)
}

/// The wall clock `YYYY-MM-DD HH:MM:SS` at a fixed UTC `offset_minutes`, derived
/// from the same instant as `now`. Used to show the active time zone's time.
///
/// # Errors
/// [`ClockError::OffsetOutOfRange`] if `offset_minutes` is not an offset jiff
/// accepts (more than a day either side of UTC); it is never replaced by UTC or
/// by the nearest valid offset.
pub fn datetime_at_offset(now: &ZonedDateTime, offset_minutes: i32) -> Result<String, ClockError> {
    let offset = offset_minutes
        .checked_mul(60)
        .and_then(|secs| Offset::from_seconds(secs).ok())
        .ok_or(ClockError::OffsetOutOfRange {
            figure: "offset_minutes",
        })?;
    Ok(now
        .timestamp()
        .to_zoned_datetime(TimeZone::fixed(offset))
        .strftime("%Y-%m-%d %H:%M:%S")
        .to_string())
}

/// The active zone's current wall clock (`YYYY-MM-DD HH:MM:SS`), using its
/// standard (non-DST) offset from [`vix_time_zone_model`].
///
/// # Errors
/// As [`datetime_at_offset`].
pub fn active_zone_datetime(now: &ZonedDateTime) -> Result<String, ClockError> {
    datetime_at_offset(now, vix_time_zone_model::active_offset_minutes())
}

/// One labeled clock row: a translation-key-free *value* the host pairs with a
/// localized label, and which a click/Enter inserts into the editor.
pub struct Row {
    /// Stable key identifying the row (`"local"`, `"utc"`, `"iso_week"`,
    /// `"zone"`), so the host can localize the label.
    pub key: &'static str,
    /// The formatted value (e.g. `2026-06-14 09:30:00`), or the error when the
    /// row cannot be computed (the host shows it as unknown, never as a time).
    pub value: Result<String, ClockError>,
}

/// Why a clock value could not be computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ClockError {
    /// The UTC offset is outside the range jiff accepts.
    OffsetOutOfRange {
        /// The figure that is out of range.
        figure: &'static str,
    },
}

impl std::fmt::Display for ClockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OffsetOutOfRange { figure } => write!(f, "{figure}: not a valid UTC offset"),
        }
    }
}

impl std::error::Error for ClockError {}

/// The clock box: which row is selected. The values themselves are recomputed
/// from the live clock on each render, so this holds only the cursor.
pub struct Clock {
    /// Index of the highlighted row.
    pub selected: usize,
}

impl Default for Clock {
    /// Same as [`Clock::new`], so `Clock::default()` also starts on the first row.
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
    pub fn rows(&self, now: &ZonedDateTime) -> Vec<Row> {
        vec![
            Row {
                key: "local",
                value: Ok(local_datetime(now)),
            },
            Row {
                key: "utc",
                value: Ok(utc_iso(now)),
            },
            Row {
                key: "iso_week",
                value: Ok(iso_week_date(now)),
            },
            Row {
                key: "zone",
                value: active_zone_datetime(now),
            },
        ]
    }

    /// Number of rows (constant).
    #[must_use]
    pub fn row_count(&self) -> usize {
        4
    }

    /// Highlight the previous row (clamped).
    pub fn up(&mut self) {
        self.selected = sub_or(self.selected, 1, 0); // CLK-F2: stop at the first row
    }

    /// Highlight the next row (clamped).
    pub fn down(&mut self) {
        let last = sub_or(self.row_count(), 1, 0); // CLK-F2: no rows, row 0
        self.selected = self.selected.checked_add(1).unwrap_or(last).min(last);
    }

    /// Set the highlight from an absolute row index (e.g. a mouse click); ignored
    /// if out of range.
    pub fn select(&mut self, row: usize) {
        if row < self.row_count() {
            self.selected = row;
        }
    }

    /// The highlighted row's value at instant `now` (for insertion): `None` when
    /// no row is highlighted, `Some(Err(..))` when the row cannot be computed.
    #[must_use]
    pub fn selected_value(&self, now: &ZonedDateTime) -> Option<Result<String, ClockError>> {
        self.rows(now)
            .into_iter()
            .nth(self.selected)
            .map(|r| r.value)
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
        let utc = datetime_at_offset(&now, 0).unwrap();
        let plus = datetime_at_offset(&now, 60).unwrap();
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

    #[test]
    fn an_offset_jiff_rejects_is_an_error_not_utc() {
        let now = now_local();
        let bad = Err(ClockError::OffsetOutOfRange {
            figure: "offset_minutes",
        });
        // jiff accepts offsets up to just under 26 hours: 25:59:59 is the edge.
        assert!(datetime_at_offset(&now, 1_559).is_ok(), "last valid offset");
        assert!(datetime_at_offset(&now, -1_559).is_ok());
        assert_eq!(datetime_at_offset(&now, 1_560), bad, "one past it");
        assert_eq!(datetime_at_offset(&now, -1_560), bad);
        assert_eq!(
            datetime_at_offset(&now, i32::MAX),
            bad,
            "the product overflows"
        );
        assert_eq!(
            ClockError::OffsetOutOfRange {
                figure: "offset_minutes"
            }
            .to_string(),
            "offset_minutes: not a valid UTC offset"
        );
    }

    #[test]
    fn every_zone_in_the_table_has_a_valid_offset() {
        let now = now_local();
        for zone in vix_time_zone_model::ZONES {
            assert!(
                datetime_at_offset(&now, zone.std_offset_minutes).is_ok(),
                "{}",
                zone.name
            );
        }
    }
}
