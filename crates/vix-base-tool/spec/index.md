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

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Negating a parsed number uses `checked_neg`. A second sign inside the digits (`--170141183460469231731687303715884105728`) parsed to `i128::MIN`, whose negation does not fit; it is now `BaseError::OutOfRange` instead of a panic in debug builds and a wrapped value in release builds.
