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
# NOTE: the foreign-target env vars in build-windows / build-linux below encode
# these specific triples in their names (cargo derives them from the triple).
# If you change WINDOWS_TARGET / LINUX_TARGET, update those env var names too.
MACOS_TARGET   ?= aarch64-apple-darwin
WINDOWS_TARGET ?= x86_64-pc-windows-gnu
LINUX_TARGET   ?= x86_64-unknown-linux-musl

# ----------------------------------------------------------------------------
# Cross-toolchains for the foreign targets
# ----------------------------------------------------------------------------
# CC  = the C compiler / linker driver. cargo uses it to link the final binary,
#       and the `cc` crate uses it to compile C dependencies (tree-sitter
#       grammars, mimalloc, image codecs) for the target.
# AR  = the matching archiver, used by the `cc` crate to build static archives.
# These default to the standard Homebrew toolchain names; override if yours
# differ.
WINDOWS_CC ?= x86_64-w64-mingw32-gcc
WINDOWS_AR ?= x86_64-w64-mingw32-ar
LINUX_CC   ?= x86_64-linux-musl-gcc
LINUX_AR   ?= x86_64-linux-musl-ar

# Build `all` when make is invoked with no explicit target.
.DEFAULT_GOAL := all

# These targets are commands, not files, so declare them PHONY: make will run
# their recipes unconditionally and never skip one because a same-named file
# happens to exist or look "up to date".
.PHONY: all test release build-macos build-windows build-linux clean

# Tests first, then the release builds. make evaluates prerequisites left to
# right and aborts on the first failure, so `test` failing means no build runs.
all: test release

# Run the whole workspace's tests (unit + integration + doctests).
test:
	$(CARGO) test --workspace

# Build every platform. Each build-* recipe is independent and self-contained.
release: build-macos build-windows build-linux

# macOS: native build on an Apple Silicon host -- no special linker needed.
build-macos:
	rustup target add $(MACOS_TARGET)
	$(CARGO) build --release --target $(MACOS_TARGET)

# Windows (GNU ABI): cross-compiled with the mingw-w64 toolchain.
# The env vars tell cargo and the `cc` crate which tools to use for this target:
#   CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER -> linker cargo invokes
#   CC_x86_64_pc_windows_gnu                  -> C compiler for the `cc` crate
#   AR_x86_64_pc_windows_gnu                  -> archiver for the `cc` crate
# (cargo's env-var spelling: target triple uppercased with '-' -> '_' for the
# LINKER var; the `cc` crate uses the lowercased triple for CC_/AR_.)
build-windows:
	rustup target add $(WINDOWS_TARGET)
	CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=$(WINDOWS_CC) \
	CC_x86_64_pc_windows_gnu=$(WINDOWS_CC) \
	AR_x86_64_pc_windows_gnu=$(WINDOWS_AR) \
	$(CARGO) build --release --target $(WINDOWS_TARGET)

# Linux (MUSL): cross-compiled with the musl toolchain, producing a fully
# static binary that runs on any x86-64 Linux without a libc dependency.
# Same env-var scheme as build-windows, for the musl triple.
build-linux:
	rustup target add $(LINUX_TARGET)
	CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=$(LINUX_CC) \
	CC_x86_64_unknown_linux_musl=$(LINUX_CC) \
	AR_x86_64_unknown_linux_musl=$(LINUX_AR) \
	$(CARGO) build --release --target $(LINUX_TARGET)

# Remove all build artifacts (the target/ directory).
clean:
	$(CARGO) clean
