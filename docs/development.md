# Development Setup Guide

This guide helps you set up your development environment for contributing to Vix.

## Prerequisites

### Required Tools

1. **Rust**: Vix requires Rust 1.70 or later (as specified by `rust-version` in
   `Cargo.toml`)
   ```sh
   # Install Rust
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   
   # Add to PATH
   source "$HOME/.cargo/env"
   
   # Verify
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

3. **C++ Compiler**: Required for Tree-sitter grammars
   ```sh
   # macOS (with Xcode)
   xcode-select --install
   
   # Ubuntu/Debian
   sudo apt install build-essential g++
   
   # Fedora
   sudo dnf install gcc-c++ make
   ```

### Recommended Tools

1. **IDE/Editor**: Any editor that supports Rust LSP
   - VS Code with rust-analyzer
   - IntelliJ IDEA with Rust plugin
   - Vix itself!

2. **Terminal**: A modern terminal with truecolor support
   - macOS: Terminal.app, iTerm2, Kitty, WezTerm
   - Linux: GNOME Terminal, Konsole, Alacritty, Kitty, WezTerm
   - Windows: Windows Terminal, ConEmu, Alacritty

3. **Docker**: For running Linux builds on macOS
   ```sh
   # macOS
   brew install docker
   
   # Ubuntu/Debian
   sudo apt install docker.io
   
   # Start Docker daemon
   sudo systemctl start docker
   ```

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
# First build (this will take a while as it downloads dependencies)
cargo build

# For development, use the debug profile
cargo build

# For a release build
cargo build --release
```

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

### VS Code

1. Install the [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer) extension
2. Open the repository root in VS Code
3. rust-analyzer will automatically index the workspace

**Recommended VS Code settings** (`settings.json`):
```json
{
  "rust-analyzer.cargo.ex extraEnv": {
    "RUSTFLAGS": "-Dwarnings"
  },
  "rust-analyzer.cargo.features": "all",
  "rust-analyzer.checkOnSave.command": "clippy",
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer",
    "editor.formatOnSave": true
  }
}
```

### IntelliJ IDEA

1. Install the Rust plugin
2. Open the repository root
3. The IDE will automatically detect the Cargo workspace

**Tip:** Enable clippy in the Rust settings for on-the-fly linting.

### Vix (Yes, you can develop Vix with Vix!)

1. Build Vix: `cargo build --release`
2. Run: `./target/release/vix`
3. Open the repository in Vix
4. Use `Ctrl+P` to find files, `F12` for go-to-definition

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

# Run doc tests
cargo test --doc

# Run integration tests only
cargo test --test '*'
```

**Note:** Some tests require a terminal and will be skipped in CI or when
not running in a TTY.

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

**Or use the Makefile:**
```sh
# Run the gate
make check

# Run tests only
make test
```

---

## Running Benchmarks

Vix includes Criterion benchmarks for performance-critical paths:

```sh
# Run all benchmarks
cargo bench

# Run benchmarks for a specific package
cargo bench -p vix-editor-core

# Run a specific benchmark
cargo bench editor_open

# Compare with previous run
cargo bench --bench editor_ops
```

Benchmark results are saved in `target/criterion/`.

---

## Development Workflow

### 1. Find Something to Work On

- Check [GitHub Issues](https://github.com/vixide/vix/issues) for open tasks
- Look for issues labeled `good first issue` or `help wanted`
- Browse the [spec directory](spec/) for features that need implementation

### 2. Create a Feature Branch

```sh
# Create and checkout a new branch
git checkout -b my-feature-branch

# Or create a branch based on a specific issue
git checkout -b fix-issue-123
```

**Branch naming convention:**
- Use kebab-case: `fix-bug-description`, `feat-new-feature`, `docs-update-guide`
- Prefix with type: `fix-`, `feat-`, `docs-`, `refactor-`, `perf-`, `test-`

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
git commit -m "feat: add new feature description

Details of what changed and why.

Generated by Mistral Vibe.
Co-Authored-By: Mistral Vibe <vibe@mistral.ai>"
```

**Commit message guidelines:**
- Use the imperative mood: "Add feature" not "Added feature"
- Start with a short summary line (50 chars or less)
- Add a blank line, then a detailed description
- Reference issue numbers if applicable
- Sign off with your name if you have a DCO

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
├── Cargo.lock           # Dependency lockfile (not checked in)
├── crates/              # Member crates (124+ crates)
│   ├── vix-*/           # Feature crates
│   └── ...
├── tui/                 # App shell (vix-tui package)
│   ├── src/
│   │   ├── main.rs      # CLI entry point
│   │   ├── lib.rs       # Library root
│   │   ├── app.rs       # App state and logic
│   │   └── ui.rs        # Rendering
│   └── Cargo.toml
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
| `spec/` | Specifications |

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
make release

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

### Running in Docker

```sh
# Build the Docker image
docker build -f docker/Dockerfile -t vix .

# Run Vix in the demo workspace
docker run --rm -it vix

# Run Vix on your own files
docker run --rm -it -v "$PWD:/work" -w /work vix .
```

### Running in Development Mode

```sh
# Run with logging enabled
RUST_LOG=debug cargo run

# Run with specific log modules
RUST_LOG=vix_editor=debug cargo run

# Run with backtrace on panic
RUST_BACKTRACE=1 cargo run
```

---

## Debugging

### Using a Debugger

Vix has a built-in debugger for debugging your code. For debugging Vix itself:

```sh
# Build with debug symbols
cargo build

# Run with lldb (macOS)
lldb -- ./target/debug/vix

# Run with gdb (Linux)
gdb --args ./target/debug/vix

# Run with Visual Studio debugger (Windows)
# Set the executable to ./target/debug/vix.exe
```

### Logging

Vix uses the `log` crate for logging. Set the `RUST_LOG` environment variable
to control logging:

```sh
# Enable debug logging for all modules
RUST_LOG=debug cargo run

# Enable debug logging for specific modules
RUST_LOG=vix_editor=debug,vix_lsp=debug cargo run

# Enable trace logging (very verbose)
RUST_LOG=trace cargo run

# Log to a file
RUST_LOG=debug cargo run 2>&1 | tee vix-debug.log
```

### Common Debug Commands

```sh
# Check Vix version
cargo run -- --version

# Run doctor to check environment
cargo run -- --doctor

# List all available actions
cargo run --example list_commands

# Show keybindings for current keymap
cargo run --example list_keybindings
```

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

---

## Community

### Getting Help

- [GitHub Discussions](https://github.com/vixide/vix/discussions) - Ask questions
- [GitHub Issues](https://github.com/vixide/vix/issues) - Report bugs
- [CONTRIBUTING.md](CONTRIBUTING.md) - Contribution guidelines

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
- Make sure you're running tests in a TTY (some tests require a terminal)
- Run `cargo test --workspace` for all tests

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
