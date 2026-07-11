# Base Tool

Convert an integer between decimal, hexadecimal, binary, and octal.

The input number is parsed with auto-detected radix — `0x`/`0X` hex,
`0b`/`0B` binary, `0o`/`0O` octal, else decimal — tolerating surrounding
whitespace, an optional leading sign, and `_` digit separators. Each function
re-renders it in one base (with the conventional prefix). Used by Tools →
Convert → Number via `App::transform_selection_or_buffer_try`.
