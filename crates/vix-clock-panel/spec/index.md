# Clock

The **clock box** shows the current date and time in several forms and inserts
any of them into the editor. Open it with **Tools → Clock…** (a toggle). Its logic
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
(`rows`, `up`/`down`/`select`, `selected_value`). The host owns `show_clock`, the
`Clock` state, the `tools.clock` toggle, key/mouse routing, and `draw_clock`. See
`clock_panel/spec/index.md`.
