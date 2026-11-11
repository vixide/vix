# Frequently Asked Questions

This page collects common questions and troubleshooting tips for Vix users.

## General

### What is Vix?

Vix is a keyboard-friendly terminal text editor and IDE built in Rust. It
combines familiar shortcuts (Ctrl+S to save, Ctrl+F to find) with powerful
features like LSP, DAP debugging, Git integration, and a database workbench —
all in a single binary that runs in your terminal.

### How do I install Vix?

See the [Install](#install) section in the README or the
[Getting Started](getting-started/index.md) guide.

### How do I update Vix?

The update method depends on how you installed:

- **Homebrew**: `brew upgrade vix-tui`
- **npm**: `npm update -g @vixide/vix`
- **cargo**: `cargo install --force vix-tui`
- **From source**: `git pull && cargo build`

### What platforms does Vix support?

Vix runs on Linux, macOS, and Windows. It supports x86_64 and aarch64
(ARM64) architectures on all platforms.

### Does Vix support my language?

Vix supports any language with a Language Server Protocol (LSP) server. It
also has built-in Tree-sitter highlighting for many languages. See the
[LSP setup guide](tutorials/05-setting-up-lsp/index.md) for details.

---

## Getting Started

### I'm new to Vix. Where should I start?

Try the interactive tutorial:

```sh
vix --tutor
```

Or work through the [written tutorials](tutorials/).

### How do I open a file?

- `vix <filename>` from the command line
- `Ctrl+P` then type the filename (Quick Open / fuzzy finder)
- `Ctrl+O` to open a file dialog

### How do I save a file?

`Ctrl+S` — just like most other applications.

### How do I quit Vix?

`Ctrl+Q` to quit. `Ctrl+W` to close the current tab.

### How do I see all keyboard shortcuts?

Press `F1` to see every shortcut in a help panel.

---

## Editing

### How do I enable multi-cursor editing?

- `Ctrl+D` to select the next occurrence of the word under cursor
- `Ctrl+Shift+L` to select all occurrences of the current selection
- `Alt+Click` (or `Ctrl+Click` on some keymaps) to add cursors manually

See the [multi-cursor tutorial](tutorials/03-find-replace-and-multi-cursor/index.md).

### How do I do column/rectangular selection?

- `Ctrl+Shift+Up/Down` to add cursors in a column
- `Shift+Alt+Drag` to select a rectangle with the mouse

### How do I comment out a line?

- `Ctrl+/` to toggle the current line's comment
- Select multiple lines and press `Ctrl+/` to comment/uncomment them all

### How do I find and replace?

- `Ctrl+F` to open the find bar
- `Ctrl+H` to open find and replace
- `F3`/`Shift+F3` to navigate between matches

### How do I undo/redo?

- `Ctrl+Z` to undo
- `Ctrl+Y` or `Ctrl+Shift+Z` to redo

Vix has a persistent undo tree: you can undo changes even after switching
files or closing and reopening Vix.

### How do I cut/copy/paste?

- `Ctrl+X` to cut
- `Ctrl+C` to copy
- `Ctrl+V` to paste

These work with the system clipboard.

---

## Performance

### Vix feels slow on my machine. What can I do?

1. **Check your terminal**: Vix requires a reasonably fast terminal emulator.
   Slow terminals (like some older terminal multiplexers) can bottleneck Vix.

2. **Reduce features**: Start Vix with `--no-default-features` for a minimal
   build with no LSP, DAP, or other optional features.

3. **Check file size**: Very large files (>100MB) may take longer to open.
   Vix is optimized for files up to 100MB (opens in ~14ms).

4. **Disable plugins**: If you have Rhai scripts or other customizations, try
   disabling them.

### Why does Vix use so much memory?

Vix loads the entire file into memory for fast editing. For a 100MB file,
this is expected. The memory usage is roughly proportional to the file
size plus the number of open buffers.

---

## Features

### How do I use Git in Vix?

- `Alt+G` to open the Git panel
- `G` then `S` to stage changes
- `G` then `C` to commit
- `G` then `L` to view the commit log

See the [Git workflow tutorial](tutorials/04-the-git-workflow/index.md).

### How do I use LSP (code completion, go-to-definition)?

LSP is enabled automatically if a language server is available on your
system. See the [LSP setup guide](tutorials/05-setting-up-lsp/index.md).

To check if LSP is working:
- Open a file in a supported language
- Move the cursor to a symbol and press `F12` for go-to-definition
- Or use `Ctrl+Space` for autocomplete

### How do I debug my code?

Vix has a built-in DAP debugger. Set breakpoints with `F9`, start debugging
with `F5`, and use `F10`/`F11` to step through code.

See the [debugging tutorial](tutorials/10-debugging-with-dap/index.md).

### How do I use the database workbench?

- `Alt+D` to open the DB menu
- Select **Connections…** to add or select a database connection
- Use the workbench to run queries, browse schemas, and export data

See the [DB workbench tutorial](tutorials/07-the-db-workbench/index.md) and
the [deep dive documentation](db/index.md).

### How do I use Org mode?

Vix has comprehensive Org mode support:
- Headlines with `*` prefixes
- TODO cycling with `Ctrl+Enter`
- Tables with formulas
- Agenda views
- Org-roam for linked notes

See the [Org mode tutorial](tutorials/06-org-mode-and-roam/index.md).

### How do I use the HTTP client?

Create a `.http` file and write HTTP requests in it. Vix will recognize the
file type and allow you to send requests.

See the [HTTP client documentation](http-client/index.md).

---

## Customization

### How do I change the theme?

- `Alt+T` to open the theme picker
- Use arrow keys to preview themes
- Press Enter to apply a theme

Or set `theme = "your-theme-name"` in your `config.toml`.

### How do I change the keymap?

- `Alt+K` to open the keymap picker
- Select from: Apple, VS Code (macOS/Windows), Emacs, Vi, Spacemacs,
  Helix, IntelliJ (macOS/Windows), Eclipse, Sublime, Zed, Fresh, ttt

Or set `keymap = "your-keymap"` in your `config.toml`.

### Where is the configuration file?

Configuration files are located in:

- **Linux**: `~/.config/vix/config.toml`
- **macOS**: `~/Library/Application Support/vix/config.toml`
- **Windows**: `%APPDATA%\vix\config.toml`

### How do I add a custom snippet?

Create a `.snippets.toml` file in your configuration directory. See the
[Snippets documentation](snippets/index.md).

### How do I write a custom script?

Vix uses Rhai for scripting. Create a `.rhai` file and add it to your
scripts directory. See the [Scripting documentation](scripting/index.md).

---

## Troubleshooting

### Vix won't start

Try running with verbose output:

```sh
RUST_LOG=debug vix
```

This will show detailed information about what Vix is doing during startup.

### I get an error about missing dependencies

Vix requires a C++ compiler for some features (Tree-sitter grammars).
Install `g++` or `clang++` on your system.

- **Ubuntu/Debian**: `sudo apt install build-essential`
- **Fedora**: `sudo dnf install gcc-c++`
- **macOS**: `xcode-select --install`
- **Windows**: Install Visual Studio with C++ support

### LSP isn't working

1. Make sure you have a language server installed for your language
2. Check that the language server is in your PATH
3. Run `vix --doctor` to check for issues
4. Enable LSP logging with `RUST_LOG=lsp=vix` and check the output

### Git integration isn't working

1. Make sure Git is installed on your system
2. Check that Git is in your PATH
3. Run `vix --doctor` to verify Git integration

### The terminal is slow or glitchy

1. Try a different terminal emulator (Alacritty, Kitty, WezTerm work well)
2. Make sure your terminal supports truecolor
3. Try disabling some Vix features to isolate the issue

### Colors look wrong

1. Make sure your terminal supports truecolor (`COLORTERM=truecolor`)
2. Try a different theme
3. Check if your terminal has a custom color scheme that's interfering

### Text rendering is broken

1. Make sure your terminal font supports all the characters you're using
2. Try a different font
3. Check if the issue is specific to certain characters or file encodings

---

## Configuration

### How do I change settings?

Edit your `config.toml` file. Common settings include:

```toml
[editor]
font_size = 14
line_height = 1.5

[theme]
name = "Dark"

[keymap]
name = "VSCode"
```

### Where can I find all available settings?

See the [Configuration documentation](configuration/index.md) for a
complete list of all settings.

---

## Community and Support

### Where can I ask questions?

- [GitHub Discussions](https://github.com/vixide/vix/discussions) —
  Ask questions, discuss features, and get help from the community
- [GitHub Issues](https://github.com/vixide/vix/issues) — Report bugs

### How do I contribute to Vix?

See the [CONTRIBUTING.md](CONTRIBUTING.md) guide.

### How do I report a bug?

1. Check if the issue has already been reported
2. Create a new issue with:
   - A clear title
   - Steps to reproduce
   - Expected vs actual behavior
   - Your OS and terminal
   - Vix version (`vix --version`)

---

## License

Vix is licensed under your choice of:
- Apache 2.0
- BSD 3-Clause
- MIT
- GPL 2.0
- GPL 3.0

See the [LICENSE](LICENSE) file for details.

---

Vix™ and Vix IDE™ are trademarks.
