# Syntax highlighting

Highlighting is Tree-sitter. A grammar parses the buffer into a tree; a `.scm`
**query** maps captures in that tree to theme token names; the renderer paints
the resulting `(start, end, token)` ranges. Grammars are feature-gated (`lang-*`,
with `syntax-common` on by default and `syntax-all` for everything), and the
queries are embedded from the repo-root `langs/<lang>/highlights.scm` with
`rust-embed`.

## Injections

A query capture named `injection.content.<lang>` marks a region to highlight
with *another* language — code blocks in Markdown, `<script>` in HTML. Each
injected language gets its own parser and its own compiled query, built when the
buffer is created and reused for every highlight pass. An injection language
that is not compiled into the binary is skipped silently: the TUI owns the
screen, so nothing may write to stderr.

## Compiled queries are cached

**A compiled query is built once per (language, query source) and shared for the
life of the process** — `code::QUERY_CACHE`, a thread-local map holding
`Rc<Query>`.

Compiling a `.scm` query is expensive: tens of milliseconds for a large grammar.
It used to happen inside every `Code::new`, which meant every file opened, every
*preview tab the explorer scans past with an arrow key*, and every split paid it
— plus once more for each injected language. Caching took opening a 200-line
Rust file from **26 ms to 0.6 ms**, and a 5,000-line file from 41 ms to 15 ms
(`cargo bench --bench editor_ops -- editor/open`).

Two properties make the cache safe:

- A compiled query is **immutable** once built, so sharing one between buffers
  cannot leak state between them.
- The key includes the query **source**, not just the language name, so a custom
  highlight override (a theme supplying its own `.scm`) never collides with the
  bundled query.

It is thread-local rather than global because `Code` is single-threaded by
construction — its injection parsers are `Rc<RefCell<Parser>>` — which keeps the
cache lock-free. Background *parsing* (buffers over 50 KB reparse on a worker
thread) moves trees, not queries, so it is unaffected.

## Reparsing

Edits reparse incrementally: the tree is told what changed (`InputEdit`) and
Tree-sitter reuses the untouched subtrees. Buffers at or above
`ASYNC_PARSE_THRESHOLD` (50 KB) reparse on a background thread and install the
result when it arrives, with stale results rejected by an edit generation
counter, so typing in a large file never blocks on a parse.

See [`crates/vix-editor-core/spec/index.md`](../index.md) for the action catalog
and [`spec/test/index.md`](../../../../spec/test/index.md) for how the benchmark
that found the query cost is run.
