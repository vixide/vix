# Checksum tool

Compute SHA-256, SHA-512, MD5, and CRC-32 checksums of text, returned as lowercase hex. Module `checksum_tool`.

- menu "Tools"
  - submenu "Checksum"
    - menuitem "SHA-256"
    - menuitem "SHA-512"
    - menuitem "MD5"
    - menuitem "CRC32"

Convert the selected text (or if no selected text then the entire buffer) via checksum SHA-256, SHA-512, MD5, or CRC-32.

MD5 and CRC-32 are included for compatibility with legacy tooling that still
expects them, not because either is cryptographically sound — SHA-256/SHA-512
are the ones to reach for when integrity actually matters.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). The hex renderer's capacity hint uses `checked_mul`.

| ID | Failure | Declared fallback | Why it is correct |
| -- | ------- | ----------------- | ----------------- |
| `TOOL-F1` | The capacity hint `len * 2` does not fit | `0` | The capacity is only an allocation hint. |
