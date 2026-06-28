# ============================================================================
# Vix build orchestration
# ============================================================================
#
# This Makefile drives the test suite and produces release binaries for the
# three shipping platforms from a single host. The intended host is macOS on
# Apple Silicon (arm64); the macOS target builds natively and the Windows and
# Linux targets are cross-compiled.
#
# ----------------------------------------------------------------------------
# Targets (run `make <name>`)
# ----------------------------------------------------------------------------
#   make            Default. Run the full test suite, then build all three
#                   release binaries. Tests GATE the builds: make runs
#                   prerequisites in order and stops at the first failure, so a
#                   failing test aborts before anything is built.
#   make test       Run the test suite only (no builds).
#   make release    Build all three release binaries (skips tests).
#   make build-macos / build-windows / build-linux
#                   Build a single platform's release binary.
#   make clean      Remove the target/ build directory.
#
# ----------------------------------------------------------------------------
# Output binaries
# ----------------------------------------------------------------------------
#   target/aarch64-apple-darwin/release/vix          (macOS, Mach-O arm64)
#   target/x86_64-pc-windows-gnu/release/vix.exe     (Windows, PE32+ x86-64)
#   target/x86_64-unknown-linux-musl/release/vix     (Linux, static ELF x86-64)
#
# ----------------------------------------------------------------------------
# Why plain cargo and not `cross`
# ----------------------------------------------------------------------------
# `cross` (the container-based cross-compiler) only publishes x86_64 build
# images. On an arm64 Mac those run under QEMU emulation, where rustc segfaults
# ("qemu: uncaught target signal 11"). Docker vs Podman makes no difference --
# both hit the same emulation layer. So instead we cross-compile with plain
# cargo and point it at NATIVE cross-toolchains: no containers, no emulation.
#
# ----------------------------------------------------------------------------
# Prerequisites
# ----------------------------------------------------------------------------
#   rustup            (the build-* recipes run `rustup target add ...`)
#   The cross-toolchains below, e.g. via Homebrew:
#     brew install mingw-w64                                   # Windows (GNU)
#     brew install messense/macos-cross-toolchains/x86_64-unknown-linux-musl
#   If your toolchains are named differently, override the *_CC / *_AR vars
#   (see below), e.g.  make build-linux LINUX_CC=musl-gcc
#
# ============================================================================

# The cargo binary. Override to use a wrapper or a pinned toolchain, e.g.
#   make CARGO="cargo +nightly"
CARGO ?= cargo

# ----------------------------------------------------------------------------
# Target triples
# ----------------------------------------------------------------------------
# The Rust target triple built for each platform. Override on the command line
# for a different arch, e.g.  make MACOS_TARGET=x86_64-apple-darwin
#
# The foreign targets cross-compile with plain cargo: the linkers and the
# CC_/CXX_/AR_ overrides for the `cc` crate live in .cargo/config.toml, so the
# build-* recipes below are just `cargo build --target <triple>`. If you change
# WINDOWS_TARGET / LINUX_TARGET, update the toolchain entries there too.
MACOS_TARGET   ?= aarch64-apple-darwin
WINDOWS_TARGET ?= x86_64-pc-windows-gnu
LINUX_TARGET   ?= x86_64-unknown-linux-musl

# Build `all` when make is invoked with no explicit target.
.DEFAULT_GOAL := all

# These targets are commands, not files, so declare them PHONY: make will run
# their recipes unconditionally and never skip one because a same-named file
# happens to exist or look "up to date".
.PHONY: all test check release build-macos build-windows build-linux clean

# Tests first, then the release builds. make evaluates prerequisites left to
# right and aborts on the first failure, so `test` failing means no build runs.
all: test release

# Run the whole workspace's tests (unit + integration + doctests).
test:
	$(CARGO) test --workspace

# Local CI-parity gate: build + clippy (pedantic, -D warnings) + tests.
# Same as scripts/check; run before pushing.
check:
	./scripts/check

# Build every platform. Each build-* recipe is independent and self-contained.
release: build-macos build-windows build-linux

# macOS: native build on an Apple Silicon host -- no special linker needed.
build-macos:
	rustup target add $(MACOS_TARGET)
	$(CARGO) build --release --target $(MACOS_TARGET)

# Windows (GNU ABI): cross-compiled with the mingw-w64 toolchain. The linker and
# the cc-rs CC/AR overrides come from .cargo/config.toml.
build-windows:
	rustup target add $(WINDOWS_TARGET)
	$(CARGO) build --release --target $(WINDOWS_TARGET)

# Linux (MUSL): cross-compiled with the musl toolchain, producing a fully static
# binary that runs on any x86-64 Linux without a libc dependency. The linker and
# the cc-rs CC/AR overrides come from .cargo/config.toml.
build-linux:
	rustup target add $(LINUX_TARGET)
	$(CARGO) build --release --target $(LINUX_TARGET)

# Remove all build artifacts (the target/ directory).
clean:
	$(CARGO) clean
