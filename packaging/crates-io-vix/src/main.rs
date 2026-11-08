//! The last release of the old `vix` crate. Vix is now published as
//! `vix-tui` (it still installs a command named `vix`), so this binary only
//! says how to switch, then exits with status 1.

fn main() {
    eprintln!(
        "Vix moved to the vix-tui crate. To switch:\n\
         \n    cargo uninstall vix\n    cargo install vix-tui\n\
         \nOther ways to install: https://vixide.github.io"
    );
    std::process::exit(1);
}
