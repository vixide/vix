//! `{{$aadToken}}`: an Azure AD access token from the Azure CLI.
//!
//! The token is fetched by running `az account get-access-token` with an `argv`
//! array (never a shell, so a crafted `.http` file cannot inject a command).
//! It is held in memory only: never cached, logged or written to disk, and
//! errors deliberately carry nothing the CLI printed.
//!
//! Called by [`crate::rest`] when expanding `{{$aadToken ...}}` in a request.
//! Owning spec: `crates/vix-http-client/spec/index.md`.

use std::process::{Command, Stdio};

/// Why a token could not be acquired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AadError {
    /// The `az` program is not installed (or not on `PATH`).
    CliMissing,
    /// An argument is not a plausible resource or tenant id.
    BadArgument,
    /// `az` ran but produced no token (not signed in, unknown resource).
    Failed,
}

impl std::fmt::Display for AadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::CliMissing => "the Azure CLI (az) is not installed",
            Self::BadArgument => "bad $aadToken argument",
            Self::Failed => "the Azure CLI could not get a token",
        })
    }
}

impl std::error::Error for AadError {}

/// Used when `{{$aadToken}}` names no resource.
pub const DEFAULT_RESOURCE: &str = "https://management.azure.com/";

/// The Azure CLI's program name (Windows installs a `.cmd` shim).
const AZ: &str = if cfg!(windows) { "az.cmd" } else { "az" };

/// Acquire a token. `args` are the words after `$aadToken`: an optional
/// resource (a URL or app id), an optional `tenantid:<id>`; the VS Code
/// `new` keyword is accepted and ignored (nothing is cached).
///
/// # Errors
/// An [`AadError`].
pub fn token(args: &[&str]) -> Result<String, AadError> {
    token_with(AZ, args)
}

/// [`token`] with an explicit program, so tests need not have `az`.
///
/// # Errors
/// An [`AadError`].
pub fn token_with(program: &str, args: &[&str]) -> Result<String, AadError> {
    let command_line = build_args(args)?;
    let out = Command::new(program)
        .args(&command_line)
        .stdin(Stdio::null())
        // Discarded on purpose: `az` can print account details or hints that
        // must never leak into an error shown in the UI.
        .stderr(Stdio::null())
        .output()
        .map_err(|e| {
            // Distinguishing "not installed" lets the UI suggest installing
            // `az`; every other spawn failure collapses to `Failed`.
            if e.kind() == std::io::ErrorKind::NotFound {
                AadError::CliMissing
            } else {
                AadError::Failed
            }
        })?;
    if !out.status.success() {
        return Err(AadError::Failed);
    }
    // `--output tsv --query accessToken` prints the bare token plus a newline.
    let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if token.is_empty() {
        Err(AadError::Failed)
    } else {
        Ok(token)
    }
}

/// The `az` argument vector for `args`. Each user-supplied word is validated
/// by [`checked`] so it can only ever be a value, never an extra option.
fn build_args(args: &[&str]) -> Result<Vec<String>, AadError> {
    let mut resource = DEFAULT_RESOURCE.to_string();
    let mut tenant = None;
    for a in args {
        if *a == "new" {
            continue;
        }
        if let Some(t) = a.strip_prefix("tenantid:") {
            tenant = Some(checked(t)?);
        } else {
            resource = checked(a)?;
        }
    }
    let mut command_line: Vec<String> = ["account", "get-access-token", "--resource"]
        .map(String::from)
        .into();
    command_line.push(resource);
    if let Some(t) = tenant {
        command_line.push("--tenant".into());
        command_line.push(t);
    }
    command_line.extend(["--query", "accessToken", "--output", "tsv"].map(String::from));
    Ok(command_line)
}

/// `s` if it is a plain id/URL: no leading `-` (an option), no whitespace.
/// The allowed set (`alphanumerics` and `-._:/@`) covers URLs, GUIDs and
/// `api://` ids but rejects shell metacharacters as defence in depth, even
/// though no shell is involved. checked(s: &str) -> Result<String, AadError> {
    let ok = !s.is_empty()
        && !s.starts_with('-')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-._:/@".contains(c));
    if ok {
        Ok(s.to_string())
    } else {
        Err(AadError::BadArgument)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_an_argv_not_a_shell_line() {
        let command_line =
            build_args(&["new", "https://vault.azure.net", "tenantid:abc-123"]).unwrap();
        assert_eq!(
            command_line,
            [
                "account",
                "get-access-token",
                "--resource",
                "https://vault.azure.net",
                "--tenant",
                "abc-123",
                "--query",
                "accessToken",
                "--output",
                "tsv"
            ]
        );
        assert_eq!(build_args(&[]).unwrap()[3], DEFAULT_RESOURCE);
    }

    #[test]
    fn refuses_option_like_or_shell_looking_arguments() {
        for bad in ["--help", "a b", "x;rm", "$(id)", "tenantid:-x", ""] {
            assert_eq!(build_args(&[bad]), Err(AadError::BadArgument), "{bad}");
        }
    }

    #[test]
    fn a_missing_cli_is_reported_as_such() {
        assert_eq!(
            token_with("vix-no-such-az-program", &[]),
            Err(AadError::CliMissing)
        );
    }

    #[cfg(unix)]
    #[test]
    fn failure_and_success_never_echo_cli_output() {
        // `false` exits non-zero; `echo` stands in for a signed-in CLI.
        assert_eq!(token_with("false", &[]), Err(AadError::Failed));
        assert_eq!(
            token_with("echo", &[]).unwrap().split(' ').next(),
            Some("account")
        );
    }
}
