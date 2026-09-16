//! API-key resolution for HTTP AI providers — the same two-step waterfall
//! `vix-db`'s `secret` module uses for database passwords (see
//! `crates/vix-db/src/secret.rs`), generalized from one saved connection to
//! one provider name: a configured command's stdout first, then the OS
//! keyring. There is no third, interactive-prompt step here (unlike a DB
//! connection, an AI request has no natural place to pause and ask) — a
//! request that needs a key and finds none simply fails with a clear reason
//! instead.

use std::process::{Command, Stdio};

/// The keyring service name under which Vix stores AI provider API keys.
const SERVICE: &str = "vix-ai";

/// Run `program` with `args`, returning its trimmed stdout on a zero exit,
/// or `None` on any failure (missing program, non-zero exit, unreadable
/// output).
fn run(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

/// Read `provider`'s key from the OS keyring, or `None` when it is absent or
/// the platform has no supported keyring. On macOS this uses the native
/// Security framework (no secret on the process argument list); on Linux it
/// runs `secret-tool lookup`.
#[must_use]
pub fn keyring_get(provider: &str) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        keyring::Entry::new(SERVICE, provider)
            .ok()?
            .get_password()
            .ok()
            .filter(|key| !key.is_empty())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        run(
            "secret-tool",
            &["lookup", "service", SERVICE, "account", provider],
        )
    }
    #[cfg(not(unix))]
    {
        let _ = provider;
        None
    }
}

/// Resolve `provider`'s API key non-interactively: try `api_key_command`
/// (any command that prints the key to stdout — `pass`, `op read`, a
/// wrapper script; run via `sh -c`, same as `vix-db`'s `password_command`),
/// then the OS keyring. `None` means neither source produced one.
#[must_use]
pub fn resolve(provider: &str, api_key_command: &str) -> Option<String> {
    let command = api_key_command.trim();
    if !command.is_empty()
        && let Some(key) = run("sh", &["-c", command]).filter(|key| !key.is_empty())
    {
        return Some(key);
    }
    keyring_get(provider)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_skips_an_empty_command_and_falls_through_to_the_keyring() {
        // No command and (very likely) no keyring entry for this name in a
        // test environment.
        assert_eq!(resolve("vix-ai-core-test-provider-no-such-entry", ""), None);
    }

    #[test]
    fn api_key_command_stdout_is_used_and_trimmed() {
        assert_eq!(
            resolve("anthropic", "printf '  sk-ant-test\\n'"),
            Some("sk-ant-test".to_string())
        );
    }

    #[test]
    fn a_failing_command_falls_through_rather_than_short_circuiting() {
        // `false` always exits non-zero and prints nothing; resolve should
        // fall through to the keyring lookup (which also finds nothing here)
        // rather than treating a non-zero exit as the key itself.
        assert_eq!(
            resolve("vix-ai-core-test-provider-no-such-entry", "false"),
            None
        );
    }
}
