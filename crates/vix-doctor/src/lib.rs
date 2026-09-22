//! Environment checks for `vix --doctor` and Help → Run Diagnostics: is
//! `git` on `PATH`, are the configured LSP servers actually spawnable, does
//! the active locale's spellcheck dictionary load, does the terminal look
//! color-capable. See `crates/vix-doctor/spec/index.md`.
//!
//! Both entry points ([`run`] plus [`format_report`]) work from a bare
//! [`vix_settings::Settings`] value, deliberately — the CLI path
//! (`vix --doctor`) runs before any `App` exists at all, so nothing here can
//! depend on the App shell or a terminal.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

// Shared workspace i18n: brings `t!` into scope unqualified and surfaces the
// translation lookup fns at this crate root (see the vix_i18n crate). T561:
// this crate originally called `t!` zero times -- shown inside a live,
// localized TUI overlay (Help → Run Diagnostics), not just the CLI-only
// `--doctor` path, so it needed this like every other user-facing crate.
#[macro_use]
extern crate vix_i18n;
vix_i18n::surface!();

use vix_settings::Settings;

/// The outcome of one [`Check`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The check succeeded.
    Pass,
    /// The check found a real problem.
    Fail,
    /// Nothing to check — a prerequisite for the check wasn't configured
    /// (e.g. no LSP servers at all), not a problem in itself.
    Skip,
}

/// One environment check's name, outcome, and a short human-readable detail.
#[derive(Debug, Clone)]
pub struct Check {
    /// Short label for the check, e.g. `"git"`.
    pub name: String,
    /// Whether it passed, failed, or was skipped.
    pub status: Status,
    /// A one-line explanation — what was checked, or why it failed/was
    /// skipped.
    pub detail: String,
}

/// Whether `program` can be spawned at all — used for both the `git` check
/// and each configured LSP server's own binary. Only the spawn itself is
/// checked (and the child is killed immediately once confirmed started);
/// a nonzero exit code from `--version` (or a server that doesn't
/// understand that flag) is not treated as failure, since the only thing
/// this needs to know is "does the shell know where to find this."
fn spawnable(program: &str) -> bool {
    std::process::Command::new(program)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|mut child| {
            let _ = child.kill();
            let _ = child.wait();
        })
        .is_ok()
}

/// Is `git` on `PATH`?
#[must_use]
pub fn check_git() -> Check {
    let ok = spawnable("git");
    Check {
        name: "git".to_string(),
        status: if ok { Status::Pass } else { Status::Fail },
        detail: if ok {
            t!("doctor.found_on_path").to_string()
        } else {
            t!("doctor.not_found_on_path").to_string()
        },
    }
}

/// One check per configured `Settings::lsp_servers` entry — empty when none
/// are configured (a skip, reported as a single entry, not silently
/// nothing).
#[must_use]
pub fn check_lsp_servers(settings: &Settings) -> Vec<Check> {
    if settings.lsp_servers.is_empty() {
        return vec![Check {
            name: t!("doctor.name_lsp_servers").to_string(),
            status: Status::Skip,
            detail: t!("doctor.lsp_none_configured").to_string(),
        }];
    }
    settings
        .lsp_servers
        .iter()
        .map(|server| {
            let program = server.command.first().map_or("", String::as_str);
            let ok = !program.is_empty() && spawnable(program);
            Check {
                name: t!("doctor.name_lsp_server", id = server.language_id).to_string(),
                status: if ok { Status::Pass } else { Status::Fail },
                detail: if program.is_empty() {
                    t!("doctor.lsp_no_command").to_string()
                } else if ok {
                    t!("doctor.lsp_found", program = program).to_string()
                } else {
                    t!("doctor.lsp_not_found", program = program).to_string()
                },
            }
        })
        .collect()
}

/// Does the active locale's spellcheck dictionary load, via the exact same
/// discovery-and-parse path (`vix_spellcheck::load_for`) the running editor
/// itself uses?
#[must_use]
pub fn check_spellcheck_dictionary(settings: &Settings) -> Check {
    match vix_spellcheck::load_for(&settings.dictionary_path, &settings.locale) {
        Ok(_) => Check {
            name: t!("doctor.name_spellcheck_dictionary").to_string(),
            status: Status::Pass,
            detail: t!("doctor.dictionary_loaded", loc = settings.locale).to_string(),
        },
        Err(e) => Check {
            name: t!("doctor.name_spellcheck_dictionary").to_string(),
            status: Status::Fail,
            detail: t!("doctor.dictionary_failed", loc = settings.locale, error = e).to_string(),
        },
    }
}

/// The pure decision behind [`check_terminal`], factored out so it's
/// testable without touching the real (process-global) environment: fails
/// only when `term` is `None` or `"dumb"`; notes `no_color_set` without
/// failing on it, since that's a deliberate opt-out rather than a
/// misconfiguration.
fn terminal_check(term: Option<&str>, no_color_set: bool) -> Check {
    let ok = term.is_some_and(|t| t != "dumb");
    let mut detail = match term {
        Some("dumb") => t!("doctor.term_dumb").to_string(),
        Some(t) => t!("doctor.term_ok", term = t).to_string(),
        None => t!("doctor.term_unset").to_string(),
    };
    if no_color_set {
        detail.push(' ');
        detail.push_str(&t!("doctor.no_color_note"));
    }
    Check {
        name: t!("doctor.name_terminal").to_string(),
        status: if ok { Status::Pass } else { Status::Fail },
        detail,
    }
}

/// Does the terminal look color-capable? See `terminal_check` for the
/// decision logic; this just supplies it the real environment.
#[must_use]
pub fn check_terminal() -> Check {
    terminal_check(
        std::env::var("TERM").ok().as_deref(),
        std::env::var_os("NO_COLOR").is_some(),
    )
}

/// Run every check against `settings`, in a fixed, stable order.
#[must_use]
pub fn run(settings: &Settings) -> Vec<Check> {
    let mut checks = vec![check_git()];
    checks.extend(check_lsp_servers(settings));
    checks.push(check_spellcheck_dictionary(settings));
    checks.push(check_terminal());
    checks
}

/// Render `checks` as one line per check: `"[PASS] name -- detail"` (or
/// `FAIL`/`SKIP`), plus a one-line pass/fail/skip summary at the end. Used
/// as-is by `vix --doctor` (joined with `\n` for stdout) and by the Help
/// menu's overlay (as the `Vec<String>` lines it already expects).
#[must_use]
pub fn format_report(checks: &[Check]) -> Vec<String> {
    let mut lines: Vec<String> = checks
        .iter()
        .map(|c| {
            let tag = match c.status {
                Status::Pass => "PASS",
                Status::Fail => "FAIL",
                Status::Skip => "SKIP",
            };
            format!("[{tag}] {} -- {}", c.name, c.detail)
        })
        .collect();
    let (pass, fail, skip) = checks
        .iter()
        .fold((0, 0, 0), |(p, f, s), c| match c.status {
            Status::Pass => (p + 1, f, s),
            Status::Fail => (p, f + 1, s),
            Status::Skip => (p, f, s + 1),
        });
    lines.push(String::new());
    lines.push(t!("doctor.summary", pass = pass, fail = fail, skip = skip).to_string());
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_is_found_on_a_dev_machine() {
        // This crate's own test suite runs under `cargo test`, which
        // presupposes a real toolchain and (in this repo, a git checkout)
        // a real `git` on PATH -- a reasonable environmental assumption for
        // a unit test, unlike assuming a *specific* LSP server is installed.
        assert_eq!(check_git().status, Status::Pass);
    }

    #[test]
    fn no_lsp_servers_configured_is_a_skip_not_a_pass_or_fail() {
        let checks = check_lsp_servers(&Settings::default());
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].status, Status::Skip);
    }

    #[test]
    fn an_unspawnable_lsp_server_command_fails() {
        let mut settings = Settings::default();
        settings.lsp_servers.push(vix_settings::LspServer {
            language_id: "made-up".to_string(),
            extensions: vec!["madeup".to_string()],
            command: vec!["vix-doctor-nonexistent-binary-xyz".to_string()],
        });
        let checks = check_lsp_servers(&settings);
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].status, Status::Fail);
    }

    #[test]
    fn an_empty_lsp_server_command_fails_without_spawning_anything() {
        let mut settings = Settings::default();
        settings.lsp_servers.push(vix_settings::LspServer {
            language_id: "broken".to_string(),
            extensions: vec![],
            command: vec![],
        });
        let checks = check_lsp_servers(&settings);
        assert_eq!(checks[0].status, Status::Fail);
        // Never assert on translated text directly (the active locale is
        // process-global and races other tests in the same binary) -- the
        // status plus a non-empty, real detail is what this test actually
        // cares about.
        assert!(!checks[0].detail.is_empty());
    }

    #[test]
    fn terminal_check_fails_on_dumb_or_unset_passes_otherwise() {
        // Exercised via the pure `terminal_check` helper, not `TERM`/
        // `NO_COLOR` themselves: mutating real process environment
        // variables from a test is inherently racy against every other
        // test in the same binary (`set_var`/`remove_var` are `unsafe` as
        // of the 2024 edition specifically because of that), and this
        // crate forbids unsafe code.
        assert_eq!(terminal_check(Some("dumb"), false).status, Status::Fail);
        assert_eq!(terminal_check(None, false).status, Status::Fail);
        assert_eq!(
            terminal_check(Some("xterm-256color"), false).status,
            Status::Pass
        );
        let noted = terminal_check(Some("xterm-256color"), true);
        assert_eq!(noted.status, Status::Pass);
        assert!(noted.detail.contains("NO_COLOR"));
    }

    #[test]
    fn format_report_counts_each_status_once() {
        let checks = vec![
            Check {
                name: "a".to_string(),
                status: Status::Pass,
                detail: String::new(),
            },
            Check {
                name: "b".to_string(),
                status: Status::Fail,
                detail: "nope".to_string(),
            },
            Check {
                name: "c".to_string(),
                status: Status::Skip,
                detail: String::new(),
            },
        ];
        let lines = format_report(&checks);
        // Never hardcode the translated summary text (the active locale is
        // process-global and races other tests in the same binary) --
        // compute it via the same `t!` call `format_report` itself made, so
        // this stays correct under whatever locale is active when it runs.
        let expected_summary = t!("doctor.summary", pass = 1, fail = 1, skip = 1).to_string();
        assert_eq!(lines.last().unwrap(), &expected_summary);
        assert!(lines[0].starts_with("[PASS] a"));
        assert!(lines[1].starts_with("[FAIL] b -- nope"));
        assert!(lines[2].starts_with("[SKIP] c"));
    }
}
