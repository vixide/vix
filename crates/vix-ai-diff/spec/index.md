# Ai Diff

Reviewable diff for AI text transforms (Annotate / Improve).

Rather than overwrite the buffer the moment the assistant replies, the host
builds a [`Review`] of the proposed change and lets the user accept or reject
it hunk by hunk. Each segment ([`Seg`]) is either unchanged context or a change the
user can toggle; [`Review::result`] reconstructs the final text from the
accepted choices. Pure data over `split_inclusive('\n')` lines, so the result
is an exact reconstruction (no line-ending guessing) and unit-testable.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Diff line ranges use `checked_add` and `get`; the old `saturating_*` calls and an `unwrap_or_default` are gone. `Review::from_texts` returns `Result<Option<Review>, ReviewError>`: `Ok(None)` still means the texts are identical, and a range the diff library names outside the text is `ReviewError::RangeOutsideText { figure }` instead of "no lines".

| ID | Failure | Declared fallback | Why it is correct |
| -- | ------- | ----------------- | ----------------- |
| `AID-F2` | The highlight moves above the first change | the first change | Moving up stops at the first change. |
