# Tutorial 8: HTTP Client & Tools Suite

Two unrelated but genuinely useful things live under the **Tools** menu: a
`.http`-style REST client for firing off real requests without leaving the
editor, and a shelf of small single-purpose converters and testers —
Base64, JWT decode, checksums, UUIDs, regex testing, format conversion, and
more. This tutorial works through both against the repository's demo
workspace, so every command below is something you can actually run.

```sh
cd examples/demo-workspace
vix .
```

## The HTTP client

Open `api.http` (`Ctrl+O`, or click it in the explorer). It's a plain-text
buffer in the "REST client" shape a request `.http` file always takes:
`METHOD url` on the first line, `Header: value` lines up to the next blank
line, then the body. Lines starting with `#` or `//` are comments — the top
of the file is comment lines explaining the shape, followed by one live
request:

```http
GET https://httpbin.org/get?workspace=demo
Accept: application/json
```

Below that, commented out, is a second example showing a `POST` with a JSON
body. [`httpbin.org`](https://httpbin.org) just echoes back whatever you send
it, so both are safe to fire repeatedly.

With the cursor anywhere in the buffer, send the active request: **Tools →
Send HTTP Request** (action `tools.http_send`, also reachable from the
command palette). Vix parses the request, sends it on a background thread
(so the UI stays responsive — the status line shows the URL while it's in
flight), and opens the response in a **new tab** as plain text: a status
line, the response headers, a blank line, then the body.

```text
HTTP/1.1 200 OK
content-type: application/json

{
  "args": {
    "workspace": "demo"
  },
  ...
}
```

A few things worth knowing before you rely on this:

- The URL must be **absolute** (`http://` or `https://`) — a bare path isn't
  recognized as a request line at all, and any other scheme is rejected
  before any network activity happens.
- An HTTP error status (4xx/5xx) is still a normal response and opens the
  same tab-with-status-line way. Only a transport failure — DNS, connection
  refused, TLS — shows up as an error message instead of a new tab.
- The method defaults to `GET` if you omit it, so `https://example.com` on
  its own is a valid request line.

Try the `POST` next: comment out the `GET` line (and its `Accept` header),
uncomment the three `POST` lines below it, then send again. The response
body's `"json"` field will echo back exactly what you sent.

This is the whole client — there's no history, no environment/variable
substitution, no collections. For the full spec (parser details, what counts
as a request line, the background-thread mechanics) see
[`docs/http-client/index.md`](../../http-client/index.md) and
[`crates/vix-http-client/spec/index.md`](../../../crates/vix-http-client/spec/index.md).

## Tools tour

Everything below lives under the **Tools** menu. Most of it works one of two
ways: a **Tools → Convert** (or **Checksum**) leaf transforms the current
**selection** — or, with nothing selected, the **whole buffer** — in place;
a `…` leaf opens a small dialog with its own fields. None of these have
default keybindings; open them from the menu or the command palette. The
full reference for every tool (including several this page skips) is
[`docs/tools/index.md`](../../tools/index.md) — this page goes deeper on a
representative subset, hands-on.

### Base64 and checksums

Select `hello` anywhere (in `notes.md`, say) and run **Tools → Convert →
Base64 → Encode** — the selection becomes `aGVsbG8=`. Select it again and
run **Decode** to get `hello` back.

**Tools → Checksum** works the same way, over four digests:

| Menu item | Action id | Digest |
| --- | --- | --- |
| SHA-256 | `tools.checksum.sha256` | cryptographic |
| SHA-512 | `tools.checksum.sha512` | cryptographic |
| MD5 | `tools.checksum.md5` | legacy compatibility only |
| CRC32 | `tools.checksum.crc32` | legacy compatibility only |

MD5 and CRC-32 are there for compatibility with older tooling that still
expects them, not because either is cryptographically sound — reach for
SHA-256/SHA-512 when integrity actually matters. Select some text and try
each; the selection is replaced with its lowercase-hex digest.

### JWT decode

**Tools → Convert → JWT Decode** (action `tools.convert.jwt`) decodes a JSON
Web Token's header and payload — the first two of its three
dot-separated, Base64URL parts — into pretty-printed JSON. It does **not**
verify the signature (there's no key to check it against), and says so.
Paste this well-known example token (the one used on jwt.io) into a scratch
buffer, select it, and decode it:

```text
eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c
```

The output:

```text
// WARNING: signature NOT verified — these claims are untrusted
// header
{
  "alg": "HS256",
  "typ": "JWT"
}

// payload
{
  "iat": 1516239022,
  "name": "John Doe",
  "sub": "1234567890"
}
```

A token whose header declares `"alg": "none"` (an unsigned token) gets a
second warning line calling that out specifically — a real footgun some
libraries have shipped bugs around.

### URL encode/decode

**Tools → Convert → URL → Encode / Decode** (`tools.convert.url.encode` /
`.decode`) percent-encodes or decodes the selection. Encoding escapes every
byte outside the RFC 3986 "unreserved" set (`A–Z a–z 0–9 - _ . ~`) — a space
becomes `%20`, never `+` — so the result is safe to drop into any part of a
URL. Select `hello world` and **Encode** → `hello%20world`; **Decode** it
back to get the space again.

### Generating IDs: UUID and ZID

**Tools → Insert → UUID** has one submenu item per UUID version (v1 through
v8 — unsortable-time, DCE security, MD5-name, random, SHA-1-name,
sortable-time, time+random, and the all-zero v8 placeholder). Put the cursor
where you want it and pick a version; it inserts the generated UUID at
point. **v4 (Random)** and **v7 (Time + Random)** are the two you'll reach
for day to day — v7 sorts chronologically by its leading bits, which v4
doesn't.

**Tools → Insert → ZID** is simpler: three menu items — **128 bit = 32
hex**, **256 bit = 64 hex**, **512 bit = 128 hex** — each generating a
secure-random lowercase hex string of that length and inserting it. Use it
wherever you need a random token or key and a UUID's dashes and version bits
aren't relevant.

### Regex tester

**Tools → Regex Tester…** opens a dialog with two fields — a pattern and a
subject text — and updates the match list (or shows the compile error) as
you type in either one. It uses the same `regex` crate as Vix's own
Find/Replace, so whatever you confirm here behaves identically in
`Ctrl+F`/`Ctrl+R` — including its limits: no backreferences, no lookaround.
Try pattern `\b\w+@\w+\.\w+\b` against a line of text containing an email
address and watch the match highlight update live.

### Format conversion

**Tools → Convert** has one submenu per format, each listing the formats it
converts *into*:

| From → To | Menu path |
| --- | --- |
| CSV → JSON | Tools → Convert → CSV → JSON |
| CSV → TSV | Tools → Convert → CSV → TSV |
| TSV → CSV / JSON | Tools → Convert → TSV → CSV / JSON |
| JSON → CSV / TSV / YAML / TOML | Tools → Convert → JSON → CSV / TSV / YAML / TOML |
| TOML → JSON | Tools → Convert → TOML → JSON |
| YAML → JSON | Tools → Convert → YAML → JSON |

CSV parsing follows RFC 4180 quoting; TSV is a plain tab split/join (no
escape mechanism, so tabs and newlines inside a field aren't supported).
Converting *to* JSON treats the first row as a header and emits one object
per remaining row; converting *from* JSON does the reverse, using the union
of every object's keys, in first-seen order, as the header row.

Try it on the real fixture data. Open `data/sample.csv`:

```csv
name,role,score
Alice,engineer,92
Bob,designer,81
Carol,engineer,88
Dave,manager,75
```

With nothing selected, run **Tools → Convert → CSV → JSON** — it acts on
the whole buffer since there's no selection, and replaces the CSV text with:

```json
[
  {
    "name": "Alice",
    "role": "engineer",
    "score": "92"
  },
  {
    "name": "Bob",
    "role": "designer",
    "score": "81"
  },
  ...
]
```

(Values come out as strings — the tool doesn't infer numeric types.) This
replaces the buffer in place, so don't save over the fixture: `Ctrl+Z`
undoes it, or just close without saving. `data/sample.tsv` holds the same
rows in tab-separated form if you want to try the TSV side instead.

### Markdown Preview

Open `notes.md` and run **Tools → Markdown Preview…** (`tools.markdown_preview`).
It renders the buffer to a read-only preview pane, scroll-synced to wherever
your cursor was in the source. Press `t` (or `T`) to open a table-of-contents
overlay built from the document's headings; `↑`/`↓`/`PageUp`/`PageDown`
navigate it, `Enter` jumps the preview to that heading and closes the TOC,
`Esc` closes just the TOC (back to the preview). `Esc` again leaves the
preview entirely.

### A few more worth knowing about

Three more Tools items are small enough to name without a full walkthrough:

- **Tools → QR Code…** (`tools.qrcode`) encodes the selection (or, with
  nothing selected, the cursor's line) into a QR code drawn with Unicode
  half-blocks in a read-only overlay — black-on-white regardless of your
  active theme, so it scans reliably. `Esc`/`Enter`/`q` close it.
- **Tools → Calculator…** (`tools.calculator`) evaluates any typed
  arithmetic/boolean formula on **Run**, then **Insert** drops the result
  at the cursor.
- **Tools → Convert → Unit Converter…** (`tools.convert.unit`) converts a
  number between units within one dimension (length, mass, temperature,
  digital data size, or time) as you type, with its own **Insert** button.

See [`docs/tools/index.md`](../../tools/index.md) for these three, the
number-base converter, the color converter, and Pomodoro — everything under
the Tools menu in one reference.

## Where to go next

- [`docs/http-client/index.md`](../../http-client/index.md) — the full HTTP
  client reference.
- [`docs/tools/index.md`](../../tools/index.md) — every Tools-menu utility,
  including the handful this page didn't cover.
- [`docs/command-palette/index.md`](../../command-palette/index.md) — every
  action above is also reachable by name from `Ctrl+P`.

---

**Previous:** [07 — The DB Workbench](../07-the-db-workbench/index.md)
**Next:** [09 — Make Vix Yours](../09-make-vix-yours/index.md)

---

Vix™ and Vix IDE™ are trademarks.
