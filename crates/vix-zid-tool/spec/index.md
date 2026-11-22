# Tools: Insert: ZID

Generate a secure random ZID: a lowercase hexadecimal identifier. This is
module `zid_tool`.

- menu "Tools"
  - submenu "Insert"
    - submenu "ZID"
      - menuitem "128 bit = 32 hex" -> Generate secure random 128-bit 32-character hexadecimal lowercase string and insert it.
      - menuitem "256 bit = 64 hex" -> Generate secure random 256-bit 64-character hexadecimal lowercase string and insert it.
      - menuitem "512 bit = 128 hex" -> Generate secure random 512-bit 128-character hexadecimal lowercase string and insert it.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). The hex renderer's capacity hint uses `checked_mul`.

| ID | Failure | Declared fallback | Why it is correct |
| -- | ------- | ----------------- | ----------------- |
| `TOOL-F1` | The capacity hint `len * 2` does not fit | `0` | The capacity is only an allocation hint. |
