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
