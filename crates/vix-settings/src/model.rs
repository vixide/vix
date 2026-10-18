//! Plain serde data types for settings sections whose behavior lives in other
//! crates (T584): DB connections, debug adapters, and org-capture templates.
//!
//! Defined here so `Settings` round-trips every section in every build — a
//! build without a capability must still load and re-save that capability's
//! config verbatim. `vix-db`, `vix-dap` and `vix-org-capture` re-export them.

use serde::{Deserialize, Serialize};

/// Which database engine a connection targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// `SQLite` file (bundled driver).
    #[default]
    Sqlite,
    /// `PostgreSQL` server (pure-Rust driver over rustls).
    Postgres,
    /// `MySQL` / `MariaDB` server (pure-Rust driver over rustls).
    Mysql,
}

impl Kind {
    /// Short display label (`sqlite`, `postgres`, `mysql`).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Kind::Sqlite => "sqlite",
            Kind::Postgres => "postgres",
            Kind::Mysql => "mysql",
        }
    }

    /// The next kind, cycling; used by the connection form's kind field.
    #[must_use]
    pub fn next(self) -> Kind {
        match self {
            Kind::Sqlite => Kind::Postgres,
            Kind::Postgres => Kind::Mysql,
            Kind::Mysql => Kind::Sqlite,
        }
    }

    /// Default server port as a string (empty for `SQLite`).
    #[must_use]
    pub fn default_port(self) -> &'static str {
        match self {
            Kind::Sqlite => "",
            Kind::Postgres => "5432",
            Kind::Mysql => "3306",
        }
    }
}

/// One saved database connection. Passwords are deliberately absent: they are
/// prompted for at connect time and held only in memory (see `spec/index.md`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Connection {
    /// Display name shown in the connections list.
    pub name: String,
    /// Database engine.
    pub kind: Kind,
    /// `SQLite` database file path (unused for server kinds).
    pub file: String,
    /// Server host (unused for `SQLite`).
    pub host: String,
    /// Server port as text; empty uses the engine default.
    pub port: String,
    /// Login user (unused for `SQLite`).
    pub user: String,
    /// Database name to open (unused for `SQLite`).
    pub database: String,
    /// Whether the connection may run writes / DDL. Defaults to `false`
    /// (read-only), so a fresh or legacy connection opens read-only until the
    /// user opts in — writes are refused client-side and, where the engine
    /// supports it, blocked at the database level (see `read_only_sql`).
    pub writable: bool,
    /// A shell command whose stdout is the password (e.g. `pass show db/prod`,
    /// `op read op://vault/db/password`). Tried first by the credential
    /// waterfall so the password prompt can be skipped (see `vix_db::secret`).
    pub password_command: String,
    /// Whether to store a prompted password in the OS keyring on a successful
    /// connect, so later connects resolve it without prompting.
    pub store_keyring: bool,
    /// SSH jump host — when set, the connection is forwarded through an
    /// `ssh -N -L` tunnel (see `vix_db::tunnel`); empty disables it.
    pub ssh_host: String,
    /// SSH login user (defaults to the ssh client's default when empty).
    pub ssh_user: String,
    /// SSH port (defaults to 22 when empty).
    pub ssh_port: String,
    /// Path to an SSH identity file (optional; agent auth is used otherwise).
    pub ssh_identity: String,
    /// TLS mode appended to the server URL. Empty (the default) preserves the
    /// driver default (`prefer` — encrypt if offered, but silently fall back to
    /// plaintext and don't verify the certificate). Set to e.g. `require`
    /// (encrypt), `verify-ca`, or `verify-full` (encrypt + verify cert/hostname)
    /// to prevent a plaintext downgrade / MITM. Rendered as `sslmode=` for
    /// Postgres and `ssl-mode=` for MySQL. Ignored for `SQLite`. For an
    /// SSH-tunnelled connection (local `127.0.0.1` endpoint) prefer `verify-ca`,
    /// since `verify-full` checks the hostname against the loopback address.
    pub sslmode: String,
}

impl Connection {
    /// Whether connecting should prompt for a password (server engines only).
    #[must_use]
    pub fn needs_password(&self) -> bool {
        !matches!(self.kind, Kind::Sqlite)
    }

    /// The access label shown in the form and connections list.
    #[must_use]
    pub fn access_label(&self) -> &'static str {
        if self.writable {
            "read-write"
        } else {
            "read-only"
        }
    }

    /// One-line summary of the target, for the connections list.
    #[must_use]
    pub fn target(&self) -> String {
        match self.kind {
            Kind::Sqlite => self.file.clone(),
            Kind::Postgres | Kind::Mysql => {
                let port = if self.port.is_empty() {
                    self.kind.default_port()
                } else {
                    &self.port
                };
                format!("{}@{}:{}/{}", self.user, self.host, port, self.database)
            }
        }
    }
}

/// A configured debug adapter (parallels `LspServer` in `vix-settings`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugAdapter {
    /// Adapter id sent in `initialize` (e.g. `"codelldb"`, `"debugpy"`).
    pub adapter_id: String,
    /// File extensions (without the dot) this adapter debugs, e.g. `["rs"]`.
    pub extensions: Vec<String>,
    /// Launch command: program then args, e.g. `["codelldb", "--port", "0"]`.
    pub command: Vec<String>,
    /// Extra fields merged into the `launch` request arguments (as TOML → JSON),
    /// e.g. `program`, `cwd`, `args`. `{program}` is replaced with the file path.
    #[serde(default)]
    pub launch: std::collections::BTreeMap<String, String>,
}

/// How the expanded template is wrapped when inserted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryType {
    /// A headline (`* ...`); the default.
    #[default]
    Entry,
    /// Raw text, inserted verbatim with no wrapper.
    Plain,
    /// A plain list item (`- ...`).
    Item,
    /// A checkbox list item (`- [ ] ...`).
    CheckItem,
    /// A table row, appended as `| ... |`.
    TableLine,
}

/// A user-configured capture template (an entry in the `org_capture_templates`
/// setting).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureTemplate {
    /// Single-character (by convention) key identifying this template.
    pub key: String,
    /// Human-readable label shown in the capture menu/chooser.
    pub description: String,
    /// How the expanded template is wrapped.
    #[serde(default)]
    pub entry_type: EntryType,
    /// Where the entry is filed; see `vix_org_capture::Target::parse`.
    #[serde(default = "default_target")]
    pub target: String,
    /// The template body: literal text plus placeholders (`%^{Prompt}`, `%t`,
    /// `%a`, …).
    pub template: String,
    /// Insert at the top of the target (headline/file) instead of the bottom.
    #[serde(default)]
    pub prepend: bool,
    /// Blank lines to pad before and after the inserted entry.
    #[serde(default)]
    pub empty_lines: u8,
    /// Skip the review step and file immediately once every `%^{}` prompt
    /// (and the tag prompt, if any) is answered.
    #[serde(default)]
    pub immediate_finish: bool,
    /// Start a `CLOCK:` entry on the newly captured headline.
    #[serde(default)]
    pub clock_in: bool,
}

fn default_target() -> String {
    "cursor".to_string()
}

/// The built-in templates seeded into `org_capture_templates` by default:
/// `Anything` (a quick `* TODO`), `Todo` (a blank `* TODO` to fill in), and
/// `Contact` (an org-contacts-style entry with a property drawer).
#[must_use]
pub fn default_capture_templates() -> Vec<CaptureTemplate> {
    vec![
        CaptureTemplate {
            key: "a".to_string(),
            description: "Anything".to_string(),
            entry_type: EntryType::Entry,
            target: default_target(),
            template: "* TODO %^{Task}".to_string(),
            prepend: false,
            empty_lines: 0,
            immediate_finish: true,
            clock_in: false,
        },
        CaptureTemplate {
            key: "t".to_string(),
            description: "Task".to_string(),
            entry_type: EntryType::Entry,
            target: default_target(),
            template: "* TODO ".to_string(),
            prepend: false,
            empty_lines: 0,
            immediate_finish: false,
            clock_in: false,
        },
        CaptureTemplate {
            key: "b".to_string(),
            description: "Babel".to_string(),
            entry_type: EntryType::Plain,
            target: default_target(),
            template: "#+begin_src %^{Language}\n%?\n#+end_src".to_string(),
            prepend: false,
            empty_lines: 0,
            immediate_finish: false,
            clock_in: false,
        },
        CaptureTemplate {
            key: "n".to_string(),
            description: "Note".to_string(),
            entry_type: EntryType::Entry,
            target: default_target(),
            template: "* %^{Note}\n  %U".to_string(),
            prepend: false,
            empty_lines: 0,
            immediate_finish: true,
            clock_in: false,
        },
        CaptureTemplate {
            key: "c".to_string(),
            description: "Contact".to_string(),
            entry_type: EntryType::Entry,
            target: default_target(),
            template: concat!(
                "* %^{Name}\n",
                "  :PROPERTIES:\n",
                "  :EMAIL: %^{Email}\n",
                "  :PHONE: %^{Phone}\n",
                "  :ADDRESS: %^{Address}\n",
                "  :BIRTHDAY: %^{Birthday}\n",
                "  :END:",
            )
            .to_string(),
            prepend: false,
            empty_lines: 0,
            immediate_finish: true,
            clock_in: false,
        },
    ]
}
