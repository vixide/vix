# Cargo fmt

For Rust code.

Format all code with rustfmt:

```sh
cargo fmt
```

Verify formatting (CI parity, and before pushing or opening a PR):

```sh
cargo fmt --check
```

Applies to every crate and all targets, including any file `main.rs`, `lib.rs`,
`tests/`, `examples/`, `benches/`.
