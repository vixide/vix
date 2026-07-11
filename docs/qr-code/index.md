# QR Code

The QR Code generator encodes text into a scannable QR code drawn with Unicode
block characters, shown in a read-only overlay you can scan straight from the
screen.

## Using it

Open it from **Tools → QR Code…** or the command palette. The encoded text is the
current selection if you have one; otherwise it is the cursor's line. (Selecting a
URL and opening QR Code is the common case.) If there is nothing to encode, Vix™
says so instead of opening.

The overlay draws the QR forced to black-on-white so it scans regardless of your
theme, with a quiet-zone border. Press **Esc**, **Enter**, or **q** to close it.

## Notes

The encoder is a thin wrapper over the `qrcode` crate (Unicode renderer only; no
image or SVG backends are compiled in). See the specification at
`crates/vix-qr-tool/spec/index.md`.

---

Vix™ and Vix IDE™ are trademarks.
