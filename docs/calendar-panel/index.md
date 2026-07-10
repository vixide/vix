# Calendar Panel

The calendar panel shows the current date and time alongside a navigable month
grid. You can page through months and insert dates, days, or timestamps into the
active editor.

## Opening the panel

Open it from the menu bar: **Tools → Calendar**. The panel appears as a modal
overlay over the editor.

## Date and time area

The top of the panel shows several date/time lines:

- The current localized local date and time, to seconds precision.
- The current UTC time in ISO 8601 format `YYYY-MM-DDTHH:MM:SSZ`.
- The current ISO 8601 commercial week date `YYYY-Www-D`, where the year is the
  ISO week-numbering year (which may occasionally differ from the Gregorian
  year), `Www` is the week number `01`–`53`, and `D` is the day of week from `1`
  (Monday) to `7` (Sunday).

These rows are left-aligned and use the foreground color. The clock keeps
ticking while the panel is open.

## Month grid

Below the date/time area is the month grid:

- A header showing `◀  Month Year  ▶`, where `◀` and `▶` are clickable
  month-navigation arrows.
- A Monday-first day grid for the month, with today highlighted.
- A help line at the bottom: `◀ ▶ month   Esc close`.

## Keybindings

| Key                 | Action                          |
| ------------------- | ------------------------------- |
| `←` / previous      | Go to the previous month        |
| `→` / next          | Go to the next month            |
| `Esc`               | Close the panel                 |

## Mouse

- Clicking `◀` or `▶` on the header row pages to the previous or next month.
- Clicking a date/time line inserts that exact string into the active editor.
- Clicking a day in the month grid inserts that date, formatted for the active
  locale: `%m/%d/%Y` for English, `%d.%m.%Y` for German, `%d/%m/%Y` for French,
  Spanish, and Welsh, and ISO `%Y-%m-%d` otherwise.

The panel stays open after an insert, so you can pick several values in a row. A
click outside the box closes it.

## Example

To insert today's date in your locale's format: open **Tools → Calendar** and
click today's highlighted cell in the month grid. To insert a full UTC
timestamp instead, click the ISO 8601 UTC line in the date/time area.

---

Vix™ and Vix IDE™ are trademarks.
