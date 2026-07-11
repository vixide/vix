# QR Code

The **QR Code** command encodes text — the current selection, or the cursor's
line when nothing is selected — into a QR code and shows it in a read-only
overlay for scanning from the screen. Open it with **Tools → QR Code…** or the
command palette. The encoder lives in the `qr_tool` module (a thin wrapper over
the `qrcode` crate); the host renders the result.

## Behavior

- The source text is the trimmed selection if non-empty, otherwise the cursor's
  line. Empty input warns instead of opening.
- `qr_tool::render` returns the QR drawn with Unicode half-block characters
  (`qrcode`'s `Dense1x2` renderer), including a quiet zone.
- The overlay draws the art forced to **black-on-white** so it scans regardless
  of the active theme, centered, with an `Esc close` hint. **Esc**/**Enter**/`q`
  close it.

## As implemented in Vix

`qr_tool::render(&str) -> Option<String>` encodes the bytes and builds the
Unicode art (`None` on empty/too-long input). `App::open_qrcode` gathers the
selection or line, stores the rendered art in the `qrcode` overlay field, and
`ui::draw_qrcode` paints it. The `qrcode` dependency uses `default-features =
false` (only the built-in Unicode renderer; no image/svg backends).
