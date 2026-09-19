# Byte size

Format a byte count as a human-readable size (`16.0 KiB`).

**Status:** Shipped (Run H, T521). One pure function, `human_bytes(n: u64)
-> String`, plus its private `u64_to_f64` helper (a lossless `u64`→`f64`
conversion via the exact-per-32-bit-half trick, avoiding `as`'s precision
loss above 2^53).

## Why this crate exists

`vix-file-information-panel` (the Explorer's file-info panel) and
`vix-system-information-panel` (disk/memory stats) each hand-rolled an
identical copy of this function, including its doc comment — real
duplication, not domain-driven similarity, since both format the exact same
quantity (a file or disk size in bytes) the exact same way. Extracted here
so a future fix (a different rounding rule, a different unit table) lands
once instead of needing to be found and repeated in both places again.

Not folded into `vix-list-state` (the other small shared-formatting-style
crate both panels already depend on) — that crate's own spec explains why
it stays scoped to list-scroll arithmetic specifically; this is a distinct,
unrelated concern with its own small test suite.

## Behavior

- Below 1024 bytes: `"<n> B"` (no decimal).
- 1024 and above: scales through `B`/`KiB`/`MiB`/`GiB`/`TiB`/`PiB`, one
  decimal place, stopping at `PiB` rather than indexing past the unit table
  for an input larger than any real file could be.
- `u64::MAX` is a real, tested input (not just realistic sizes) — this is
  a pure function with no assumption its caller already bounded `n`.

## Adoption

Both consumer crates replaced their own `human_bytes`/`u64_to_f64` with a
plain re-export/delegation to this crate; their own public API (still named
`human_bytes` at each crate's own call sites) is unchanged.
