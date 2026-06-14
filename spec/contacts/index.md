# Contacts

The **Contacts** browser shows the vCard files in a directory and lets you read
a chosen contact's fields — name, emails, phones, organization, notes, and more.
Open it with **Tools → Contacts…**. It is a two-stage overlay: a list of
contacts, and, on top of it, a single-contact view of one parsed vCard.

The contacts feature reads `.vcf` files only; it never writes them. Parsing
follows vCard 4.0 ([RFC 6350](https://www.rfc-editor.org/info/rfc6350/)).

## Opening

**Tools → Contacts…** scans the contacts directory for files ending in `.vcf`
(case-insensitive) and lists them. The directory is resolved like this:

- If the `contacts_dir` setting is non-empty, that path is scanned.
- Otherwise the **workspace root** is scanned.

See `spec/configuration/index.md` for the `contacts_dir` setting. If the scan
finds no vCard files, a status note reports
`No vCard files in the contacts directory` and the browser opens empty.

Each `.vcf` file is read and parsed, and its **display name** is taken from the
vCard `FN` (or a `Given Family` derived from `N`). A file that yields no usable
name — i.e. would be `(unnamed)` — falls back to its filename stem. Contacts are
then sorted case-insensitively by display name.

## Contact browser

The browser is a single-column table of contact display names. One row is
highlighted; the highlight is what `Enter` or a click acts on. The list scrolls
when it has more contacts than fit, and shows a scrollbar when it overflows.

| Key / action    | Effect                                      |
| --------------- | ------------------------------------------- |
| `↑` / `↓`       | Move the highlight up / down one row        |
| `PgUp` / `PgDn` | Move one page                               |
| `Home` / `End`  | Jump to the first / last contact            |
| `Enter` / click | Open the highlighted contact's vCard view   |
| `Esc`           | Close the browser                           |

Clicking a visible row both selects it and opens its vCard view in one action.

## vCard view

Opening a contact reads its `.vcf` file again and parses it, then shows the
result as a table of labelled fields. The panel title is the contact's display
name. This view sits **above** the browser, so closing it returns you to the
list rather than dismissing the feature.

Property names map to friendly labels, and any `TYPE` parameters are appended in
parentheses:

| vCard property | Label          |     | vCard property | Label        |
| -------------- | -------------- | --- | -------------- | ------------ |
| `FN`           | Name           |     | `URL`          | URL          |
| `N`            | Name (full)    |     | `IMPP`         | IM           |
| `NICKNAME`     | Nickname       |     | `BDAY`         | Birthday     |
| `ORG`          | Organization   |     | `ANNIVERSARY`  | Anniversary  |
| `TITLE`        | Title          |     | `NOTE`         | Note         |
| `ROLE`         | Role           |     | `CATEGORIES`   | Categories   |
| `EMAIL`        | Email          |     | `GEO`          | Location     |
| `TEL`          | Phone          |     | `TZ`           | Time zone    |
| `ADR`          | Address        |     | `LANG`         | Language     |

For example, `EMAIL;TYPE=work:…` is labelled **Email (work)** and
`TEL;TYPE=work,voice:…` is **Phone (work/voice)**. Any property without a mapped
label keeps its raw (uppercased) name.

Structured values are flattened for display: the `N` (`Family;Given;…`) and
`ADR` (`PO;Ext;Street;Locality;Region;Postal;Country`) components are joined into
a single comma-separated line, with empty components dropped. Other values have
embedded newlines collapsed to spaces so each field is one row.

Binary and bookkeeping properties are omitted entirely: `PHOTO`, `LOGO`,
`SOUND`, `KEY`, `PRODID`, `REV`, and `UID`.

| Key / action    | Effect                                       |
| --------------- | -------------------------------------------- |
| `↑` / `↓`       | Move the highlight up / down one row         |
| `PgUp` / `PgDn` | Move one page                                |
| `Home` / `End`  | Jump to the first / last field               |
| `Enter` / click | Insert the highlighted field's value         |
| `Esc`           | Return to the contact browser                |

Pressing `Enter` (or clicking a field) inserts that field's value into the
active editor at the cursor, so you can pull an email address or phone number
straight into your text. Inserting an empty value does nothing. The field list
shows a scrollbar when it overflows.

## How vCards are parsed

The parser is small, pure, and dependency-free. It reads vCard text and produces
a flat list of properties, each a `(name, params, value)` triple, handling the
parts of the grammar that matter for displaying a contact:

- **Line unfolding** — per RFC 6350, a content line that begins with a space or
  tab is a continuation of the previous line; the leading whitespace is removed
  and the text appended. A folded `FN:Very Long\n  Name` reads as
  `Very Long Name`.
- **The `name;PARAM=value:VALUE` shape** — the text up to the first `:` is split
  on `;`. The first segment is the property name; the rest are parameters.
- **Group prefixes** — a leading group like `item1.EMAIL` is stripped to
  `EMAIL`, and the name is uppercased.
- **Parameters** — `KEY=value` segments become `(KEY, value)` pairs with the key
  uppercased. A comma-separated parameter list such as `TYPE=work,voice` is split
  into one pair per value. A legacy bare type (`TEL;WORK:…`) is recorded as a
  `TYPE` parameter.
- **Value unescaping** — the escapes `\\`, `\n` / `\N`, `\,`, and `\;` are
  decoded in the value; an unknown escape keeps its backslash.
- **Dropped lines** — `BEGIN`, `END`, and `VERSION` lines are never included in
  the property list.

The parser exposes lookups used by the panels: `value(name)` and `get(name)`
(first match, case-insensitive), `all(name)` (every match), `Property::param`
(first value for a parameter key), `Property::types` (the `TYPE` values joined
with `/`), and `display_name` (the `FN`, else `Given Family` from `N`, else
`(unnamed)`).

## As implemented in Vix

**Status:** Shipped. The feature spans three pure-data crates plus host glue in
`src/app.rs` and `src/ui.rs`.

- **`vix-vcard-parser`** — the vCard 4.0 / RFC 6350 parser. `parse(text)`
  returns a `Vcard` of `Property { name, params, value }` after unfolding,
  group-prefix stripping, parameter parsing, and value unescaping. No IO. See
  `vix-vcard-parser/spec/index.md`.
- **`vix-contact-panel`** — the contact list. Holds `Contact { name, path }`
  rows and the selection + scroll state (`up`/`down`, `page_up`/`page_down`,
  `select_index`, `ensure_visible`, `selected_path`). See
  `vix-contact-panel/spec/index.md`.
- **`vix-vcard-panel`** — the single-vCard view. Turns a parsed `Vcard` into
  `(label, value)` rows (label mapping, `TYPE` suffixing, `N`/`ADR` flattening,
  hiding of binary/bookkeeping properties) and tracks selection + scroll;
  `selected_value` is what gets inserted. See `vix-vcard-panel/spec/index.md`.

The host owns the filesystem work and wiring: `open_contacts` resolves the
directory (`settings.contacts_dir`, else the workspace root), reads each `.vcf`,
computes display names, sorts, and opens the `ContactPanel`; `open_selected_vcard`
re-reads the chosen file and opens the `VcardPanel`; key/mouse handlers
(`contacts_key`/`contacts_mouse`, `vcard_key`/`vcard_mouse`) route input; and
`insert_selected_vcard_value` inserts a chosen field into the editor. The menu
entry is **Tools → Contacts…** (`tools.contacts`).
