# Structural search & replace

Structural search & replace: a pattern language with metavariable holes,
matched against source text token-by-token. A pattern like
`if $COND { $$BODY }` matches any `if` statement regardless of how its
condition or body are formatted, and `$$BODY` captures the whole body as
one piece you can reuse in the replacement.

## Pattern syntax

- `$NAME` — a **single hole**: matches exactly one lexical unit — one
  token (an identifier, number, string, or operator), or a whole
  bracketed group (`(...)`, `[...]`, `{...}`) when the next source token
  opens one. It can never span *multiple* units on its own (`x > 0` is
  three units — an identifier, an operator, a number — so matching all of
  it needs `$$NAME`, not `$NAME`).
- `$$NAME` — a **multi hole**: matches a run of zero or more tokens,
  never crossing an unmatched closing bracket relative to where it
  started (so `f($$ARGS)` stops its capture at the `)` that closes `f(`,
  not some later one). Holes are matched **lazily** — as little as
  possible, extending only as far as the rest of the pattern requires — so
  a trailing `$$NAME` with nothing after it in the pattern matches an
  empty capture, not "everything to the end of the file".
- Everything else in the pattern is matched literally, ignoring
  whitespace/formatting differences from the source (`a+b`, `a + b`, and
  `a  +  b` all match the pattern `a + b`).
- A hole's name follows ordinary identifier rules (a letter or `_`, then
  letters/digits/`_`), so `$5` in a replacement template is literal text,
  not a reference to a hole named `"5"`.

A replacement template uses the same `$NAME`/`$$NAME` syntax; each
placeholder is substituted with the **original source text** the hole
captured (not a token reconstruction), so the replacement preserves
whatever formatting that piece already had.

## Matching approach: token/bracket-based, not tree-sitter

The wider T201 task envisioned reusing tree-sitter for structural
matching, falling back to bracket-balanced text matching when no grammar
is loaded. This crate implements only the latter, as the **primary**
mechanism rather than a fallback: genuine tree-sitter-based matching would
need per-grammar node-equivalence handling across the ~15 grammars Vix
loads — a substantially larger project than the token-based approach here,
which already delivers the core value (parameterized, bracket-aware
structural replace) for every language Vix supports, uniformly, without
per-language work.

The tokenizer is deliberately generic (identifiers, numbers, quoted
strings with `\`-escapes, bracket pairs, and runs of common operator
punctuation) rather than per-language — good enough to make holes
bracket-aware, not a validating lexer for any specific language.

No attempt is made at guaranteeing polynomial-time matching for
pathological patterns (many holes against adversarial input) — realistic
patterns against realistic file sizes are fast in practice, which is what
this optimizes for.

## As implemented in Vix

- `crate::{Pattern, Match, render_replacement}` (this crate) are pure,
  extensively unit-tested logic: no I/O, no `App` dependency.
  `Pattern::compile` parses a pattern string; `Pattern::find_all`/
  `find_from` search a source string; `render_replacement` substitutes a
  match's captures into a replacement template.
- The host (`App`, `src/app.rs`) provides two scopes, both reached from
  the same two-prompt sequence (pattern, then replacement):
  - **Buffer** (**Edit → Structural Replace…**, `edit.structural_replace`)
    — an interactive step-through mirroring `QueryReplace`'s own
    `y`/`n`/`!`/`q` session exactly (a sibling `StructuralReplace`
    session/field, not the same one, since its matcher is a `Pattern`
    rather than a `Regex`). Scoped to the active **selection** when one
    exists at the time the replacement prompt is accepted, otherwise the
    whole buffer.
  - **Workspace** (**Edit → Structural Replace in Workspace…**,
    `edit.structural_replace_workspace`) — computes a plan across every
    file under the workspace root and reuses `ReplaceConfirm`'s existing
    preview-and-confirm step verbatim (the same one workspace
    search-and-replace uses), rather than building a second confirm UI.
