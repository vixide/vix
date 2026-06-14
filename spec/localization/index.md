# Localization

Vix is fully localizable. Every piece of user-facing text — menu labels, status
messages, prompts, confirmations, the command palette, and help rows — is looked
up by key at runtime rather than hard-coded, so the whole interface can switch
languages on the fly. The translations live in one data file; the active language
is a single setting; and any text that has not yet been translated falls back to
English automatically.

## How it works: rust-i18n and `app.yml`

Localization is built on the [`rust-i18n`](https://crates.io/crates/rust-i18n)
crate (version 4.x). The library crate initializes it once, near the top of
`src/lib.rs`:

```rust
i18n!("locales", fallback = "en");
```

This tells `rust-i18n` to load every file under `locales/` at compile time and to
treat **English (`en`) as the fallback** for any missing translation. In Vix all
the strings live in a single bundle, `locales/app.yml`.

Each entry in `app.yml` is a translation **key** with one value per language:

```yaml
menu.file:
  en: "File"
  es: "Archivo"
  fr: "Fichier"
  de: "Datei"
  # … one line per supported language
```

The first line of the file, `_version: 2`, selects the `rust-i18n` multi-language
file format (one key, many languages) rather than one file per language.

### Looking up text: the `t!` macro

Code never writes a user-facing literal directly. Instead it calls the `t!`
macro with a key:

```rust
t!("status.ai_busy")                       // simple lookup
t!("status.locale", locale = code)         // with an interpolated argument
t!("confirm.delete", n = paths.len())      // pluralizable / counted message
```

`t!` resolves the key against the **currently active locale** (set with
`rust_i18n::set_locale`). Arguments are interpolated into the value using
`%{name}` placeholders — for example `status.locale` is `"Language: %{locale}"`
in English and `"Idioma: %{locale}"` in Spanish.

### English fallback

Because the bundle was initialized with `fallback = "en"`, a key that has **no
value for the active language** falls back to its English value rather than
showing a blank or the raw key. This is deliberate: every language is selectable
immediately, and translation coverage can be filled in incrementally without ever
leaving gaps in the UI. As of writing, English is complete and the other
languages range from partially to fully translated; untranslated keys simply read
in English until someone adds them.

## Available languages

The set of selectable UI languages is defined as pure data in the
`vix-locale-model` crate (`LOCALES`). Each entry pairs a **locale code** (the
value passed to `rust-i18n`) with its **endonym** — the language's name written
in itself, which is the convention for language pickers. English is first because
it is the fallback; the constructed languages are listed last.

| Code  | Endonym            | Language               |
|-------|--------------------|------------------------|
| `en`  | English            | English (fallback)     |
| `es`  | Español            | Spanish                |
| `fr`  | Français           | French                 |
| `de`  | Deutsch            | German                 |
| `cy`  | Cymraeg            | Welsh                  |
| `ga`  | Gaeilge            | Irish                  |
| `gd`  | Gàidhlig           | Scottish Gaelic        |
| `pl`  | Polski             | Polish                 |
| `pt`  | Português          | Portuguese             |
| `ru`  | Русский            | Russian                |
| `ar`  | العربية            | Arabic                 |
| `hi`  | हिन्दी              | Hindi                  |
| `bn`  | বাংলা              | Bengali                |
| `zh`  | 中文               | Chinese                |
| `ja`  | 日本語             | Japanese               |
| `it`  | Italiano           | Italian                |
| `ko`  | 한국어             | Korean                 |
| `tr`  | Türkçe             | Turkish                |
| `nl`  | Nederlands         | Dutch                  |
| `vi`  | Tiếng Việt         | Vietnamese             |
| `id`  | Bahasa Indonesia   | Indonesian             |
| `th`  | ไทย                | Thai                   |
| `fa`  | فارسی              | Persian                |
| `uk`  | Українська         | Ukrainian              |
| `el`  | Ελληνικά           | Greek                  |
| `tlh` | tlhIngan Hol       | Klingon (constructed)  |
| `sjn` | Edhellen           | Sindarin (constructed) |

The codes are the canonical `rust-i18n` lookup keys; `vix_locale_model::by_code`
resolves a code back to its `Locale`, or `None` if it is not bundled.

## Changing the language: the Locale chooser

The UI language is chosen through **View → Locale**, which opens an overlay
listing every language by its endonym. The chooser is built on the
`vix-locale-chooser` crate, which re-exports `Locale`/`LOCALES` and adds a small
selection state.

Behavior:

- **Open** — the overlay opens highlighting the currently active language (or the
  first locale, English, if the active code is somehow not in the list).
- **Up / Down** — move the highlight, wrapping around at the ends. Moving the
  highlight calls `rust_i18n::set_locale` for the highlighted language, so the
  **entire UI re-renders in that language as a live preview** while you browse.
- **Mouse** — clicking a row in the overlay selects that language and previews it
  live, the same as moving with the arrow keys.
- **Enter** — commits the highlighted language: it stays applied, the choice is
  saved to `settings.locale`, and the status bar confirms with
  `status.locale` (`"Language: <code>"`).
- **Esc** — cancels: the language active when the chooser opened is restored
  (the chooser remembers it as `original`), nothing is persisted, and the status
  bar shows `status.locale_unchanged` (`"Language unchanged"`).

Because Up/Down preview by actually setting the locale, Esc must restore the
original — which is exactly why the chooser tracks both the `selected` and the
`original` index. The committed value `settings.locale` is reloaded on the next
launch, so the chosen language is sticky across runs.

A locale change also drives spell-checking: the editor reloads the Hunspell
dictionary for the new UI locale when spell-checking is on (a missing dictionary
just leaves the checker inert). Some date formatting is locale-aware too — the
calendar inserts a clicked day using a `strftime` pattern chosen per active
locale.

## The `--locale` command-line override

The binary accepts a `--locale` (`-l`) flag that overrides the saved language
**for one run only**:

```sh
vix --locale fr             # start in French this run
vix -l ja file.rs           # start in Japanese, open file.rs
```

At startup `src/main.rs` resolves the effective locale as the CLI flag if given,
otherwise the persisted `settings.locale`, and applies it with
`rust_i18n::set_locale` before the UI is built (so even the first-run welcome
screen appears in the right language). The flag is **not written back** to
settings — it is a transient override. Changing the language in the Locale
chooser during that session still persists normally.

## Key namespaces

Keys in `app.yml` are grouped by a dotted namespace prefix. The main namespaces:

| Namespace  | Purpose                                                            |
|------------|-------------------------------------------------------------------|
| `menu.*`   | Menu-bar names and menu item labels                               |
| `ui.*`     | In-pane and overlay UI labels, headings, and chrome              |
| `status.*` | Transient status-bar messages (often with interpolated arguments) |
| `msg.*`    | Notices and error messages (e.g. `msg.save_failed`)               |
| `prompt.*` | Input-prompt labels (open, save-as, rename, run command, …)       |
| `cmd.*`    | Command-related labels                                            |
| `palette.*`| Command-palette text                                              |
| `help.*`   | Keyboard-shortcut help-row descriptions                           |
| `theme.*`  | Theme-related labels                                              |
| `welcome.*`| First-run welcome content                                         |
| `confirm.*`| Confirmation prompts (e.g. `confirm.delete`, counted)             |

New user-facing text should be added as a key under the appropriate namespace —
at minimum with an `en` value — and looked up through `t!`; other languages can
follow later thanks to the English fallback.

## As implemented in Vix

- **`vix-locale-model`** is the pure-data home of the language list: the `Locale`
  struct (`code` + `name` endonym), the `LOCALES` array in chooser order (English
  first as the fallback, constructed languages last), and the `by_code` lookup.
  It has no UI dependencies. See `vix-locale-model/src/lib.rs`.
- **`vix-locale-chooser`** re-exports `Locale`/`LOCALES` and adds the overlay's
  `Chooser` state: `open(current_code)`, `up`/`down` (wrapping), and the
  `selected_code` / `original_code` accessors used to preview, commit, and revert.
  See `vix-locale-chooser/src/lib.rs`.
- The **host** (`src/app.rs`) wires it together as `LocaleChooser`:
  `open_locale_chooser`, the `locale_key` / `locale_mouse` handlers (live preview
  via `rust_i18n::set_locale`, Enter persists to `settings.locale`, Esc reverts),
  and `view.locale` as the menu action.
- The **binary** (`src/main.rs`) parses `--locale`, resolves it against
  `settings.locale`, and calls `rust_i18n::set_locale` at startup.
- The **bundle** lives in `locales/app.yml`, loaded by `i18n!("locales",
  fallback = "en")` in `src/lib.rs` and read everywhere through the `t!` macro.
