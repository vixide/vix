# Ai Diff

Reviewable diff for AI text transforms (Annotate / Improve).

Rather than overwrite the buffer the moment the assistant replies, the host
builds a [`Review`] of the proposed change and lets the user accept or reject
it hunk by hunk. Each segment ([`Seg`]) is either unchanged context or a change the
user can toggle; [`Review::result`] reconstructs the final text from the
accepted choices. Pure data over `split_inclusive('\n')` lines, so the result
is an exact reconstruction (no line-ending guessing) and unit-testable.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Diff line ranges and the change highlight use `checked_*`; the old `saturating_*` calls and an `unwrap_or_default` are gone.

| ID | Failure | Declared fallback | Why it is correct |
| -- | ------- | ----------------- | ----------------- |
| `AID-F1` | A line range does not fit or lies outside the source | no lines | The range cannot name any real line. |
| `AID-F2` | The highlight moves above the first change | the first change | Moving up stops at the first change. |
