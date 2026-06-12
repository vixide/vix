# vix-spellcheck

The `vix-spellcheck` crate.

**Status:** Shipped (first cut) — **View → Editor → Toggle Spellcheck** (or the
command palette / `view.spellcheck`) underlines misspelled words **in comments
and string literals only**, in red. Off by default; the choice persists via the
`spellcheck` setting.

## How it works

- The crate wraps the pure-Rust [`spellbook`] Hunspell checker. A `SpellChecker`
  loads an `index.aff` + `index.dic` pair from a `dictionaries/<locale>/`
  directory — the [wooorm/dictionaries] layout — resolving the active UI locale
  against a fallback chain (e.g. `en-GB` → `en`, then `en`).
- The dictionaries directory is the `dictionaries_dir` setting (default
  `./dictionaries`, resolved relative to the working directory). See
  `docs/configuration.md`.
- The language follows the **UI locale** (View → Locale); changing locale reloads
  the dictionary. A missing dictionary leaves spell-checking silently inert.
- The host asks the editor for its **comment and string** character ranges (from
  Tree-sitter capture names), runs `SpellChecker::misspellings_in` over each, and
  draws a red underline on a dedicated editor mark channel (separate from the
  search-hit underline). Code-like tokens are skipped: all-caps acronyms
  (`HTTP`), camel/Pascal-case identifiers (`fooBar`), and very short words.
- A session **user dictionary** (added words) and an **ignore** set are supported
  by the crate.

## Roadmap

- `Ctrl+;` suggestions popup at the cursor word, with **Add to dictionary** and
  **Ignore** actions (the crate API for this — `suggest`, `add_word`,
  `ignore_word` — already exists; the UI is pending).
- Persisting the user dictionary across sessions.

[`spellbook`]: https://crates.io/crates/spellbook
[wooorm/dictionaries]: https://github.com/wooorm/dictionaries
