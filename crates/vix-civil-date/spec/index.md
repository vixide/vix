# Civil date

Howard Hinnant's civil-date algorithm: convert between a day count since
1970-01-01 (the Unix epoch) and a `(year, month, day)` calendar date, with
no dependency on a date/time crate.

**Status:** Shipped (Run H, T520). Two pure functions, inverses of each
other: `civil_from_days(days: i64) -> (i64, i64, i64)` and
`days_from_civil(year: i64, month: i64, day: i64) -> i64`. Proleptic
Gregorian, valid for any `i64` day count (extends indefinitely in both
directions, unlike a real calendar's practical range).

## Why this crate exists

`vix-file-information-panel` (file mtime display), `vix-git` (commit
timestamps), and `vix-org` (timestamp arithmetic — schedule/deadline shifts,
the agenda's date math) each hand-rolled an independent copy of this exact
~15-line algorithm (same magic constants: `719_468`, `146_097`, `36_524`,
`146_096`) — real duplication, not domain-driven similarity, since none of
the three otherwise depends on a date/time library. Extracted here so a
future fix lands once instead of needing to be found and repeated three
times, and so the algorithm finally has its own direct unit tests (round-trip
and known-reference-date checks) rather than only ever being exercised
indirectly through each crate's higher-level date logic.

## Adoption

`vix-file-information-panel::format_unix_time` and `vix-git`'s internal
`epoch_to_date` both call `civil_from_days` directly, keeping their own
public formatting functions unchanged. `vix-org` additionally uses
`days_from_civil` for its own reverse direction (schedule/deadline shifting,
weekday computation, and timestamp-to-minutes arithmetic) — that half was
never duplicated elsewhere, so it moved here rather than staying behind.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). **Changed:** `civil_from_days` and `days_from_civil` return `Option<i64>`-shaped results. A `month` outside 1 to 12 or a `day` outside 1 to 31 used to produce a number that named no real date; it is now `None`, as is any year or day count too large for the arithmetic (it used to overflow). A day the month does not have (31 in June) is still accepted and rolls into the next month, so callers keep validating real dates. The derivation's intermediate values carry `RCA-EXC-2` proofs from the bracketed ranges.
