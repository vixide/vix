# Getting Started

## Install

Pick whichever fits how you work. All of these ship the same binary; none
requires a Rust toolchain except building from source.

**macOS or Linux, via Homebrew:**

```sh
brew install vixide/homebrew-tap/vix
```

**macOS or Linux, via the installer script** (downloads the right prebuilt
archive for your platform and puts `vix` on your `PATH`):

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/vixide/vix/releases/latest/download/vix-installer.sh | sh
```

**Windows, via PowerShell:**

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/vixide/vix/releases/latest/download/vix-installer.ps1 | iex"
```

**Via npm** (any platform npm supports):

```sh
npm install -g @vixide/vix
```

**Direct download:** prebuilt archives (`.tar.xz` for macOS/Linux, `.msi`/
`.zip` for Windows) are attached to every
[GitHub Release](https://github.com/vixide/vix/releases).

**From source** (needs a Rust toolchain, 1.96+):

```sh
git clone https://github.com/vixide/vix.git
cd vix
cargo build --release   # binary at target/release/vix
```

There is no `.deb` package yet — see
[`spec/debian/index.md`](../../spec/debian/index.md) if you're interested in
adding one.

## First launch

```sh
vix              # open rooted at the current directory
vix src/main.rs  # open one or more files directly
```

The first time Vix runs anywhere, it shows a scrollable welcome overlay —
see [`docs/welcome-panel/index.md`](../welcome-panel/index.md). Press `Esc`
to close it; it won't show automatically again (reopen any time from
**Help → Welcome…**).

For the file/folder/clock glyphs in the explorer and status areas to render
as intended, use a [Nerd Font] in your terminal. Vix works without one — it
just falls back to plainer characters.

## The 10 things to learn first

Vix's keybindings vary by **keymap** (Apple, VSCode, Emacs, Vi, Spacemacs,
IntelliJ, Eclipse, Sublime Text — see **View → Keymap**, and
[`docs/reference/`](../reference/actions.md) for the full binding tables).
Everything below shows the **Apple** keymap, Vix's default — every action is
also always reachable from its menu or the command palette regardless of
keymap.

1. **Open a file** — `Ctrl+O` (**File → Open…**), or browse the file
   explorer (see below) and press `Enter` on an entry. `Ctrl+N` starts a new,
   unsaved buffer.
2. **Save** — `Ctrl+S` (**File → Save**). `Ctrl+Shift+S` is Save As.
3. **The command palette** — `Ctrl+P` (**Tools → Command Palette…**) is the
   fastest way into anything else in this list, and everything not listed
   here: type to fuzzy-search files (default) or prefix with `>` to search
   *commands* by name instead of remembering a keybinding.
4. **The file explorer** — `Ctrl+B` toggles it, `Ctrl+E` focuses it directly.
   It's the left-hand pane in the screenshot at the top of this repo's
   [`index.md`](../../index.md).
5. **Find (and replace)** — `Ctrl+F` finds in the current file; `Ctrl+R` is
   find & replace; `Ctrl+Shift+F` searches the whole workspace. See
   [`docs/find/index.md`](../find/index.md).
6. **Panes and docks** — the explorer (left), messages (right), and an
   optional bottom dock (terminal, test results, DB workbench, …) frame the
   editor's tab area in the middle; `F6` moves focus to the other editor
   pane when more than one is open. See
   [`docs/left-dock/index.md`](../left-dock/index.md),
   [`docs/right-dock/index.md`](../right-dock/index.md), and
   [`docs/bottom-dock/index.md`](../bottom-dock/index.md).
7. **Undo and redo** — `Ctrl+Z` / `Ctrl+Shift+Z`. Unlike most bindings in
   this list, these two are wired the same in *every* keymap — they never
   change.
8. **Git** — **Git → Changes…** opens the working-tree status panel (stage,
   unstage, diff, commit); see
   [`docs/git-panel/index.md`](../git-panel/index.md) for the rest (blame,
   log, conflicts, and more).
9. **Help** — `F1` opens a searchable overlay of every keyboard shortcut in
   your active keymap. **Help** also has the welcome screen, license, and
   issue-reporting link.
10. **Quit** — `Ctrl+Q` (**File → Quit**). Vix prompts before discarding any
    unsaved buffer.

## Where to go next

- [`docs/index.md`](../index.md) — the full documentation map.
- [`docs/configuration/index.md`](../configuration/index.md) — every
  settings key, and where the config file lives.
- [`docs/keybindings/index.md`](../keybindings/index.md) and
  [`docs/keymaps/index.md`](../keymaps/index.md) — the complete keybinding
  reference and how to switch keymaps.
- The `for-*-users` pages (in
  [`docs/SUMMARY.md`](../SUMMARY.md#guides)) if you're coming from Vim,
  Emacs, Spacemacs, IntelliJ, Eclipse, Sublime Text, or VS Code.

---

Vix™ and Vix IDE™ are trademarks.
