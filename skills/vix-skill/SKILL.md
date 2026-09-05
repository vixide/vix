---
name: vix-skill
description: Help someone use the Vix™ IDE editor day-to-day — keybindings, menus, the command palette, keymaps, settings, and features. Use whenever a Vix user asks "how do I do X in Vix", asks what a shortcut/menu item does, wants help configuring Vix, or is troubleshooting the app itself. Not for editing Vix's own source code — see vix-maintainer-skill for that.
---

# vix-skill — using the Vix™ editor

Vix is a keyboard-friendly terminal text editor (a "Simple Terminal Rust IDE")
built on `ratatui`. This skill is a cheat sheet for helping someone *use* it —
not for changing its source. If the question is about modifying Vix's own
code, defer to `vix-maintainer-skill` instead.

If a checkout of the `vix` repo is available (e.g. you're running inside one,
or the user points you at one), prefer its `docs/<topic>/index.md` files and
`index.md` for exhaustive, currently-accurate detail over what's summarized
here — this file is a fast, self-contained reference, not the full manual.
Otherwise answer from what's below; it covers the commands people ask about
most.

## The menu bar

Left to right: **Vix · File · Edit · View · Go · Run · AI · DB · Git · Org ·
Tools · Help**. Open the bar with `F10`, or jump straight to a menu with its
Alt mnemonic (`Alt+F` File, `Alt+E` Edit, `Alt+I` View, `Alt+N` Go, `Alt+R`
Run, `Alt+A` AI, `Alt+D` DB, `Alt+G` Git, `Alt+O` Org, `Alt+T` Tools, `Alt+H`
Help). Arrows navigate, Enter runs the highlighted item, Esc closes. Typing a
letter jumps to the next item starting with it (type-ahead), and `▸` marks a
submenu (Right/click opens it, Left/Esc backs out). `F1` opens a searchable
keyboard-shortcut browser if someone just wants to type and filter instead of
memorizing this list.

## Keymaps — pick the navigation style you already know

**View → Keymap…** switches between ten keyboard styles, and the choice
persists (`keymap` setting): **Apple** (default), **VSCode macOS/Windows**,
**Emacs**, **Vi**, **Spacemacs**, **IntelliJ macOS/Windows**, **Eclipse**, and
**Sublime Text**. On macOS, `Command` does whatever `Control` does in every
keymap (the terminal must forward the shortcut for this to work). If someone
says "this doesn't feel like Vix, it feels like my old editor" — that's the
point; ask which keymap they're on, or suggest switching to the one matching
their muscle memory.

## Global keybindings (Apple keymap — the default)

| Shortcut | Action |
| --- | --- |
| `Ctrl+N` / `Ctrl+O` / `Ctrl+Shift+O` | New buffer / Open file… / Open Recent… |
| `Ctrl+S` / `Ctrl+Shift+S` | Save / Save As… |
| `Ctrl+W` / `Ctrl+Shift+T` | Close tab / Reopen last closed tab |
| `Ctrl+Q` | Quit |
| `Ctrl+P` | Command palette |
| `Ctrl+F` / `Ctrl+R` / `Ctrl+Alt+R` | Find / Find & Replace / Interactive query-replace |
| `Ctrl+Shift+F` | Search across the whole workspace |
| `F3` / `Shift+F3` | Find next / previous |
| `Alt+N` / `Alt+P` | Find next/previous occurrence of the current selection |
| `Ctrl+B` / `Ctrl+E` | Toggle file explorer / focus explorer↔editor |
| `Alt+Left` / `Alt+Right` | Position history: back / forward |
| `F12` | Go to definition under the cursor |
| `F10` | Open/close the menu bar |
| `F1` | Searchable keyboard-shortcut browser |

Other keymaps remap these to their own conventions (e.g. Emacs `Ctrl+X
Ctrl+F` to open, Vi `:q!` to quit) — if a shortcut above doesn't work, check
**View → Keymap…** first.

## Command palette (`Ctrl+P`)

One prompt, five modes via a leading prefix character:

| Prefix | Mode | Finds |
| --- | --- | --- |
| *(none)* | File finder | Fuzzy-match files in the workspace |
| `>` | Commands | Search and run any editor command |
| `#` | Buffers | Switch between open buffers by name |
| `:` | Go to line | Jump to a line number |
| `@` | Symbols | Jump to a declaration in the current file |

Space-separated terms match independently (`feat group` matches
`features/groups/view.tsx`). `Tab` accepts the top suggestion, `Enter`
commits the highlighted one. Append `:<line>[:<col>]` to a filename in the
file finder to jump straight to that position after opening.

## Settings and configuration

Vix stores settings as TOML via [`confy`], one file, no separate preferences
dialog — **Vix → Settings…** opens it directly in the editor like any other
file:

| Platform | Path |
| --- | --- |
| Linux | `~/.config/vix/config.toml` |
| macOS | `~/Library/Application Support/rs.vix/config.toml` |
| Windows | `%APPDATA%\vix\config\config.toml` |

It's safe to hand-edit or delete — a missing file or unknown/missing key
falls back to defaults, and it's recreated with defaults on first save. Some
settings people ask about often: `keymap` (navigation style), `theme`
(`"dark"`/`"light"`/a custom theme name), `locale` (UI language), `soft_wrap`,
`line_numbers`, `indent_style`/`tab_width`, `format_on_save`, `auto_save`,
`persistent_undo`, `lsp_servers` (per-extension language servers, empty by
default — Vix ships none built in), `ai_command` (the shell command the **AI**
menu runs). The full table with every key, type, default, and meaning is in
`docs/configuration/index.md` when a checkout is available.

## Feature areas, and where to point someone for depth

Vix bundles a large feature set; skim the list below and reach for
`docs/<topic>/index.md` (or ask a follow-up) rather than guessing at
specifics you're not sure of:

- **Editing** — multi-cursor and column/block editing, surround, align,
  increment/decrement, comment toggle/banners, Emmet expansion, snippets
  (prefix + Tab), smart Home, auto-pair brackets, format-on-save/auto-save.
- **Navigation** — go to definition/symbol, matching-tag jump, go to
  percent/byte, jump-to-line-label, structural selection, position history.
- **Find** — incremental search, regex/case/smart-case/whole-word, capture
  groups, workspace-wide search/replace, interactive query-replace.
- **Edit surfaces** (Edit → Mode) — view/edit the buffer as a CSV/TSV table,
  a folding outline, a JSON/YAML tree, a hex byte dump, or SQL statements.
- **Git** (Git menu) — status/diff/blame, stage/unstage/revert per hunk,
  branch switch & merge, stash, amend, merge-conflict resolver.
- **DB workbench** (DB menu) — connect to SQLite/PostgreSQL/MySQL, browse
  schema, run queries with autocomplete, natural-language-to-SQL assistant.
- **Org mode** (Org menu) — headline/TODO/checkbox editing, table editor with
  `TBLFM`, Column View, Org-roam nodes/dailies/backlinks.
- **Project / tasks** (Run, Tools menus) — per-project-type lifecycle
  commands, discovered npm/Make/Rake/etc. tasks, test-at-point, integrated
  terminal, debugger (DAP).
- **LSP** — diagnostics, hover, completion, go-to, configured via
  `lsp_servers` (empty by default; point it at any language server binary).

If a checkout is available, `docs/` has one directory per topic above (and
many more — `docs/for-emacs-users/`, `docs/for-vim-users/`, etc., for
migrating from another editor); `index.md` at the repo root has the full
feature list and an ASCII screenshot of the layout.

## Troubleshooting

- **"A shortcut doesn't do what I expect"** — check `keymap` first (View →
  Keymap…); the same physical key can mean different things across keymaps.
- **"Vix looks/behaves wrong after an update"** — settings are
  forward-compatible (missing keys default), so a stale `config.toml` is
  rarely the cause; a corrupt file can safely be deleted and will be
  recreated with defaults.
- **"I want a fresh start"** — delete the config file at the path above; no
  other state needs clearing for settings alone (undo history, macros, and
  session state live in sibling files in the same config directory and are
  independently safe to delete).
- **"How do I get to X's exact keybinding/menu path"** — `F1`'s searchable
  keyboard-shortcut browser is faster than reciting the tables here for a
  one-off lookup.

[`confy`]: https://crates.io/crates/confy
