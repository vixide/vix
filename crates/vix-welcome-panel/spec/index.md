# Welcome Panel

The first-run welcome screen's scroll state.

Vix shows this overlay the first time it runs (and on demand from **Help →
Welcome…**). The *text* lives in the host's i18n catalog (the `welcome.body`
locale key) so it is translatable; this crate is pure state — it holds the
lines the host hands it and tracks the scroll offset. The host renders the
visible window with a scrollbar and forwards scroll keys.

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). Scrolling uses `checked_*` with declared fallbacks; the old `saturating_*` calls are gone.

| ID | Failure | Declared fallback | Why it is correct |
| -- | ------- | ----------------- | ----------------- |
| `WEL-F1` | A scroll subtraction goes below zero | `0` | A short text does not scroll, and moving up stops at the top. |
| `WEL-F2` | A scroll step does not fit | the last valid offset | Moving down stops at the end of the text. |
