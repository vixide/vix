# Localization

Shared i18n backend for the Vix workspace (single embedded locale table). Vix
is fully localizable: every piece of user-facing text — menu labels, status
messages, prompts, confirmations, the command palette, and help rows — is looked
up by key at runtime rather than hard-coded, so the whole interface can switch
languages on the fly. The translations live in `locales/`, one file per key
namespace; the active language is a single setting; and any text that has not
yet been translated falls back to English automatically.

## How it works: rust-i18n and `locales/`

Localization is built on the [`rust-i18n`](https://crates.io/crates/rust-i18n)
crate (version 4.x). `rust_i18n::i18n!` embeds the whole translation table into
whichever crate invokes it, so a naive per-crate `i18n!` call in a 107-crate
workspace would embed the whole `locales/` directory once per crate. Instead,
the `vix-i18n` crate invokes it exactly **once**:

```rust
// crates/vix-i18n/src/lib.rs
rust_i18n::i18n!("../../locales", fallback = "en");
```

and every other crate calls `vix_i18n::surface!()` once at its root plus
`vix_i18n::t!`/`#[macro_use] extern crate vix_i18n;` to reuse that single
embedded table instead of creating their own.

This tells `rust-i18n` to load **every file** under `locales/` at
**macro-expansion time** (i.e. when `vix-i18n` compiles), merge them into one
table, and treat **English (`en`) as the fallback** for any missing
translation. Splitting by namespace (T148) keeps any one file — and so any one
translation PR's diff — well short of the ~28,900-line single file it used to
be, named `app.yml`, before T148 (a name still worth searching Vix's own
history for).

Each entry in a `locales/*.yml` file is a translation **key** with one value
per language:

```yaml
menu.file:
  en: "File"
  es: "Archivo"
  fr: "Fichier"
  de: "Datei"
  # … one line per supported language
```

The first line of every file, `_version: 2`, selects the `rust-i18n`
multi-language file format (one key, many languages) rather than one file per
language — each file declares it independently, since `rust-i18n` merges the
files but reads this header per file.

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
`locale_model` crate (`LOCALES`). Each entry pairs a **locale code** (the
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

## Changing the language: the Locale submenu

The UI language is chosen through **View → Locale**, a submenu listing every
language by its endonym (built from `vix_locale_model::LOCALES`). Selecting a
language applies it immediately, saves it to `settings.locale`, and confirms with
`status.locale` (`"Language: <code>"`). Each item dispatches `view.locale:<code>`.
The committed value is reloaded on the next launch, so the chosen language is
sticky across runs.

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
submenu during that session still persists normally.

## Key namespaces

Keys are grouped by a dotted namespace prefix, and — since T148 — that
grouping *is* the file split: every key under a given prefix lives in the one
`locales/*.yml` file named for it (a small catch-all groups several
low-volume namespaces that don't each need their own file). The main
namespaces:

| Namespace   | File               | Purpose                                                            |
|-------------|---------------------|---------------------------------------------------------------------|
| `menu.*`    | `locales/menu.yml`   | Menu-bar names and menu item labels                                 |
| `ui.*`      | `locales/ui.yml`     | In-pane and overlay UI labels, headings, and chrome                 |
| `status.*`  | `locales/status.yml` | Transient status-bar messages (often with interpolated arguments)   |
| `action.*`  | `locales/action.yml` | `vix-action-catalog` titles for actions with no menu leaf (T147)    |
| `cmd.*`     | `locales/cmd.yml`    | Command-palette `>` mode labels (`palette::COMMANDS`)                |
| `msg.*`     | `locales/msg.yml`    | Notices and error messages (e.g. `msg.save_failed`)                 |
| `prompt.*`  | `locales/prompt.yml` | Input-prompt labels (open, save-as, rename, run command, …)         |
| `help.*`    | `locales/help.yml`   | Keyboard-shortcut help-row descriptions                              |
| `palette.*`, `agenda.*`, `blame.*`, `welcome.*`, `scratch.*`, `confirm.*` | `locales/misc.yml` | Low-volume namespaces (command-palette chrome, Org agenda, git blame, first-run welcome, the scratch buffer, counted confirmation prompts) |

New user-facing text should be added as a key under the appropriate namespace
(in its namespace's file, or `misc.yml` for a genuinely new low-volume one) —
at minimum with an `en` value — and looked up through `t!`; other languages can
follow later thanks to the English fallback.

## Rebuilds: `crates/vix-i18n/build.rs`

Because every `locales/*.yml` file is read at **macro-expansion time** rather
than via `include_str!`, Cargo has no built-in way to know those files affect
`vix-i18n`'s compiled output — it only tracks `.rs` sources by default. Without
a `cargo:rerun-if-changed` hint, editing a translation file (no `.rs`
change) does **not** trigger a `vix-i18n` recompile: `cargo build`/`cargo test`
report success using the previously-embedded, now-stale table, and any UI text
added or changed since the last real rebuild renders as its raw key (e.g.
`menu.item.org.capture.task` instead of "Task…") — rust-i18n's behavior for a
key with no matching entry, which is indistinguishable from a genuinely stale
embed. `crates/vix-i18n/build.rs` fixes this by emitting a
`cargo:rerun-if-changed` for the `locales/` directory itself *and* every file
inside it: on most filesystems a directory's own mtime only changes when an
entry is added or removed, not when an existing file's content changes, so
watching the directory alone would miss the common case (editing a
translation).

`vix-menu`'s `every_menu_label_translates` test (`crates/vix-menu/src/lib.rs`)
guards against this regressing again: it walks the whole menu tree and asserts
every `menu.`-prefixed label actually translates (differs from its own key),
catching both a missing catalog entry and a stale embed after a real rebuild.
It intentionally skips runtime-built items whose `label` is literal display
text rather than an i18n key (the View → Theme/Locale/Time Zone submenus).

## As implemented in Vix

- **`locale_model`** is the pure-data home of the language list: the `Locale`
  struct (`code` + `name` endonym), the `LOCALES` array in chooser order (English
  first as the fallback, constructed languages last), and the `by_code` lookup.
  It has no UI dependencies. See `crates/vix-locale-model/src/lib.rs`.
- The **host** (`src/app.rs`) builds the View → Locale submenu from `LOCALES` and
  applies a chosen language by code (`set_locale_by_code`): it calls
  `rust_i18n::set_locale`, persists to `settings.locale`, and confirms via
  `status.locale`. Each submenu item dispatches `view.locale:<code>`.
- The **binary** (`src/main.rs`) parses `--locale`, resolves it against
  `settings.locale`, and calls `rust_i18n::set_locale` at startup.
- The **bundle** lives in `locales/*.yml` (one file per key namespace, T148),
  loaded once by `rust_i18n::i18n!("../../locales", fallback = "en")` in
  `crates/vix-i18n/src/lib.rs` (a `build.rs` there makes Cargo track every file
  in the directory so edits actually trigger a rebuild) and read everywhere
  through `vix_i18n::t!`.
