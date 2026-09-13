# Calendar Panel

The calendar panel is a navigable month grid for inserting a chosen date into
the active editor.

Live date/time strings (local time, UTC, ISO week) are a **separate** box —
**Tools → Clock…**, see [`docs/clock/index.md`](../clock/index.md). They used
to live in the calendar panel; each box now does one thing.

## Opening the panel

Open it from the menu bar: **Tools → Calendar…** (`tools.calendar`, a toggle).
There is no default keybinding or command-palette entry; use the menu.

## Layout

- A month header `◀  Month Year  ▶`. The `◀` / `▶` glyphs are clickable
  month-navigation arrows.
- A Monday-first day grid: a weekday header row, then up to six week rows. The
  selected day (the keyboard cursor) is reverse-highlighted; today is marked
  only when the displayed month is the current month.

## Keybindings

| Key                     | Action                            |
| ------------------------ | ---------------------------------- |
| `←` / `→`                | Move the selected day by one day   |
| `↑` / `↓`                | Move the selected day by one week  |
| `Ctrl` + arrows          | Change the month                   |
| `Ctrl` + `Shift` + arrows | Change the year                   |
| `Enter`                  | Insert the selected date, close    |
| `Esc` / `q`              | Close without inserting            |

Opening the panel always snaps the selection back to the present month.

## Mouse

- Clicking `◀` / `▶` pages the month.
- Clicking a day cell inserts that date, locale-formatted (`%m/%d/%Y` for
  English, `%d.%m.%Y` for German, `%d/%m/%Y` for French/Spanish/Welsh, ISO
  `%Y-%m-%d` otherwise). The panel stays open, so you can pick several dates
  in a row.
- A click outside the box closes it.

## Example

To insert today's date in your locale's format: open **Tools → Calendar…** and
click today's highlighted cell (or just press **Enter** — it starts selected).

## Notes

Implemented by `vix-calendar-panel`; see
`crates/vix-calendar-panel/spec/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
