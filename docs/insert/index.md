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

## Lorem ipsum

**Insert → Lorem ipsum** inserts placeholder text — Words, a Sentence, or a
Paragraph — derived deterministically from a fixed passage.

## Date/Time

**Insert → Date/Time** inserts the current local time formatted as **ISO 8601**
(`YYYY-MM-DDTHH:MM:SS`), **RFC 3339** (with the UTC offset), or **Epoch** (Unix
seconds). For a navigable date picker, use **Tools → Calendar** instead.

See the specifications under [`spec/tools/insert/`](../../spec/tools/insert/).
