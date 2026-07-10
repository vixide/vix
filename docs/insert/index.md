# Insert

The **Tools → Insert** submenu inserts generated content at the cursor. (It was
previously named "Generate"; the UUID and ZID generators still live here, joined
by snippet and date helpers.)

## UUID

**Insert → UUID** offers RFC 4122 / RFC 9562 versions 1–8, labeled by what each
encodes (e.g. *4 = Random*, *7 = Time + Random*). Choosing one inserts a fresh
UUID string.

## ZID

**Insert → ZID** inserts a secure random lowercase-hex string of the chosen width:
128-bit (32 hex), 256-bit (64 hex), or 512-bit (128 hex).

## Markdown

**Insert → Markdown** drops in small Markdown templates: Headline 1/2/3, a Link, a
bullet List, a Table, and a Todos checklist.

## HTML

**Insert → HTML** drops in the HTML equivalents: `<h1>`–`<h3>` headlines, an `<a>`
link, a `<ul>` list, and a full `<table>` (thead / tbody / tfoot).

## SQL

**Insert → SQL** drops in ready-to-edit PostgreSQL statements: **Alter Role**,
**Create Extension** (a commented list of common extensions), **Create Function**
(an `updated_at()` trigger function), **Create User**, **Grant Create**, **Grant
Usage**, and **Create Table** (with an identity key, timestamps, an `updated_at`
trigger, and a trigram GIN index). The snippets use placeholders (`alice`,
`items`) meant to be edited after insertion.

## LaTeX

**Insert → LaTeX** drops in Org/LaTeX-style markup: headlines (`* `, `** `), a
link, **bold**/*italic*/_underline_, a table, Org planning lines (Deadline,
Scheduled), timestamps (plain, range, repeater), block constructs (Quote, Verse,
Center), and a Drawer. The placeholder text is meant to be edited after insertion.

## Org

**Insert → Org** drops in Org-mode building blocks: Title/Author keywords,
headlines, links and images, plain/ordered/check lists, a table, TODO/DONE
items, planning lines (Deadline, Scheduled), timestamps (plain, range, repeater),
a Drawer, and a Properties drawer. The Org submenu also holds the inline
**markers** — Tag `:`, Bold `*`, Italic `/`, Underline `_`, Strikethrough `+`,
Code `~`, Verbatim `=` — each of which toggles the marker around the selection
(e.g. selecting `text` and choosing Bold gives `*text*`; choosing it again
removes the markers).

**Insert → Begin-End** toggles an Org block around the selection — Comment,
Center, Quote, Verse (`#+BEGIN_QUOTE … #+END_QUOTE`).

## Lorem ipsum

**Insert → Lorem ipsum** inserts placeholder text — Words, a Sentence, or a
Paragraph — derived deterministically from a fixed passage.

## Date/Time

**Insert → Date/Time** inserts the current local time formatted as **ISO 8601**
(`YYYY-MM-DDTHH:MM:SS`), **RFC 3339** (with the UTC offset), or **Epoch** (Unix
seconds). For a navigable date picker, use **Tools → Calendar** instead.

See the specifications under [`spec/tools/insert/`](../../spec/tools/insert/).

---

Vix™ and Vix IDE™ are trademarks.
