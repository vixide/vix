# vix-clock-panel

The **clock box** (Tools → Clock…): live date/time strings plus a small
selectable row model. Split out of `vix-calendar-panel` so the clock (live times)
and the calendar (a navigable month grid) are independent.

## Strings

Pure functions over [`jiff`] computed from a `Zoned` "now":

- `now_local()` — the current instant in the system zone.
- `local_clock(now)` — `HH:MM:SS` (local).
- `local_datetime(now)` — `YYYY-MM-DD HH:MM:SS` (local).
- `utc_iso(now)` — `YYYY-MM-DDTHH:MM:SSZ` (UTC instant).
- `iso_week_date(now)` — ISO 8601 commercial date `YYYY-Www-D`.
- `datetime_at_offset(now, minutes)` — wall clock at a fixed UTC offset.
- `active_zone_datetime(now)` — wall clock in the application-wide active time
  zone, using its **standard** (non-DST) offset from `vix-time-zone-model`.

## Rows

`Clock` holds the selected row. `rows(now)` returns four `Row`s in order — local
date-time, UTC, ISO commercial week date, and the active time zone's time — each
a `(key, value)` pair (the host localizes the label from `key`). `up`/`down`
move the highlight (clamped), `select(row)` sets it from a click, and
`selected_value(now)` is the string the host inserts on accept.

The host draws the box (one row per time, a help line), routes keys (↑↓ select,
Enter insert + close, Esc/q close) and mouse (click a row to insert + close).
This crate is pure logic with no rendering.
