#![allow(clippy::pedantic)] // folded subcrate: kept at its original (non-pedantic) lint level
//! Time-zone modeling for Vix.
//!
//! This crate owns two things:
//!
//! - [`ZONES`]: the full IANA *canonical* zone table — one [`Zone`] per name from
//!   the system tz database, carrying its **standard** (non-DST) UTC offset, a
//!   DST flag, and the standard-period abbreviation. The data is generated (see
//!   `src/zones.rs` and `spec/index.md`); offsets are standard time, so they do
//!   not shift with daylight saving.
//! - The single application-wide **active time zone**, mirroring how the theme
//!   model holds the one active theme. UI crates (the time-zone chooser) set it;
//!   readers (the clock panel, status bar) query it. It defaults to UTC.
//!
//! The crate is pure data with no dependencies and no I/O.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::sync::RwLock;

mod zones;
pub use zones::ZONES;

/// One IANA canonical time zone.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Zone {
    /// Canonical IANA name, e.g. `"America/New_York"`.
    pub name: &'static str,
    /// Standard (non-DST) offset from UTC, in minutes (e.g. `-300` for UTC−05:00).
    pub std_offset_minutes: i32,
    /// Whether the zone observes daylight saving at some point in the year.
    pub has_dst: bool,
    /// Abbreviation during the standard period, e.g. `"EST"` (may be empty or a
    /// numeric form like `"+05"` for zones the database does not name).
    pub abbrev: &'static str,
}

impl Zone {
    /// The offset formatted as `UTC±HH:MM` (standard time).
    #[must_use]
    pub fn offset_label(&self) -> String {
        format_offset(self.std_offset_minutes)
    }
}

/// Format an offset in minutes as `UTC±HH:MM`.
#[must_use]
pub fn format_offset(minutes: i32) -> String {
    let sign = if minutes < 0 { '-' } else { '+' };
    let m = minutes.abs();
    format!("UTC{sign}{:02}:{:02}", m / 60, m % 60)
}

/// Index of `name` in [`ZONES`] (exact match), if present.
#[must_use]
pub fn index_of(name: &str) -> Option<usize> {
    ZONES.iter().position(|z| z.name == name)
}

/// The index of the UTC zone, used as the default active zone.
#[must_use]
pub fn utc_index() -> usize {
    index_of("UTC").unwrap_or(0)
}

// The active zone: an index into `ZONES`. `None` means "not yet set" and reads
// as UTC.
static ACTIVE: RwLock<Option<usize>> = RwLock::new(None);

/// Set the active zone by canonical `name`. Returns `true` if the name is known
/// (and was applied); `false` leaves the active zone unchanged.
pub fn set_active(name: &str) -> bool {
    match index_of(name) {
        Some(i) => {
            *ACTIVE.write().expect("time-zone lock") = Some(i);
            true
        }
        None => false,
    }
}

/// The active [`Zone`] (UTC until one is set).
#[must_use]
pub fn active() -> &'static Zone {
    let idx = ACTIVE.read().expect("time-zone lock").unwrap_or_else(utc_index);
    &ZONES[idx.min(ZONES.len() - 1)]
}

/// The active zone's canonical name.
#[must_use]
pub fn active_name() -> &'static str {
    active().name
}

/// The active zone's standard UTC offset, in minutes.
#[must_use]
pub fn active_offset_minutes() -> i32 {
    active().std_offset_minutes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_and_has_utc() {
        assert!(!ZONES.is_empty());
        assert!(index_of("UTC").is_some());
        assert!(index_of("America/New_York").is_some());
        let sorted = ZONES.windows(2).all(|w| w[0].name <= w[1].name);
        assert!(sorted, "zones must be sorted by name");
    }

    #[test]
    fn offset_formatting() {
        assert_eq!(format_offset(0), "UTC+00:00");
        assert_eq!(format_offset(-300), "UTC-05:00");
        assert_eq!(format_offset(330), "UTC+05:30");
        assert_eq!(format_offset(840), "UTC+14:00");
    }

    #[test]
    fn active_defaults_to_utc_then_follows_set() {
        // Default (before any set on this lock) reads as UTC.
        assert_eq!(active().std_offset_minutes, 0);
        assert!(set_active("America/New_York"));
        assert_eq!(active_name(), "America/New_York");
        assert!(!set_active("Not/AZone"));
        assert_eq!(active_name(), "America/New_York");
        // Restore for any other test ordering.
        assert!(set_active("UTC"));
    }
}
