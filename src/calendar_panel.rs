//! Vix Calendar: the navigable month grid shown in Vix's calendar box. See
//! `spec/index.md`.
//!
//! This crate is pure logic over [`jiff`] — it computes a day grid but does no
//! rendering, so the host draws it in whatever style it likes and the logic
//! stays unit-testable without a terminal. The live date/time strings (local,
//! UTC, ISO week, active zone) moved to `vix-clock-panel`.

#![warn(clippy::pedantic)]

use jiff::civil::Date;
use jiff::{ToSpan, Zoned};

/// Current time in the system's local time zone.
#[must_use] 
pub fn now_local() -> Zoned {
    Zoned::now()
}

/// The first day of the month containing `date`.
fn first_of_month(date: Date) -> Date {
    Date::new(date.year(), date.month(), 1).expect("first of month is always valid")
}

/// A month laid out as weeks of seven optional day numbers, Monday first.
/// `None` marks padding cells before the 1st and after the last day.
pub struct MonthGrid {
    /// Rows of the grid, each a week of seven cells (Monday first); `None` cells
    /// are padding before the 1st and after the last day.
    pub weeks: Vec<[Option<u8>; 7]>,
    /// Day-of-month of "today", set only when this grid's month is the actual
    /// current local month; otherwise `None` (so a navigated-to month has no
    /// highlight).
    pub today: Option<u8>,
}

/// Build the day grid for the month containing `month` (any day within it).
/// Highlights today only when that month is the current local month.
#[must_use] 
pub fn month_grid(month: Date) -> MonthGrid {
    let first = first_of_month(month);
    // 0 = Monday .. 6 = Sunday — the column the 1st lands in.
    let lead = usize::try_from(first.weekday().to_monday_zero_offset()).unwrap_or(0);
    let days = u8::try_from(first.days_in_month()).unwrap_or(0);

    let now = now_local().date();
    let today = (now.year() == first.year() && now.month() == first.month())
        .then_some(u8::try_from(now.day()).unwrap_or(0));

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

/// The calendar box's month view and its navigation state. The displayed month
/// is independent of the live clock, so the user can page back and forth with
/// [`Calendar::prev_month`] / [`Calendar::next_month`] while the date/time area
/// keeps showing the present.
pub struct Calendar {
    /// First day of the month currently displayed.
    shown: Date,
    /// The selected day (the keyboard cursor). The displayed month follows it.
    selected: Date,
}

impl Default for Calendar {
    fn default() -> Self {
        Calendar::new()
    }
}

impl Calendar {
    /// A calendar showing the current local month, with today selected.
    #[must_use] 
    pub fn new() -> Self {
        let today = now_local().date();
        Calendar { shown: first_of_month(today), selected: today }
    }

    /// Keep the displayed month in sync with the selected day.
    fn sync_shown(&mut self) {
        self.shown = first_of_month(self.selected);
    }

    /// Move the selection by `days` (negative = earlier); the view follows.
    pub fn move_days(&mut self, days: i64) {
        self.selected = if days >= 0 {
            self.selected.saturating_add(days.days())
        } else {
            self.selected.saturating_sub((-days).days())
        };
        self.sync_shown();
    }

    /// Move the selection by `months` (negative = earlier); the view follows.
    pub fn move_months(&mut self, months: i64) {
        self.selected = if months >= 0 {
            self.selected.saturating_add(months.months())
        } else {
            self.selected.saturating_sub((-months).months())
        };
        self.sync_shown();
    }

    /// Move the selection by `years` (negative = earlier); the view follows.
    pub fn move_years(&mut self, years: i64) {
        self.selected = if years >= 0 {
            self.selected.saturating_add(years.years())
        } else {
            self.selected.saturating_sub((-years).years())
        };
        self.sync_shown();
    }

    /// The selected date.
    #[must_use] 
    pub fn selected(&self) -> Date {
        self.selected
    }

    /// The selected day-of-month, only when the selection is in the displayed
    /// month (so the host can highlight that cell).
    #[must_use] 
    pub fn selected_day_in_shown(&self) -> Option<u8> {
        (self.selected.year() == self.shown.year() && self.selected.month() == self.shown.month())
            .then_some(u8::try_from(self.selected.day()).unwrap_or(0))
    }

    /// The selected date formatted with a `strftime` `pattern`.
    #[must_use] 
    pub fn selected_formatted(&self, pattern: &str) -> String {
        self.selected.strftime(pattern).to_string()
    }

    /// Move the view (and selection) to the next month.
    pub fn next_month(&mut self) {
        self.move_months(1);
    }

    /// Move the view (and selection) to the previous month.
    pub fn prev_month(&mut self) {
        self.move_months(-1);
    }

    /// Snap the view and selection back to today.
    pub fn reset(&mut self) {
        let today = now_local().date();
        self.selected = today;
        self.shown = first_of_month(today);
    }

    /// First day of the displayed month.
    #[must_use] 
    pub fn shown_month(&self) -> Date {
        self.shown
    }

    /// Month-and-year heading for the displayed month, e.g. `June 2026`.
    #[must_use] 
    pub fn title(&self) -> String {
        self.shown.strftime("%B %Y").to_string()
    }

    /// Day grid for the displayed month (today highlighted only if it is the
    /// current local month).
    #[must_use] 
    pub fn grid(&self) -> MonthGrid {
        month_grid(self.shown)
    }

    /// Format day-of-month `day` in the displayed month with a `strftime`
    /// `pattern` (e.g. `"%Y-%m-%d"`). Returns `None` if `day` is not a valid day
    /// of the displayed month. The host chooses the pattern (e.g. per locale).
    #[must_use] 
    pub fn format_day(&self, day: u8, pattern: &str) -> Option<String> {
        let date = Date::new(self.shown.year(), self.shown.month(), i8::try_from(day).ok()?).ok()?;
        Some(date.strftime(pattern).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_has_valid_day_count() {
        let count = month_grid(now_local().date())
            .weeks
            .iter()
            .flatten()
            .filter(|c| c.is_some())
            .count();
        assert!((28..=31).contains(&count), "days in month: {count}");
    }

    #[test]
    fn current_month_highlights_today_others_do_not() {
        let mut cal = Calendar::new();
        assert!(cal.grid().today.is_some(), "current month highlights today");
        cal.next_month();
        assert!(cal.grid().today.is_none(), "a future month has no today");
        cal.prev_month();
        assert!(cal.grid().today.is_some(), "back to the current month");
    }

    #[test]
    fn format_day_uses_the_displayed_month() {
        let cal = Calendar::new();
        let first = cal.shown_month();
        let got = cal.format_day(15, "%Y-%m-%d").unwrap();
        assert_eq!(got, format!("{:04}-{:02}-15", first.year(), first.month()));
        // Day 0 is never valid.
        assert!(cal.format_day(0, "%Y-%m-%d").is_none());
    }

    #[test]
    fn navigation_wraps_year_boundaries() {
        let mut cal = Calendar::new();
        let start = cal.shown_month();
        for _ in 0..12 {
            cal.next_month();
        }
        let after = cal.shown_month();
        assert_eq!(after.year(), start.year() + 1);
        assert_eq!(after.month(), start.month());
        cal.reset();
        assert_eq!(cal.shown_month(), start);
    }
}
