# Dictionaries

Spell checking (see `vix-spellcheck.md`) reads **Hunspell** dictionaries from a
`dictionaries/` directory, one subdirectory per locale, each holding the standard
`index.aff` (affix rules) and `index.dic` (word list) pair:

```
dictionaries/
  en/   { index.aff, index.dic }
  en-GB/{ index.aff, index.dic }
  es/   { index.aff, index.dic }
  fr/   …
```

The directory location is the `dictionaries_dir` setting (default
`./dictionaries`, resolved relative to the working directory — see
`docs/configuration.md`). The spell-check language follows the UI locale, falling
back to the base language and then `en`.

## Obtaining the dictionaries

The dictionaries are **not vendored in this repository** — the full set is ~287 MB
across 92 locales, so it is listed in `.gitignore`. Use the standard
[wooorm/dictionaries] set (Hunspell `index.aff` + `index.dic` per locale, which is
exactly the layout above). Fetch the locales you need into `dictionaries/`, for
example:

- Clone <https://github.com/wooorm/dictionaries> and copy its `dictionaries/<locale>`
  folders into this project's `dictionaries/`, or
- Install the matching `dictionary-<locale>` npm packages and copy their
  `index.aff` / `index.dic` into `dictionaries/<locale>/`.

Only the locales you actually use are required; at minimum `dictionaries/en/`
for default English spell checking. A missing dictionary leaves spell checking
silently inert (no error), so the app runs fine without it.

[wooorm/dictionaries]: https://github.com/wooorm/dictionaries
