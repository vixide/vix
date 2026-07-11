# spellcheck

The `spellcheck` crate.

**Status:** Shipped — **View → Editor → Toggle Spellcheck** (or the command
palette / `view.spellcheck`) underlines misspelled words **in comments and
string literals only**, in red. Off by default; the choice persists via the
`spellcheck` setting. With the cursor on a misspelled word, **`Ctrl+;`** opens a
suggestions popup: `↑`/`↓` select, `Enter` replaces, `a` adds the word to the
session dictionary, `i` ignores it for the session, `Esc` closes.

## How it works

- The crate wraps the pure-Rust [`spellbook`] Hunspell checker. A `SpellChecker`
  loads an `.aff` + `.dic` pair, supporting both the standard Hunspell layout
  (`<dir>/en_US.{aff,dic}`) and the [wooorm/dictionaries] layout
  (`<dir>/<locale>/index.{aff,dic}`).
- Dictionaries are **autodetected** from the platform's standard Hunspell
  directories (`/usr/share/hunspell`, `/Library/Spelling`,
  `/opt/homebrew/share/hunspell`, `$XDG_DATA_HOME/hunspell`, …) plus whatever
  `hunspell -D` reports. The `dictionary_path` setting adds one more directory to
  search first; `./dictionaries` (the repo's bundled set) is also searched. See
  `docs/configuration.md` and `dictionaries.md`.
- The dictionary name is resolved from the UI locale, trying `en-GB`, `en_GB`,
  then the base `en`, and finally any `en_*` file (e.g. `en_US`).
- The language follows the **UI locale** (View → Locale); changing locale reloads
  the dictionary. A missing dictionary leaves spell-checking silently inert.
- The host asks the editor for its **comment and string** character ranges (from
  Tree-sitter capture names), runs `SpellChecker::misspellings_in` over each, and
  draws a red underline on a dedicated editor mark channel (separate from the
  search-hit underline). Code-like tokens are skipped: all-caps acronyms
  (`HTTP`), camel/Pascal-case identifiers (`fooBar`), and very short words.
- A **user dictionary** (added words) and an **ignore** set are supported by the
  crate. Added words **persist across sessions**: the spell-suggest popup's "add
  word" appends to `user_dictionary.txt` (one word per line, in the config
  directory; `Settings::user_dictionary_path`), and the host loads it into every
  new checker in `ensure_speller`. The ignore set remains per-session.

## Roadmap

- A UI to review/remove persisted user-dictionary words (currently edit the file).

[`spellbook`]: https://crates.io/crates/spellbook
[wooorm/dictionaries]: https://github.com/wooorm/dictionaries
