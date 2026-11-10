# Vix for Fresh Users

Fresh is a modern terminal text editor written in Rust that brings CUA-style
keybindings and GUI-like workflows to the terminal. If you've been using Fresh
and appreciate its clean design, intuitive shortcuts, and focus on the editing
experience, Vix offers a compelling next step with even more built-in
capabilities — all while maintaining the terminal-first philosophy you love.

## The One-Minute Pitch

Vix is a terminal IDE that shares Fresh's DNA: both are written in Rust, both
prioritize keyboard-driven workflows, and both aim to provide a modern editing
experience in the terminal. Where Fresh excels at being a fast, focused editor,
Vix expands on that foundation by bundling additional IDE features that Fresh
users often rely on external tools for.

```sh
cargo run                    # open Vix in the current directory
# then: View -> Keymap -> Fresh
```

With the **Fresh keymap** active, your muscle memory will feel right at home.

## Your Muscle Memory, Preserved

Vix ships with a dedicated **Fresh keymap** that maps Fresh's familiar shortcuts:

| Fresh Key | Action | Vix Equivalent (Fresh keymap) |
|-----------|--------|------------------------------|
| `Ctrl+P` | Open file | `Ctrl+P` - Quick Open (fuzzy file finder) |
| `Ctrl+E` | Toggle explorer focus | `Ctrl+E` - Toggle Explorer Focus |
| `Ctrl+B` | Toggle explorer | `Ctrl+B` - Toggle Explorer |
| `Ctrl+S` | Save | `Ctrl+S` - Save |
| `Ctrl+Q` | Quit | `Ctrl+Q` - Quit |
| `Ctrl+N` | New file | `Ctrl+N` - New |
| `Ctrl+O` | Open path | `Ctrl+O` - Open Path |
| `Ctrl+F` | Find | `Ctrl+F` - Find |
| `Ctrl+R` | Find & Replace | `Ctrl+R` - Find & Replace |
| `Ctrl+/` | Toggle comment | `Ctrl+/` - Toggle Comment |
| `Ctrl+K` | Cut to line end | `Ctrl+K` - Cut to Line End |
| `Ctrl+W` | Select word | `Ctrl+W` - Select Word |
| `Ctrl+H` | Delete word left | `Ctrl+H` - Delete Word Left |
| `Ctrl+G` | Go to line | `Ctrl+G` - Go to Line |
| `Ctrl+T` | Transpose characters | `Ctrl+T` - Transpose Characters |
| `Ctrl+L` | Select line | `Ctrl+L` - Select Line |
| `Alt+Left` | Back | `Alt+Left` - Back |
| `Alt+Right` | Forward | `Alt+Right` - Forward |
| `Alt+Up` | Move line up | `Alt+Up` - Move Line Up |
| `Alt+Down` | Move line down | `Alt+Down` - Move Line Down |
| `Alt+.` | Code action | `Alt+.` - Code Action |
| `Alt+/` | Search workspace | `Alt+/` - Search in Workspace |
| `Ctrl+]` | Match bracket | `Ctrl+]` - Jump to Matching Bracket |

**Note**: On macOS, Vix automatically maps `Command` to `Control`, so Fresh's
CUA-style shortcuts work naturally.

## What You'll Recognize

Fresh users will feel immediately at home with:

- **CUA-style keybindings**: The familiar `Ctrl+S`, `Ctrl+C`, `Ctrl+V` workflow
- **Terminal-first design**: Fast, lightweight, runs anywhere
- **Modern Rust foundation**: Performance and safety you expect
- **No configuration required**: Works great out of the box
- **Fuzzy file finder**: `Ctrl+P` just like Fresh
- **Explorer sidebar**: `Ctrl+E` to focus, `Ctrl+B` to toggle
- **Multi-cursor support**: Built-in, no plugins needed
- **Tree-sitter syntax highlighting**: Rich syntax awareness

## What Vix Adds

Beyond Fresh's excellent editor core, Vix bundles features you'd typically need
separate tools or plugins for:

### Built-in Language Intelligence

- **LSP (Language Server Protocol)**: Diagnostics, hover, completion, rename,
  references, code actions, inlay hints, call hierarchy — all wired up automatically
- **DAP (Debug Adapter Protocol)**: Full debugger with breakpoints, stepping,
  call stack, variables, watches, and REPL
- **Tree-sitter**: Advanced syntax highlighting and code navigation

### Version Control

- **Git integration**: Stage hunks, commit, push, pull, log, blame, diff
- **Interactive commit graph**: Visualize your repository history
- **Hunk-by-hunk staging**: Fine-grained control over what gets committed
- **Merge conflict resolver**: Built-in tool for resolving merge conflicts

### Project Tools

- **Integrated terminal**: Full terminal emulator built into the editor
- **Task runner**: Automatic detection and running of npm, yarn, pnpm, bun,
  Deno, Composer, just, go-task, Rake, Make tasks
- **REST client**: Built-in HTTP client for `.http` files with environments
  and request chaining
- **Container menu**: Manage Docker and Podman containers
- **Database workbench**: Full SQLite, PostgreSQL, MySQL client with schema
  browser, query editor, and result grid

### Editing Power Features

- **Persistent undo tree**: Never lose your work; undo history persists across sessions
- **Undo tree visualization**: Browse and restore from any point in your edit history
- **Multiple cursors**: Edit multiple locations at once
- **Column editing**: Edit rectangular selections
- **Structural selection**: Select by syntax (function, class, etc.)
- **Snippets**: Built-in snippet support with tab stops
- **Macros**: Record and replay editing actions
- **Clipboard history**: Access previously copied text

### Workspace Features

- **Workspace search**: Fast fuzzy search across all files
- **Symbol navigation**: Jump to definitions, references, implementations
- **Outline view**: See the structure of your file
- **Todo tree**: See all TODO/FIXME comments in your project
- **Session management**: Save and restore your editing session
- **Tabs and splits**: Organize your workspace with tabs and split panes

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

### AI Integration

- **AI chat panel**: Chat with AI assistants
- **Inline completion**: Ghost-text suggestions from AI
- **MCP server**: Model Context Protocol support
- **Agent Client Protocol**: Integration with AI agents

### Desktop GUIs

- **8 desktop frontends**: Same editor, native window (GPUI, egui, iced,
  slint, gtk4, bui, zgui)

## Key Differences to Be Aware Of

While Vix aims to be familiar to Fresh users, there are some differences:

### What's the Same

- **Terminal-first philosophy**: Runs in any terminal, over SSH, in tmux
- **Keyboard-driven**: All features accessible via keyboard
- **Fast and lightweight**: Native binary, minimal dependencies
- **Modern Rust codebase**: Same language, similar architecture

### What's Different

| Feature | Fresh | Vix |
|---------|-------|-----|
| **Default keymap** | Fresh keymap | Apple keymap (switch to Fresh via View → Keymap) |
| **Integrated terminal** | No | Yes, built-in |
| **Git integration** | No | Yes, full client |
| **LSP/DAP** | Yes | Yes, more features |
| **Database workbench** | No | Yes |
| **Org mode** | No | Yes, full support |
| **Container management** | No | Yes |
| **REST client** | No | Yes |
| **Multi-cursor** | Yes | Yes, more features |
| **Undo tree** | Limited | Full persistent tree |
| **Session restore** | Yes | Yes, more comprehensive |

## Migrating from Fresh

### Step 1: Install Vix

```sh
# Using cargo (recommended for developers)
cargo install vix-tui

# Using Homebrew
brew install vixide/homebrew-tap/vix-tui

# Using npm
npm install -g @vixide/vix
```

### Step 2: Switch to Fresh Keymap

1. Open Vix: `vix .`
2. Press `Ctrl+Shift+P` (or use the menu bar)
3. Type `keymap` and select **View → Keymap → Fresh**

Or add to your config:

```toml
# ~/.config/vix/config.toml
keymap = "fresh"
```

### Step 3: Customize to Your Liking

Fresh users often have custom keybindings. In Vix, you can:

1. **Add custom bindings** via `keybindings.toml`:

```toml
# ~/.config/vix/keybindings.toml
[[bindings]]
key = "C-S-d"
action = "edit.duplicate_line"
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

Once you're comfortable with the basics, explore Vix's additional capabilities:

- **Git**: Press `Ctrl+Shift+P`, type `git`, explore the Git menu
- **Debugger**: Set breakpoints, start debugging from the Run menu
- **Database**: Connect to a database from the DB menu
- **Terminal**: Open with `` Ctrl+` `` (in most keymaps)
- **LSP**: Hover, go to definition, rename - all work automatically

## Where Fresh Might Still Win

Fresh is an excellent editor, and there are areas where it may still have advantages:

- **Simplicity**: Fresh's focused scope makes it easier to understand
- **Startup speed**: Fresh may be slightly faster for simple editing
- **Customization**: Fresh's configuration may feel more familiar to some users
- **Plugin ecosystem**: Fresh has a growing plugin system

However, for users who want **more built-in capabilities without sacrificing
the terminal-first experience**, Vix offers a compelling alternative.

## An Honest Experiment

Give Vix a try for a week on a real project:

1. Install Vix and switch to the Fresh keymap
2. Use your familiar Fresh shortcuts: `Ctrl+P`, `Ctrl+S`, `Ctrl+F`, etc.
3. Gradually explore Vix's additional features:
   - Try the Git panel (`Ctrl+Shift+P` → `git`)
   - Use the integrated terminal (`` Ctrl+` ``)
   - Try go-to-definition (`F12`)
   - Explore the LSP features
4. Notice how Vix handles things Fresh leaves to external tools:
   - Git staging and committing
   - Debugging
   - Database queries
   - Container management

## Learning Resources

### Official Documentation

- **[Main Documentation](https://vixide.github.io/vix/)** - Complete feature tour
- **[Keybindings Reference](../reference/keybindings-fresh.md)** - All Fresh bindings
- **[Settings Reference](../configuration/index.md)** - All configuration options
- **[Getting Started](../getting-started/index.md)** - Beginner's guide

### Migration Guides

- **[For VS Code Users](../vix-for-visual-studio-code-users/index.md)** - Compare with VS Code
- **[For Vim Users](../vix-for-vim-users/index.md)** - For Vim users
- **[For Emacs Users](../vix-for-emacs-users/index.md)** - For Emacs users
- **[For Helix Users](../vix-for-helix-users/index.md)** - For Helix users
- **[Feature Comparison](../compare/index.md)** - Full comparison matrix

### Community

- **[GitHub Discussions](https://github.com/vixide/vix/discussions)** - Ask questions, share ideas
- **[GitHub Issues](https://github.com/vixide/vix/issues)** - Report bugs, request features
- **Discord** - Real-time chat (link in README)

## Troubleshooting

### My Fresh shortcuts aren't working

Make sure you've switched to the Fresh keymap:
- `View → Keymap → Fresh`
- Or set `keymap = "fresh"` in `~/.config/vix/config.toml`

### How do I do [X] in Vix?

Check the command palette (`Ctrl+Shift+P` or `Ctrl+P` → `>`) for the action.
All actions are searchable.

### I miss Fresh's [feature]

Vix is actively developed. If there's a Fresh feature you miss:
1. Check if it exists under a different name
2. Search the issues for feature requests
3. Open a new issue with your feature request

### Performance seems slower than Fresh

Vix has more features built-in, which can affect startup time slightly.
However, editing performance should be comparable. If you notice
performance issues:
- Run `vix --doctor` to check for common issues
- Check your system resources
- Report the issue with details about your system

---

## Conclusion

If you love Fresh's terminal-first approach, keyboard-driven workflow, and
modern Rust foundation, Vix offers a natural next step with additional
built-in capabilities that eliminate the need for external tools and plugins.

Vix isn't trying to replace Fresh — it's offering a different take on what
a terminal editor can be. For Fresh users who find themselves wishing for
more built-in features without sacrificing the terminal experience, Vix
is worth exploring.

**Try it for a week. Your muscle memory will feel at home, and you might
discover features you didn't know you needed.**

---

Vix™ and Vix IDE™ are trademarks.
