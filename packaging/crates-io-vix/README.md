# vix (moved to vix-tui)

Vix, the terminal editor and IDE, is now published as
[`vix-tui`](https://crates.io/crates/vix-tui). It still installs a command
named `vix`.

```sh
cargo uninstall vix
cargo install vix-tui
```

This last `vix` release (1.7.2) contains no editor. Its `vix` command
prints these steps and exits. Version 1.7.1 and earlier are still on
crates.io if you need them (`cargo install vix --version 1.7.1`).

Other ways to install (Homebrew, npm, installers, Linux packages):
<https://vixide.github.io>. Source: <https://github.com/vixide/vix>.
