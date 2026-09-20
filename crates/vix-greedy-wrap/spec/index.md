# Greedy wrap

Greedy word-wrap of a single paragraph to a column width, hard-breaking any
word longer than the width so no line ever overflows.

**Status:** Shipped (Run H, T526). One pure function:
`wrap_line(line: &str, width: usize) -> Vec<String>`.

## Why this crate exists

`vix-welcome-panel` (the first-run welcome overlay) and `vix-ai-panel` (the
AI chat transcript) each hand-rolled an independent greedy word-wrapper —
same core algorithm, but with a real, undocumented behavior difference: the
welcome panel left an over-long word (a long URL, say) to overflow the line,
while the AI panel hard-broke it character-by-character. Nothing suggested
this was deliberate (no doc explained why a welcome screen and a chat
transcript should wrap differently), so it was flagged rather than picked
apart silently — the user chose "always break over-long words" as the one
merged behavior. This crate is that merged implementation.

Not the same problem as `vix-textops::wrap`/`wrap_chunk`, which wrap the full
*editor buffer* (multi-line, cursor-aware, reflow-on-edit) — a harder, more
stateful problem this crate deliberately stays out of. `wrap_line` here
takes one already-known paragraph (no embedded newlines) and a width, and
returns its wrapped lines; the caller decides how to split a multi-line
source into paragraphs first (see `vix-welcome-panel::Panel::wrap_to`).

## API

```rust
assert_eq!(vix_greedy_wrap::wrap_line("the quick brown", 9), vec!["the quick", "brown"]);
// A word longer than width is hard-broken, not left to overflow.
assert_eq!(vix_greedy_wrap::wrap_line("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
// A blank line yields a single empty string; width 0 returns the line unchanged.
assert_eq!(vix_greedy_wrap::wrap_line("", 9), vec![""]);
assert_eq!(vix_greedy_wrap::wrap_line("anything", 0), vec!["anything"]);
```

## Consumers

- `vix-welcome-panel`: wrapping the welcome screen's source paragraphs to
  the render width.
- `vix-ai-panel`: wrapping AI chat turns to the render width.
