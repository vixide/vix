# main.rs and lib.rs boilerplate

Every compilation root of the `vix` package — `src/main.rs`, `src/lib.rs`, each
file under `tests/` and `examples/` (and `benches/`, if any) — must begin with
comprehensive rustdoc comments then this section. (Lint attributes are per–crate
root, so each target needs its own copy.)

```rust
// Always start with high quality coding conventions.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

// When we build for MUSL static, use faster memory allocator.
#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
```
