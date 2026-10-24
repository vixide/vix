//! Build-time capabilities (T586): which optional areas of Vix this binary was
//! compiled with, and the action-id prefixes each one owns.
//!
//! Every capability is default-on, so a normal build is unchanged. A capability
//! compiled out is *gated at the root, filtered at runtime*: the area's code and
//! dependencies are `cfg`-gated here in the root package, while `vix-menu` and
//! `vix-palette` stay `cfg`-free and simply hide whatever [`is_available`]
//! rejects. See `spec/cargo-features/index.md`.

/// An optional area of Vix that a Cargo feature can compile out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    /// The DB workbench and the SQL editor (`sql`).
    Sql,
    /// Inline image preview (`image`).
    Image,
    /// The Rhai scripting engine (`script`).
    Script,
    /// The integrated PTY terminal (`terminal`).
    Terminal,
    /// The HTTP client (`http`).
    Http,
    /// The AI assistant (`ai`).
    Ai,
    /// Git integration (`git`).
    Git,
    /// Jujutsu integration (`jj`).
    Jj,
    /// Language servers (`lsp`): Vix starts the configured server programs.
    Lsp,
    /// Debug adapters (`dap`): Vix starts the configured adapter programs.
    Dap,
    /// The shell runner (`tasks`): Run Command, project tasks, tests, and
    /// everything else that hands a command line to the platform shell.
    Tasks,
    /// Asking the installed `hunspell` where its dictionaries live (`hunspell`).
    /// Owns no actions; it only decides whether that program is ever started.
    Hunspell,
}

impl Capability {
    /// Every capability, in a stable order.
    pub const ALL: [Capability; 12] = [
        Capability::Sql,
        Capability::Image,
        Capability::Script,
        Capability::Terminal,
        Capability::Http,
        Capability::Ai,
        Capability::Git,
        Capability::Jj,
        Capability::Lsp,
        Capability::Dap,
        Capability::Tasks,
        Capability::Hunspell,
    ];

    /// The Cargo feature (and user-visible) name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Capability::Sql => "sql",
            Capability::Image => "image",
            Capability::Script => "script",
            Capability::Terminal => "terminal",
            Capability::Http => "http",
            Capability::Ai => "ai",
            Capability::Git => "git",
            Capability::Jj => "jj",
            Capability::Lsp => "lsp",
            Capability::Dap => "dap",
            Capability::Tasks => "tasks",
            Capability::Hunspell => "hunspell",
        }
    }

    /// Whether this build includes the capability.
    #[must_use]
    pub fn enabled(self) -> bool {
        match self {
            Capability::Sql => cfg!(feature = "sql"),
            Capability::Image => cfg!(feature = "image"),
            Capability::Script => cfg!(feature = "script"),
            Capability::Terminal => cfg!(feature = "terminal"),
            Capability::Http => cfg!(feature = "http"),
            Capability::Ai => cfg!(feature = "ai"),
            Capability::Git => cfg!(feature = "git"),
            Capability::Jj => cfg!(feature = "jj"),
            Capability::Lsp => cfg!(feature = "lsp"),
            Capability::Dap => cfg!(feature = "dap"),
            Capability::Tasks => cfg!(feature = "tasks"),
            Capability::Hunspell => cfg!(feature = "hunspell"),
        }
    }

    /// The capability that owns `action`, by action-id prefix, or `None` for
    /// core actions.
    #[must_use]
    pub fn for_action(action: &str) -> Option<Capability> {
        const PREFIXES: &[(&str, Capability)] = &[
            ("db.", Capability::Sql),
            ("tools.edit_sql", Capability::Sql),
            ("script.", Capability::Script),
            ("script:", Capability::Script),
            ("tools.terminal", Capability::Terminal),
            ("tools.http_send", Capability::Http),
            ("ai.", Capability::Ai),
            ("git.", Capability::Git),
            ("jj.", Capability::Jj),
            ("lsp.", Capability::Lsp),
            // Every `run.*` action is a debugger (DAP) action.
            ("run.", Capability::Dap),
            ("project.", Capability::Tasks),
            ("tools.run_command", Capability::Tasks),
            ("tools.cancel_command", Capability::Tasks),
            ("tools.tasks", Capability::Tasks),
            ("tools.test", Capability::Tasks),
        ];
        PREFIXES
            .iter()
            .find(|(p, _)| action.starts_with(p))
            .map(|&(_, c)| c)
    }
}

/// Whether `action` may run (and be listed) in this build: core actions always,
/// a capability's actions only when it is compiled in.
#[must_use]
pub fn is_available(action: &str) -> bool {
    Capability::for_action(action).is_none_or(Capability::enabled)
}

/// The names of the capabilities compiled into this build, in [`Capability::ALL`]
/// order (for `--version --json` and `--doctor`).
#[must_use]
pub fn enabled_names() -> Vec<&'static str> {
    Capability::ALL
        .into_iter()
        .filter(|c| c.enabled())
        .map(Capability::name)
        .collect()
}

/// Every `--doctor` check for this build: the standard set from `vix-doctor`,
/// minus the probes that start a program (`git`, each LSP server) when that
/// capability is compiled out (a locked-down build never spawns them, not even
/// to look), plus the capabilities row.
#[must_use]
pub fn doctor_checks(settings: &crate::settings::Settings) -> Vec<vix_doctor::Check> {
    let mut checks = vix_doctor::run(settings);
    checks.push(doctor_check());
    checks
}

/// A `--doctor` / Help → Run Diagnostics row listing the compiled-in
/// capabilities, so a bug report says which build the user ran.
#[must_use]
pub fn doctor_check() -> vix_doctor::Check {
    vix_doctor::Check {
        name: t!("doctor.name_capabilities").to_string(),
        status: vix_doctor::Status::Pass,
        detail: enabled_names().join(", "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_map_to_their_capability() {
        assert_eq!(
            Capability::for_action("db.connections"),
            Some(Capability::Sql)
        );
        assert_eq!(
            Capability::for_action("script:foo"),
            Some(Capability::Script)
        );
        assert_eq!(
            Capability::for_action("tools.terminal"),
            Some(Capability::Terminal)
        );
        assert_eq!(Capability::for_action("git.push"), Some(Capability::Git));
        assert_eq!(Capability::for_action("jj.log"), Some(Capability::Jj));
        assert_eq!(Capability::for_action("lsp.hover"), Some(Capability::Lsp));
        assert_eq!(Capability::for_action("run.start"), Some(Capability::Dap));
        assert_eq!(
            Capability::for_action("project.compile"),
            Some(Capability::Tasks)
        );
        assert_eq!(
            Capability::for_action("tools.run_command"),
            Some(Capability::Tasks)
        );
        assert_eq!(Capability::for_action("file.save"), None);
        // Prefix match is on the dot: `github.*`-style ids are not Git's.
        assert_eq!(Capability::for_action("gitx.thing"), None);
    }

    #[test]
    fn default_build_has_every_capability() {
        if cfg!(feature = "sql") {
            assert!(is_available("db.connections"));
        }
        assert_eq!(
            enabled_names().len(),
            Capability::ALL.iter().filter(|c| c.enabled()).count()
        );
    }
}
