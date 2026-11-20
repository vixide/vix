//! The login shell's `PATH`, for a desktop GUI started outside a terminal.
//!
//! A GUI launched from the macOS Dock or Finder, or from a Linux desktop
//! launcher, inherits the session's minimal `PATH` (on macOS
//! `/usr/bin:/bin:/usr/sbin:/sbin`), not the one the user's shell profile
//! builds, so `~/.cargo/bin/rust-analyzer`, Homebrew's tools and the like
//! are not found. [`login_shell_path`] asks the user's login shell for its
//! `PATH`; [`merge_paths`] and [`adds_entries`] are the pure parts. The GUIs
//! re-exec themselves with the result before anything else starts
//! ([`reexec_with_login_shell_path`]). Owning spec:
//! `crates/vix-project-env/spec/index.md` ("Login shell PATH").

/// Marks the `PATH` in the shell's output, so lines a profile prints around
/// it (a greeting, a `motd`) are ignored.
pub const MARKER: &str = "__VIX_LOGIN_PATH__";

/// Set (to `1`) in the environment of a GUI that has already re-executed
/// with the login shell's `PATH`, so it never tries again.
pub const DONE_VAR: &str = "VIX_LOGIN_SHELL_PATH";

/// The script the login shell runs: print `PATH` between two [`MARKER`]s.
/// It is the same in sh, bash, zsh, fish and nushell's `-c`.
#[must_use]
pub fn script() -> String {
    format!("printf '%s%s%s' {MARKER} \"$PATH\" {MARKER}")
}

/// The `PATH` between the first two [`MARKER`]s in `output`, if it is
/// there and not empty.
#[must_use]
pub fn parse_marked(output: &str) -> Option<&str> {
    let start = output.find(MARKER)? + MARKER.len();
    let len = output[start..].find(MARKER)?;
    let path = output[start..start + len].trim();
    (!path.is_empty()).then_some(path)
}

/// `login`'s entries, then each of `current`'s not already among them, joined
/// with `:`. Empty entries are dropped and no entry appears twice.
#[must_use]
pub fn merge_paths(login: &str, current: Option<&str>) -> String {
    let mut out: Vec<&str> = Vec::new();
    for entry in login.split(':').chain(current.unwrap_or("").split(':')) {
        if !entry.is_empty() && !out.contains(&entry) {
            out.push(entry);
        }
    }
    out.join(":")
}

/// Whether `login` has an entry `current` lacks: only then is re-executing
/// worth it (a GUI started from a terminal already has them all).
#[must_use]
pub fn adds_entries(login: &str, current: Option<&str>) -> bool {
    let have: Vec<&str> = current.unwrap_or("").split(':').collect();
    login
        .split(':')
        .any(|entry| !entry.is_empty() && !have.contains(&entry))
}

/// Ask `shell` (`$SHELL`) for its login-shell `PATH`: it runs as
/// `shell -l -i -c <script>` with stdin closed, killed after `timeout`.
/// `None` if it cannot start, fails, times out or prints no marked `PATH`.
/// Always `None` without the `process` feature.
#[must_use]
pub fn login_shell_path(shell: &str, timeout: std::time::Duration) -> Option<String> {
    #[cfg(feature = "process")]
    {
        let home = std::env::var_os("HOME").map_or_else(|| "/".into(), std::path::PathBuf::from);
        let out = crate::run::run_tool(shell, &["-l", "-i", "-c", &script()], &home, timeout, &[])?;
        parse_marked(&out).map(str::to_string)
    }
    #[cfg(not(feature = "process"))]
    {
        let _ = (shell, timeout);
        None
    }
}

/// For a desktop GUI's `main`, first thing: on Unix, when standard input is
/// not a terminal (so the GUI was not started from a shell), [`DONE_VAR`] is
/// unset and `$SHELL`'s login `PATH` ([`login_shell_path`], 3 seconds at
/// most) has entries this process's `PATH` lacks, replace this process with
/// itself, same arguments, with the [merged](merge_paths) `PATH` and
/// [`DONE_VAR`] set. Returns (doing nothing) in every other case, and if the
/// re-exec fails. A no-op on Windows, whose GUI apps get the user's `PATH`
/// from the registry, and without the `process` feature.
pub fn reexec_with_login_shell_path() {
    #[cfg(all(unix, feature = "process"))]
    {
        use std::io::IsTerminal as _;
        if std::env::var_os(DONE_VAR).is_some() || std::io::stdin().is_terminal() {
            return;
        }
        let Some(shell) = std::env::var("SHELL").ok().filter(|s| !s.is_empty()) else {
            return;
        };
        let Some(login) = login_shell_path(&shell, std::time::Duration::from_secs(3)) else {
            return;
        };
        let current = std::env::var("PATH").ok();
        if !adds_entries(&login, current.as_deref()) {
            return;
        }
        crate::run::reexec_with_path(&merge_paths(&login, current.as_deref()), DONE_VAR);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_marked_path_is_found_among_profile_noise() {
        let out = format!("Welcome!\n{MARKER}/opt/homebrew/bin:/usr/bin{MARKER}\nbye");
        assert_eq!(parse_marked(&out), Some("/opt/homebrew/bin:/usr/bin"));
        assert_eq!(parse_marked("no markers"), None);
        assert_eq!(parse_marked(&format!("{MARKER}only one")), None);
        assert_eq!(parse_marked(&format!("{MARKER}  {MARKER}")), None);
    }

    #[test]
    fn merging_puts_login_entries_first_without_duplicates() {
        assert_eq!(
            merge_paths("/a:/b::/usr/bin", Some("/usr/bin:/bin:/a")),
            "/a:/b:/usr/bin:/bin"
        );
        assert_eq!(merge_paths("/a", None), "/a");
    }

    #[test]
    fn only_a_path_with_new_entries_is_worth_a_reexec() {
        let dock = Some("/usr/bin:/bin:/usr/sbin:/sbin");
        assert!(adds_entries("/Users/me/.cargo/bin:/usr/bin:/bin", dock));
        // Started from a terminal: the same entries, maybe reordered.
        assert!(!adds_entries("/bin:/usr/bin", dock));
        assert!(!adds_entries("", dock));
    }

    #[cfg(all(unix, feature = "process"))]
    #[test]
    fn a_real_sh_reports_its_path() {
        let path = login_shell_path("/bin/sh", std::time::Duration::from_secs(10));
        assert!(path.is_some_and(|p| !p.is_empty()));
        assert_eq!(
            login_shell_path("/nonexistent/shell", std::time::Duration::from_secs(1)),
            None
        );
    }
}
