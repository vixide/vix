# Rust `.cargo/config.toml` MUSL

Cross-compiling to the Linux MUSL static target (`x86_64-unknown-linux-musl`)
from macOS needs a real cross-toolchain, not the host's own `musl-gcc`
wrapper (which targets the host and can't produce Linux objects — vendored C
dependencies like `libmimalloc-sys`, the tree-sitter grammars, and image
codecs fail to compile without one). Install it via Homebrew:

```sh
brew install FiloSottile/musl-cross/musl-cross
```

Then in `.cargo/config.toml` (repo root — this key is ignored inside
`Cargo.toml` itself, which only accepts platform-specific *dependency*
tables):

```toml
[target.x86_64-unknown-linux-musl]
linker = "x86_64-linux-musl-gcc"
```

`CC_`/`CXX_`/`AR_` overrides for vendored C code also belong in cargo's config
(under `[env]`), pointed at the matching cross compilers/archiver:

```toml
[env]
CC_x86_64-unknown-linux-musl = "x86_64-linux-musl-gcc"
CXX_x86_64-unknown-linux-musl = "x86_64-linux-musl-g++"
AR_x86_64-unknown-linux-musl = "x86_64-linux-musl-ar"
```

The same file (`.cargo/config.toml`) also configures the
`x86_64-pc-windows-gnu` cross target the same way, via `mingw-w64`
(`brew install mingw-w64`) — see the file itself for the exact entries. A
plain `cargo build --target x86_64-unknown-linux-musl` then cross-compiles
correctly; the Makefile's `build-linux`/`build-windows` targets rely on
exactly this.

## Static, non-PIE link (T670)

The musl target must produce a fully static executable. The target table
therefore also sets

```toml
rustflags = ["-C", "target-feature=+crt-static", "-C", "relocation-model=static"]
```

Without it, linking through a `musl-gcc` wrapper gave a dynamically linked
PIE with `INTERP /lib/ld-musl-x86_64.so.1`: it could not start on glibc
systems and crashed at startup on Alpine. `scripts/check-static-binary`
fails if a binary is dynamically linked or has a `PT_INTERP`; it runs after
the build in `release-minimal.yml` and over the musl release archive in
`linux-packages.yml`. (A `RUSTFLAGS` environment variable replaces this
config setting, so release jobs must not set one.)
