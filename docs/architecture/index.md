# Vix Architecture Overview

Vix is a **keyboard-friendly terminal IDE** built with a clean, modular
architecture. This document provides a high-level overview of Vix's architecture
to help new contributors understand how the pieces fit together.

## Philosophy

Vix follows several key architectural principles:

1. **Crate-per-feature**: Each major feature lives in its own Rust crate
2. **Spec-driven**: Every crate has a specification that is the source of truth
3. **Zero `unsafe`**: The entire codebase contains no `unsafe` Rust code
4. **Terminal-first**: Designed to work perfectly in terminals, over SSH, in tmux
5. **Progressive enhancement**: Core editing works without LSP, Git, or other features

## High-Level Architecture

At run time a key press travels from the terminal (through `crossterm`) into
the App shell, `vix-tui`, which owns the event loop, the `App` state and all
rendering (`ratatui`), and calls into the feature crates. The desktop GUIs in
`gui/` wrap the same App shell instead of a terminal.

### Crate layers (checked against `cargo metadata`)

The build-time picture is a layered dependency graph. A crate's *layer* is the
longest chain of workspace dependencies below it: layer 0 crates depend on no
other workspace crate, and each layer depends only on layers below it. The App
shell `vix-tui` is the single crate at the top: it depends directly on 142 of
the 147 member crates under `crates/` (the other five, such as
`vix-civil-date` and `vix-jupyter-core`, it reaches through other crates).
The diagram shows the layers with a few well-known crates in each; every arrow
is a real direct dependency ("depends on"), not every dependency is drawn.

```mermaid
flowchart TD
    gui["Desktop GUIs in gui/ (a separate workspace)"]
    subgraph L5["Layer 5: App shell (1 crate)"]
        tui["vix-tui"]
    end
    subgraph L4["Layer 4 (2 crates)"]
        script["vix-script"]
        fhir["vix-fhir"]
    end
    subgraph L3["Layer 3: feature hosts (20 crates)"]
        editor["vix-editor"]
        lsp["vix-lsp"]
        dap["vix-dap"]
        git["vix-git"]
        db["vix-db"]
        undo["vix-undo-store"]
        http["vix-http-client"]
    end
    subgraph L2["Layer 2 (7 crates)"]
        settings["vix-settings"]
        authority["vix-authority"]
        session["vix-session"]
        macros["vix-macros"]
    end
    subgraph L1["Layer 1 (54 crates)"]
        core["vix-editor-core"]
        fileops["vix-fileops"]
        theme["vix-theme"]
        menu["vix-menu"]
        palette["vix-palette"]
    end
    subgraph L0["Layer 0: no workspace dependencies (64 crates)"]
        input["vix-input"]
        i18n["vix-i18n"]
        list_state["vix-list-state"]
        encoding["vix-encoding"]
        lsp_core["vix-lsp-core"]
        textops["vix-textops"]
        theme_model["vix-theme-model"]
        keymap_model["vix-keymap-model"]
    end
    gui --> tui
    tui --> script
    tui --> fhir
    tui --> editor
    tui --> lsp
    tui --> dap
    tui --> db
    tui --> i18n
    script --> git
    script --> macros
    fhir --> http
    fhir --> list_state
    editor --> core
    editor --> authority
    editor --> theme
    editor --> textops
    undo --> core
    undo --> settings
    lsp --> settings
    lsp --> authority
    lsp --> lsp_core
    dap --> lsp_core
    git --> authority
    git --> list_state
    db --> settings
    db --> i18n
    settings --> fileops
    authority --> fileops
    session --> fileops
    core --> input
    fileops --> encoding
    theme --> theme_model
    menu --> keymap_model
    menu --> i18n
    palette --> i18n
```

`tui/tests/docs_crate_graph.rs` keeps this diagram honest: it runs
`cargo metadata`, fails if an arrow between two crates is not a direct
dependency, if a crate is drawn in the wrong layer, or if a layer's crate count
or the "142 of the 147" sentence above no longer matches, and checks that every
crate this page or the [crate map](../crate-map.md) names really exists. To
list the layers yourself:

```sh
cargo metadata --no-deps --format-version 1 --offline
```

and follow each package's `dependencies` whose `kind` is not `"dev"`. For
*which crate owns what*, read the [crate map](../crate-map.md); it is the
maintained crate-by-crate reference, and this page only sketches the shape.

### Layering rules

Beyond "each layer depends only on layers below it" (which the definition of
a layer guarantees), the graph keeps four rules. `tui/tests/crate_layering.rs`
enforces them, reading `cargo metadata` and the `gui/*/Cargo.toml` manifests,
and its failure message names each offending edge (`from -> to`):

1. **Nothing depends on the App shell.** No workspace member depends on
   `vix-tui`, as a normal, build or dev-dependency; the desktop GUIs in `gui/`
   are the only crates above it.
2. **Only `vix-input` and `vix-tui` use `crossterm`.** Terminal I/O stays in
   the App shell and key-event conversion, so every other crate works under a
   desktop GUI with no terminal.
3. **Model and core crates know no UI surface.** A crate named `*-core` or
   `*-model` reaches no crate named `*-panel`, `*-picker`, `*-preview` or
   `*-view`, directly or through other crates.
4. **The GUIs never depend on each other.** No `gui/vix-*/Cargo.toml` names
   another GUI crate; what two GUIs share lives in `vix-tui` or under `crates/`.

A rule is added here only when the current graph already keeps it. The full
graph, every crate with its layer, direct workspace dependencies and number
of dependents, is the generated [Crate Layers](../reference/crate-layers.md)
reference (`cargo run --example list_commands -- --write`; `scripts/check`
fails when it is stale).

## Workspace Structure

```
vix/
├── Cargo.toml                    # Root workspace manifest
├── AGENTS.md                     # Contribution guidelines
├── plan.md                       # Improvement roadmap
├── tasks.md                      # Detailed task checklist
├── spec/                         # Cross-cutting specifications
│   ├── ci/                       # CI/CD specs
│   ├── packaging/                # Packaging specs
│   └── ...
├── tui/                          # Main app shell (vix-tui package)
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs               # Entry point
│   │   ├── app.rs                # App state and logic
│   │   ├── app/*.rs              # Feature modules (git, lsp, org, etc.)
│   │   ├── ui.rs                 # UI rendering
│   │   └── ui/*.rs               # UI components
│   ├── examples/                 # Cargo examples
│   ├── benches/                  # Benchmarks
│   └── tests/                    # Integration tests
├── crates/                       # 147 feature crates (vix-*)
│   ├── vix-editor/               # Editor host
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   └── spec/
│   ├── vix-editor-core/          # Core editor widget
│   ├── vix-lsp/                   # LSP client
│   ├── vix-lsp-core/             # LSP protocol types
│   ├── vix-dap/                   # DAP debugger
│   ├── vix-git/                   # Git integration
│   ├── vix-db/                    # Database workbench
│   ├── vix-org/                   # Org mode
│   ├── vix-terminal/              # Integrated terminal
│   ├── vix-http-client/           # REST client
│   ├── vix-file-browser-panel/   # File explorer
│   └── ... (130+ more)
├── gui/                          # Desktop GUI front ends
│   ├── Cargo.toml
│   ├── vix-gpui/
│   ├── vix-egui/
│   ├── vix-iced/
│   ├── vix-slint/
│   ├── vix-gtk4/
│   ├── vix-bui/
│   └── vix-zgui/
├── fuzz/                         # Fuzz targets
├── scripts/                      # Build and check scripts
└── docs/                         # User documentation
```

## App Shell Architecture

The **App shell** (`tui/`) is the central coordinator that ties everything together:

```mermaid
classDiagram
    class App {
        -root: PathBuf
        -editor: Editor
        -settings: Settings
        -focus: Focus
        -flags: Flags
        +on_key()
        +on_mouse()
        +run_action()
        +tick()
        +draw()
    }
    
    class Editor {
        -buffer: Buffer
        -cursors: MultiCursor
        -history: History
        -syntax: Syntax
        +open()
        +close()
        +input()
        +render()
    }
    
    class Lsp {
        -clients: HashMap~LanguageServer~
        +start_server()
        +send_request()
        +handle_response()
    }
    
    class Git {
        -repo: Repository
        +refresh()
        +stage_hunk()
        +commit()
    }
    
    App --> Editor : owns
    App --> Lsp : owns
    App --> Git : owns
    App --> Settings : owns
    App --> Terminal : owns
    App --> Db : owns
    App --> Im : owns
```

### Key App Components

| File | Purpose |
|------|---------|
| `tui/src/main.rs` | Entry point, terminal setup, event loop |
| `tui/src/app.rs` | Main App struct, key/mouse handling, action dispatch |
| `tui/src/app/git.rs` | Git integration logic |
| `tui/src/app/lsp_dap.rs` | LSP and DAP coordination |
| `tui/src/app/cowork.rs` | Real-time collaboration |
| `tui/src/app/remote_session.rs` | Remote editing over SSH |
| `tui/src/ui.rs` | Main UI rendering |
| `tui/src/ui/*.rs` | UI components (panels, overlays, etc.) |

## Editor Architecture

The editor is split across multiple crates:

```mermaid
flowchart TD
    editor["vix-editor: editor host"] --> core["vix-editor-core: editor widget, rope buffer, Tree-sitter"]
    editor --> fileops["vix-fileops: file operations"]
    editor --> encoding["vix-encoding: character encodings"]
    undo["vix-undo-store: persistent undo"] --> core
    editorconfig["vix-editorconfig"] --> authority["vix-authority"]
```

### Editor Core Components

| Crate | Purpose |
|-------|---------|
| `vix-editor` | Editor host, tab management, split panes |
| `vix-editor-core` | Core editor widget: rope buffer, cursors, Tree-sitter parsing and highlighting, rendering |
| `vix-editorconfig` | EditorConfig file support |
| `vix-undo-store` | Persistent undo tree with branching |
| `vix-fileops` | File operations (read, write, watch) |
| `vix-encoding` | Character encoding handling |

## Feature Crate Categories

### Code Intelligence

```mermaid
flowchart TD
    lsp["vix-lsp: LSP client"] --> lsp_core["vix-lsp-core: protocol framing and types"]
    dap["vix-dap: DAP client"] --> lsp_core
    lsp --> project_env["vix-project-env: project environment"]
    lsp --> settings["vix-settings"]
    dap --> settings
```

| Crate | Description |
|-------|-------------|
| `vix-lsp` | LSP client implementation |
| `vix-lsp-core` | LSP protocol types and message parsing |
| `vix-dap` | Debug Adapter Protocol client |

### Version Control

| Crate | Description |
|-------|-------------|
| `vix-git` | Git integration (status, diff, stage, commit) |
| `vix-git-graph` | Interactive commit graph visualization |
| `vix-review` | Code review with side-by-side diff |
| `vix-conflict-tool` | Merge conflict resolution |

### Data & Documents

| Crate | Description |
|-------|-------------|
| `vix-db` | Database workbench (SQLite, PostgreSQL, MySQL) |
| `vix-edit-table` | CSV/TSV table editing |
| `vix-edit-sql` | SQL file editing with syntax awareness |
| `vix-edit-value` | JSON/YAML/TOML editing |
| `vix-edit-outline` | Outline/structural editing |
| `vix-edit-bytes` | Hex dump editing |
| `vix-large-file` | Large file viewer (multi-GB files) |
| `vix-markdown-preview` | Markdown preview rendering |

### Org Mode

| Crate | Description |
|-------|-------------|
| `vix-org` | Core Org mode parsing and editing |
| `vix-org-table` | Org table editing with formulas |
| `vix-org-capture` | Org capture templates |
| `vix-org-contacts` | Org contacts management |
| `vix-roam` | Org-roam with backlinks |
| `vix-column-view` | Column View for Org and file browser |

### Projects & Tools

| Crate | Description |
|-------|-------------|
| `vix-project-env` | Project environment detection (direnv, mise, venv) |
| `vix-tasks` | Task runner (npm, make, etc.) |
| `vix-terminal` | Integrated terminal emulator |
| `vix-http-client` | REST client for `.http` files |
| `vix-oci` | Open Container Initiative support |
| `vix-oci-panel` | Container management UI |

### Collaboration

| Crate | Description |
|-------|-------------|
| `vix-cowork` | Real-time collaborative editing |
| `vix-cowork-net` | Cowork network protocol |
| `vix-im` | Instant messaging (IRC, Matrix) |
| `vix-im-panel` | IM UI panel |
| `vix-matrix-protocol` | Matrix protocol support |
| `vix-im-irc` | IRC client implementation |

### AI & Assistants

| Crate | Description |
|-------|-------------|
| `vix-ai-core` | AI provider abstraction |
| `vix-ai-panel` | AI chat panel UI |
| `vix-ai-diff` | AI-assisted diff review |
| `vix-acp` | Agent Client Protocol client |
| `vix-mcp-server` | Model Context Protocol server |

### UI & Rendering

| Crate | Description |
|-------|-------------|
| `vix-menu` | Menu bar and context menus |
| `vix-palette` | Command palette |
| `vix-status-bar-panel` | Status bar |
| `vix-bottom-dock` | Bottom dock area |
| `vix-left-dock` | Left dock (file browser) |
| `vix-right-dock` | Right dock area |
| `vix-welcome-panel` | Welcome screen |
| `vix-theme` | Theme loading and management |
| `vix-theme-model` | Theme data model |
| `vix-theme-editor-panel` | Theme editor UI |

### Settings & State

| Crate | Description |
|-------|-------------|
| `vix-settings` | Configuration management |
| `vix-settings-bundle` | Bundled settings presets |
| `vix-session` | Session state management |
| `vix-workspace` | Workspace state management |
| `vix-keymap-model` | Keymap definitions |
| `vix-keybindings` | Keybinding registry |
| `vix-snippets` | Snippet management |
| `vix-script` | Rhai scripting |
| `vix-macros` | Keyboard macro recording |

### File Browser & Navigation

| Crate | Description |
|-------|-------------|
| `vix-file-browser-panel` | File explorer UI |
| `vix-file-information-panel` | File info display |
| `vix-workspace-dashboard-panel` | Workspace dashboard |
| `vix-text-information-panel` | Text info display |
| `vix-tags` | Tag management |
| `vix-todo-tree` | TODO tree |
| `vix-workspace-search` | Workspace search |
| `vix-find-panel` | Find/replace UI |

### Utilities

| Crate | Description |
|-------|-------------|
| `vix-i18n` | Internationalization |
| `vix-locale-model` | Locale management |
| `vix-clipboard` | Clipboard access |
| `vix-authority` | Authority abstraction (local, SSH, containers) |
| `vix-byte-size` | Byte size formatting |
| `vix-civil-date` | Civil date handling |
| `vix-time-zone-model` | Time zone management |
| `vix-uuid-tool` | UUID generation |
| `vix-base64-tool` | Base64 encoding/decoding |
| `vix-jwt-tool` | JWT encoding/decoding |
| `vix-qr-tool` | QR code generation |
| `vix-calendar-panel` | Calendar panel |
| `vix-clock-panel` | Clock panel |
| `vix-system-information-panel` | System info display |
| `vix-vcard-panel` | vCard display |
| `vix-vcard-parser` | vCard parsing |
| `vix-fhir` | FHIR support |
| `vix-hl7` | HL7 v2 support |

## Desktop GUI Architecture

Vix supports 7 desktop GUI front ends, each wrapping the same App shell:

```mermaid
graph TD
    A[App Shell] --> B[vix-gpui]
    A --> C[vix-egui]
    A --> D[vix-iced]
    A --> E[vix-slint]
    A --> F[vix-gtk4]
    A --> G[vix-bui]
    A --> H[vix-zgui]
    
    B --> I[GPUI Framework]
    C --> J[egui Framework]
    D --> K[iced Framework]
    E --> L[Slint Framework]
    F --> M[GTK4 Framework]
    G --> N[BUI Framework]
    H --> O[zgui Framework]
    
    I --> P[Skia Renderer]
    J --> Q[Softbuffer Renderer]
    K --> R[wgpu Renderer]
    L --> S[OpenGL Renderer]
    M --> T[GTK Renderer]
    N --> U[BUI Renderer]
    O --> V[wgpu Renderer]
```

| Frontend | Framework | License | Notes |
|----------|-----------|---------|-------|
| `vix-gpui` | GPUI | MIT/Apache-2.0 | GPU-accelerated, native menu bar |
| `vix-egui` | egui | MIT/Apache-2.0 | Immediate mode, simple |
| `vix-iced` | iced | MIT/Apache-2.0 | Elm-style, CSS styling |
| `vix-slint` | Slint | GPL-3.0-only | Declarative, OpenGL |
| `vix-gtk4` | GTK4 | LGPL | Native GTK integration |
| `vix-bui` | BUI | GPL-3.0-only | Blender's UI toolkit |
| `vix-zgui` | zgui | Apache-2.0 | Reactive, CSS, wgpu |

All front ends:
- Share the same `App` shell from `tui/`
- Use the same key handling, action dispatch, and state management
- Render using the framework's capabilities
- Support the same features as the TUI version

## Data Flow

### Key Press Flow

```mermaid
flowchart TD
    A[User Presses Key] --> B[Terminal]
    B --> C[App::on_key]
    C --> D{Keymap Active?}
    D -->|Yes| E[Keymap Dispatch]
    D -->|No| F[Global Key Handling]
    E --> G[Match Key in Keymap]
    G --> H[Find Action ID]
    H --> I[App::run_action]
    I --> J[Action Dispatch]
    J --> K[Update State]
    J --> L[Queue Render]
    L --> M[App::draw]
    M --> N[ratatui::Terminal::draw]
```

### LSP Request Flow

```mermaid
flowchart TD
    A[User Types] --> B[Editor::input]
    B --> C[Trigger LSP Request]
    C --> D[vix-lsp::request]
    D --> E[Send to Language Server]
    E --> F[Receive Response]
    F --> G[vix-lsp::handle_response]
    G --> H[Update Editor State]
    H --> I[Queue Render]
```

### Git Operation Flow

```mermaid
flowchart TD
    A[User Triggers Git Action] --> B[App::run_action]
    B --> C[git.* action]
    C --> D[vix-git::handle]
    D --> E[Spawn git Command]
    E --> F[Parse Output]
    F --> G[Update Git State]
    G --> H[Refresh UI]
```

## Internationalization (i18n)

Vix uses a robust i18n system:

```mermaid
flowchart TD
    A[User Text] -->|t! macro| B[Lookup in Catalog]
    B --> C[crates/vix-i18n]
    C --> D[Load Locale File]
    D --> E[Return Translated String]
    E --> F[Display to User]
    
    C --> G[locales/action.yml]
    C --> H[locales/menu.yml]
    C --> I[locales/ui.yml]
    C --> J[... one file per key namespace, each key holding all 15 languages]
```

**Supported Languages:**
- English (en)
- Spanish (es)
- French (fr)
- German (de)
- Welsh (cy)
- Irish (ga)
- Scottish Gaelic (gd)
- Polish (pl)
- Portuguese (pt)
- Russian (ru)
- Arabic (ar)
- Hindi (hi)
- Bengali (bn)
- Chinese (zh)
- Japanese (ja)

## Settings System

Vix uses a hierarchical settings system:

```mermaid
flowchart LR
    builtin["Built-in defaults"] --> user["User: config.toml in the confy config folder (~/.config/vix/ on Linux)"]
    user --> project["Project: .vix/settings.toml in the workspace root"]
    project --> language["Language: a languages table per language id, per buffer"]
```

`vix-settings` loads four layers, each overriding the one before it
(`crates/vix-settings/src/layers.rs`; see its spec for the details):
1. Built-in defaults
2. User settings (`config.toml`, located by `confy`)
3. Project settings (`<root>/.vix/settings.toml`)
4. Per-language overrides (`[languages.<id>]`), resolved per buffer

## Theme System

```mermaid
flowchart TD
    A[Theme Selection] --> B[Load Theme]
    B --> C[Built-in Themes]
    B --> D[Custom Themes]
    C --> E[21 Bundled Themes]
    D --> F[~/.config/vix/themes/*.json]
    E --> G[Theme Model]
    F --> G
    G --> H[Apply Colors]
    H --> I[Render UI]
```

**Bundled Themes:**
- Default (light/dark variants)
- High contrast themes
- Popular color schemes (Catppuccin, etc.)

## Keymap System

Vix supports 14 keymaps:

```mermaid
flowchart TD
    A[Keymap Selection] --> B[Load Keymap]
    B --> C[vix-keymap-model]
    C --> D[Apple]
    C --> E[VSCode macOS]
    C --> F[VSCode Windows]
    C --> G[Emacs]
    C --> H[Vi]
    C --> I[Spacemacs]
    C --> J[Helix]
    C --> K[IntelliJ macOS]
    C --> L[IntelliJ Windows]
    C --> M[Eclipse]
    C --> N[Sublime]
    C --> O[Zed]
    C --> P[Fresh]
    C --> Q[ttt]
```

## Performance Architecture

Vix is designed for performance:

### Background Processing

```mermaid
flowchart TD
    A[Editor::input] --> B[Queue Background Task]
    B --> C[Background Thread Pool]
    C --> D[Process Task]
    D --> E[Send Result]
    E --> F[Apply to Editor]
    
    C --> G[Tree-sitter Parsing]
    C --> H[LSP Requests]
    C --> I[Git Operations]
    C --> J[Workspace Search]
```

### Lazy Loading

```mermaid
flowchart TD
    A[App Startup] --> B[Load Essential Components]
    B --> C[Defer Optional Features]
    C --> D[Load on First Use]
    
    D --> E[LSP Servers]
    D --> F[Git Integration]
    D --> G[Database Workbench]
    D --> H[Terminal]
    D --> I[AI Panel]
```

### Caching

```mermaid
flowchart TD
    A[First Request] --> B[Check Cache]
    B -->|Miss| C[Process Request]
    C --> D[Store in Cache]
    D --> E[Return Result]
    B -->|Hit| E
    
    A --> F[Subsequent Request]
    F --> B
```

## Security Architecture

Vix takes security seriously:

```mermaid
flowchart TD
    A[User Input] --> B[Validation]
    B --> C[Sanitization]
    C --> D[Sandboxing]
    D --> E[Execution]
    
    A --> F[File Path]
    F --> G[Check for Traversal]
    G --> H[Check for Special Files]
    H --> I[Safe Open]
    
    A --> J[Shell Command]
    J --> K[Check for Dangerous Patterns]
    K --> L[Execute with Restrictions]
    
    A --> M[Rhai Script]
    M --> N[Disable File/Network/Process Access]
    N --> O[Execute in Sandbox]
```

### Security Features

- **Zero `unsafe` Rust**: No unsafe code anywhere in the codebase
- **Sandboxed scripts**: Rhai scripts have no file/network/process access
- **Workspace trust**: Project scripts run only after explicit trust
- **Keyring integration**: Secrets stored in native credential stores
- **Input validation**: All external inputs are validated
- **Path sanitization**: Prevents directory traversal attacks
- **Command sanitization**: Prevents shell injection attacks

## Testing Architecture

Vix has comprehensive testing:

```mermaid
flowchart TD
    A[Development] --> B[Unit Tests]
    A --> C[Integration Tests]
    A --> D[Snapshot Tests]
    A --> E[Fuzz Tests]
    A --> F[Benchmarks]
    
    B --> G[cargo test --workspace]
    C --> G
    D --> H[TestBackend]
    E --> I[cargo +nightly fuzz]
    F --> J[cargo bench]
    
    G --> K[CI: GitHub/GitLab/Codeberg]
    H --> K
    I --> K
    J --> K
```

### Test Layers

| Layer | Framework | Purpose |
|-------|-----------|---------|
| Unit Tests | Rust `#[test]` | Test individual functions |
| Integration Tests | ratatui `TestBackend` | Test component interactions |
| Snapshot Tests | insta | Test UI rendering |
| Fuzz Tests | cargo-fuzz | Test parser edge cases |
| Benchmarks | Criterion | Test performance |

## CI/CD Architecture

```mermaid
flowchart TD
    A[Push to main] --> B[GitHub Actions]
    A --> C[GitLab CI]
    A --> D[Codeberg CI]
    
    B --> E[Run scripts/check]
    C --> E
    D --> E
    
    E --> F[cargo fmt --check]
    E --> G[cargo build]
    E --> H[cargo clippy]
    E --> I[cargo test]
    E --> J[cargo doc]
    E --> K[scripts/check-docs]
    E --> L[lychee link check]
    
    E --> M[Generate Gate Attestation]
    M --> N[refs/notes/vix-gate]
    
    N --> O[pre-push Hook]
    O --> P[Verify Attestation]
```

## Summary

Vix's architecture is designed for:

1. **Modularity**: Each feature is isolated in its own crate
2. **Maintainability**: Clear boundaries and specifications
3. **Extensibility**: Easy to add new features
4. **Performance**: Background processing, lazy loading, caching
5. **Security**: Sandboxing, validation, zero unsafe
6. **Internationalization**: Full support for 15 languages
7. **Accessibility**: High contrast, keyboard-only, screen reader friendly

The architecture enables Vix to be:
- **Fast**: Native binary, optimized rendering
- **Reliable**: Comprehensive testing, zero unsafe
- **Flexible**: Multiple keymaps, themes, configurations
- **Extensible**: Scripting, plugins (future), customization
- **Portable**: Works anywhere Rust runs

---

## Next Steps

If you're new to Vix's codebase, here are some good next steps:

1. **Read AGENTS.md** - Detailed conventions and workflow
2. **Read agents/workflow.md** - Spec-driven development guide
3. **Explore the crates** - Each crate has its own `spec/index.md`
4. **Run the examples** - `cargo run --example list_commands`
5. **Check the docs** - `docs/` has user-facing documentation
6. **Join the community** - Ask questions in GitHub Discussions

---

## Resources

- [AGENTS.md](../../AGENTS.md) - Contribution guidelines
- [agents/workflow.md](../../agents/workflow.md) - Development workflow
- [plan.md](../../plan.md) - Improvement roadmap
- [tasks.md](../../tasks.md) - Detailed tasks
- [docs/](.) - User documentation
- [spec/](../../spec/) - Specifications
