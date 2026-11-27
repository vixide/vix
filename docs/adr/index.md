# Architecture Decision Records

An architecture decision record (ADR) writes down one decision that shapes the
whole codebase: what was decided, when, what forced it, and what it costs. Each
record here was written after the fact, in November 2026, from the repository's
own history: the commits that made the change, the rules in
[`AGENTS.md`](../../AGENTS.md) and [`agents/`](../../agents/), the
cross-cutting specs in [`spec/`](../../spec/index/index.md), and
[`CHANGELOG.md`](../../CHANGELOG.md). Every claim of fact names the file or
commit it comes from. Where the history does not record *why* something was
chosen, the record says so instead of supplying a reason.

| # | Decision | Date | Status |
| - | -------- | ---- | ------ |
| [1](0001-rust.md) | Write Vix in Rust | 2026-06-08 | Accepted |
| [2](0002-ratatui.md) | Build the terminal UI on `ratatui` and `crossterm` | 2026-06-08 | Accepted |
| [3](0003-no-unsafe.md) | Forbid `unsafe` code | 2026-06-09 | Accepted |
| [4](0004-crate-layout.md) | A Cargo workspace of focused crates under a thin App shell | 2026-07-10 | Accepted (the third layout; supersedes a single crate of modules) |
| [5](0005-feature-flags.md) | Optional capabilities as default-on Cargo features | 2026-10-18 | Accepted (reverses "wired in means always compiled") |

## Format

Each record has the same parts:

- **Status**: Accepted, Superseded or Reversed, with the record or commit that
  changed it.
- **Date**: the date of the first commit that put the decision into the tree.
- **Context**: what the project looked like and what pushed for a decision.
- **Decision**: what was decided, in the words of the files that enforce it.
- **Consequences**: what followed, good and bad, including later changes.
- **Enforced by**: the files and checks that keep the decision true today.

Commit hashes are abbreviated; `git show <hash>` shows the full change.

## Adding a record

Add a record when a change sets a rule every later change must follow, or
reverses one. Number it after the last record, add it to the table above and
to [`docs/SUMMARY.md`](../SUMMARY.md), and cite the files and commits it
rests on. Do not edit an accepted record's Decision to describe a new one:
write a new record and set the old one's Status to Superseded.
