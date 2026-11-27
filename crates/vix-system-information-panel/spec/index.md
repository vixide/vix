# System Information Panel

A read-only snapshot of host system information, plus the panel's selection:
the row-selection and scroll state.

Vix's Tools menu offers a *System Information* panel: a scrollable table of
facts about the host — operating system, CPU, memory, swap, disks, uptime,
and the current environment — gathered once when the panel opens (a static
snapshot, not a live monitor). The user browses with the arrow keys (or the
mouse) and can insert any value into the active editor. This crate gathers the
data (via [`sysinfo`]) and tracks the highlighted row and scroll offset; the
host renders the table, maps clicks to rows, and inserts the chosen value.

The Memory section ends with *Vix resident memory*: the resident set size of
the Vix process itself (T830). `resident_memory()` reads it, for this one
process only, so it is cheap enough for the frame profiler (`VIX_PROFILE`) to
sample once a second; the row is left out where the platform does not report
it.

A fact the platform can't report is shown as *unknown* (a name, version,
hostname or uptime), shown as zero (load average), or left out (the CPU
counts, the Memory section, the Storage section) — the panel always opens.

## See also

- [file-information-panel spec](../../vix-file-information-panel/spec/) — shared info-panel behavior

## Checked arithmetic

The crate follows the `rust-checked-arithmetic` spec and, outside tests, denies `arithmetic_side_effects`, `integer_division`, `modulo_arithmetic`, `as_conversions`, `indexing_slicing` and `string_slice` (RCA-ENF-3). The uptime breakdown uses `checked_div` and `checked_rem`.

| ID | Failure | Declared fallback | Why it is correct |
| -- | ------- | ----------------- | ----------------- |
| `SYS-F1` | A division or remainder by a constant fails | `0` | The divisors are constants above zero, so this cannot happen; zero is the neutral answer. |
