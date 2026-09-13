# Tools

The **Tools** menu collects a set of small, single-purpose utilities alongside
the editor's larger panels — quick converters and testers you open, use for a
moment, and close. Most work one of two ways: a **Tools → Convert** (or
**Checksum**) leaf transforms the selection (or, with nothing selected, the
whole buffer) in place; a `…` leaf opens a small dialog with its own fields and
an **Insert** or **Run** button. None of the tools below has a default
keybinding or command-palette entry — open them from the menu.

## Number base converter

Converts an integer between decimal, hexadecimal, binary, and octal. The
number is parsed with an auto-detected radix — `0x`/`0X` for hex, `0b`/`0B` for
binary, `0o`/`0O` for octal, otherwise decimal — tolerating surrounding
whitespace, a leading `+`/`-` sign, and `_` digit separators. Each menu item
re-renders the value in one target base with its conventional prefix.

Open it from **Tools → Convert → Number → Dec / Hex / Bin / Oct**. It runs on
the selection, or the whole buffer if nothing is selected.

Example: select `255` and choose **Hex** → `0xff`. Select `0xff` and choose
**Dec** → `255`.

See `crates/vix-base-tool/spec/index.md`.

## Base16 color themes

Not a Tools-menu item at all — `vix-base16` is a set of built-in color
*themes*, not a converter. It ships about a dozen well-known
[base16](https://github.com/chriskempson/base16) palettes (each a 16-color
scheme, `base00`–`base0F`: `base00`–`base07` running dark to light for
backgrounds and foregrounds, `base08`–`base0F` as accents), turned into full
Vix themes and merged into the bundled set. They show up under **View →
Theme**, named `Base16 …` to avoid colliding with the hand-authored JSON
themes in `themes/`.

See `crates/vix-base16/spec/index.md`.

## Base64

Encodes or decodes text as standard Base64 (RFC 4648, `+`/`/` alphabet with
`=` padding). Decoding is lenient about whitespace and newlines — the common
case when pasting wrapped Base64 — and reports an error if the input isn't
valid Base64 or doesn't decode to UTF-8 text.

Open it from **Tools → Convert → Base64 → Encode** or **Decode**. It runs on
the selection, or the whole buffer if nothing is selected.

Example: select `hello` and choose **Encode** → `aGVsbG8=`. Select `aGVsbG8=`
and choose **Decode** → `hello`.

See `crates/vix-base64-tool/spec/index.md`.

## Calculator

Opens a dialog with a text field for typing any arithmetic/boolean formula. Press
**Run** to evaluate it (via the `evalexpr` expression engine) and see the
result, then **Insert** to drop the result into the editor at the cursor.

Open it from **Tools → Calculator…**.

See `crates/vix-calculator-tool/spec/index.md`.

## Checksum

Computes a cryptographic or non-cryptographic digest of text, rendered as
lowercase hex. The crate's own spec documents SHA-256 and SHA-512; the menu
also wires up MD5 and CRC32 digests through the same tool.

Open it from **Tools → Checksum → SHA-256 / SHA-512 / MD5 / CRC32**. It runs
on the selection, or the whole buffer if nothing is selected.

See `crates/vix-checksum-tool/spec/index.md`.

## Color converter

A dialog with three fields — HEX, RGB, and HSL. Type a value into any one of
them and the other two update instantly, so you can copy whichever format you
need. Supports shorthand hex (`#rgb`) and percentage HSL.

Open it from **Tools → Color Converter…**.

See `crates/vix-color-converter-tool/spec/index.md`.

## JWT Decode

Decodes a JSON Web Token's header and payload into pretty-printed JSON. A JWT
is `header.payload.signature`, each part Base64URL-encoded (no padding); this
tool decodes the first two parts only — the signature is left untouched and is
**not verified** (there's no key to check it against). The output is prefixed
with an explicit warning that the claims are unverified/untrusted, and calls
out `alg: none` (an unsigned token) specifically when it sees one.

Open it from **Tools → Convert → JWT Decode**. It runs on the selection, or
the whole buffer if nothing is selected.

See `crates/vix-jwt-tool/spec/index.md`.

## Pomodoro

A work/break countdown timer. The dialog defaults to 25 minutes and is
adjustable before you start it. Click **Start** to close the dialog and begin
counting down in the background — it keeps running even while the dialog is
closed. Click **Stop** to cancel the countdown and reset back to the time it
started from. When the timer reaches zero, Vix™ shows a "Pomodoro break: 5
minutes" alert with a **Cancel** button; the break counts down on its own and
the alert closes automatically when it reaches zero.

Open it from **Tools → Pomodoro…**.

See `crates/vix-pomodoro-tool/spec/index.md`.

## Regex tester

A live regular-expression tester with two fields — the pattern and the
subject text — that shows matches (or the compile error) as you type. It
matches using the same `regex` crate engine as Vix's own Find/Replace, so it
is the same regex flavor you'd use there: no backreferences or lookaround.

Open it from **Tools → Regex Tester…**.

See `crates/vix-regex-tool/spec/index.md`.

## Unit converter

A dialog with a numeric field and two unit dropdowns (from/to). Typing a
number updates the converted output automatically; an **Insert** button drops
the number and its unit into the editor. Units are grouped into five
dimensions — length, mass, temperature, digital data size, and time —
and conversion is only defined within one dimension (there's no length-to-mass
conversion).

Open it from **Tools → Convert → Unit Converter…**.

See `crates/vix-unit-converter-tool/spec/index.md`.

## URL

Percent-encodes or decodes text. Encoding escapes every byte outside the RFC
3986 "unreserved" set (`A–Z a–z 0–9 - _ . ~`) — so a space becomes `%20`, never
`+` — making the result safe to drop into any part of a URL. Decoding reverses
`%XX` escapes and reports an error if the result isn't valid UTF-8.

Open it from **Tools → Convert → URL → Encode** or **Decode**. It runs on the
selection, or the whole buffer if nothing is selected.

Example: select `hello world` and choose **Encode** → `hello%20world`. Select
`hello%20world` and choose **Decode** → `hello world`.

See `crates/vix-url-tool/spec/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
