//! Howard Hinnant's civil-date algorithm: convert between a day count since
//! 1970-01-01 (the Unix epoch) and a `(year, month, day)` calendar date, with
//! no dependency on a date/time crate.
//!
//! Extracted (Run H, T520) after this exact ~15-line algorithm was found
//! hand-rolled independently in three crates (`vix-file-information-panel`,
//! `vix-git`, `vix-org`) — real duplication risk (three unrelated copies of
//! the same magic-constant arithmetic), not domain-driven similarity, since
//! none of the three crates otherwise depend on a date/time library.

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

/// Convert a day count since 1970-01-01 to a `(year, month, day)` civil date.
/// The inverse of [`days_from_civil`]. `None` when the day count is so large
/// that the arithmetic does not fit an `i64`.
///
/// Works in the proleptic Gregorian calendar for negative day counts (dates
/// before 1970) too. The bracketed ranges in the body are the invariants of
/// each intermediate value in Hinnant's derivation.
///
/// # Examples
/// ```
/// assert_eq!(vix_civil_date::civil_from_days(0), Some((1970, 1, 1)));
/// assert_eq!(vix_civil_date::civil_from_days(-1), Some((1969, 12, 31)));
/// assert_eq!(vix_civil_date::civil_from_days(i64::MAX), None);
/// ```
#[must_use]
pub fn civil_from_days(z: i64) -> Option<(i64, i64, i64)> {
    // Shift the epoch from 1970-01-01 to 0000-03-01 so the leap day is the last
    // day of the (March-based) year, which makes the arithmetic closed-form.
    let z = z.checked_add(719_468)?;
    // An "era" is a 400-year Gregorian cycle (146_097 days); `div_euclid`
    // keeps `doe` non-negative for dates before the epoch.
    let era = z.div_euclid(146_097);
    // The day of the era is in [0, 146096], so everything below is small.
    #[expect(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        reason = "RCA-EXC-2: doe is in [0, 146096] (z minus its era's start), so every intermediate below stays within a few million; the quotients are floor divisions by non-zero constants"
    )]
    let (yoe, m, d) = {
        let doe = z.checked_sub(era.checked_mul(146_097)?)?; // [0, 146096]
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
        let mp = (5 * doy + 2) / 153; // [0, 11]
        let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
        // `mp` counts months from March; map back to calendar months, and January
        // and February belong to the *next* civil year (hence `m <= 2` below).
        let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
        (yoe, m, d)
    };
    let year = era
        .checked_mul(400)?
        .checked_add(yoe)?
        .checked_add(i64::from(m <= 2))?;
    Some((year, m, d))
}

/// Convert a `(year, month, day)` civil date to a day count since 1970-01-01.
/// The inverse of [`civil_from_days`]. `None` when `month` is not in `1..=12`,
/// `day` is not in `1..=31`, or the result does not fit an `i64`. A day that
/// the month does not have (`31` in June) is accepted and rolls into the next
/// month; the caller validates real dates.
///
/// # Examples
/// ```
/// assert_eq!(vix_civil_date::days_from_civil(1970, 1, 1), Some(0));
/// assert_eq!(vix_civil_date::days_from_civil(2024, 2, 29), Some(19_782));
/// assert_eq!(vix_civil_date::days_from_civil(2024, 13, 1), None);
/// ```
#[must_use]
pub fn days_from_civil(year: i64, month: i64, day: i64) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // January/February count as the tail of the previous March-based year.
    let y = if month <= 2 {
        year.checked_sub(1)?
    } else {
        year
    };
    let era = y.div_euclid(400);
    let yoe = y.checked_sub(era.checked_mul(400)?)?; // [0, 399]
    #[expect(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        reason = "RCA-EXC-2: month is 1 to 12, day 1 to 31 and yoe 0 to 399, so every intermediate below stays within a few hundred thousand"
    )]
    let doe = {
        let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1; // [0, 365]
        yoe * 365 + yoe / 4 - yoe / 100 + doy // [0, 146096]
    };
    era.checked_mul(146_097)?
        .checked_add(doe)?
        .checked_sub(719_468)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_reference_dates() {
        // The Unix epoch itself.
        assert_eq!(civil_from_days(0), Some((1970, 1, 1)));
        assert_eq!(days_from_civil(1970, 1, 1), Some(0));
        // A well-known date, cross-checked against `date -d @1600000000 -u`.
        assert_eq!(civil_from_days(18_518), Some((2020, 9, 13)));
        assert_eq!(days_from_civil(2020, 9, 13), Some(18_518));
        // A leap-day, and the day right after it.
        assert_eq!(civil_from_days(19_782), Some((2024, 2, 29)));
        assert_eq!(civil_from_days(19_783), Some((2024, 3, 1)));
        // Before the epoch.
        assert_eq!(civil_from_days(-1), Some((1969, 12, 31)));
        assert_eq!(days_from_civil(1969, 12, 31), Some(-1));
    }

    #[test]
    fn out_of_range_inputs_are_none_not_a_guess() {
        // Month and day outside their ranges are not dates.
        for (y, m, d) in [(2024, 0, 1), (2024, 13, 1), (2024, 1, 0), (2024, 1, 32)] {
            assert_eq!(days_from_civil(y, m, d), None, "{y}-{m}-{d}");
        }
        // A year or day count too large for the arithmetic is None, not a wrap.
        assert_eq!(days_from_civil(i64::MAX, 6, 1), None);
        assert_eq!(days_from_civil(i64::MIN, 1, 1), None);
        assert_eq!(civil_from_days(i64::MAX), None);
        // The month and day edges are accepted.
        assert!(days_from_civil(2024, 1, 1).is_some() && days_from_civil(2024, 12, 31).is_some());
    }

    #[test]
    fn round_trips_across_a_wide_range() {
        // Every day count in a ~1000-year span, both directions, must round-trip
        // exactly -- the two functions are meant to be exact inverses.
        for days in (-200_000..200_000).step_by(37) {
            let (y, m, d) = civil_from_days(days).unwrap();
            assert_eq!(
                days_from_civil(y, m, d),
                Some(days),
                "round trip failed for day {days} -> {y:04}-{m:02}-{d:02}"
            );
        }
    }

    #[test]
    fn month_and_day_are_always_in_range_for_a_real_date() {
        for days in (-100_000..100_000).step_by(11) {
            let (_, m, d) = civil_from_days(days).unwrap();
            assert!(
                (1..=12).contains(&m),
                "month {m} out of range for day {days}"
            );
            assert!((1..=31).contains(&d), "day {d} out of range for day {days}");
        }
    }

    #[test]
    fn last_day_of_a_400_year_era_round_trips() {
        // Mutation T606: the `146_096` term only matters on the final day of a
        // 400-year era (0000-03-01 based), e.g. 2000-02-29 -> 2000-12-31 spans.
        assert_eq!(days_from_civil(2000, 12, 31), Some(11_322));
        assert_eq!(civil_from_days(11_322), Some((2000, 12, 31)));
        assert_eq!(civil_from_days(11_016), Some((2000, 2, 29)));
        // Every day across a whole era, so each boundary term is exercised.
        for z in -146_097..=146_097 {
            let (y, m, d) = civil_from_days(z).unwrap();
            assert_eq!(days_from_civil(y, m, d), Some(z), "day {z} -> {y}-{m}-{d}");
        }
    }
}
