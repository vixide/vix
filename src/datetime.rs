//! Date/time helpers built on the `jiff` crate.
//!
//! Provides the strings shown in the calendar box: local clock, UTC in ISO
//! 8601, and the ISO 8601 commercial (week-date) format `YYYY-Www-D`, plus the
//! day grid for the in-house month view.

use jiff::civil::Date;
use jiff::Zoned;

/// Current time in the system's local time zone.
pub fn now_local() -> Zoned {
    Zoned::now()
}

/// `HH:MM:SS` in the local zone.
pub fn local_clock(now: &Zoned) -> String {
    now.strftime("%H:%M:%S").to_string()
}

/// ISO 8601 instant in UTC: `YYYY-MM-DDTHH:MM:SSZ`.
pub fn utc_iso(now: &Zoned) -> String {
    now.timestamp()
        .strftime("%Y-%m-%dT%H:%M:%SZ")
        .to_string()
}

/// ISO 8601 commercial date: `YYYY-Www-D` (week-numbering year, week 01..53,
/// day 1=Monday..7=Sunday).
pub fn iso_week_date(now: &Zoned) -> String {
    let iso = now.date().iso_week_date();
    format!(
        "{:04}-W{:02}-{}",
        iso.year(),
        iso.week(),
        iso.weekday().to_monday_one_offset()
    )
}

/// Month-and-year heading for the calendar, e.g. `June 2026`.
pub fn month_title(now: &Zoned) -> String {
    now.strftime("%B %Y").to_string()
}

/// A month laid out as weeks of seven optional day numbers, Monday first.
/// `None` marks padding cells before the 1st and after the last day.
pub struct MonthGrid {
    pub weeks: Vec<[Option<u8>; 7]>,
    pub today: u8,
}

/// Build the day grid for the month containing `now`.
pub fn month_grid(now: &Zoned) -> MonthGrid {
    let today = now.day() as u8;
    let first = Date::new(now.year(), now.month(), 1).expect("valid first-of-month");
    // 0 = Monday .. 6 = Sunday — the column the 1st lands in.
    let lead = first.weekday().to_monday_zero_offset() as usize;
    let days = first.days_in_month() as u8;

    let mut weeks: Vec<[Option<u8>; 7]> = Vec::new();
    let mut week = [None; 7];
    let mut col = lead;
    for day in 1..=days {
        week[col] = Some(day);
        col += 1;
        if col == 7 {
            weeks.push(week);
            week = [None; 7];
            col = 0;
        }
    }
    if col != 0 {
        weeks.push(week);
    }
    MonthGrid { weeks, today }
}
