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
#[allow(
    clippy::struct_excessive_bools,
    reason = "each bool is an independent clap `#[arg(long)]` flag; the derive needs one named field per flag"
)]
#[derive(clap::Parser, Debug, Clone)]
#[command(
    name = "vix",
    about = "Vix: Simple Terminal Rust IDE",
    disable_version_flag = true
)]
/// The parsed `vix` command line. Each field is one flag or positional
/// argument; `main` inspects the headless ones first ([`Cli::version`],
/// [`Cli::mcp`], the plugin and settings-bundle commands) and passes the rest
/// into the `App`.
pub struct Cli {
    /// File(s) to open on startup; the last one is focused. A single `-`
    /// reads standard input into an unsaved scratch buffer instead of
    /// opening a file (e.g. `git show HEAD:path | vix -`). A single
    /// `ssh://[user@]host[:port]/path` (or `/ssh:host:/path`) works on that
    /// folder over SSH (experimental); `docker://[user@]container/path`,
    /// `podman://…`, `k8s://namespace/pod/container/path` and
    /// `wsl://distro/path` work inside a running container, pod or WSL
    /// distribution. A single folder (`vix .`, `vix ~/src/app`) is opened as
    /// the workspace instead ([`Cli::take_workspace_dir`]).
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

    /// Serve the Model Context Protocol over stdin/stdout instead of
    /// starting the editor, so an AI agent can read this workspace as Vix
    /// sees it: open buffers, symbols, outline, offline diagnostics and
    /// SQLite schemas. Read-only. The workspace is the one directory given
    /// as an argument, or the current directory. See `spec/mcp-server`.
    #[arg(long)]
    pub mcp: bool,

    /// Serve Vix's remote-agent protocol over stdin/stdout instead of
    /// starting the editor. A Vix on another machine starts this over SSH
    /// to work on this machine's files faster; it is not for interactive
    /// use. File-system operations only; exits when its input ends. See
    /// `docs/remote-editing`.
    #[arg(long)]
    pub remote_agent: bool,

    /// Manage Rhai plugins, then exit without starting the editor:
    /// `install <git-url>[@rev]` clones a plugin into the scripts directory
    /// and pins its commit in `plugins.lock` (disabled until enabled in
    /// **Vix → Plugins…**); `list` shows them; `update [NAME]` moves one
    /// (or all) to the newest commit, disabling any whose code changed;
    /// `remove NAME` deletes one with its trust record. See
    /// `crates/vix-script/spec/index.md`, "Plugins".
    #[arg(long, num_args = 1..=2, value_names = ["COMMAND", "ARG"])]
    pub plugin: Option<Vec<String>>,
}

impl Cli {
    /// When the only path given is an existing folder, take it out of
    /// [`Cli::files`] and return it, canonicalized, to be the workspace root:
    /// `vix ~/src/app` opens that project, as other editors do, rather than
    /// failing to open a folder as a file. `None`, and [`Cli::files`]
    /// untouched, otherwise (no path, several, a file, or a missing path).
    pub fn take_workspace_dir(&mut self) -> Option<std::path::PathBuf> {
        let dir = self.workspace_dir()?;
        self.files.clear();
        Some(dir)
    }

    /// The folder [`Cli::take_workspace_dir`] would take, without taking it.
    #[must_use]
    pub fn workspace_dir(&self) -> Option<std::path::PathBuf> {
        let [only] = self.files.as_slice() else {
            return None;
        };
        only.canonicalize().ok().filter(|p| p.is_dir())
    }

    /// The workspace root for a local run: [`Cli::workspace_dir`], else the
    /// current directory (else `.`). The desktop GUIs use this; the terminal
    /// binary also handles remote targets first.
    #[must_use]
    pub fn root_dir(&self) -> std::path::PathBuf {
        self.workspace_dir().unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
        })
    }

    /// These arguments with a lone workspace folder taken out of
    /// [`Cli::files`], for `App::start` once the folder is the root.
    #[must_use]
    pub fn without_workspace_dir(&self) -> Cli {
        let mut cli = self.clone();
        cli.take_workspace_dir();
        cli
    }
}

#[cfg(test)]
mod workspace_dir_tests {
    use super::Cli;
    use clap::Parser as _;

    #[test]
    fn a_single_folder_becomes_the_workspace() {
        let dir = std::env::temp_dir().join(format!("vix-cli-ws-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        let parse = |args: &[&std::path::Path]| {
            Cli::parse_from(
                std::iter::once("vix".as_ref()).chain(args.iter().map(|p| p.as_os_str())),
            )
        };

        let mut cli = parse(&[&dir.join("sub")]);
        let sub = dir.join("sub").canonicalize().unwrap();
        assert_eq!(
            cli.workspace_dir(),
            Some(sub.clone()),
            "asking takes nothing"
        );
        assert_eq!(cli.root_dir(), sub);
        assert_eq!(
            cli.without_workspace_dir().files,
            Vec::<std::path::PathBuf>::new()
        );
        assert_eq!(cli.files.len(), 1, "the original keeps its argument");
        assert_eq!(cli.take_workspace_dir(), Some(sub));
        assert!(
            cli.files.is_empty(),
            "the folder is not also opened as a file"
        );

        for args in [
            vec![dir.join("a.txt")],
            vec![dir.join("missing")],
            vec![dir.join("sub"), dir.join("a.txt")],
            vec![],
        ] {
            let refs: Vec<&std::path::Path> =
                args.iter().map(std::path::PathBuf::as_path).collect();
            let mut cli = parse(&refs);
            let before = cli.files.clone();
            assert_eq!(cli.root_dir(), std::env::current_dir().unwrap(), "{args:?}");
            assert_eq!(cli.take_workspace_dir(), None, "{args:?}");
            assert_eq!(cli.files, before);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
