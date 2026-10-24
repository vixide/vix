# main.rs and lib.rs boilerplate

Every compilation root of the `vix` package — `tui/src/main.rs`, `tui/src/lib.rs`, each
file under `tests/` and `tui/examples/` (and `tui/benches/`, if any) — must begin with
comprehensive rustdoc comments. The lint policy (`unsafe_code`, `missing_docs`,
`clippy::pedantic`) is not repeated here: it comes from the root
`[workspace.lints]` table through each crate's `[lints] workspace = true`.

```rust
// When we build for MUSL static, use faster memory allocator.
#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
```
