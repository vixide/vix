//! Howard Hinnant's civil-date algorithm: convert between a day count since
//! 1970-01-01 (the Unix epoch) and a `(year, month, day)` calendar date, with
//! no dependency on a date/time crate.
//!
//! Extracted (Run H, T520) after this exact ~15-line algorithm was found
//! hand-rolled independently in three crates (`vix-file-information-panel`,
//! `vix-git`, `vix-org`) — real duplication risk (three unrelated copies of
//! the same magic-constant arithmetic), not domain-driven similarity, since
//! none of the three crates otherwise depend on a date/time library.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Convert a day count since 1970-01-01 to a `(year, month, day)` civil date.
/// The inverse of [`days_from_civil`].
#[must_use]
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (era * 400 + yoe + i64::from(m <= 2), m, d)
}

/// Convert a `(year, month, day)` civil date to a day count since 1970-01-01.
/// The inverse of [`civil_from_days`]. `month`/`day` are not validated —
/// an out-of-range value (e.g. `month: 13`) still produces a well-defined
/// result via the same arithmetic, just not one naming a real date.
#[must_use]
pub fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_reference_dates() {
        // The Unix epoch itself.
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        // A well-known date, cross-checked against `date -d @1600000000 -u`.
        assert_eq!(civil_from_days(18_518), (2020, 9, 13));
        assert_eq!(days_from_civil(2020, 9, 13), 18_518);
        // A leap-day, and the day right after it.
        assert_eq!(civil_from_days(19_782), (2024, 2, 29));
        assert_eq!(civil_from_days(19_783), (2024, 3, 1));
        // Before the epoch.
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(days_from_civil(1969, 12, 31), -1);
    }

    #[test]
    fn round_trips_across_a_wide_range() {
        // Every day count in a ~1000-year span, both directions, must round-trip
        // exactly -- the two functions are meant to be exact inverses.
        for days in (-200_000..200_000).step_by(37) {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(
                days_from_civil(y, m, d),
                days,
                "round trip failed for day {days} -> {y:04}-{m:02}-{d:02}"
            );
        }
    }

    #[test]
    fn month_and_day_are_always_in_range_for_a_real_date() {
        for days in (-100_000..100_000).step_by(11) {
            let (_, m, d) = civil_from_days(days);
            assert!(
                (1..=12).contains(&m),
                "month {m} out of range for day {days}"
            );
            assert!((1..=31).contains(&d), "day {d} out of range for day {days}");
        }
    }
}
