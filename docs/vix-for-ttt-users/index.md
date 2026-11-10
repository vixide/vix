# Vix for ttt Users

If you've been using [ttt](https://github.com/eugenioenko/ttt) — the fast,
keyboard-driven terminal text editor written in Go — you'll find Vix to be a
natural fit. Both editors share a focus on speed, keyboard efficiency, and a
modern editing experience in the terminal. Vix builds on these principles
while adding comprehensive IDE features that ttt users often need separate
tools for.

## The One-Minute Pitch

Vix is a terminal IDE written in Rust that offers everything ttt does well
(fuzzy file finder, multi-cursor, LSP, themes, mouse support) and adds
a full suite of IDE features: Git integration, debugger, database workbench,
Org mode, HTTP client, container management, and more — all in a single
static binary.

```sh
cargo run                    # open Vix in the current directory
# then: View -> Keymap -> ttt
```

With the **ttt keymap** active, your fingers will already know most of the
shortcuts.

## Your Muscle Memory, Preserved

Vix ships with a dedicated **ttt keymap** that maps ttt's familiar shortcuts:

### File Operations

| ttt Key | Action | Vix Equivalent (ttt keymap) |
|---------|--------|------------------------------|
| `Ctrl+P` | Open file | `Ctrl+K P` or `Ctrl+P` - Quick Open |
| `Ctrl+S` | Save | `Ctrl+K S` or `Ctrl+S` - Save |
| `Ctrl+Q` | Quit | `Ctrl+Q` - Quit |
| `Ctrl+Shift+P` | Command palette | `Ctrl+Shift+P` - Command Palette |

### Navigation

| ttt Key | Action | Vix Equivalent (ttt keymap) |
|---------|--------|------------------------------|
| `Ctrl+E` | Explorer | `Ctrl+K E` - Toggle Explorer |
| `Ctrl+K E` | Focus explorer | `Ctrl+K E` - Toggle Explorer Focus |
| `Ctrl+O` | Open folder | `Ctrl+K O` - Open Path |
| `Ctrl+F` | Find | `Ctrl+F` - Find |
| `Ctrl+R` | Find & Replace | `Ctrl+R` - Find & Replace |
| `Ctrl+G` | Go to line | `Ctrl+G` - Go to Line |

### Editing

| ttt Key | Action | Vix Equivalent (ttt keymap) |
|---------|--------|------------------------------|
| `Ctrl+K C` | Copy | `Ctrl+C` - Copy |
| `Ctrl+K X` | Cut | `Ctrl+X` - Cut |
| `Ctrl+K V` | Paste | `Ctrl+V` - Paste |
| `Ctrl+K Z` | Undo | `Ctrl+Z` - Undo |
| `Ctrl+K Y` | Redo | `Ctrl+Y` - Redo (or `Ctrl+Shift+Z`) |
| `Ctrl+K A` | Select all | `Ctrl+A` - Select All |
| `Ctrl+D` | Duplicate line | `Ctrl+D` - Duplicate Line |
| `Ctrl+Shift+D` | Delete line | `Ctrl+Shift+D` - Delete Line |

### ttt-Specific Chords

Vix's ttt keymap preserves ttt's two-key `Ctrl+K` chord prefixes:

| ttt Chord | Action | Vix (ttt keymap) |
|-----------|--------|------------------|
| `Ctrl+K P` | Open file | `file.open` (Quick Open) |
| `Ctrl+K S` | Save as | `file.save_as` |
| `Ctrl+K F` | Format | `format.document` |
| `Ctrl+K [` | Toggle fold | `edit.toggle_fold` |
| `Ctrl+K 0` | Fold all | `edit.fold_all` |
| `Ctrl+K 9` | Unfold all | `edit.unfold_all` |
| `Ctrl+K L` | Toggle fold | `edit.toggle_fold` |
| `Ctrl+K J` | Unfold all | `edit.unfold_all` |
| `Ctrl+K E` | Explorer | `view.explorer` |
| `Ctrl+K O` | Open folder | `file.open_path` |
| `Ctrl+K C` | Copy | `edit.copy` |
| `Ctrl+K X` | Cut | `edit.cut` |
| `Ctrl+K V` | Paste | `edit.paste` |
| `Ctrl+K Z` | Undo | `edit.undo` |
| `Ctrl+K Y` | Redo | `edit.redo` |
| `Ctrl+K A` | Select all | `edit.select_all` |

### Window Management

| ttt Key | Action | Vix Equivalent (ttt keymap) |
|---------|--------|------------------------------|
| `Ctrl+K W` | Close tab | `Ctrl+W` - Close Tab |
| `Ctrl+K N` | New tab | `Ctrl+N` or `Ctrl+T` - New Tab |
| `Ctrl+K Up` | Previous tab | `Ctrl+PageUp` or `Ctrl+Shift+Tab` |
| `Ctrl+K Down` | Next tab | `Ctrl+PageDown` or `Ctrl+Tab` |
| `Ctrl+\\` | Split right | `Ctrl+\\` - Split Right |
| `Ctrl+K \\` | Split right | `Ctrl+K \\` - Split Right |

**Note**: Vix supports all the standard ttt shortcuts you're familiar with.

## What You'll Recognize

ttt users will immediately feel at home with:

- **Fuzzy file finder**: `Ctrl+P` or `Ctrl+K P` opens the quick file picker
- **Command palette**: `Ctrl+Shift+P` for all commands
- **Multi-cursor support**: Built-in, no plugins needed
- **Tree-sitter syntax highlighting**: Rich syntax awareness
- **LSP integration**: Language servers work automatically
- **Themes**: Multiple built-in themes, plus custom theme support
- **Mouse support**: Full mouse support (click, drag, wheel)
- **Terminal-first**: Runs in any terminal, over SSH, in tmux

## What Vix Adds

Beyond ttt's excellent editor core, Vix bundles features that ttt users
often rely on external tools for:

### Language Intelligence

- **LSP (Language Server Protocol)**: Full support with diagnostics, hover,
  completion, rename, references, code actions, inlay hints, call hierarchy,
  semantic tokens, document colors, and links
- **DAP (Debug Adapter Protocol)**: Full debugger with breakpoints, stepping,
  call stack, variables, watches, REPL, data breakpoints, and instruction
  breakpoints
- **On-type formatting**: Automatic formatting as you type
- **Pull diagnostics**: Get diagnostics from language servers

### Version Control

- **Git integration**: Full client with stage hunks, commit, push, pull,
  log, blame, diff, stash, branches, and more
- **Interactive commit graph**: Visualize your repository history
- **Hunk-by-hunk staging**: Fine-grained control over what gets committed
- **Merge conflict resolver**: Built-in tool for resolving merge conflicts
- **Jujutsu support**: Built-in support for the Jujutsu VCS

### Project Tools

- **Integrated terminal**: Full terminal emulator built into the editor
- **Task runner**: Automatic detection and running of npm, yarn, pnpm, bun,
  Deno, Composer, just, go-task, Rake, Make tasks
- **REST client**: Built-in HTTP client for `.http` files with environments,
  variables, and request chaining
- **Container menu**: Manage Docker, Podman, and nerdctl containers
- **Database workbench**: Full SQLite, PostgreSQL, MySQL client with schema
  browser, query editor, result grid, table editing, and schema compare

### Editing Features

- **Persistent undo tree**: Never lose your work; undo history persists
  across sessions with branching
- **Undo tree visualization**: Browse and restore from any point
- **Multiple cursors**: Edit multiple locations at once
- **Column editing**: Edit rectangular selections
- **Structural selection**: Select by syntax (function, class, etc.)
- **Snippets**: Built-in snippet support with tab stops
- **Macros**: Record and replay editing actions
- **Clipboard history**: Access previously copied text
- **Safe saving**: Writes to temp file then renames, preserving hard links

### Workspace Features

- **Workspace search**: Fast fuzzy search across all files
- **Symbol navigation**: Jump to definitions, references, implementations
- **Outline view**: See the structure of your file
- **Todo tree**: See all TODO/FIXME comments in your project
- **Session management**: Save and restore your editing session
- **Tabs and splits**: Organize with tabs and split panes
- **Minimap**: Visual overview of your file
- **Breadcrumbs**: Shows your location in the file structure

### Org Mode

- **Full Org mode support**: Headlines, TODO cycling, checkboxes, folding
- **Agenda**: See all your tasks in one view
- **Org-roam**: Note-taking with backlinks and graph visualization
- **Tables**: Spreadsheet-style tables with formulas
- **Column View**: Compact view of your Org data
- **Export**: Export to Markdown, HTML, and other formats

### Collaboration

- **Cowork**: Real-time collaborative editing (opt-in build feature)
- **IM (Instant Messaging)**: Code-aware chat with IRC and Matrix support
- **path:line references**: Share links to specific locations in files
- **Code-aware messages**: Threads anchored to lines in your code

### AI Integration

- **AI chat panel**: Chat with AI assistants
- **Inline ghost-text completion**: Suggestions appear inline
- **MCP server**: Model Context Protocol support (`vix --mcp`)
- **Agent Client Protocol**: Integration with AI agents
- **AI providers**: Support for Anthropic, OpenAI, Ollama, and custom endpoints

### Desktop GUIs

- **8 desktop frontends**: Same editor, native window (GPUI, egui, iced,
  slint, gtk4, bui, zgui)

## Key Differences

### What's the Same

- **Terminal-first philosophy**: Runs in any terminal
- **Keyboard-driven**: All features accessible via keyboard
- **Fast and lightweight**: Native binary, minimal dependencies
- **Modern approach**: Written in modern languages (Rust vs Go)
- **Fuzzy finder**: `Ctrl+P` for files
- **Command palette**: Search and run commands
- **Multi-cursor**: Edit multiple locations
- **LSP**: Language server support

### What's Different

| Feature | ttt | Vix |
|---------|-----|-----|
| **Language** | Go | Rust |
| **Default keymap** | ttt keymap | Apple keymap (switch to ttt via View → Keymap) |
| **Integrated terminal** | No | Yes, built-in |
| **Git integration** | No | Yes, full client |
| **Debugger** | No | Yes, full DAP debugger |
| **Database workbench** | No | Yes |
| **Org mode** | No | Yes, full support |
| **Container management** | No | Yes |
| **REST client** | No | Yes |
| **Jujutsu support** | No | Yes |
| **Multi-cursor depth** | Good | More features (select all occurrences, column select) |
| **Undo** | Linear | Branching tree with visualization |
| **Session restore** | Yes | More comprehensive |
| **Feature breadth** | Editor-focused | Full IDE features |

## Migrating from ttt

### Step 1: Install Vix

```sh
# Using cargo (recommended for developers)
cargo install vix-tui

# Using Homebrew
brew install vixide/homebrew-tap/vix-tui

# Using npm
npm install -g @vixide/vix
```

### Step 2: Switch to ttt Keymap

1. Open Vix: `vix .`
2. Press `Ctrl+Shift+P` (or use the menu bar)
3. Type `keymap` and select **View → Keymap → ttt**

Or add to your config:

```toml
# ~/.config/vix/config.toml
keymap = "ttt"
```

### Step 3: Customize Your Setup

ttt users often have custom configurations. In Vix, you can:

1. **Add custom bindings** via `keybindings.toml`:

```toml
# ~/.config/vix/keybindings.toml
[[bindings]]
key = "C-k-c"
action = "edit.copy"

[[bindings]]
key = "C-k-v"
action = "edit.paste"
```

2. **Create custom scripts** in `~/.config/vix/scripts/*.rhai`:

```rhai
// Example: Insert current date
fn insert_date() {
    let date = now();
    buffer_set_text(date);
}
```

3. **Create custom themes** in `~/.config/vix/themes/*.json`

### Step 4: Explore Vix's Additional Features

Once you're comfortable with the basics, explore what Vix adds:

- **Git**: Press `Ctrl+Shift+P`, type `git`, explore the Git menu
- **Debugger**: Set breakpoints, start debugging from the Run menu
- **Database**: Connect to a database from the DB menu
- **Terminal**: Open with `` Ctrl+` `` (in most keymaps)
- **LSP**: Hover, go to definition, rename - all work automatically
- **Org mode**: Create Org files and use all Org features

## Where ttt Might Still Win

ttt is an excellent editor, and there are areas where it may have advantages:

- **Written in Go**: Simpler runtime, potentially easier to hack on
- **Single binary**: Slightly smaller binary size
- **Focus**: More focused on being a great editor
- **Plugin system**: Growing plugin ecosystem in Go

However, for users who want **more built-in capabilities without sacrificing
the terminal experience**, Vix offers a compelling superset of ttt's features.

## An Honest Experiment

Try Vix for a week on a real project:

1. Install Vix and switch to the ttt keymap
2. Use your familiar ttt shortcuts: `Ctrl+K P`, `Ctrl+K S`, `Ctrl+E`, etc.
3. Notice how the `Ctrl+K` chords you're used to work the same way
4. Gradually explore Vix's additional features:
   - Try the Git panel (`Ctrl+Shift+P` → `git`)
   - Use the integrated terminal (`` Ctrl+` ``)
   - Try go-to-definition (`F12`)
   - Explore the LSP features (hover, completion, rename)
   - Connect to a database (DB menu)
5. Experience having all these tools available without leaving your editor

## Learning Resources

### Official Documentation

- **[Main Documentation](https://vixide.github.io/vix/)** - Complete feature tour
- **[Keybindings Reference](../reference/keybindings-ttt.md)** - All ttt bindings
- **[Settings Reference](../configuration/index.md)** - All configuration options
- **[Getting Started](../getting-started/index.md)** - Beginner's guide

### Migration Guides

- **[For VS Code Users](../vix-for-visual-studio-code-users/index.md)** - Compare with VS Code
- **[For Vim Users](../vix-for-vim-users/index.md)** - For Vim users
- **[For Emacs Users](../vix-for-emacs-users/index.md)** - For Emacs users
- **[For Helix Users](../vix-for-helix-users/index.md)** - For Helix users
- **[For Fresh Users](../vix-for-fresh-users/index.md)** - For Fresh users
- **[For Sublime Users](../vix-for-sublime-users/index.md)** - For Sublime users
- **[Feature Comparison](../compare/index.md)** - Full comparison matrix

### Community

- **[GitHub Discussions](https://github.com/vixide/vix/discussions)** - Ask questions, share ideas
- **[GitHub Issues](https://github.com/vixide/vix/issues)** - Report bugs, request features
- **Discord** - Real-time chat (link in README)

## Troubleshooting

### My ttt shortcuts aren't working

Make sure you've switched to the ttt keymap:
- `View → Keymap → ttt`
- Or set `keymap = "ttt"` in `~/.config/vix/config.toml`

The `Ctrl+K` chords are part of the ttt keymap, so they won't work until you
switch to it.

### How do I do [X] in Vix?

Check the command palette (`Ctrl+Shift+P` or `Ctrl+P` → `>`) for the action.
All actions are searchable.

### I miss ttt's [feature]

Vix is actively developed. If there's a ttt feature you miss:
1. Check if it exists under a different name
2. Search the issues for feature requests
3. Open a new issue with your feature request

### Performance seems different than ttt

Vix has more features built-in, which can affect resource usage slightly.
However, editing performance should be excellent. If you notice performance
issues:
- Run `vix --doctor` to check for common issues
- Check your system resources
- Report the issue with details about your system

---

## Security Notes

Like ttt, Vix takes security seriously:

- **Zero `unsafe` Rust**: No unsafe code anywhere in the codebase
- **Sandboxed scripts**: Rhai scripts have no file/network/process access
- **Workspace trust**: Project scripts run only after you trust the workspace
- **Keyring integration**: DB passwords and API keys use native credential stores
- **Regular audits**: Security audits are conducted regularly

See [SECURITY.md](../../SECURITY.md) for the full security policy.

---

## Conclusion

If you love ttt's speed, keyboard efficiency, and terminal-first approach, Vix
offers a natural evolution with additional built-in capabilities. Vix preserves
ttt's excellent editing experience while adding comprehensive IDE features
that eliminate the need for external tools.

Vix isn't trying to replace ttt — it's offering a different vision of what
a terminal editor can be. For ttt users who find themselves wanting more
built-in functionality, Vix is a compelling choice.

**Try it for a week. Your muscle memory will feel at home, and you'll
discover features that make your workflow even more efficient.**

---

Vix™ and Vix IDE™ are trademarks.
