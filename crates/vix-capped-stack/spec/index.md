# Capped stack

Push an item onto a `Vec`, evicting the oldest entry first if that would
grow it past a cap.

**Status:** Shipped (Run H, T525). One pure function,
`push_capped<T>(stack: &mut Vec<T>, item: T, cap: usize)`.

## Why this crate exists

Every `vix-edit-*` crate's undo stack (`vix-edit-bytes`, `vix-edit-sql`,
`vix-edit-table`, `vix-edit-outline`, `vix-edit-value`) independently
hand-rolled the identical 4-line "push, then trim the oldest entry if over
the cap" idiom, each with its own `const HISTORY_CAP: usize = 200;` — real
duplication of one small, easy-to-get-subtly-wrong operation (off-by-one on
the comparison, evicting the wrong end), not domain-driven similarity.

Named for the operation it performs, not for undo specifically — nothing
here is undo-history-aware (each crate keeps its own `Snapshot` type and
`restore`/undo/redo semantics; two of the five had already converged on a
shared internal `restore()` helper, three still push/pop each stack
independently). This deliberately does **not** collide in name or purpose
with `vix-undo-store`, which is a different feature entirely: *persistent*,
per-file undo history saved to disk across sessions.

## Behavior

`push_capped` evicts **at most one** entry per call (index `0`, the
oldest) — it mirrors every original call site's own `if` (not `while`)
exactly, on the assumption every caller already holds the invariant "at or
under cap before this push," which every current caller does (each pushes
exactly one entry, then checks). A stack that somehow started more than one
entry over cap stays over cap after one call — this is not a general
"enforce the cap no matter the starting state" clamp, and none of the five
adopting crates needed one.

## Adoption

Each crate's own `push_undo` (or equivalent) body — `self.undo.push(item);
if self.undo.len() > HISTORY_CAP { self.undo.remove(0); }` — became `vix_
capped_stack::push_capped(&mut self.undo, item, HISTORY_CAP);`. Each
crate's own `Snapshot` type, `HISTORY_CAP` constant value, and undo/redo
method bodies are otherwise unchanged.
