# Fuzzing

Coverage-guided fuzzing over Vix's **pure cores** — the parsers and text
transforms that take input Vix did not write: a file a user opened, bytes a
language server sent, a half-typed table, a buffer full of conflict markers.

Unit tests check what a function does with the input we thought of. Fuzzing
checks that it does not *panic* on the input we did not — and in an editor a
panic is data loss, because the buffer dies with the process.

This is a **separate workspace** (`fuzz/Cargo.toml` declares its own
`[workspace]`), so `cargo build` and `cargo check --workspace` at the repo root
never try to build targets that need nightly and sanitizer instrumentation.

## Running

```sh
cargo install cargo-fuzz                 # once
cargo +nightly fuzz list                 # the targets below
cargo +nightly fuzz run textops          # fuzz until you stop it (Ctrl+C)
cargo +nightly fuzz run textops -- -max_total_time=60   # or for a fixed time
cargo +nightly fuzz build                # just compile every target
```

A crash writes the input to `fuzz/artifacts/<target>/` and prints the path.
Reproduce and minimize it with:

```sh
cargo +nightly fuzz run textops fuzz/artifacts/textops/crash-<hash>
cargo +nightly fuzz tmin textops fuzz/artifacts/textops/crash-<hash>
```

Then turn the minimized input into a **unit test in the crate that owns the
code** — the fuzz target found it, but the regression belongs next to the
function, where it runs on every `cargo test`.

## Targets

| Target | Fuzzes | The invariant |
| ------ | ------ | ------------- |
| `textops` | Every whole-text transform and cursor-relative rewrite in `vix-textops` | No panic on any text/cursor pair; a returned cursor is always a valid char offset into the returned text (the host feeds it straight to `set_cursor`); `to_lf` is idempotent; `rot13` is its own inverse |
| `org_table` | `vix-org-table`: `parse`/`render`/`align`/`recalc`/`from_delimited`/`parse_tblfm` and the field/row motions | No panic for any buffer text and any (possibly out-of-range) cursor; `render` → `parse` round-trips without losing rows |
| `lsp_frame` | `vix-lsp-core`'s `Content-Length` decoder, fed in arbitrary-sized chunks | A hostile or truncated header leaves the decoder buffering rather than panicking or over-allocating |
| `vcard` | `vix-vcard-parser` (RFC 6350) and the accessors on the parsed card | Parsing third-party `.vcf` files is total; reading fields from a malformed card is total too |
| `conflict` | `vix-conflict-tool`'s marker finder and resolver | Returned offsets are ordered, in range, and on char boundaries — the resolver splices the buffer at them |
| `http_request` | `vix-http-client::parse_request` on `.http` buffers | Parsing is total; a request that parses has a method and a URL |
| `query_replace` | `vix-find-panel`: `matches`/`next_match`/`replace_all`/`replace_one`/`unescape`/`SearchBar::pattern`/`PathFilter` | No panic on any query/text/replacement-template triple, including multi-byte UTF-8; matches are ordered and in range; `replace_all`'s count agrees with `matches().len()` |
| `tabular_convert` | `vix-convert-tabular` (CSV/TSV/JSON core shared by all six CSV/TSV/JSON `Tools → Convert` crates) | Writing then reparsing is a *fixed point* — `write_csv(parse_csv(write_csv(rows))) == write_csv(rows)` (not "reparses to the exact original rows": the formula-injection guard on `=`/`+`/`-`/`@`/tab/CR-leading fields intentionally changes them, but is itself idempotent) |
| `structured_convert` | The four JSON⇄YAML/JSON⇄TOML `Tools → Convert` crates | Converting valid JSON to YAML/TOML and back reparses to an *equivalent* value (exact for strings/bools/null/structure; a tiny relative tolerance for numbers, since decimal-text float formatting isn't bit-exact — found on the first run: a 30-significant-digit float's last digit shifted) |
| `macro_tokens` | `vix-macros`: `decode_key`/`decode`, the `macros.toml` token parser | No panic on any token text (a user can hand-edit the file); a token that decodes re-encodes to a non-empty token |

## Adding a target

1. The code must be **pure** — text (or bytes) in, value out, no filesystem, no
   network, no terminal. If it is not, extract the pure part first; that is the
   house pattern anyway (see `agents/conventions.md`, "Pure-logic modules").
2. Add the crate to `fuzz/Cargo.toml`'s `[dependencies]` and a `[[bin]]` entry.
3. Write `fuzz_targets/<name>.rs`. Assert the *invariants*, not just the absence
   of a panic — a fuzz target that only calls the function finds crashes but
   misses wrong answers.
4. Document it in the table above.

## Notes

- `fuzz/artifacts/` and `fuzz/corpus/` are gitignored; a crash worth keeping
  becomes a unit test, not a committed corpus file.
- Fuzzing is not part of the CI gate or the MSRV promise (`cargo-fuzz` needs
  nightly). It is a tool for the person changing a parser.
- See [`spec/test/index.md`](../spec/test/index.md) for how fuzzing fits with the
  unit, integration, property, and benchmark layers.
- Improvement plan T007 also asked for a "modeline parsing" target
  (vim/emacs-style `-*- mode: … -*-` / `vim:` per-file settings comments). Vix
  has no such feature — no code parses a modeline anywhere in the workspace —
  so there is nothing pure to fuzz. Adding one would be a feature first (with
  its own spec), fuzzed after.
