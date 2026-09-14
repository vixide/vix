# Dictionaries and Spell Checking

Vix™ can spell-check your prose while leaving your code alone: it underlines
misspelled words **in comments and string literals only**. Spell checking uses
**Hunspell** dictionaries, which Vix autodetects from your system or loads from a
directory you configure.

## Turning spell checking on

Spell checking is **off by default**. Toggle it with **View → Editor → Toggle
Spellcheck** (or the command palette, action `view.spellcheck`). The choice
persists across sessions via the `spellcheck` setting.

When on, misspelled words are underlined in **red**. Only comment and string
ranges are checked — code identifiers are not flagged. Vix also skips code-like
tokens it does find inside those ranges: all-caps acronyms (`HTTP`),
camel/Pascal-case identifiers (`fooBar`), and very short words.

## Suggestions popup

Place the cursor on a misspelled word and press **`Ctrl+;`** to open the
suggestions popup:

| Key       | Action                                  |
| --------- | --------------------------------------- |
| `↑` / `↓` | Select a suggestion                     |
| `Enter`   | Replace the word with the suggestion    |
| `a`       | Add the word to the session dictionary  |
| `i`       | Ignore the word for the session         |
| `Esc`     | Close the popup                         |

Words you **add** persist across sessions: they are appended to
`user_dictionary.txt` in the config directory (one word per line) and loaded into
the checker on every launch. The **ignore** set is still per-session. To remove a
persisted word, edit that file.

## How dictionaries are found

Vix resolves a dictionary in this order of locations:

1. The `dictionary_path` setting, if set — searched **first**.
2. The repo's bundled `./dictionaries` directory.
3. The platform's standard Hunspell directories, plus whatever `hunspell -D`
   reports.

### Standard Hunspell directories

**Unix:**

- `/usr/share/hunspell/`
- `/usr/local/share/hunspell/`
- `$XDG_DATA_HOME/hunspell` (falling back to `~/.local/share/hunspell`)

**macOS:**

- `/Library/Spelling/`
- `~/Library/Spelling/`
- `/opt/homebrew/share/hunspell/`
- `/System/Library/Services/AppleSpell.service/Contents/Resources/AppleSpell.8`
- `~/Library/Dictionaries/`
- `$XDG_DATA_HOME/hunspell` (falling back to `~/.local/share/hunspell`)

Vix also runs `hunspell -D` and searches the directories that command reports.

## Supported dictionary layouts

Each dictionary is an `.aff` (affix rules) + `.dic` (word list) pair. Vix accepts
two on-disk layouts:

- **Standard Hunspell** — `<dir>/<name>.{aff,dic}`, for example
  `/usr/share/hunspell/en_US.aff` and `en_US.dic`.
- **wooorm/dictionaries** — `<dir>/<locale>/index.{aff,dic}`, for example
  `dictionaries/en/index.aff` and `dictionaries/en/index.dic`.

A `dictionaries/` directory using the wooorm layout looks like:

```
dictionaries/
  en/    { index.aff, index.dic }
  en-GB/ { index.aff, index.dic }
  es/    { index.aff, index.dic }
  fr/    …
```

## Locale resolution

The spell-check language follows the **UI locale** (set in **View → Locale**).
Changing the locale reloads the dictionary.

To resolve a dictionary for a locale such as `en-GB`, Vix tries, in order:
`en-GB`, then `en_GB`, then the base language `en`, and finally any `en_*` file
(for example `en_US`). The language follows the UI locale, falling back to the
base language and then `en`.

A missing dictionary leaves spell checking **silently inert** — there is no error,
and the app runs fine without it.

## The `dictionary_path` setting

The `dictionary_path` setting (default empty) adds one directory that Vix
searches **first**, before the bundled and system locations. Set it in your
configuration (see `../configuration/index.md`) to point at your own dictionary
collection.

Note the spelling: the setting is `dictionary_path` (not `dictionary_dir`).

## Obtaining dictionaries

The dictionaries are **not vendored in this repository** — the full set is about
287 MB across 92 locales, so it is listed in `.gitignore`. Use the standard
[wooorm/dictionaries] set, whose `index.aff` + `index.dic` per-locale layout
matches the directory structure above. Fetch only the locales you need into
`dictionaries/`; at minimum, `dictionaries/en/` for default English. For example:

- Clone <https://github.com/wooorm/dictionaries> and copy its
  `dictionaries/<locale>` folders into this project's `dictionaries/`, or
- Install the matching `dictionary-<locale>` npm packages and copy their
  `index.aff` / `index.dic` into `dictionaries/<locale>/`.

## Example

Enable spell checking and fix a typo in a comment:

1. Open **View → Editor → Toggle Spellcheck**.
2. A misspelled word in a comment or string is underlined in red.
3. Move the cursor onto it and press `Ctrl+;`.
4. Pick a suggestion with `↑` / `↓` and press `Enter` to replace it — or press
   `a` to add it to your session dictionary, or `i` to ignore it this session.

[wooorm/dictionaries]: https://github.com/wooorm/dictionaries

---

Vix™ and Vix IDE™ are trademarks.
