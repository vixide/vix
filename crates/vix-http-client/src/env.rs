//! REST Client environments: `.vix/http-env.json`.
//!
//! ```json
//! {
//!   "$shared": { "host": "https://dev.example.com" },
//!   "dev":  { "token": "{{$processEnv DEV_TOKEN}}" },
//!   "prod": { "host": "https://example.com", "token": "{{$processEnv PROD_TOKEN}}" }
//! }
//! ```
//!
//! `$shared` variables apply under every environment. The active environment's
//! variables override the `.http` file's own `@name = value` definitions,
//! which override `$shared` (see [`Environments::variables`]). A secret should
//! not be written into this file: reference it with `{{$processEnv NAME}}`,
//! and keep the file out of version control.

use std::path::{Path, PathBuf};

/// The environments file, relative to the workspace root.
pub const ENV_FILE: &str = ".vix/http-env.json";

/// The remembered active environment's name, relative to the workspace root.
pub const ACTIVE_FILE: &str = ".vix/http-env.active";

/// Why the environments file could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EnvError {
    /// The file could not be read.
    Read(String),
    /// The file is not valid JSON.
    Json(String),
    /// The top level, or an environment, is not a JSON object.
    NotAnObject(String),
}

impl std::fmt::Display for EnvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read(e) => write!(f, "cannot read {ENV_FILE}: {e}"),
            Self::Json(e) => write!(f, "{ENV_FILE} is not valid JSON: {e}"),
            Self::NotAnObject(n) => write!(f, "`{n}` in {ENV_FILE} must be an object"),
        }
    }
}

impl std::error::Error for EnvError {}

/// A variable list, `(name, value)` in order.
pub type Vars = Vec<(String, String)>;

/// The parsed environments file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Environments {
    /// The `$shared` variables.
    pub shared: Vars,
    /// Named environments, sorted by name.
    pub named: Vec<(String, Vars)>,
}

/// Parse the text of an environments file. Scalar values (string, number,
/// boolean) become variables; nested values are ignored.
///
/// # Errors
/// An [`EnvError`] for invalid JSON or a non-object section.
pub fn parse(text: &str) -> Result<Environments, EnvError> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| EnvError::Json(e.to_string()))?;
    let serde_json::Value::Object(top) = value else {
        return Err(EnvError::NotAnObject("$".into()));
    };
    let mut out = Environments::default();
    for (name, section) in top {
        let serde_json::Value::Object(map) = section else {
            return Err(EnvError::NotAnObject(name));
        };
        let vars: Vars = map
            .into_iter()
            .filter_map(|(k, v)| match v {
                serde_json::Value::String(s) => Some((k, s)),
                serde_json::Value::Number(n) => Some((k, n.to_string())),
                serde_json::Value::Bool(b) => Some((k, b.to_string())),
                _ => None,
            })
            .collect();
        if name == "$shared" {
            out.shared = vars;
        } else {
            out.named.push((name, vars));
        }
    }
    out.named.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

impl Environments {
    /// The environment names, sorted.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.named.iter().map(|(n, _)| n.as_str()).collect()
    }

    /// The variable list for `file` (the `.http` file's own variables) under
    /// the `active` environment, ordered so that the *last* definition wins
    /// (as [`crate::rest::Context::vars`] resolves): `$shared`, then the
    /// file, then the environment. An unknown `active` adds nothing.
    #[must_use]
    pub fn variables(&self, active: Option<&str>, file: &[(String, String)]) -> Vars {
        let mut out = self.shared.clone();
        out.extend_from_slice(file);
        if let Some((_, vars)) = active.and_then(|a| self.named.iter().find(|(n, _)| n == a)) {
            out.extend(vars.iter().cloned());
        }
        out
    }
}

/// Read `<root>/.vix/http-env.json`; `Ok(None)` when it does not exist.
///
/// # Errors
/// An [`EnvError`] when it exists but is unreadable or malformed.
pub fn load(root: &Path) -> Result<Option<Environments>, EnvError> {
    let path: PathBuf = root.join(ENV_FILE);
    match vix_authority::current().read_to_string(&path) {
        Ok(text) => parse(&text).map(Some),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(EnvError::Read(e.to_string())),
    }
}

/// The workspace's remembered active environment, if any.
#[must_use]
pub fn read_active(root: &Path) -> Option<String> {
    let text = vix_authority::current()
        .read_to_string(&root.join(ACTIVE_FILE))
        .ok()?;
    let name = text.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// Remember `name` (or forget the choice with `None`) for this workspace.
///
/// # Errors
/// The I/O error from writing the file.
pub fn write_active(root: &Path, name: Option<&str>) -> std::io::Result<()> {
    let auth = vix_authority::current();
    let path = root.join(ACTIVE_FILE);
    match name {
        Some(n) => {
            if let Some(dir) = path.parent() {
                auth.create_dir_all(dir)?;
            }
            auth.write(&path, n.as_bytes())
        }
        None => match auth.remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "$shared": {"host": "https://shared", "ver": 2},
        "prod": {"host": "https://prod", "nested": {"x": 1}},
        "dev": {"token": "{{$processEnv T}}", "debug": true}
    }"#;

    #[test]
    fn parses_shared_and_sorted_named_sections() {
        let e = parse(SAMPLE).unwrap();
        assert_eq!(e.names(), ["dev", "prod"]);
        assert_eq!(e.shared[1], ("ver".into(), "2".into()));
        // Nested values are skipped, scalars kept.
        assert_eq!(e.named[1].1, vec![("host".into(), "https://prod".into())]);
    }

    #[test]
    fn precedence_is_environment_over_file_over_shared() {
        let e = parse(SAMPLE).unwrap();
        let file = vec![("host".to_string(), "https://file".to_string())];
        let last = |vars: &Vars, n: &str| {
            vars.iter()
                .rev()
                .find(|(k, _)| k == n)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(
            last(&e.variables(None, &file), "host").unwrap(),
            "https://file"
        );
        assert_eq!(
            last(&e.variables(Some("prod"), &file), "host").unwrap(),
            "https://prod"
        );
        assert_eq!(
            last(&e.variables(Some("nope"), &[]), "host").unwrap(),
            "https://shared"
        );
    }

    #[test]
    fn rejects_malformed_files() {
        assert!(matches!(parse("{"), Err(EnvError::Json(_))));
        assert!(matches!(parse("[]"), Err(EnvError::NotAnObject(_))));
        assert!(matches!(parse(r#"{"dev": 1}"#), Err(EnvError::NotAnObject(n)) if n == "dev"));
    }

    #[test]
    fn active_choice_round_trips_per_workspace() {
        let dir = std::env::temp_dir().join(format!("vix-env-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(read_active(&dir), None);
        assert_eq!(load(&dir).unwrap(), None);
        write_active(&dir, Some("dev")).unwrap();
        assert_eq!(read_active(&dir).as_deref(), Some("dev"));
        write_active(&dir, None).unwrap();
        assert_eq!(read_active(&dir), None);
        write_active(&dir, None).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
