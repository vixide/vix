# Color converter

Parse and convert colors between HEX, RGB, and HSL. Module `color_converter_tool`.

In menu "Tools", add menuitem "Color Converter..."

Convert colors between HEX, RGB, and HSL formats instantly. Enter any value and all three formats update in real time — ideal for CSS development and design handoff. Supports shorthand hex (#rgb) and percentage HSL. Everything runs locally in your browser with no upload or tracking.

## How to use this tool

- Enter a HEX, RGB, or HSL value in any of the three fields.
- All other formats update instantly.
- Copy the value you need for your CSS or design tool.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Hex, RGB and HSL parsing use `chars`, `get` and `checked_*`; the three fields are reached by destructuring, with `Field::next`/`Field::previous` for focus wrapping. Non-ASCII hex text is `None` whatever its byte length. No fallbacks are declared.
