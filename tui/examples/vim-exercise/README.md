# Vim migration exercise

A tiny Rust crate with one bug in two places, for the runnable exercise in
[`docs/vix-for-vim-users`](../../../docs/vix-for-vim-users/index.md). Copy it
somewhere outside the repository and make it a git repository first, so the
last step (staging the fix) has something to stage:

```sh
cp -R tui/examples/vim-exercise /tmp/vim-exercise
cd /tmp/vim-exercise && git init -q && git add -A && git commit -qm start
```

It needs nothing but `cargo` and `git`, and no network. The automated test
`tui/tests/integration/vim_exercise.rs` walks the same steps, so they keep
working.
