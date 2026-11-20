# Worldwide

Vix's target is the whole world. It is not aimed at the United States or the
United Kingdom, or at English-speaking users, with everyone else handled as an
afterthought.

## Rule

Any standard, protocol, convention, locale, format, or similar choice Vix
adopts must give **equitable capabilities** to every country, region, and
federation worldwide. A feature that works fully for one place and only
partially, or not at all, for another is a defect, not a limitation to
document and move on.

## What this covers

- **Standards and protocols**: prefer international standards (ISO, Unicode,
  IETF, W3C, CLDR) over national or vendor-specific ones. Where a regional
  standard is added (for example a national calendar or address format), it
  sits beside the others as a peer, not as the default that others are measured
  against.
- **Locales**: language, script, text direction, plural rules, collation,
  number, date, time, currency, and measurement formats work the same way for
  every supported locale. No locale's data is a second-class copy of the
  English data.
- **Conventions**: week start, calendar system, time zone, name order, paper
  size, and keyboard layout are configurable and never assumed. The
  Gregorian calendar, a Monday or Sunday week start, a 12-hour clock, `MM/DD`
  ordering, and US-layout keys are defaults to override, not facts to build in.
- **Regions and federations**: countries, territories, and unions or
  federations of states are all first-class. Do not model the world as a list
  of nation states only, and do not hard-code one jurisdiction's law,
  holidays, or identifiers (tax IDs, phone numbers, postal codes, IBANs).
- **Text**: every user-facing string goes through `t!` (see
  [Localization](../../crates/vix-i18n/spec/index.md)); examples, sample data,
  and documentation use varied, realistic names, places, and formats.

## How to apply

- When adding a feature, ask which countries or regions it would not work for,
  and fix that before shipping, or record the gap as a task in `tasks.md`.
- When choosing between standards, pick the one that serves the most regions
  equitably; if none does, support more than one.
- Tests cover non-English, non-Latin, right-to-left, and non-US/UK inputs for
  anything that parses, formats, sorts, or displays locale-sensitive data.

## Acceptance criteria for worldwide editing

| Workflow | Required behavior | Evidence |
| --- | --- | --- |
| Navigate to a result | A workspace match, symbol, reference or diagnostic opens at the first scalar value of its target in the logical source line. Convert a protocol position using the encoding of the server that supplied it. | Select a result after a wide glyph, an emoji and a combining mark. Assert the cursor in the target buffer. |
| Resolve a save warning | Every language offered in the locale chooser has its own wording for an overwrite, an external-change conflict and an unrepresentable edit, including the choices the user can make. File names and error details survive interpolation. | Check direct catalog entries for each chooser language and exercise a warning with a non-Latin path. |
| Pick a calendar day | A user can set the first weekday independently of the UI language. Headers, dates and mouse targets use the same weekday order. Changing the first weekday preserves the selected date. | Select the same date by keyboard and by mouse under Monday, Sunday and Saturday starts, including a grid that spans six weeks. |

Implementation status and remaining work are tracked in [Run X](../../tasks.md#run-x--worldwide-editing).
