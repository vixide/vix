//! The command-line interface definition, shared by the `vix` binary
//! (`src/main.rs`) and the man-page generator (`examples/generate_man.rs`,
//! T307, `tasks.md`) so the two can never drift: one `clap` definition, one
//! generated page.

// `--version` is hand-rolled (`disable_version_flag`) rather than left to
// clap's automatic short-circuiting handler, so `--version --json` can be
// told apart from plain `--version` before anything exits (T208). A plain
// comment, not a doc comment: clap's derive would otherwise show this
// implementation rationale as the command's own `--help` text.
/// Command-line interface for Vix.
// `clippy::struct_excessive_bools`: each is an independent, freely-combinable
// `#[arg(long)]` CLI flag, not related state a bitflags field could group --
// `clap::Parser`'s derive needs one named `bool` field per flag, so there is
// no bitflags-shaped alternative here the way T149 found for `App`/`Settings`.
#[allow(clippy::struct_excessive_bools)]
#[derive(clap::Parser, Debug)]
#[command(
    name = "vix",
    about = "Vix: Simple Terminal Rust IDE",
    disable_version_flag = true
)]
pub struct Cli {
    /// File(s) to open on startup; the last one is focused. A single `-`
    /// reads standard input into an unsaved scratch buffer instead of
    /// opening a file (e.g. `git show HEAD:path | vix -`).
    pub files: Vec<std::path::PathBuf>,

    /// UI language as a locale code (e.g. en, es, fr, de, cy). Overrides the
    /// saved `locale` setting for this run only.
    #[arg(short, long)]
    pub locale: Option<String>,

    /// Open a read-only unified-diff overlay comparing two files directly,
    /// bypassing the active buffer — the shape a `git difftool` driver
    /// invokes with (`$LOCAL $REMOTE`). See `docs/cli/index.md` for the git
    /// config snippet.
    #[arg(long, num_args = 2, value_names = ["OLD", "NEW"])]
    pub diff: Option<Vec<std::path::PathBuf>>,

    /// Print the version and exit. Combine with `--json` for a
    /// machine-readable form.
    #[arg(long)]
    pub version: bool,

    /// With `--version`, print `{"name", "version"}` as JSON instead of
    /// plain text.
    #[arg(long, requires = "version")]
    pub json: bool,

    /// Launch straight into the interactive tutorial (also reachable from a
    /// running session via **Help → Tutorial**), in place of any files also
    /// passed. See `crates/vix-tutor/spec/index.md`.
    #[arg(long)]
    pub tutor: bool,

    /// Check the environment for common setup friction (is `git` on
    /// `PATH`, are configured LSP servers spawnable, does the active
    /// locale's spellcheck dictionary load, does the terminal look
    /// color-capable) and print a plain pass/fail report, then exit
    /// without starting the editor. Also reachable from a running session
    /// via **Help → Run Diagnostics**. See `crates/vix-doctor/spec/index.md`.
    #[arg(long)]
    pub doctor: bool,

    /// Bundle `config.toml`, `macros.toml`, `keybindings.toml`, the user
    /// dictionary, and the active custom theme into a single file at
    /// `PATH`, then exit without starting the editor. Also reachable from
    /// a running session via **Vix → Export Settings…**. See
    /// `crates/vix-settings-bundle/spec/index.md`.
    #[arg(long, value_name = "PATH")]
    pub export_settings: Option<std::path::PathBuf>,

    /// Restore a bundle written by `--export-settings` (or **Vix → Export
    /// Settings…**) from `PATH`, then exit without starting the editor —
    /// any file about to be overwritten is renamed to `<name>.bak` first.
    /// Also reachable from a running session via **Vix → Import
    /// Settings…**.
    #[arg(long, value_name = "PATH")]
    pub import_settings: Option<std::path::PathBuf>,
}
