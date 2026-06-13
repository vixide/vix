# Keymap chooser

Keyboard ways to navigate the editor, menus, file explorer, etc.

| Title | Tooltip        | Example: open file chooser | Example: quit |
| ----- | -------------- | -------------------------- | ------------- |
| Apple | Apple controls | control-o                  | control-q     |
| macOS VSCode | VS Code (macOS) bindings | control-p (Quick Open) | control-q |
| Emacs | Emacs chords   | control-x-f                | control-x-c   |
| Vim   | Vim modes      | :Ex                        | :q!           |

Exactly one keymap is active at a time.

The user can choose which keymap is active by using the "View" menu -> "Keymap..." menu item.

## As implemented in Vix

**Status:** Shipped. The chooser (`vix-keymap-chooser`) and a working subset of
each keymap's bindings are implemented; the choice persists in the `keymap`
setting (default `apple`). The sections below describe the broader philosophy; the
table here is exactly what Vix dispatches today. Menu mnemonics (`Alt+…`) and the
function keys (`F1`, `F3`, `F10`, `F12`) work in every keymap.

| Keymap | What Vix does today |
| ------ | ------------------- |
| **Apple** (default) | Modifier shortcuts — see `../docs/keybindings.md`. |
| **macOS VSCode** | VS Code's signature shortcuts: `Ctrl+P` Quick Open, `Ctrl+Shift+P` Command Palette, `Ctrl+Shift+O` Go to Symbol, `Ctrl+G` Go to Line, `Ctrl+B` sidebar, `Ctrl+Shift+E` explorer; plus the familiar editing chords `Ctrl+S` save, `Ctrl+W` close, `Ctrl+N` new, `Ctrl+F` find, `Ctrl+Shift+F` find in workspace, `Ctrl+R` replace, `Ctrl+/` comment. |
| **Emacs** | `Ctrl+X` prefix: `Ctrl+X Ctrl+F` open, `Ctrl+X Ctrl+S` save, `Ctrl+X Ctrl+C` quit, `Ctrl+X k` close. Motion: `Ctrl+F/B/N/P` char/line, `Ctrl+A/E` line ends, `Ctrl+V` page down, `Ctrl+D` delete, `Ctrl+S` find, `Ctrl+G` cancel. |
| **Vim** | Modal. Normal: `h/j/k/l`, `0`/`$`, `x`, `i/a/o/O` (→ Insert). Insert: `Esc` → Normal. Command line: `:w`, `:q`, `:q!`, `:wq`/`:x`, `:Ex`. The status bar shows `-- NORMAL --` / `-- INSERT --` / the `:` line. |

Not yet built (described below but not implemented): Vim counts and operators
(`3w`, `dd`, `gg`/`G`), Emacs `Meta`/`M-x` (Alt is reserved for menu mnemonics),
and registers / visual mode.

## Details

Keyboard navigation styles differ fundamentally based on how they treat the keyboard: as a way to trigger system actions (Apple), as a language for text manipulation (Vim), or as a deeply layered set of "chords" for executing functions (Emacs). [1, 2, 3]

### 1. Apple macOS System-Wide Shortcuts

Apple macOS focuses on accessibility and productivity across the entire operating system, using modifier keys to trigger menus and move focus between UI elements. [4, 5]

- Application Control: ⌘ Tab cycles between open apps; ⌘ Space opens Spotlight to launch anything instantly.
- UI Focus: Full Keyboard Access (enabled in Accessibility settings) allows you to use Tab and Shift Tab to highlight any button, menu, or field on the screen. Space then activates the highlighted item.
- Window Management: ⌘ ~ (Tilde) cycles between windows of the same app, while ^ ↑ (Control-Up) opens Mission Control for a birds-eye view. [5, 6, 7, 8, 9]

### 2. Vim: Modal Navigation

Vim is modal, meaning the keyboard's behavior changes depending on which "mode" you are in. It prioritizes keeping your hands on the "home row" to minimize movement. [10, 11, 12, 13, 14]

- Normal Mode: The default state where keys move the cursor rather than typing. h, j, k, and l act as arrow keys (Left, Down, Up, Right).
- Vim "Language": Navigation is like a sentence. Pressing 3w moves you forward 3 words; d$ deletes to the end of the line ($).
- Precision Jumps: Use gg to go to the start of a file, G for the end, or { / } to jump between paragraphs. [11, 15, 16, 17, 18]

### 3. Emacs: Chorded Navigation

Emacs uses modifiers and "chords"—combinations of keys held down together (like Ctrl or Alt)—to execute powerful functions without switching modes. [19, 20, 21, 22, 23]

- Basic Movement: ⌃ f (Forward), ⌃ b (Backward), ⌃ n (Next line), and ⌃ p (Previous line).
- Semantic Chunks: Modifiers change the scale of movement. ⌃ f moves one character, while ⌥ f (Meta-f) moves one whole word.
- Nested Commands: Complex actions often use sequences, such as ⌃ x ⌃ f to find/open a file or ⌃ x ⌃ c to exit. [19, 24, 25, 26, 27]

## Comparison Summary

| Feature      | macOS                      | Vim                            | Emacs                      |
| ------------ | -------------------------- | ------------------------------ | -------------------------- |
| Logic        | System commands & UI focus | Modal "language" (Verb + Noun) | Layered modifiers & chords |
| Primary Keys | Command (⌘) + Tab          | Home row (h, j, k, l)          | Control (⌃) + Meta (⌥)     |
| Philosophy   | Universal Accessibility    | Speed and Home Row efficiency  | Everything is a function   |
