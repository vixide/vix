# vCard Panel

Displays a single contact's vCard as a table of labelled fields. Opened from the
[contact browser](../vix-contact-panel/spec/index.md) (Enter or click a contact).

## As implemented in Vix

**Status:** Shipped. The `vix-vcard-panel` crate turns a parsed `Vcard` (from
`vix-vcard-parser`) into `(label, value)` rows and holds the row-selection +
scroll state; pure data. The host renders the overlay and inserts a chosen value.

**Rows.** Property names map to friendly labels (`FN`→Name, `EMAIL`→Email,
`TEL`→Phone, `ADR`→Address, `ORG`→Organization, `URL`→URL, `BDAY`→Birthday,
`NOTE`→Note, …); `TYPE` parameters are appended (e.g. `Email (work)`); structured
`N`/`ADR` components are flattened to a comma-separated line. Binary/bookkeeping
properties (PHOTO, LOGO, KEY, PRODID, REV, UID) are omitted. The panel title is
the contact's display name.

| Key / action  | Effect                                          |
| ------------- | ----------------------------------------------- |
| `↑` / `↓`     | Move the highlight                              |
| `PgUp`/`PgDn` | Move one page                                   |
| `Home`/`End`  | Jump to the first / last field                  |
| `Enter` / click | Insert the highlighted field's value           |
| `Esc`         | Return to the contact browser                   |

The field list has a scrollbar when it overflows.
