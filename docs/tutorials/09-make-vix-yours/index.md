# Tutorial 9: Make Vix Yours

Four things make Vix feel like *your* editor instead of a stranger's:
**themes** (color), **keymaps** (which keys do what), **snippets**
(reusable text), and the **settings file** (everything else). This
tutorial walks through all four, hands-on, against the demo workspace.

Open it if you don't already have it open:

```sh
cd examples/demo-workspace
vix .
```

From the explorer (`Ctrl+B` to show it, `Ctrl+E` to focus it), open
`examples/demo-workspace/rust-app/src/main.rs`. It's a small Rust file
with keywords, strings, a comment or two, and a couple of `TODO`/`FIXME`
markers — real syntax-highlighted code to preview changes against as we
go, rather than an empty buffer.

## Themes: browsing and switching

Open **View → Theme…**. It's a live-preview chooser: `↑`/`↓` (or a mouse
click) applies each theme to the *real* running UI immediately, so with
`main.rs` visible behind the menu you can watch `fn`, `use`, and `mut`
change color as you move the selection, not just imagine it from a name.
`Enter` keeps the highlighted theme and saves it to the `theme` setting;
`Esc` cancels and restores whatever was active before you opened the menu.

Vix ships a lot of themes to browse: `Dark` and `Light` (the two
defaults), `Darker`, `Darkest`, `Lighter`, `Lightest`, `Turbo`,
`Solarized Dark`, `Solarized Light`, `Dracula`, `Nord`, `Gruvbox Dark`,
`Monokai`, `One Dark`, `Tokyo Night`, `Catppuccin Mocha`, `High Contrast`
(WCAG AA, 11:1 or better everywhere), `Phosphor Amber`, `Phosphor Green`,
`Safelight Red` — plus a full set of `Base16 …`-prefixed themes generated
from bundled [base16](https://github.com/chriskempson/base16) palettes,
in the same list. See [`docs/themes/index.md`](../../themes/index.md) for
the complete set and the underlying JSON format.

## Themes: the interactive Theme Editor

Picking between finished themes is one thing; building your own from
scratch is another. Open **View → Edit Theme…** (also in the command
palette, `Ctrl+P` then `>edit theme`). It opens over a working copy of
whatever theme is currently active — 15 rows, one per color slot: the
menu bar, status bar, left dock (explorer), and right dock (messages),
each with a separate foreground and background; the editor's own
foreground, background, and cursor color; and the four syntax colors
(keyword, string, comment, number). Each row shows its current
`#RRGGBB` value.

- `↑`/`↓` (or the mouse) move the highlight between rows.
- `Enter` on a row opens the same **X11 color picker** Tools → X11
  Colors uses elsewhere in Vix — browse or search named colors rather
  than typing hex by hand. Picking one applies it to the slot and
  returns you to the editor.
- Every change is applied **live** — pick a new value for the "syntax ›
  keyword" row and watch `fn`/`use`/`mut` in `main.rs` change color right
  behind the overlay, the same live-preview mechanism the plain theme
  chooser uses.
- `Ctrl+S` (Save As) prompts for a name and writes the result to
  `~/.config/vix/themes/<name>.json`, then makes it the active theme —
  the same outcome as picking any other theme from **View → Theme…**.
- `Esc` closes the editor and reverts to whichever theme was active
  before you opened it, if you haven't saved. An edit you don't save
  never lingers as the active theme.

Try it: open the editor, change the syntax "comment" slot to something
loud, `Ctrl+S`, name it `my-theme`. It now shows up in **View → Theme…**
alongside the bundled ones, and `~/.config/vix/themes/my-theme.json` is
just a JSON file you can also hand-edit (or check into a dotfiles repo) —
see [`docs/themes/index.md`](../../themes/index.md) for the field
reference if you'd rather write one by hand than click through the
editor.

## Keymaps: switching families

A keymap decides what your keys *do*, not what they look like. Open
**View → Keymap** — a submenu, not an overlay — and you'll find ten
entries: **Apple** (the default), **VSCode macOS**, **VSCode Windows**,
**Emacs**, **Vi**, **Spacemacs**, **IntelliJ macOS**, **IntelliJ
Windows**, **Eclipse**, and **Sublime Text**. Selecting one applies it
immediately and saves it to the `keymap` setting, so it's what you get
next launch too.

Each keymap changes the same actions to different keys — for example
"open the file chooser" is `Ctrl+O` under Apple, `Ctrl+P` (Quick Open)
under VSCode, `Ctrl+X Ctrl+F` under Emacs, and `:Ex`/`Ctrl+E` under Vi.
Two things never change no matter which keymap is active: `Ctrl+Z` /
`Ctrl+Shift+Z` for undo/redo, and every menu mnemonic (`Alt+F/E/V/T/H`)
and function key (`F1`, `F3`, `F10`, `F12`). `F1` in particular opens a
searchable overlay of every shortcut in your *current* keymap — the
fastest way to answer "wait, what does this key do now?" right after
switching. The full per-keymap tables live in
[`docs/keymaps/index.md`](../../keymaps/index.md).

## Keymaps: rebinding one key

Switching families is coarse; sometimes you just want one key to do
something else. Open **Vix → Keybindings…** — a sibling of the read-only
`F1` panel that adds rebind and reset. It lists the *active* keymap's
top-level bindings plus the keymap-agnostic shared ones (things like
`Alt+Left`/`Alt+Right` for position history, which work the same in
every keymap), each row showing its **Action**, its current **Shortcut**,
and its **Source** — built-in, a user override, or the name of a script
that bound it.

- Type to filter both columns live, case-insensitively.
- Click a column header to sort by it; click again to reverse.
- `↑`/`↓`, `PgUp`/`PgDn`, or the mouse wheel move the selection.
- `Enter` on a row opens a prompt: type the new key as text — the same
  `C-`/`A-`/`S-` grammar a recorded macro uses, e.g. `C-S-k` for
  `Ctrl+Shift+K` — and `Enter` to confirm.
- `Delete` resets the selected row to its built-in default. It only does
  anything when the row's Source is a user override — you can't "reset"
  a binding you never changed.
- `Esc` closes the overlay.

Try it: type `duplicate` to filter down to "Duplicate line", press
`Enter`, type `C-S-k`, press `Enter` again. The Source column now shows
it as a user override, and `Ctrl+Shift+K` duplicates a line immediately —
no restart needed. Under the hood this wrote to `keybindings.toml` in
Vix's config directory (next to `config.toml`) via the same
`[[binding]]` format you could hand-edit yourself:

```toml
[[binding]]
key_token = "C-S-k"
action_id = "edit.duplicate_line"
```

Two overrides claiming the same key — from `keybindings.toml`, from a
[script](../../scripting/index.md), or one of each — are **both
rejected**, reported in the message drawer rather than one silently
winning. An override that simply claims a key a built-in binding already
used still wins outright, but is reported once so a built-in that stops
firing doesn't surprise you later.

A chorded keymap's chord *continuations* — Emacs's `Ctrl+X`/`Ctrl+C`
families, Spacemacs's `Space` leader — aren't listed in the editor and
can't be rebound this way; the override layer only resolves top-level
keys. If you hand-edit `keybindings.toml` directly (instead of through
the editor), pick up the change with **Tools → Reload Keybindings**
rather than restarting. Full detail:
[`crates/vix-keybindings/spec/index.md`](../../../crates/vix-keybindings/spec/index.md)
and
[`crates/vix-keybinding-editor-panel/spec/index.md`](../../../crates/vix-keybinding-editor-panel/spec/index.md).

## Snippets: inserting and creating

Snippets are reusable text with navigable tabstops, TextMate/VS
Code-style but stored as JSON. Two ways to use one:

- **Tools → Snippets…** lists every snippet in scope for the active
  buffer; type to filter by name, prefix, or description, `Enter`
  inserts it. Try it on `main.rs` — filter to `function` and you'll find
  the bundled **Rust function** snippet.
- **Prefix expansion**: type a snippet's prefix, then press `Tab`. The
  bundled snippets are picker-only (no prefix), but anything you save
  yourself gets one — see below.

And one way to make your own from text you already wrote —
**New Snippet from Selection**: select the `word_counts` function body in
`main.rs`, then **Tools → New Snippet from Selection…**. It prompts for a
**prefix**, which becomes both the saved snippet's name and its
expansion trigger — type `wc` and confirm. The selection is saved to your
**global** snippet file, `~/.config/vix/global/snippets/snippets.json`
(created if it doesn't exist), as ordinary JSON:

```json
{
  "wc": {
    "prefix": "wc",
    "body": [
      "fn word_counts(text: &str) -> HashMap<&str, u32> {",
      "\tlet mut counts = HashMap::new();",
      "\tfor word in text.split_whitespace() {",
      "\t\t*counts.entry(word).or_insert(0) += 1;",
      "\t}",
      "\tcounts",
      "}"
    ]
  }
}
```

Open a new, empty buffer, type `wc`, press `Tab` — it expands right back
out. Edit the file directly to add tabstops (`${1:placeholder}` for a
pre-filled, select-on-arrival field; `$1`, `$2`, … for empty ones visited
in order; `$0` for where the cursor lands last) — the picker-inserted
version and the prefix-expanded version both go through the same tabstop
engine, so either path benefits.

Snippets are gathered from several scopes and merged, later ones
shadowing earlier ones by name: **bundled** (built into Vix), **global**
(`~/.config/vix/global/snippets/*.json`, what you just wrote to),
**media-type** (config- or project-level, scoped to a buffer's media
type, e.g. `media-types/text/rust/snippets/*.json` for Rust files only),
and **project** (`<project root>/config/snippets/snippets.json` by
default — configurable with the `project_snippets` setting). Full format
and scope reference:
[`crates/vix-snippets/spec/index.md`](../../../crates/vix-snippets/spec/index.md).

## Settings: the config file

Everything above — the active theme's name, the active keymap's id, dock
widths, whether the explorer shows on startup — lives in one TOML file,
`config.toml`:

| Platform | Path |
| -------- | ---- |
| Linux | `~/.config/vix/config.toml` |
| macOS | `~/Library/Application Support/rs.vix/config.toml` |
| Windows | `%APPDATA%\vix\config\config.toml` |

**Vix → Settings…** opens it directly as an ordinary, editable buffer —
Vix saves your current in-app settings first, so what you see always
matches what's actually active right now. Most of what's in there you'll
never hand-edit: toggling line numbers, resizing a dock, or picking a
theme/locale/keymap from its menu already writes back here for you on
quit. A handful of settings, though, have **no menu equivalent** and can
only be set by editing this file — `wrap_column` (the column **Edit →
Wrap** fills text to; default `80`), `ai_command` (the shell template the
**AI** menu runs), `test_command`, `dictionary_path`, `contacts_dir`,
`project_snippets`, and a few more. The full list, with types and
defaults, is in
[`docs/reference/settings.md`](../../reference/settings.md) and
[`docs/configuration/index.md`](../../configuration/index.md).

One gotcha worth knowing before you hand-edit: Vix treats `config.toml`
as something it *writes*, not something it re-reads mid-session — it
loads the file once at startup and, from then on, persists its in-memory
copy back over the file every time it has reason to (choosing a theme,
changing the keymap, quitting). So an edit you type into that buffer and
save *while Vix is still running* gets silently overwritten the next time
that same session persists settings — including the moment you quit it
normally. **Make hand-edits with Vix closed** — any text editor, not just
Vix — then launch (or relaunch) Vix to pick them up. Try it: quit Vix,
edit `wrap_column` down to `60` in `config.toml` with your usual editor,
then reopen the demo workspace and run **Edit → Wrap** on a long line in
`notes.md` — it fills at the new width.

A few keys are **managed by Vix, not hand-edited** at all: `recent_files`,
`command_recents`, and `db_connections` are written back as you use the
editor, the same way the theme/keymap/locale choices are.

## Where to go next

- [`docs/configuration/index.md`](../../configuration/index.md) and
  [`docs/reference/settings.md`](../../reference/settings.md) — every
  settings key, generated and hand-written references.
- [`docs/themes/index.md`](../../themes/index.md) — the full theme JSON
  format, for writing one by hand instead of clicking through the editor.
- [`docs/keymaps/index.md`](../../keymaps/index.md) and
  [`docs/keybindings/index.md`](../../keybindings/index.md) — every
  binding, in every keymap.
- [`crates/vix-snippets/spec/index.md`](../../../crates/vix-snippets/spec/index.md)
  — the snippet file format and scope resolution in full.

---

**Previous:** [08 — HTTP Client & Tools Suite](../08-http-client-and-tools-suite/index.md)
**Next:** [10 — Debugging with DAP](../10-debugging-with-dap/index.md)

---

Vix™ and Vix IDE™ are trademarks.
