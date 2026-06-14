# Calendar

The **calendar box** shows a navigable month grid and inserts a chosen date into
the editor. Open it with **Tools → Calendar…** (a toggle). Its logic lives in the
`vix-calendar-panel` crate; the host renders the box and routes input.

Live date/time strings are **not** part of the calendar — they live in the clock
box (**Tools → Clock…**, see `spec/clock/index.md`). The calendar is just the
month area.

## Layout

- A month header `◀  Month Year  ▶`. The `◀` / `▶` glyphs are clickable
  month-nav arrows (column 0 and column 20 of the header row).
- A Monday-first day grid: a weekday header row, then up to six week rows. The
  **selected day** (the keyboard cursor) is reverse-highlighted; **today** is
  marked only when the displayed month is the current month.

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

`vix-calendar-panel` is pure logic over `jiff`: `now_local`, the `Calendar`
navigation state (`move_days`/`move_months`/`move_years`, `selected`,
`selected_formatted`, `reset`, `title`), and `month_grid` (a `MonthGrid` of
optional day numbers, with `today` set only for the current month). The host owns
`show_calendar`, the `Calendar` state, the `tools.calendar` toggle, key/mouse
routing, and `draw_calendar`. See `vix-calendar-panel/spec/index.md`.
