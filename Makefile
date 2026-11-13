# ============================================================================
# Vix build orchestration
# ============================================================================
#
# This Makefile drives the test suite and produces release binaries for the
# three shipping platforms from a single host. The intended host is macOS on
# Apple Silicon (arm64); the macOS target builds natively and the Windows and
# Linux targets are cross-compiled.
#
# Run `make help` to list every target. Each target documents itself in a
# block between two `##` lines just above it; `make help` prints those blocks.
# `make` with no target runs `make all`: the test suite first, then all three
# release builds. make runs prerequisites in order and stops at the first
# failure, so a failing test aborts before anything is built.
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
# ("qemu: uncaught target signal 11"). So instead we cross-compile with plain
# cargo and point it at NATIVE cross-toolchains: no containers, no emulation.
# Use Podman for container operations (Docker-compatible CLI).
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

##
# make help: Display this help message.
##
.PHONY: help
help:
	@awk '/^##/{a=1-a}a' $(MAKEFILE_LIST) | cut -c3-

##
# make all: Test first, then build. make evaluates prerequisites left to
# right and aborts on the first failure, so a failing test means no build runs.
##
.PHONY: all
all: test build

##
# make test: Test everything: GitHub Discussions, then the whole workspace's
# unit, integration and doc tests.
##
.PHONY: test
test: test-github-discussions
	$(CARGO) test --workspace

##
# make test-github-discussions: Test that GitHub Discussions is enabled on
# vixide/vix (the README links to it). Skips when `gh` is missing or not
# signed in.
##
.PHONY: test-github-discussions
test-github-discussions:
	bin/test-github-discussions

##
# make check: Run the local CI-parity gate, scripts/check: fmt, build,
# clippy (pedantic, -D warnings), tests, docs and more. Run it before pushing.
##
.PHONY: check
check:
	scripts/check

##
# make check-env: Print some of this project's important tools, their
# versions and paths, to help with installation, debugging and maintenance.
##
.PHONY: check-env
check-env::
	@echo "Print some of this project's important tools, their versions and paths."

check-env:: check-uname check-pnpm

##
# make check-uname: Print the uname information of the system.
##
.PHONY: check-uname
check-uname:
	@printf '\n### uname ###\n'
	uname -a

##
# make check-pnpm: Print the version and path of the command `pnpm`.
##
.PHONY: check-pnpm
check-pnpm:
	@printf '\n### pnpm ###\n'
	pnpm --version || true
	which -a pnpm || true

##
# make hooks: Install the repository hooks (T761, T760).
# Sets core.hooksPath so the hooks in scripts/hooks/ are used. Includes:
#   - pre-push: refuses to push main or a tag without a valid full-tier gate
#     attestation (scripts/attest)
#   - pre-commit: runs cargo fmt on staged Rust files for consistent formatting
# It only sets git config; `git config --unset core.hooksPath` removes it.
# Skip pre-commit formatting with VIX_SKIP_PRECOMMIT_FMT=1.
##
.PHONY: hooks
hooks:
	git config core.hooksPath scripts/hooks
	@echo "hooks installed: core.hooksPath = scripts/hooks"

##
# make build: Build every platform (skips tests).
# Each build-* recipe is independent and self-contained.
##
.PHONY: build
build: build-macos build-windows build-linux

##
# make build-macos: Build for macOS, natively on an Apple Silicon host.
# No special linker needed.
##
.PHONY: build-macos
build-macos:
	rustup target add $(MACOS_TARGET)
	$(CARGO) build --release --target $(MACOS_TARGET)

##
# make build-windows: Build for Windows (GNU ABI), cross-compiled with the
# mingw-w64 toolchain. The linker and the cc-rs CC/AR overrides come
# from .cargo/config.toml.
##
.PHONY: build-windows
build-windows:
	rustup target add $(WINDOWS_TARGET)
	$(CARGO) build --release --target $(WINDOWS_TARGET)

##
# make build-linux: Build for Linux (musl), cross-compiled with the musl
# toolchain, producing a fully static binary that runs on any x86-64 Linux
# without a libc dependency. The linker and the cc-rs CC/AR overrides come
# from .cargo/config.toml.
##
.PHONY: build-linux
build-linux:
	rustup target add $(LINUX_TARGET)
	$(CARGO) build --release --target $(LINUX_TARGET)

##
# make clean: Remove all build artifacts (the target/ directory).
##
.PHONY: clean
clean:
	$(CARGO) clean

##
# make lint: Proofread everything.
##
.PHONY: lint
lint: lint-markdown

##
# make lint-markdown: Proofread Markdown files: spelling and links
# (scripts/check-docs), plus markdownlint-cli2 when it is installed.
##
.PHONY: lint-markdown
lint-markdown:
	@scripts/lint-markdown.sh check --quiet

##
# make publish-github-pages: Publish the website subproject, vixide.github.io/,
# to its own read-only sibling repository (spec/monorepo-github-pages/).
##
.PHONY: publish-github-pages
publish-github-pages:
	bin/publish-github-pages
