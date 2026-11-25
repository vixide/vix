# Affix

Add, drop, and toggle a `prefix`/`suffix` pair around `text` (a conventional
wrap: the prefix goes before the text, the suffix after).

```
use vix_affix::{add, drop, toggle};
assert_eq!(add("alfa", "bravo", "charlie"), "bravoalfacharlie");
assert_eq!(drop("bravoalfacharlie", "bravo", "charlie"), "alfa");
assert_eq!(toggle("alfa", "bravo", "charlie"), "bravoalfacharlie");
assert_eq!(toggle("bravoalfacharlie", "bravo", "charlie"), "alfa");
```

So [`add`] returns `prefix + text + suffix`; [`drop`] removes a leading
`prefix` and a trailing `suffix`; [`toggle`] drops them when `text` is already
wrapped (starts with `prefix` and ends with `suffix`), otherwise adds them.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). The room check for a prefix and a suffix uses `checked_add`. No fallbacks are declared: if the lengths do not fit, the text is not treated as wrapped.
