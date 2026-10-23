# Clippy

For Rust code.

Set it once, in the workspace `Cargo.toml`:

```toml
[workspace.lints.clippy]
pedantic = { level = "warn", priority = -1 }
```

and give every crate `[lints] workspace = true`. That covers all targets,
including `main.rs`, `lib.rs`, `tests/`, `examples/`, `benches/` and `build.rs`,
without repeating an attribute in each file (T599).
