# Development Setup Guide

This guide helps you set up your development environment for contributing to Vix.

## Prerequisites

### Required Tools

1. **Rust**, through [rustup](https://rustup.rs). The repository's
   `rust-toolchain.toml` pins the toolchain it builds with (1.100.0 today, with
   `clippy`, `rustfmt` and `llvm-tools`), and rustup installs it the first
   time you run `cargo` in the checkout. The minimum supported Rust version
   for users is `rust-version` in the root `Cargo.toml` (1.98 today).
   ```sh
   # Install Rust
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   
   # Add to PATH
   source "$HOME/.cargo/env"
   
   # Verify (run inside the checkout, so the pinned toolchain answers)
   rustc --version
   cargo --version
   ```

2. **Git**: For version control
   ```sh
   # macOS
   brew install git
   
   # Ubuntu/Debian
   sudo apt install git
   
   # Fedora
   sudo dnf install git
   ```

3. **C/C++ compiler**: required to build the Tree-sitter grammars
   ```sh
   # macOS (with Xcode)
   xcode-select --install
   
   # Ubuntu/Debian
   sudo apt install build-essential g++
   
   # Fedora
   sudo dnf install gcc-c++ make
   ```

### Recommended Tools

1. **IDE/Editor**: any editor with a rust-analyzer client (see
   [IDE Setup](#ide-setup) below)

2. **Terminal**: A modern terminal with truecolor support
   - macOS: Terminal.app, iTerm2, Kitty, WezTerm
   - Linux: GNOME Terminal, Konsole, Alacritty, Kitty, WezTerm
   - Windows: Windows Terminal, ConEmu, Alacritty

3. **Podman**: For running Linux builds on macOS (Docker-compatible)
   ```sh
   # macOS
   brew install podman
   
   # Ubuntu/Debian
   sudo apt install podman
   
   # Start Podman machine
   podman machine start
   ```

   **Note:** Podman provides a Docker-compatible CLI, so `docker` commands also work.

---

## Getting Started

### 1. Clone the Repository

```sh
# Clone with SSH (recommended)
git clone git@github.com:vixide/vix.git
cd vix

# Or with HTTPS
git clone https://github.com/vixide/vix.git
cd vix
```

### 2. Build Vix

```sh
# Build the `vix` command (the first build downloads and compiles every dependency)
cargo build

# Build every workspace crate, not only the App shell
cargo build --workspace

# A release build (size-optimized and stripped, as shipped)
cargo build --release
```

The root `Cargo.toml` sets `default-members = ["tui"]`, so a bare `cargo
build`, `cargo run` or `cargo test` at the root acts on the App shell
(package `vix-tui`) only; pass `--workspace` or `-p <crate>` for the rest.
The desktop GUIs (`gui/`) and the fuzz targets (`fuzz/`) are separate
workspaces with their own `Cargo.toml`.

**Several checkouts or worktrees?** Give each its own target directory
(`CARGO_TARGET_DIR=…`): cargo locks a target directory while it builds, so
two checkouts sharing one wait on each other, and a deleted worktree can leave
stale compile-time paths behind in a shared one.

### 3. Run Vix

```sh
# Run from the repository root
cargo run

# Run with release optimizations
cargo run --release

# Run on a specific directory
cargo run -- path/to/directory
```

---

## IDE Setup

Any editor with a [rust-analyzer](https://rust-analyzer.github.io/) client
works; the repository ships no editor-specific project files (there is no
`.vscode/` or `.idea/`). Whatever you use, set it up so that it:

- **opens the repository root**, so rust-analyzer loads the whole workspace
  (all 148 member crates and `tui/`), not one crate;
- **checks with clippy**, because the gate runs
  `cargo clippy --workspace --all-targets -- -D warnings` with
  `clippy::pedantic` on for every crate, and a pedantic warning is an error;
- **formats with rustfmt on save** (the gate runs `cargo fmt --all --check`);
- **keeps the default features.** The gate checks the default feature set and
  `--no-default-features`; turning on every feature at once pulls in optional
  integrations the gate never builds together;
- **uses its own target directory**, so the language server's background
  checks do not hold the cargo lock while you build or test in a terminal.

### VS Code

1. Install the [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer) extension
2. Open the repository root in VS Code
3. rust-analyzer will automatically index the workspace

**Recommended VS Code settings** (`settings.json`):
```json
{
  "rust-analyzer.check.command": "clippy",
  "rust-analyzer.cargo.targetDir": true,
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer",
    "editor.formatOnSave": true
  }
}
```

`"rust-analyzer.cargo.targetDir": true` makes rust-analyzer build in a
subdirectory of the target directory instead of sharing (and locking) the one
your terminal uses. For a debugger, the
[CodeLLDB](https://marketplace.visualstudio.com/items?itemName=vadimcn.vscode-lldb)
extension drives the same `lldb` the [Debugging](#debugging) section uses.

### RustRover and IntelliJ IDEA

1. Use RustRover, or IntelliJ IDEA with the Rust plugin
2. Open the repository root; the IDE detects the Cargo workspace
3. In the Rust settings, choose Clippy as the external linter and enable
   rustfmt on save

### Vim, Neovim, Helix, Zed, Emacs

Point the editor's language-server client at `rust-analyzer` (installed with
`rustup component add rust-analyzer`) and, where it offers the choice, set the
check command to `clippy`.

### Vix (yes, you can develop Vix with Vix)

Vix's own LSP client starts `rust-analyzer` for Rust files (see
[Language Server Protocol](language-server-protocol/index.md)), so building
Vix and opening the repository root in it gives you diagnostics,
go-to-definition and completion on its own source:

```sh
cargo run --release -- .
```

---

## Running Tests

Vix has comprehensive test coverage. Run tests with:

```sh
# Run all tests
cargo test --workspace

# Run tests for a specific package
cargo test -p vix-editor-core

# Run a specific test
cargo test test_name -p vix-editor-core

# Run doc tests (of the App shell; add -p <crate> for another)
cargo test --doc

# Run one integration test file of the App shell (tui/tests/<name>.rs)
cargo test -p vix-tui --test snapshots

# See a test's output even when it passes
cargo test -p vix-textops -- --nocapture
```

`tui/` sets `autotests = false`, so each integration test file under
`tui/tests/` is listed as a `[[test]]` in `tui/Cargo.toml`; add yours there.
A few integration tests drive an external tool (for example
`tui/tests/e2e_tmux.rs` needs `tmux`) and skip themselves when it is not
installed.

---

## Running the Quality Gate

Before committing, run the full quality gate:

```sh
# Fast tier (fmt, clippy, tests of changed crates)
scripts/check --tier fast

# Push tier (full workspace build, clippy, tests)
scripts/check --tier push

# Full tier (everything including docs)
scripts/check
```

**Or use the Makefile** (`make help` lists every target):
```sh
# Run the gate (scripts/check)
make check

# Run the tests (it first checks over the network that GitHub Discussions is enabled)
make test
```

Only the default full tier of `scripts/check` counts before landing; the
faster tiers are for iterating. See
[Spec-Driven Workflow](contributing-workflow.md) for the whole landing loop.

### Git hooks

The hooks are opt-in: `make hooks` sets `git config core.hooksPath
scripts/hooks`, and `git config --unset core.hooksPath` removes them. Both are
POSIX `sh`.

- **pre-commit** checks what is staged, in seconds, and never changes the index
  or your files. It refuses the commit when a staged `.rs` file is not
  formatted as `cargo fmt` would format it (fix: `cargo fmt`, then stage the
  result); when a staged file holds what looks like a real credential (the
  rules and allowlist are in `scripts/secret-patterns`, shared with
  `tui/tests/secret_scan.rs`); when it adds a file over 1 MiB; and, when a file
  under `tui/src/` or `crates/*/src/` is staged, when
  `tui/tests/code_lints.rs` fails (compiled alone with `rustc`, about a
  second; it reads the working tree).
- **pre-push** refuses `main` and tags without a valid gate attestation
  (`scripts/attest`). Another branch can be pushed unattested with
  `VIX_ALLOW_UNATTESTED=1`; the hook then runs `scripts/check --tier fast`
  first when the pushed commit is `HEAD`, and refuses the push if it fails.

| Variable | Effect |
| -------- | ------ |
| `VIX_SKIP_PRECOMMIT_FMT=1` | Skip the format check |
| `VIX_SKIP_PRECOMMIT_LINTS=1` | Skip the code lints |
| `VIX_MAX_ADD_BYTES=<bytes>` | Raise the size limit for new files (default 1048576) |
| `VIX_ALLOW_UNATTESTED=1` | Push a non-main branch without an attestation |

`git commit --no-verify` and `git push --no-verify` skip the hooks; the gate
and CI still run every check. `scripts/test-attest` tests both hooks.

---

## Running Benchmarks

Vix's Criterion benchmarks live in `tui/benches/` (`text_ops`, `editor_ops`,
`search_and_palette`, `startup`, `frame_ops`, `gutter_diff`, `render_ops`):

```sh
# Run every benchmark
cargo bench

# Run one benchmark file
cargo bench --bench text_ops

# Run only the benchmarks whose name contains a filter
cargo bench --bench text_ops -- sentence_starts
```

Criterion keeps each run under `target/criterion/` and reports the change
against the previous run. The baselines, the profiling tools and how to read
the numbers are in [Performance](performance/index.md).

---

## Development Workflow

### 1. Find Something to Work On

- Check [GitHub Issues](https://github.com/vixide/vix/issues) for open tasks
- Look for issues labeled `good first issue` or `help wanted`
- Browse the [spec directory](../spec/) for features that need implementation

### 2. Create a Feature Branch

```sh
# Create and checkout a new branch
git checkout -b my-feature-branch

# Or create a branch based on a specific issue
git checkout -b fix-issue-123
```

**Branch naming convention:** a type, a slash and a short kebab-case name,
with the task id when there is one: `fix/t846-vi-parity`,
`feat/whole-word-search`, `docs/t836-contributor-docs`,
`chore/t833-code-lints`.

### 3. Make Changes

- Follow the existing code style
- Add tests for new functionality
- Update documentation if needed
- Keep commits focused and well-described

### 4. Test Your Changes

```sh
# Run tests for the packages you changed
cargo test -p package-name

# Run the full test suite
cargo test --workspace

# Run the quality gate
scripts/check --tier fast
```

### 5. Commit Your Changes

```sh
# Stage changes
git add -p  # Interactive staging

# Or stage all
git add -A

# Commit with a good message
git commit -m "feat(find): add whole-word search

Details of what changed and why."
```

**Commit message guidelines:**
- Use the imperative mood: "Add feature" not "Added feature"
- Start with a short summary line (50 chars or less)
- Add a blank line, then a detailed description
- Reference issue numbers or task ids (`T836`) if applicable
- Sign the commit (the project uses SSH-signed commits)
- If an AI assistant helped, end the message with a `Co-Authored-By:` trailer
  naming the model; see [AI Statement](contributing-ai-statement.md)

### 6. Push and Create a Pull Request

```sh
# Push your branch
git push origin my-feature-branch

# Then create a PR on GitHub
```

**Pull request guidelines:**
- Use a clear, descriptive title
- Include a detailed description of what changed
- Reference related issues
- Add a `Closes #123` comment if the PR fixes an issue
- Wait for CI to pass before requesting review

### 7. Address Feedback

- Respond to review comments promptly
- Update commits with `git commit --amend` or add new commits
- Push updates with `git push`
- Use `git rebase -i` to squash or reorder commits if needed

---

## Project Structure

### Workspace Layout

```
vix/
├── Cargo.toml           # Workspace manifest
├── Cargo.lock           # Dependency lockfile (checked in)
├── crates/              # The 148 `vix-*` member crates
│   ├── vix-*/           # Feature crates
│   └── ...
├── tui/                 # App shell (vix-tui package)
│   ├── src/
│   │   ├── main.rs      # CLI entry point
│   │   ├── lib.rs       # Library root
│   │   ├── app.rs       # App state and logic (more in app/*.rs)
│   │   └── ui.rs        # Rendering (more in ui/*.rs)
│   ├── tests/           # Integration and snapshot tests
│   ├── benches/         # Criterion benchmarks
│   ├── examples/        # Headless examples and doc generators
│   └── Cargo.toml
├── gui/                 # Desktop GUIs (a separate workspace)
├── fuzz/                # Fuzz targets (a separate workspace)
├── docs/                # Documentation
├── spec/                # Specifications
├── scripts/             # Development scripts
└── ...
```

### Key Directories

| Directory | Purpose |
|-----------|---------|
| `crates/vix-editor-core/` | Core editor widget and engine |
| `crates/vix-keymap-model/` | Keymap definitions |
| `crates/vix-theme/` | Theme system |
| `crates/vix-lsp/` | LSP client |
| `crates/vix-db/` | Database workbench |
| `crates/vix-git/` | Git integration |
| `tui/src/app/` | App logic by feature area |
| `tui/src/ui/` | UI rendering by component |
| `docs/` | User documentation |
| `spec/` | Cross-cutting specifications (each crate's own spec is `crates/<crate>/spec/index.md`) |

For which crate owns which feature, see the [Crate Map](crate-map.md); for
how the layers fit together, see [Architecture](architecture/index.md).

### Finding Code

- Use `grep` or `rg` (ripgrep) to search:
  ```sh
  # Search for a function
  rg "fn my_function" --type rust
  
  # Search for a struct
  rg "struct MyStruct" --type rust
  
  # Search in a specific directory
  rg "pattern" crates/vix-editor-core/
  ```

- Use Vix's own search:
  - `Ctrl+P` then `>` for command palette
  - `Ctrl+Shift+F` for workspace search
  - `Ctrl+T` for workspace symbol search

---

## Building for Different Platforms

### Cross-Compilation

Vix supports cross-compilation to different platforms:

```sh
# Build for macOS (native on macOS)
cargo build --release --target aarch64-apple-darwin

# Build for Windows (requires mingw-w64)
cargo build --release --target x86_64-pc-windows-gnu

# Build for Linux (requires musl toolchain)
cargo build --release --target x86_64-unknown-linux-musl
```

**Install cross-compilation tools:**

```sh
# macOS (Homebrew)
brew install mingw-w64
brew install messense/macos-cross-toolchains/x86_64-unknown-linux-musl

# Ubuntu/Debian
sudo apt install gcc-mingw-w64 g++-mingw-w64
```

### Using the Makefile

The Makefile provides convenient targets for cross-compilation:

```sh
# Build for all platforms
make build

# Build for macOS only
make build-macos

# Build for Windows only
make build-windows

# Build for Linux only
make build-linux

# Clean all build artifacts
make clean
```

---

## Running in Different Environments

### Running in Podman

```sh
# Build the Podman image
podman build -f docker/Dockerfile -t vix .

# Run Vix in the demo workspace
podman run --rm -it vix

# Run Vix on your own files
podman run --rm -it -v "$PWD:/work" -w /work vix .
```

**Note:** Podman is rootless by default and provides a Docker-compatible CLI.
Use `podman` or `docker` commands interchangeably.

---

## Debugging

### Where Vix's own diagnostics go

The TUI owns the terminal, so Vix never prints while it runs: a stray
`println!` would scribble over the frame, and `tui/tests/code_lints.rs`
fails the tests on one in library code. Vix has no log file and reads no
`RUST_LOG`; its warnings and errors go to the **Messages** drawer in the right
dock, and the output of tasks, tests and language servers to the **Output**
dock. When reporting or chasing a problem, look there first.

`vix --doctor` (or **Help → Run Diagnostics**) checks the environment: `git` on
`PATH`, the configured language servers, the spell-check dictionary and the
terminal's color support (see `crates/vix-doctor/spec/index.md`).

### Panics and crash reports

A panic restores the terminal, saves a [crash report](crash-reports/index.md)
with a backtrace (always captured, so `RUST_BACKTRACE` is not needed) to the
`crashes` folder in Vix's data folder, and prints its path. **Help → Report a
Crash…** opens the newest one. A debug build (`[profile.dev]` keeps line
tables) gives file and line numbers in the backtrace's frames.

### Reproducing without a terminal

Most bugs reproduce faster without the terminal in the way. The App shell is
also a library (crate name `vix`), and its examples drive it headlessly:

```sh
# Draw one 80x24 frame of the real TUI as plain text, with a file open
cargo run --example render_frame -- Cargo.toml

# Type into a buffer, run a find-and-replace and save, all through key events
cargo run --example headless_edit
```

A bug you can reproduce that way belongs in a test: `tui/tests/snapshots.rs`
renders frames the same way (with `ratatui`'s `TestBackend`) and compares them
with golden screens under `tui/tests/snapshots/`.

### Using a debugger

Use `rust-lldb` or `rust-gdb` (shipped with the toolchain; they load Rust's
pretty-printers, so a `PathBuf` or a `String` prints as text). `[profile.dev]`
keeps only line tables, which is enough for breakpoints and backtraces; to
inspect variables, build with full debug information:

```sh
# Full debug information for the workspace crates (dependencies stay without)
CARGO_PROFILE_DEV_DEBUG=full cargo build --example render_frame

# Stop in App::new and look at a variable
rust-lldb --batch -o "breakpoint set --name '<vix::app::App>::new'" -o run \
  -o "frame variable root" -o kill -- target/debug/examples/render_frame Cargo.toml
```

The toolchain uses Rust's v0 symbol mangling, so give `lldb` the full path the
way it prints one: an inherent method is `<crate::module::Type>::method` (a short regex
such as `App::new` matches nothing). `breakpoint set --file app.rs --line N`
works too.

To debug the interactive TUI itself, start it in one terminal
(`cargo build && target/debug/vix`) and attach from another, so the debugger's
own output does not fight the TUI for the screen: `rust-lldb -p <pid>` (or
`rust-gdb -p <pid>` on Linux). On Windows, open `target\debug\vix.exe` in
the Visual Studio debugger.

### When it is slow, not wrong

Start Vix with `VIX_PROFILE=profile.txt` to get per-frame timings of a real
session, then use a sampling profiler to see where the time goes; both are in
[Performance](performance/index.md#profiling-a-session-vix_profile).

### Common debug commands

```sh
# Check Vix version
cargo run -- --version

# Check the environment (git, language servers, dictionary, colors)
cargo run -- --doctor

# List every command-palette command with its i18n key and action id
cargo run --example list_commands
```

The full action, setting and keybinding references in `docs/reference/` are
generated by `cargo run --example list_commands -p vix-tui -- --write`.

---

## Contributing

### Code Style

- Follow Rust naming conventions
- Use `snake_case` for functions and variables
- Use `PascalCase` for types and traits
- Use `SCREAMING_SNAKE_CASE` for constants
- Use `//` comments for line comments
- Use `///` for doc comments
- Use `//!` for module-level doc comments

### Linting

Vix uses strict linting:

```sh
# Run clippy
cargo clippy --workspace --all-targets -- -D warnings

# Run fmt
cargo fmt --check

# Fix formatting issues
cargo fmt
```

**Note:** All public items must have doc comments. The build will fail
if any public item is missing documentation.

### Testing

- All new code should have corresponding tests
- Unit tests go in the same file as the code or in a `tests` module
- Integration tests go in the `tests` directory
- Use `#[cfg(test)]` for test-only code

### Documentation

- Update documentation when adding new features
- Keep specs up to date
- Use cross-references to other documentation

### Submitting Changes

1. Sign your commits (optional but recommended)
2. Run the full quality gate: `scripts/check`
3. Push to your fork
4. Create a pull request
5. Wait for CI to pass
6. Request review from maintainers

### Releases

Contributors do not cut releases. A release is a signed version tag on a
commit already on `main`, which starts `.github/workflows/release.yml`
(generated by `dist` from `dist-workspace.toml`); tagging and publishing are
the maintainer's decision. [Releasing](release/index.md) describes the whole
process, including the local alternative `scripts/release-local`.

---

## Community

### Getting Help

- [GitHub Discussions](https://github.com/vixide/vix/discussions) - Ask questions
- [GitHub Issues](https://github.com/vixide/vix/issues) - Report bugs
- [CONTRIBUTING.md](../CONTRIBUTING.md) - Contribution guidelines

### Staying Updated

- Watch the repository for releases
- Join the community discussions
- Follow the changelog

---

## Troubleshooting

### Common Issues

**Build fails with linker errors:**
- Make sure you have a C++ compiler installed
- Check that your Rust version matches the `rust-version` in `Cargo.toml`

**Tests fail:**
- Run the one failing test on its own (`cargo test -p <crate> <name>`) to
  rule out interference from another test
- A snapshot test that fails after a deliberate UI change needs its golden
  screen updated and read line by line (see `agents/conventions.md`)

**Formatting errors:**
- Run `cargo fmt` to fix formatting
- Check the output of `cargo fmt --check`

**Clippy warnings:**
- Run `cargo clippy --workspace --all-targets -- -D warnings`
- Fix all warnings before committing

**Git hooks not working:**
- Run `make hooks` to install git hooks
- Or manually set `git config core.hooksPath scripts/hooks`

---

## Quick Start Checklist

- [ ] Install Rust
- [ ] Install Git
- [ ] Install C++ compiler
- [ ] Clone the repository
- [ ] Build with `cargo build`
- [ ] Run tests with `cargo test`
- [ ] Run the quality gate with `scripts/check --tier fast`
- [ ] Install git hooks with `make hooks`
- [ ] Start contributing!

---

**Happy developing!**

Vix™ and Vix IDE™ are trademarks.
