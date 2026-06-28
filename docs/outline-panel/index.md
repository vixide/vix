# Outline Panel

The outline panel lists the symbols in the active buffer — functions, structs,
modules, and more — so you can see a file's structure at a glance and jump to
any symbol.

## Opening the panel

Open it with **`Ctrl+Shift+B`**, or run **"Outline"** from the command palette.
The panel appears as a modal overlay over the editor.

On open, the panel selects the symbol the cursor is currently inside, so you
start at your current location in the file.

## Symbol list

Each entry shows a **kind prefix** followed by the symbol name. Prefixes
include `fn`, `struct`, `mod`, and `impl`, among others, which helps you tell at
a glance what kind of symbol each row is.

Symbols come from the same fast, offline, language-agnostic heuristic used by
go-to-symbol (`palette::symbols`), so no language server or network access is
required.

## Keybindings

| Key                       | Action                                          |
| ------------------------- | ----------------------------------------------- |
| `↑` / `↓`                 | Move the selection up / down one symbol         |
| `PageUp` / `PageDown`     | Move the selection by a page                    |
| `Home` / `End`            | Jump to the first / last symbol                 |
| `Enter`                   | Jump to the selected symbol and close the panel |
| `Esc`                     | Close the panel                                 |

## Mouse

A click on an entry jumps to that symbol in the file.

## Example

While editing a Rust file, press `Ctrl+Shift+B`. The panel lists entries such as
`struct App`, `impl App`, and `fn on_key`. Move to the symbol you want and press
`Enter` to jump straight to it.

## Outline sidebar (persistent)

Beyond the modal panel, a persistent **outline sidebar** can stay open while you
edit. Toggle it from **View → Layout → Outline Sidebar** (action
`view.outline_dock`); the preference persists in `show_outline_dock` (width
`outline_width`).

The sidebar shows the same symbol list and **follows the cursor**: its highlight
tracks the symbol you're currently inside, and the list rescans automatically as
you edit. **Click** any row to jump straight to that symbol. It docks on the right,
beside the editor.

## Roadmap

- A status-bar Outline button to toggle the sidebar.
- Collapsible nesting (group methods under their `impl`/`class`).
