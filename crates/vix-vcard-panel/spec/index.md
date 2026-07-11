# Vcard Panel

Display one parsed vCard as a table of labelled fields, plus the panel's
row-selection + scroll state.

[`Panel::open`] takes a parsed [`Vcard`](vix_vcard_parser::Vcard) and turns
its properties into friendly `(label, value)` [`Row`]s — mapping names to
readable labels, appending `TYPE` parameters (e.g. `Phone (work)`), and
flattening structured `N`/`ADR` values. Pure data; the host renders the rows
and inserts the selected value into the editor.

## See also

- [contact-panel spec](../../vix-contact-panel/spec/) — shared contacts model
