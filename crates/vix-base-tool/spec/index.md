# Base Tool

Convert an integer between decimal, hexadecimal, binary, and octal.

The input number is parsed with auto-detected radix — `0x`/`0X` hex,
`0b`/`0B` binary, `0o`/`0O` octal, else decimal — tolerating surrounding
whitespace, an optional leading sign, and `_` digit separators. Each function
re-renders it in one base (with the conventional prefix). Used by Tools →
Convert → Number via `App::transform_selection_or_buffer_try`.

## Errors (T610)

`BaseError`; `to_dec`/`to_hex`/`to_bin`/`to_oct` return `Result<String, BaseError>`:

| Variant | Meaning |
| --- | --- |
| `NoNumber` | no digits (empty input, or only a sign or radix prefix) |
| `Invalid(ParseIntError)` | digits invalid in the detected base, or overflowing `i128` |

The type is `#[non_exhaustive]` with a hand-written `Display`/`Error` (no `thiserror`). `Display` is the same text the earlier `String` error carried, so the translated `status.convert_failed` line is unchanged.
