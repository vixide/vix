# Vix Calendar

vix-calendar crate

Calendar box:

- Datetime area
  - Current localized local date and current localized local time using seconds precision.
  - Current UTC time in ISO 8601 format `YYYY-MM-DDTHH:MM:SSZ`
  - Current ISO 8601 commercial week date `YYYY-Www-D` — the ISO week-numbering
    year (which may occasionally differ from the Gregorian year), `Www` the week
    number `01..53`, and `D` the day of week `1` (Monday) .. `7` (Sunday)
  - Blank spacer line
- Calendar month area
  - an in-house Monday-first day grid computed with `jiff`, highlighting today
  - right arrow -> next month
  - left arrow -> previous month

Constraints:

- No icon before date/time rows — the first line is just 2026-06-08 14:23:01 (the 🕐 stays only in the panel title). All datetime rows are left-aligned.

- All datetime rows use the foreground color (Span::raw) not theme::dim(), so all datetime rows match the other datetime rows.
