# Calendar

Calendar box: the navigable month grid. It inserts a chosen date into
the editor. Open it with **Tools → Calendar…** (a toggle). Its logic lives in the
`calendar_panel` crate; the host renders the box and routes input.

Live date/time strings are **not** part of the calendar — they live in the clock
box (**Tools → Clock…**, see `crates/vix-clock-panel/spec/index.md`). The calendar is just the
month area.

## Layout

- A month header `◀  Month Year  ▶`. The `◀` / `▶` glyphs are clickable
  month-nav arrows (column 0 and column 20 of the header row). The month
  name is translated per `settings.locale` (`calendar.month_01`..
  `calendar.month_12` in `crates/vix-i18n/locales/calendar.yml`), not `jiff`'s
  locale-invariant `%B`.
- A day grid that starts on Monday: a weekday header row, then up to six
  week rows. The **selected day** (the keyboard cursor) is
  reverse-highlighted; **today** is marked only when the displayed month is
  the current month.

**Planned (T797):** a first-weekday setting, any of the seven days and
independent of the UI language. Settings saved without it keep the Monday
start. The weekday header, where each date sits and what a mouse click
selects must all come from the same grid, so the keyboard and the mouse pick
the same date whatever the first weekday is. See the
[worldwide acceptance criteria](../../../spec/worldwide/index.md#acceptance-criteria-for-worldwide-editing).

## Navigation

- **← / →** move the selected day by one day; **↑ / ↓** by one week. The
  displayed month follows the selection.
- **Ctrl + arrows** change the month; **Ctrl + Shift + arrows** change the year.
- **Enter** inserts the selected date (locale-formatted) and closes the box.
- **Esc** / **q** close the box. Opening always snaps back to the present month.

## Mouse

- Clicking `◀` / `▶` pages the month.
- Clicking a day cell (cells are three columns wide; week rows start at row 2)
  inserts that date, locale-formatted. The box stays open so several days can be
  picked.
- A click outside the box closes it.

## As implemented in Vix

`calendar_panel` is pure logic over `jiff`: `now_local`, the `Calendar`
navigation state (`move_days`/`move_months`/`move_years`, `selected`,
`selected_formatted`, `reset`, `title`), and `month_grid` (a `MonthGrid` of
optional day numbers, with `today` set only for the current month). The host owns
the `Visible::CALENDAR` flag, the `Calendar` state, the `tools.calendar` toggle, key/mouse
routing, and `draw_calendar`. See `crates/vix-calendar-panel/spec/index.md`.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Moving the selection builds a `Span` with `try_*` and adds it with `checked_add`; the old `saturating_*` calls are gone, and the week grid uses `get_mut`. A span the date library refuses (for example `i64::MAX` days) is an input error: `move_days`, `move_months`, `move_years`, `next_month` and `prev_month` return `Err(MoveError::InvalidSpan { figure })` and leave the selection alone. A valid span that would leave the calendar's own limits stops at the current selection, the declared boundary of navigation.

| ID | Failure | Declared fallback | Why it is correct |
| -- | ------- | ----------------- | ----------------- |
| `CAL-F1` | A valid span would take the date past the calendar's limits | the selection stays where it is | The limits are the end of the calendar, like the last row of a list. |
| `CAL-F2` | A week column count does not fit | the end of the week | The grid has seven columns. |
