# Internationalization (i18n)

The whole Vix™ UI is translatable. Translations are powered by
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
for the rest. Filling them in is YAML-only (see below). Right-to-left
scripts (e.g. Arabic) are reordered into visual order after each frame is drawn
(UAX #9), controlled by the `bidi` setting: `"auto"` (default; on when the UI
language is right-to-left), `"on"`, or `"off"`. Turn it off if your terminal
already reorders bidirectional text itself, otherwise the text is reversed twice.
With a right-to-left language the layout is mirrored too (`mirror_layout`:
`"auto"` default, `"on"`, `"off"`): the explorer moves to the right, the message
drawer and other docks to the left, and the editor's scroll bar to the left.
Mouse dragging to resize a dock follows the mirrored edges. The menu bar
mirrors too: the first menu is rightmost, dropdowns line up with their title's
right edge, submenus open to the left, and the dock toggles move to the left
edge. In the explorer, the menu dropdowns, the messages dock and the bottom dock,
text is right-aligned and tree indentation grows from the right. The status bar
swaps its two halves, and the tab strip sits against the right edge (clicks follow
the drawing). The dialogs right-align their text too; the Find box and Calculator move
their recorded button rectangles with the drawing, and the info box and
Pomodoro dialog are centred, so clicks always land on what is drawn.

## Choosing a language

Three ways, in increasing precedence:

1. **Setting** — the `locale` key in the config file (see
   [configuration.md](../configuration/index.md)).
2. **In-app** — **View → Locale…** (↑↓ to preview live, Enter to apply and save,
   Esc to cancel). Languages are listed by their endonym.
3. **CLI** — `vix --locale fr` overrides both for one run, without saving.

At startup `main` resolves the locale (`--locale` → `locale` setting → default
`en`) and calls `rust_i18n::set_locale` before building the UI, so even the
welcome messages appear in the chosen language.

## How it works in the code

- `crates/vix-i18n/src/lib.rs` initializes the catalog once, but not via
  `rust_i18n::i18n!` — that macro's codegen builds every locale eagerly, so
  `build.rs` merges `locales/` at build time and splits it per locale, and
  `lib.rs` parses each locale's blob lazily, only when something actually
  asks for a translation in it (T517). Every other crate still reads through
  `vix_i18n::t!` unchanged.
- All translation strings live under `locales/`, one file per key namespace
  (`menu.yml`, `status.yml`, …, T148) — rust-i18n merges every file in the
  directory into one table, each following its own "version 2" format (every
  language under each dotted key).
- Data-only crates (menus, command palette, theme names, keyboard help) store
  i18n **keys**, not translated text; the host calls `t!(key)` when rendering.
- Interpolated values use `%{name}` placeholders, e.g.
  `status.saved: "Saved %{path}"`, called as `t!("status.saved", path = …)`.

## Adding or extending a language

No code changes are needed — edit YAML only.

1. Open the `locales/*.yml` file for the key's namespace (see "Key namespace"
   below) — `crates/vix-i18n/locales/menu.yml` for a menu label, and so on.
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
   `LOCALES` in the `locale_model` module (`code` + endonym `name`).

Because English is the fallback, you can translate incrementally: add the keys
you have, and the rest stay in English until you fill them in.

## Key namespace

Each namespace lives in the `locales/*.yml` file named for it, except a
handful of low-volume ones grouped into `crates/vix-i18n/locales/misc.yml`.

| Prefix          | File                 | Used for                                          |
| --------------- | -------------------- | -------------------------------------------------- |
| `menu.*`        | `menu.yml`           | Menu names and items (keyed by action).             |
| `action.*`      | `action.yml`         | `vix-action-catalog` titles for menu-less actions.  |
| `cmd.*`         | `cmd.yml`             | Command-palette command labels.                     |
| `ui.*`          | `ui.yml`              | Panel/overlay titles, hints, field labels, toggles. |
| `help.*`        | `help.yml`            | Keyboard-shortcut descriptions.                     |
| `prompt.*`      | `prompt.yml`          | Open/Save-as prompt titles.                         |
| `msg.*`         | `msg.yml`             | Welcome and message-drawer / error text.            |
| `status.*`      | `status.yml`          | Status-bar messages.                                |
| `palette.*`, `agenda.*`, `blame.*`, `welcome.*`, `scratch.*`, `confirm.*` | `misc.yml` | Command-palette chrome, Org agenda, git blame, first-run welcome, the scratch buffer, counted confirmations. |

## See also

- `crates/vix-i18n/spec/index.md` — the localization specification (source of truth).

---

Vix™ and Vix IDE™ are trademarks.

## Counted messages

`rust-i18n` has no plural rules: a key is one fixed string whatever the number. English
gets away with `item(s)`, but Arabic (six number categories), Russian and Polish (three
each) read wrongly at most counts when the number precedes the noun (`%{n} wierszy` is
right for 5, wrong for 1 and 2). So in `ar`, `ru` and `pl` a counted message is phrased
**count-invariantly**: a label, a colon, the number (`Wiersze: %{n}`, `Строк: %{count}`,
`الصفوف: %{count}`). `tui/tests/i18n_keys.rs` keeps the list of counted keys
(`COUNTED_KEYS`) and fails if one of those three locales puts the count directly before a
noun (an abbreviation like `elem.` or a one/two-letter particle is fine). Add a new counted
key to that list. Welsh, Irish and Scottish Gaelic have several plural categories too; they
were not part of this change and still use the single English-shaped form.
