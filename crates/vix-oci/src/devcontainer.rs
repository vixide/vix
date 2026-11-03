//! Dev containers (T718): just enough of `devcontainer.json` and the
//! [`devcontainer` CLI](https://github.com/devcontainers/cli) to reopen a
//! workspace in its dev container.
//!
//! Vix does not implement the Dev Container specification. The CLI builds
//! and starts the container (`devcontainer up`, which runs the Dockerfile,
//! features and lifecycle commands); Vix reads two fields of the
//! configuration as fallbacks and the CLI's result line, then attaches to
//! the running container through `docker exec` like any other container.
//! Nothing here starts a process.
//!
//! The flow, driven by the App: [`parse_config`] the file found via
//! [`CONFIG_FILES`] (for fallbacks only), run [`up_argv`] through
//! [`crate::run`], read the result with [`parse_up`], then pick the in-container
//! path with [`workspace_folder`]. Owning spec: `crates/vix-oci/spec/index.md`
//! and the repo-level `spec/open-container`.
//!
//! ```
//! use vix_oci::devcontainer::{parse_config, strip_jsonc};
//! let config = parse_config("{ // note\n \"name\": \"dev\", }").unwrap();
//! assert_eq!(config.name.as_deref(), Some("dev"));
//! assert_eq!(strip_jsonc("[1, /* x */ 2,]").replace(' ', ""), "[1,2]");
//! ```

use crate::OciError;

/// Where a workspace's dev container configuration lives, in the order the
/// CLI looks (relative to the workspace root).
pub const CONFIG_FILES: [&str; 2] = [".devcontainer/devcontainer.json", ".devcontainer.json"];

/// The fields of `devcontainer.json` Vix uses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Config {
    /// `name`, for messages.
    pub name: Option<String>,
    /// `workspaceFolder`: where the workspace is inside the container.
    pub workspace_folder: Option<String>,
    /// `remoteUser`: who Vix works as inside.
    pub remote_user: Option<String>,
}

/// Parse a `devcontainer.json` (JSON with comments and trailing commas).
///
/// # Errors
/// [`OciError::Parse`] when it is not a JSON object after removing comments.
pub fn parse_config(text: &str) -> Result<Config, OciError> {
    let value: serde_json::Value = serde_json::from_str(&strip_jsonc(text))
        .map_err(|e| OciError::Parse(format!("devcontainer.json: {e}")))?;
    let obj = value
        .as_object()
        .ok_or_else(|| OciError::Parse("devcontainer.json is not an object".into()))?;
    let text = |key: &str| {
        obj.get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .filter(|s| !s.is_empty())
    };
    Ok(Config {
        name: text("name"),
        workspace_folder: text("workspaceFolder"),
        remote_user: text("remoteUser"),
    })
}

/// Turn JSONC into JSON: drop `//` and `/* */` comments and commas before a
/// closing `]` or `}`, leaving strings (and the line structure) alone.
#[must_use]
pub fn strip_jsonc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        // Inside a string everything is copied verbatim, so a `//` in a URL
        // is not mistaken for a comment; a backslash escapes the next char.
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_string = true;
                out.push(c);
            }
            // Line comment: skip to the newline but keep the newline itself.
            ('/', Some('/')) => {
                for n in chars.by_ref() {
                    if n == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            // Block comment. Newlines inside are kept so error line numbers
            // from the JSON parser still point at the right place.
            ('/', Some('*')) => {
                chars.next();
                let mut prev = ' ';
                for n in chars.by_ref() {
                    if n == '\n' {
                        out.push('\n');
                    }
                    if prev == '*' && n == '/' {
                        break;
                    }
                    prev = n;
                }
            }
            _ => out.push(c),
        }
    }
    drop_trailing_commas(&out)
}

/// Remove each `,` followed (after whitespace) by `]` or `}`, outside
/// strings.
fn drop_trailing_commas(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if c == '\\' {
                if let Some(&n) = chars.get(i + 1) {
                    out.push(n);
                    i += 1;
                }
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            out.push(c);
        } else if c == ',' {
            // Look past whitespace: a comma is trailing only if the next
            // significant character closes the container.
            let next = chars[i + 1..].iter().find(|n| !n.is_whitespace());
            if !matches!(next, Some(']' | '}')) {
                out.push(c);
            }
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

/// The arguments for `devcontainer up` on `workspace` (a local directory),
/// with `docker_path` naming the engine when it is not `docker` (Podman).
///
/// # Errors
/// [`OciError::InvalidArgument`] for an empty or option-shaped path.
pub fn up_argv(workspace: &str, docker_path: Option<&str>) -> Result<Vec<String>, OciError> {
    crate::argv::check_path(workspace)?;
    let mut args = vec![
        "up".to_string(),
        "--workspace-folder".to_string(),
        workspace.to_string(),
    ];
    if let Some(p) = docker_path {
        crate::argv::check_path(p)?;
        args.push("--docker-path".into());
        args.push(p.to_string());
    }
    Ok(args)
}

/// What `devcontainer up` reported.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Up {
    /// The running container's id.
    pub container_id: String,
    /// The user the CLI says to work as, if it said.
    pub remote_user: Option<String>,
    /// The workspace inside the container, if it said.
    pub remote_workspace_folder: Option<String>,
}

/// Read the result line `devcontainer up` prints last on stdout
/// (`{"outcome":"success","containerId":…}`).
///
/// # Errors
/// [`OciError::Parse`] when there is no result line or no container id;
/// [`OciError::Failed`] with the CLI's own message for `"outcome":"error"`.
pub fn parse_up(stdout: &str) -> Result<Up, OciError> {
    let value = stdout
        .lines()
        .rev()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
        // Scan from the end: earlier lines are build log output, possibly
        // JSON too, and only the final result object has an `outcome`.
        .find(|v| v.get("outcome").is_some())
        .ok_or_else(|| OciError::Parse("devcontainer up printed no result".into()))?;
    let text = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .filter(|s| !s.is_empty())
    };
    if text("outcome").as_deref() != Some("success") {
        let message = [text("message"), text("description")]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(": ");
        return Err(OciError::Failed {
            code: None,
            stderr: message,
        });
    }
    let container_id = text("containerId")
        .ok_or_else(|| OciError::Parse("devcontainer up reported no container".into()))?;
    Ok(Up {
        container_id,
        remote_user: text("remoteUser"),
        remote_workspace_folder: text("remoteWorkspaceFolder"),
    })
}

/// Where the workspace is inside the container: what `up` reported, else
/// the configuration's `workspaceFolder` (with
/// `${localWorkspaceFolderBasename}` filled in), else the specification's
/// default `/workspaces/<basename>`.
#[must_use]
pub fn workspace_folder(up: &Up, config: &Config, basename: &str) -> String {
    // Precedence: what the CLI actually mounted beats the config's intent,
    // which beats the spec default.
    if let Some(f) = &up.remote_workspace_folder {
        return f.clone();
    }
    if let Some(f) = &config.workspace_folder {
        return f.replace("${localWorkspaceFolderBasename}", basename);
    }
    format!("/workspaces/{basename}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonc_comments_and_trailing_commas_are_dropped() {
        let text = r#"{
  // a comment with "quotes"
  "name": "Rust // not a comment",
  /* block
     comment */
  "workspaceFolder": "/work/${localWorkspaceFolderBasename}",
  "remoteUser": "vscode",
  "features": { "a": {}, },
  "forwardPorts": [3000, 8080,],
}"#;
        let c = parse_config(text).unwrap();
        assert_eq!(c.name.as_deref(), Some("Rust // not a comment"));
        assert_eq!(
            c.workspace_folder.as_deref(),
            Some("/work/${localWorkspaceFolderBasename}")
        );
        assert_eq!(c.remote_user.as_deref(), Some("vscode"));
        assert!(parse_config("[1]").is_err());
        assert!(parse_config("{").is_err());
        assert_eq!(strip_jsonc(r#"{"a":"x\"//,}"}"#), r#"{"a":"x\"//,}"}"#);
    }

    #[test]
    fn up_results_parse() {
        let out = "some log\n{\"outcome\":\"success\",\"containerId\":\"abc123\",\"remoteUser\":\"node\",\"remoteWorkspaceFolder\":\"/workspaces/app\"}\n";
        let up = parse_up(out).unwrap();
        assert_eq!(up.container_id, "abc123");
        assert_eq!(up.remote_user.as_deref(), Some("node"));
        let err =
            parse_up("{\"outcome\":\"error\",\"message\":\"boom\",\"description\":\"no docker\"}")
                .unwrap_err();
        assert!(err.to_string().contains("boom: no docker"), "{err}");
        assert!(parse_up("nothing").is_err());
        assert!(parse_up("{\"outcome\":\"success\"}").is_err());
    }

    #[test]
    fn the_workspace_folder_falls_back_in_order() {
        let up = parse_up("{\"outcome\":\"success\",\"containerId\":\"c\"}").unwrap();
        let none = Config::default();
        assert_eq!(workspace_folder(&up, &none, "app"), "/workspaces/app");
        let cfg = Config {
            workspace_folder: Some("/w/${localWorkspaceFolderBasename}".into()),
            ..Config::default()
        };
        assert_eq!(workspace_folder(&up, &cfg, "app"), "/w/app");
        let up = parse_up(
            "{\"outcome\":\"success\",\"containerId\":\"c\",\"remoteWorkspaceFolder\":\"/x\"}",
        )
        .unwrap();
        assert_eq!(workspace_folder(&up, &cfg, "app"), "/x");
    }

    #[test]
    fn up_arguments_are_checked() {
        assert_eq!(
            up_argv("/home/me/app", Some("/usr/bin/podman")).unwrap(),
            [
                "up",
                "--workspace-folder",
                "/home/me/app",
                "--docker-path",
                "/usr/bin/podman"
            ]
        );
        assert!(up_argv("-x", None).is_err());
    }
}
