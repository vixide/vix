# Internationalization (i18n)

The whole Vix UI is translatable. Translations are powered by
[`rust-i18n`](https://crates.io/crates/rust-i18n): user-facing text is looked up
at render time with the `t!` macro, so switching language re-renders everything
immediately.

## Bundled languages

| Code | Language          | Endonym  | Code | Language    | Endonym  |
| ---- | ----------------- | -------- | ---- | ----------- | -------- |
| `en` | English           | English  | `pl` | Polish      | Polski   |
| `es` | Spanish           | Español  | `pt` | Portuguese  | Português|
| `fr` | French            | Français | `ru` | Russian     | Русский  |
| `de` | German            | Deutsch  | `ar` | Arabic      | العربية  |
| `cy` | Welsh             | Cymraeg  | `hi` | Hindi       | हिन्दी    |
| `ga` | Irish             | Gaeilge  | `bn` | Bengali     | বাংলা    |
| `gd` | Scottish Gaelic   | Gàidhlig | `zh` | Chinese     | 中文     |
|      |                   |          | `ja` | Japanese    | 日本語   |

English is the **fallback**: any key missing from another language renders its
English text, so a partially-translated language still works everywhere.
Coverage varies — English, Spanish, French, German, and Welsh are complete; the
others currently translate the menu bar and theme names and fall back to English
for the rest. Filling them in is YAML-only (see below). Note that Vix renders
left-to-right, so right-to-left scripts (e.g. Arabic) are not yet laid out RTL.

## Choosing a language

Three ways, in increasing precedence:

1. **Setting** — the `locale` key in the config file (see
   [configuration.md](configuration.md)).
2. **In-app** — **View → Locale…** (↑↓ to preview live, Enter to apply and save,
   Esc to cancel). Languages are listed by their endonym.
3. **CLI** — `vix --locale fr` overrides both for one run, without saving.

At startup `main` resolves the locale (`--locale` → `locale` setting → default
`en`) and calls `rust_i18n::set_locale` before building the UI, so even the
welcome messages appear in the chosen language.

## How it works in the code

- `src/lib.rs` initializes the catalog once: `i18n!("locales", fallback = "en")`.
- All translation strings live in `locales/app.yml` (rust-i18n "version 2"
  format: one file, every language under each dotted key).
- Data-only crates (menus, command palette, theme names, keyboard help) store
  i18n **keys**, not translated text; the host calls `t!(key)` when rendering.
- Interpolated values use `%{name}` placeholders, e.g.
  `status.saved: "Saved %{path}"`, called as `t!("status.saved", path = …)`.

## Adding or extending a language

No code changes are needed — edit YAML only.

1. Open `locales/app.yml`.
2. For each key, add (or correct) your language line. Example:

   ```yaml
   menu.file:
     en: "File"
     es: "Archivo"
     fr: "Fichier"
     de: "Datei"
     cy: "Ffeil"
     it: "File"        # add a new language here
   ```

3. To make a new language selectable in **View → Locale…**, add it to
   `LOCALES` in the `vix-locale-chooser` crate (`code` + endonym `name`).

Because English is the fallback, you can translate incrementally: add the keys
you have, and the rest stay in English until you fill them in.

## Key namespace

| Prefix          | Used for                                          |
| --------------- | ------------------------------------------------- |
| `menu.*`        | Menu names and items (keyed by action).           |
| `palette.*`     | Command-palette mode labels.                      |
| `cmd.*`         | Command-palette command labels.                   |
| `theme.*`       | Built-in theme names.                             |
| `ui.*`          | Panel/overlay titles, hints, field labels, toggles. |
| `help.*`        | Keyboard-shortcut descriptions.                   |
| `prompt.*`      | Open/Save-as prompt titles.                       |
| `msg.*`         | Welcome and message-drawer / error text.          |
| `status.*`      | Status-bar messages.                              |

## See also

- `spec/locale-chooser.md` — the localization specification (source of truth).
