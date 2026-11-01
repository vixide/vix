# Calculator

Evaluate a typed formula, plus the Calculator dialog's editing state.

In menu "Tools", add menuitem "Calculator...".

This opens a calculator tool with a textarea; the user can type any formula, then "Run". 

Then the tool shows the calculation result, along with an "Insert" button that inserts the result into the editor text.

## Errors (T610)

`CalcError`; `eval` returns `Result<String, CalcError>` and `Outcome::Err` carries one:

| Variant | Meaning |
| --- | --- |
| `Eval { message }` | malformed formula, unknown variable or function, or a failure while evaluating |

The type is `#[non_exhaustive]` with a hand-written `Display`/`Error` (no `thiserror`). `Display` is the same text the earlier `String` error carried, so the translated `status.convert_failed` line is unchanged.
