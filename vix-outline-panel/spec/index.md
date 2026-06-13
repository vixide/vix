# vix-outline-panel

**Status:** Shipped (first cut) — **`Ctrl+Shift+B`** (or the command palette
**"Outline"**) opens a panel listing the active buffer's symbols, each with its
kind prefix (`fn`, `struct`, `mod`, `impl`, …) and name. `↑`/`↓` (and
PageUp/PageDown/Home/End) move the selection; `Enter` or a click jumps to the
symbol; `Esc` closes. On open it selects the symbol the cursor is currently
inside. Symbols come from the same fast, offline, language-agnostic heuristic as
go-to-symbol (`palette::symbols`).

Roadmap: a persistent (non-modal) side panel that stays open and auto-scrolls to
follow the cursor while editing, and a status-bar Outline button.

Outline Panel

subcrate: vix-outline-panel

In addition to the modal outline (cmd-shift-o), Vix offers an outline panel. The outline panel can be deployed via cmd-shift-b (outline panel: toggle focus via the command palette), or by clicking the Outline Panel button in the status bar.

When viewing a "singleton" buffer (i.e., a single file on a tab), the outline panel works similarly to that of the outline modal－it displays the outline of the current buffer's symbols. Each symbol entry shows its type prefix (such as "struct", "fn", "mod", "impl") along with the symbol name, helping you quickly identify what kind of symbol you're looking at. Clicking on an entry allows you to jump to the associated section in the file. The outline view will also automatically scroll to the section associated with the current cursor position within the file.
