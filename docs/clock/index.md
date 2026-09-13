# Clock

The clock box shows several live date/time strings and lets you insert any of
them into the editor with one keystroke or click.

## Opening the box

Open it from the menu bar: **Tools → Clock…** (`tools.clock`, a toggle — opening
it again closes it). There is no default keybinding or command-palette entry;
use the menu.

## Rows

The box lists four values, recomputed every render:

| Row      | Example                | Source                                    |
| -------- | ----------------------- | ------------------------------------------ |
| Local    | `2026-06-14 09:30:00`   | system local date-time, seconds precision  |
| UTC      | `2026-06-14T13:30:00Z`  | ISO 8601 UTC instant                       |
| ISO week | `2026-W24-7`            | ISO 8601 commercial (week) date            |
| *(zone)* | `2026-06-14 09:30:00`   | the active time zone's wall clock          |

The last row is labeled with the active time zone's name (set via **Tools →
Time Zone…**) and shows that zone's **standard** (non-DST) offset.

## Interaction

- **↑ / ↓** move the highlight; **Enter** inserts the highlighted value and
  closes the box.
- **Click** a row to insert it — the box stays open, so you can pick several
  values in a row.
- **Esc** / **q** close without inserting.

## Relationship to the calendar and Insert → Date/Time

The clock (live time strings) and the calendar (a navigable month grid,
**Tools → Calendar…**) are separate boxes — see
[`docs/calendar-panel/index.md`](../calendar-panel/index.md). **Insert →
Date/Time** (see [`docs/insert/index.md`](../insert/index.md)) is a third,
menu-only surface over the same formatting logic, for inserting ISO
8601/RFC 3339/Epoch without opening a box at all.

## Example

Open **Tools → Clock…**, arrow down to the UTC row, and press **Enter** to
insert `2026-06-14T13:30:00Z` at the cursor.

## Notes

Implemented by `vix-clock-panel`; see `crates/vix-clock-panel/spec/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
