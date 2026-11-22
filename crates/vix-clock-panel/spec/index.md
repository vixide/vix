# Clock

Clock box: live date/time strings (local, UTC, ISO week, active zone) + rows.
It inserts any of them into the editor. Open it with **Tools → Clock…** (a toggle). Its logic
lives in the `clock_panel` crate; the host renders the box and routes input.

## Rows

The box lists four live values, recomputed each render:

| Row      | Example                     | Source                                    |
| -------- | --------------------------- | ----------------------------------------- |
| Local    | `2026-06-14 09:30:00`       | system local date-time, seconds precision |
| UTC      | `2026-06-14T13:30:00Z`      | ISO 8601 UTC instant                      |
| ISO week | `2026-W24-7`                | ISO 8601 commercial (week) date           |
| *(zone)* | `2026-06-14 09:30:00`       | the active time zone's wall clock         |

The last row is labeled with the active time zone's name and shows the time at
that zone's **standard** (non-DST) offset (see `crates/vix-time-zone-model/spec/index.md`).

## Interaction

- **↑ / ↓** move the highlight; **Enter** inserts the highlighted value into the
  active editor and closes the box.
- **Click** a row to insert it; the box stays open so several values can be
  picked. A click outside the box closes it.
- **Esc** / **q** close without inserting.

## Relationship to the calendar

The clock (live times) and the calendar (a navigable month grid, **Tools →
Calendar…**) are separate boxes. The date-time strings used to live in the
calendar box; they moved to the clock so each box does one thing. See
`crates/vix-calendar-panel/spec/index.md`.

## As implemented in Vix

`clock_panel` is pure logic over `jiff` and `time_zone_model`: the string
builders (`local_clock`, `local_datetime`, `utc_iso`, `iso_week_date`,
`datetime_at_offset`, `active_zone_datetime`) and a `Clock` row model
(`rows`, `up`/`down`/`select`, `selected_value`). The host owns the `Visible::CLOCK` flag, the
`Clock` state, the `tools.clock` toggle, key/mouse routing, and `draw_clock`. See
`crates/vix-clock-panel/spec/index.md`.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Fixed-offset clocks use `checked_mul`. An offset jiff rejects is an input error: `datetime_at_offset` and `active_zone_datetime` return `Result<String, ClockError>` (`ClockError::OffsetOutOfRange { figure }`) instead of showing UTC or clamping, and `Row::value` is that `Result`, so the host shows a row that cannot be computed as unknown ("—") and never inserts it. A test checks that every zone in the time-zone table has a valid offset. Row selection uses `checked_*`.

| ID | Failure | Declared fallback | Why it is correct |
| -- | ------- | ----------------- | ----------------- |
| `CLK-F1` | A row step goes below the first or past the last row | the first or last row | Row selection stops at either end, like a list. |
