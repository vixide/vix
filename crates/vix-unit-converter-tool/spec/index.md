# Unit Converter

Convert a number between physical units, plus the Unit Converter dialog's
state. This is module `unit_converter_tool`.

- menu "Tools"
  - submenu "Convert"
    - menuitem "Unit Converter..."

The user sees a text field for typing any numbers, and a select box for choosing the input units, and a select box for choosing the output units.

When the user types numbers, the output text automatically updates.

There is an "Insert" button. When the user clicks the button, then insert the text and unit into the editor.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Cycling a unit uses `checked_add` and `checked_rem_euclid`. `Converter::cycle` returns `Result<(), ConverterError>`: a unit index (a public field) already past the last unit is `ConverterError::OutOfRange { figure, len }` and a step that does not fit is `ConverterError::Overflow { figure }`; the selection is left unchanged, never moved to the first unit. `insert_text` is empty when there is nothing to convert, including an unknown unit, and no longer relies on a documented panic.
