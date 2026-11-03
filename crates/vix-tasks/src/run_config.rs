//! Named run configurations (T724): saved, shareable, compound, with
//! before-launch steps.
//!
//! A project keeps its configurations in `.vix/run.toml` (committed, so a team
//! shares them):
//!
//! ```toml
//! [[config]]
//! name = "api"
//! command = "cargo run -p api"
//! cwd = "services/api"          # optional, relative to the project root
//! env = { PORT = "8080" }       # optional
//! before = ["build"]            # run (and succeed) first; a config or a task
//!
//! [[config]]
//! name = "stack"
//! compound = ["api", "web"]     # run together
//! ```
//!
//! This module is pure: [`parse`](run_config::parse) reads the file, [`plan`](run_config::plan) flattens a
//! configuration into ordered [`Plan`](run_config::Plan) stages (before-launch steps first;
//! the members of a compound together), and [`Plan::shell_command`](run_config::Plan::shell_command) renders
//! one shell line for the host's single-command pipeline.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use serde::Deserialize;

use crate::Task;

/// One named run configuration as written in `.vix/run.toml`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct RunConfig {
    /// Display name, also how `before` and `compound` refer to it.
    pub name: String,
    /// The shell command to run (exclusive with `compound`).
    #[serde(default)]
    pub command: Option<String>,
    /// Working directory, relative to the project root.
    #[serde(default)]
    pub cwd: Option<String>,
    /// Environment variables set for the command.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Configurations (or `tasks.toml` tasks) that must run first.
    #[serde(default)]
    pub before: Vec<String>,
    /// Configurations to run together (exclusive with `command`).
    #[serde(default)]
    pub compound: Vec<String>,
}

/// The `run.toml` schema.
#[derive(Debug, Default, Deserialize)]
struct RunFile {
    #[serde(default)]
    config: Vec<RunConfig>,
}

/// Why a run configuration could not be read or planned.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RunConfigError {
    /// The file is not valid TOML for the schema (carries the parser message).
    Parse(String),
    /// Two configurations share a name.
    Duplicate(String),
    /// A configuration has neither or both of `command` and `compound`.
    Invalid(String),
    /// A `before` or `compound` entry names nothing known.
    Unknown {
        /// The configuration that refers to it.
        config: String,
        /// The missing name.
        name: String,
    },
    /// Configurations depend on each other in a loop.
    Cycle(String),
    /// The requested configuration does not exist.
    NotFound(String),
}

impl fmt::Display for RunConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(m) => write!(f, "{m}"),
            Self::Duplicate(n) | Self::Invalid(n) | Self::Cycle(n) | Self::NotFound(n) => {
                write!(f, "{n}")
            }
            Self::Unknown { config, name } => write!(f, "{config}: {name}"),
        }
    }
}

impl std::error::Error for RunConfigError {}

/// Parse a `run.toml` body, validating names and shapes.
///
/// # Errors
/// [`RunConfigError::Parse`], [`RunConfigError::Duplicate`] or
/// [`RunConfigError::Invalid`].
pub fn parse(text: &str) -> Result<Vec<RunConfig>, RunConfigError> {
    let file: RunFile = toml::from_str(text).map_err(|e| RunConfigError::Parse(e.to_string()))?;
    let mut seen = BTreeSet::new();
    for c in &file.config {
        if c.name.trim().is_empty() {
            return Err(RunConfigError::Invalid(String::new()));
        }
        if !seen.insert(c.name.clone()) {
            return Err(RunConfigError::Duplicate(c.name.clone()));
        }
        let has_cmd = c.command.as_deref().is_some_and(|s| !s.trim().is_empty());
        if has_cmd != c.compound.is_empty() {
            return Err(RunConfigError::Invalid(c.name.clone()));
        }
    }
    Ok(file.config)
}

/// Load `<root>/.vix/run.toml`. A missing file is an empty list.
///
/// # Errors
/// As [`parse`].
pub fn load(root: &Path) -> Result<Vec<RunConfig>, RunConfigError> {
    match vix_authority::current().read_to_string(&root.join(".vix").join("run.toml")) {
        Ok(text) => parse(&text),
        Err(_) => Ok(Vec::new()),
    }
}

/// One command to run, with where and how.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// The configuration or task it came from.
    pub name: String,
    /// The shell command.
    pub command: String,
    /// Working directory relative to the project root.
    pub cwd: Option<String>,
    /// Environment variables.
    pub env: BTreeMap<String, String>,
}

/// Ordered stages; the steps of one stage run together, a stage starts only
/// when the previous one succeeded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// The stages in order.
    pub stages: Vec<Vec<Step>>,
}

/// Flatten configuration `name` into a [`Plan`]. A `before` entry that is not
/// a configuration is looked up among `tasks` (the `tasks.toml` list).
///
/// # Errors
/// [`RunConfigError::NotFound`], [`RunConfigError::Unknown`] or
/// [`RunConfigError::Cycle`].
pub fn plan(configs: &[RunConfig], tasks: &[Task], name: &str) -> Result<Plan, RunConfigError> {
    let mut plan = Plan::default();
    let mut done = BTreeSet::new();
    let mut stack = Vec::new();
    expand(configs, tasks, name, None, &mut plan, &mut done, &mut stack)?;
    Ok(plan)
}

/// Depth-first expansion of `name` and its `before` dependencies into `plan`
/// stages, dependencies first. `done` memoizes finished names (diamond
/// dependencies run once); `stack` is the current path, so revisiting a name on
/// it is a [`RunConfigError::Cycle`]. `referrer` is the config that named
/// `name` (None for the root), used for error messages and to allow a plain
/// discovered task as a `before` entry.
fn expand(
    configs: &[RunConfig],
    tasks: &[Task],
    name: &str,
    referrer: Option<&str>,
    plan: &mut Plan,
    done: &mut BTreeSet<String>,
    stack: &mut Vec<String>,
) -> Result<(), RunConfigError> {
    if done.contains(name) {
        return Ok(());
    }
    if stack.iter().any(|s| s == name) {
        return Err(RunConfigError::Cycle(name.to_string()));
    }
    let Some(cfg) = configs.iter().find(|c| c.name == name) else {
        // A before entry may be a plain task.
        if let (Some(_), Some(task)) = (referrer, tasks.iter().find(|t| t.name == name)) {
            done.insert(name.to_string());
            plan.stages.push(vec![Step {
                name: task.name.clone(),
                command: task.command.clone(),
                cwd: None,
                env: BTreeMap::new(),
            }]);
            return Ok(());
        }
        return Err(match referrer {
            Some(r) => RunConfigError::Unknown {
                config: r.to_string(),
                name: name.to_string(),
            },
            None => RunConfigError::NotFound(name.to_string()),
        });
    };
    stack.push(name.to_string());
    for b in &cfg.before {
        expand(configs, tasks, b, Some(name), plan, done, stack)?;
    }
    let mut stage = Vec::new();
    if cfg.compound.is_empty() {
        stage.push(step_of(cfg));
    } else {
        for m in &cfg.compound {
            let Some(member) = configs.iter().find(|c| &c.name == m) else {
                return Err(RunConfigError::Unknown {
                    config: name.to_string(),
                    name: m.clone(),
                });
            };
            if stack.iter().any(|s| s == m) {
                return Err(RunConfigError::Cycle(m.clone()));
            }
            for b in &member.before {
                expand(configs, tasks, b, Some(m), plan, done, stack)?;
            }
            if member.compound.is_empty() {
                stage.push(step_of(member));
            } else {
                // A nested compound runs as its own earlier-or-same group:
                // flatten its members into this stage.
                for mm in &member.compound {
                    if let Some(leaf) = configs.iter().find(|c| &c.name == mm)
                        && leaf.compound.is_empty()
                    {
                        stage.push(step_of(leaf));
                    }
                }
            }
        }
    }
    stack.pop();
    done.insert(name.to_string());
    plan.stages.push(stage);
    Ok(())
}

/// The runnable [`Step`] for one configuration; a config without a `command`
/// (a pure grouping of `before` entries) becomes an empty command.
fn step_of(c: &RunConfig) -> Step {
    Step {
        name: c.name.clone(),
        command: c.command.clone().unwrap_or_default(),
        cwd: c.cwd.clone(),
        env: c.env.clone(),
    }
}

/// Which shell dialect to render for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shell {
    /// `sh`-compatible (Linux, macOS, BSD).
    Posix,
    /// Windows `cmd.exe`.
    Cmd,
}

/// Quote `s` as one POSIX shell word.
fn posix_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

impl Step {
    /// This step as one shell command (cwd and env applied).
    #[must_use]
    pub fn shell_command(&self, shell: Shell) -> String {
        use std::fmt::Write as _;
        match shell {
            Shell::Posix => {
                let mut s = String::new();
                if let Some(cwd) = &self.cwd {
                    let _ = write!(s, "cd {} && ", posix_quote(cwd));
                }
                if !self.env.is_empty() {
                    s.push_str("env ");
                    for (k, v) in &self.env {
                        let _ = write!(s, "{k}={} ", posix_quote(v));
                    }
                }
                s.push_str(&self.command);
                s
            }
            Shell::Cmd => {
                let mut s = String::new();
                if let Some(cwd) = &self.cwd {
                    let _ = write!(s, "cd /d \"{cwd}\" && ");
                }
                for (k, v) in &self.env {
                    let _ = write!(s, "set \"{k}={v}\" && ");
                }
                s.push_str(&self.command);
                s
            }
        }
    }
}

impl Plan {
    /// The whole plan as one shell line: stages joined with `&&` (so a failed
    /// stage stops the rest); the steps of a stage run together on POSIX
    /// shells (`cmd.exe` has no job control, so they run in order there).
    #[must_use]
    pub fn shell_command(&self, shell: Shell) -> String {
        let stages: Vec<String> = self
            .stages
            .iter()
            .filter(|s| !s.is_empty())
            .map(|stage| {
                let cmds: Vec<String> = stage.iter().map(|s| s.shell_command(shell)).collect();
                match (shell, cmds.len()) {
                    (_, 1) => cmds.into_iter().next().unwrap_or_default(),
                    (Shell::Posix, _) => {
                        let jobs: Vec<String> = cmds.iter().map(|c| format!("( {c} ) &")).collect();
                        format!("{{ {} wait; }}", jobs.join(" "))
                    }
                    (Shell::Cmd, _) => cmds.join(" && "),
                }
            })
            .collect();
        stages.join(" && ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
        [[config]]
        name = "build"
        command = "cargo build"

        [[config]]
        name = "api"
        command = "cargo run -p api"
        cwd = "services/api"
        env = { PORT = "8080" }
        before = ["build"]

        [[config]]
        name = "web"
        command = "npm run dev"

        [[config]]
        name = "stack"
        compound = ["api", "web"]
    "#;

    #[test]
    fn parses_and_validates() {
        let c = parse(SAMPLE).unwrap();
        assert_eq!(c.len(), 4);
        assert_eq!(c[1].env.get("PORT").map(String::as_str), Some("8080"));
        assert_eq!(
            parse("[[config]]\nname='a'\n").unwrap_err(),
            RunConfigError::Invalid("a".into())
        );
        assert_eq!(
            parse("[[config]]\nname='a'\ncommand='x'\n[[config]]\nname='a'\ncommand='y'\n")
                .unwrap_err(),
            RunConfigError::Duplicate("a".into())
        );
        assert_eq!(
            parse("[[config]]\nname='a'\ncommand='x'\ncompound=['b']\n").unwrap_err(),
            RunConfigError::Invalid("a".into())
        );
        assert!(matches!(parse("= nope"), Err(RunConfigError::Parse(_))));
        assert_eq!(parse("").unwrap(), []);
    }

    #[test]
    fn plan_puts_before_steps_first() {
        let c = parse(SAMPLE).unwrap();
        let p = plan(&c, &[], "api").unwrap();
        assert_eq!(p.stages.len(), 2);
        assert_eq!(p.stages[0][0].name, "build");
        assert_eq!(p.stages[1][0].name, "api");
        assert_eq!(
            p.shell_command(Shell::Posix),
            "cargo build && cd 'services/api' && env PORT='8080' cargo run -p api"
        );
    }

    #[test]
    fn compound_runs_members_together() {
        let c = parse(SAMPLE).unwrap();
        let p = plan(&c, &[], "stack").unwrap();
        // build (api's before), then api + web together.
        assert_eq!(p.stages.len(), 2);
        assert_eq!(p.stages[1].len(), 2);
        let sh = p.shell_command(Shell::Posix);
        assert!(sh.starts_with("cargo build && { ( cd 'services/api'"));
        assert!(sh.ends_with("( npm run dev ) & wait; }"));
        let cmd = p.shell_command(Shell::Cmd);
        assert!(cmd.contains("set \"PORT=8080\" && cargo run -p api && npm run dev"));
    }

    #[test]
    fn before_may_name_a_task_and_shared_befores_run_once() {
        let c = parse(
            "[[config]]\nname='a'\ncommand='A'\nbefore=['lint']\n\
             [[config]]\nname='b'\ncommand='B'\nbefore=['lint']\n\
             [[config]]\nname='ab'\ncompound=['a','b']\n",
        )
        .unwrap();
        let tasks = [Task {
            name: "lint".into(),
            command: "make lint".into(),
            matcher: None,
        }];
        let p = plan(&c, &tasks, "ab").unwrap();
        assert_eq!(p.shell_command(Shell::Cmd), "make lint && A && B");
    }

    #[test]
    fn reports_unknown_cycle_and_missing() {
        let c = parse("[[config]]\nname='a'\ncommand='A'\nbefore=['zzz']\n").unwrap();
        assert_eq!(
            plan(&c, &[], "a").unwrap_err(),
            RunConfigError::Unknown {
                config: "a".into(),
                name: "zzz".into()
            }
        );
        assert_eq!(
            plan(&c, &[], "nope").unwrap_err(),
            RunConfigError::NotFound("nope".into())
        );
        let c = parse(
            "[[config]]\nname='a'\ncommand='A'\nbefore=['b']\n\
             [[config]]\nname='b'\ncommand='B'\nbefore=['a']\n",
        )
        .unwrap();
        assert!(matches!(plan(&c, &[], "a"), Err(RunConfigError::Cycle(_))));
    }

    #[test]
    fn single_quotes_in_values_are_escaped() {
        let s = Step {
            name: "x".into(),
            command: "run".into(),
            cwd: None,
            env: BTreeMap::from([("K".into(), "it's".into())]),
        };
        assert_eq!(s.shell_command(Shell::Posix), "env K='it'\\''s' run");
    }

    #[test]
    fn load_of_a_missing_file_is_empty() {
        assert_eq!(load(Path::new("/nonexistent/project")).unwrap(), []);
    }
}
