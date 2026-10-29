//! The workspace boundary: resolving paths inside the root, refusing secret
//! files, reading bounded text, and finding SQLite databases.
//!
//! Every file the server touches goes through [`resolve`], which normalizes
//! the request lexically, canonicalizes it, and checks the canonical path
//! still lies under the canonical root, so neither `..` nor a symbolic link
//! can lead outside the workspace.

use std::fs;
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};

/// Largest file, in bytes, the server reads (2 MiB).
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// Deepest directory level the SQLite search descends to.
pub const MAX_DB_DEPTH: usize = 8;

/// Most SQLite databases the search returns.
pub const MAX_DATABASES: usize = 16;

/// The 16-byte header every SQLite 3 database file starts with.
const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";

/// Directory names the SQLite search never enters (hidden directories are
/// skipped as well).
const SKIP_DIRS: &[&str] = &["target", "node_modules"];

/// File extensions a SQLite database candidate may carry.
const DB_EXTENSIONS: &[&str] = &["db", "db3", "sqlite", "sqlite3"];

/// Resolve `request` (workspace-relative, or absolute) to a canonical path of
/// an existing regular file inside `root`.
///
/// `root` must already be canonical. The request is first normalized
/// lexically (no `..` may climb above the root), then canonicalized; the
/// canonical result must still start with `root`.
///
/// # Errors
///
/// Returns a message for a path that is empty, leaves the root, does not
/// exist, is not a regular file, or names a secret (see [`is_secret`]).
pub fn resolve(root: &Path, request: &str) -> Result<PathBuf, String> {
    if request.trim().is_empty() {
        return Err("path is empty".into());
    }
    let lexical = lexical_join(root, Path::new(request))
        .ok_or_else(|| format!("{request}: outside the workspace root"))?;
    let canonical = lexical
        .canonicalize()
        .map_err(|_| format!("{request}: no such file in the workspace"))?;
    if !canonical.starts_with(root) || canonical == root {
        return Err(format!("{request}: outside the workspace root"));
    }
    if !canonical.is_file() {
        return Err(format!("{request}: not a regular file"));
    }
    let rel = canonical.strip_prefix(root).unwrap_or(&canonical);
    if is_secret(rel) {
        return Err(format!("{request}: refused, the file may hold secrets"));
    }
    Ok(canonical)
}

/// Join `request` onto `root` lexically: `.` is dropped, `..` pops a
/// component and may not climb above `root`; an absolute request must lie
/// under `root` once normalized. `None` when the result would leave `root`.
fn lexical_join(root: &Path, request: &Path) -> Option<PathBuf> {
    let base = if request.is_absolute() {
        PathBuf::new()
    } else {
        root.to_path_buf()
    };
    let floor = if request.is_absolute() {
        0
    } else {
        root.components().count()
    };
    let mut out = base;
    for component in request.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if out.components().count() <= floor || !out.pop() {
                    return None;
                }
            }
            Component::RootDir | Component::Prefix(_) => out.push(component.as_os_str()),
        }
    }
    out.starts_with(root).then_some(out)
}

/// Whether a workspace-relative path names a file that may hold secrets:
/// `.env` and `.env.*`, private keys and certificates, `.netrc`, `.pgpass`,
/// `credentials*`, and anything under `.git/` or `.ssh/`.
#[must_use]
pub fn is_secret(rel: &Path) -> bool {
    let in_secret_dir = rel
        .components()
        .any(|c| matches!(c, Component::Normal(part) if part == ".git" || part == ".ssh"));
    if in_secret_dir {
        return true;
    }
    let Some(name) = rel.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    let extension = Path::new(&lower)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    lower == ".env"
        || lower.starts_with(".env.")
        || matches!(extension, "pem" | "key" | "p12" | "pfx")
        || ["id_rsa", "id_ecdsa", "id_ed25519", "id_dsa", "credentials"]
            .iter()
            .any(|prefix| lower.starts_with(prefix))
        || lower == ".netrc"
        || lower == ".pgpass"
}

/// `abs` relative to `root` with `/` separators, or `abs` itself when it is
/// not under `root`.
#[must_use]
pub fn relative(root: &Path, abs: &Path) -> String {
    let rel = abs.strip_prefix(root).unwrap_or(abs);
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Read the text of the file `request` names inside `root`, returning its
/// workspace-relative path and contents.
///
/// # Errors
///
/// Everything [`resolve`] refuses, plus a file over [`MAX_FILE_BYTES`], a
/// binary file (one holding a NUL byte), or invalid UTF-8.
pub fn read_text(root: &Path, request: &str) -> Result<(String, String), String> {
    let path = resolve(root, request)?;
    let rel = relative(root, &path);
    let size = fs::metadata(&path)
        .map_err(|e| format!("{rel}: {e}"))?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(format!(
            "{rel}: {size} bytes is over the {MAX_FILE_BYTES}-byte limit"
        ));
    }
    let bytes = fs::read(&path).map_err(|e| format!("{rel}: {e}"))?;
    if bytes.contains(&0) {
        return Err(format!("{rel}: binary file"));
    }
    let text = String::from_utf8(bytes).map_err(|_| format!("{rel}: not UTF-8 text"))?;
    Ok((rel, text))
}

/// Every SQLite database inside `root`, sorted: files with a database
/// extension whose first 16 bytes are the SQLite header. Walks at most
/// [`MAX_DB_DEPTH`] levels, skips hidden directories, `target` and
/// `node_modules`, never follows symbolic links, refuses secret paths, and
/// stops after [`MAX_DATABASES`].
#[must_use]
pub fn find_sqlite(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    walk(root, root, 0, &mut found);
    found.sort();
    found.truncate(MAX_DATABASES);
    found
}

/// Depth-first helper for [`find_sqlite`].
fn walk(root: &Path, dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    if depth > MAX_DB_DEPTH || found.len() >= MAX_DATABASES {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        if found.len() >= MAX_DATABASES {
            return;
        }
        // `DirEntry::file_type` does not follow symbolic links, so a link is
        // neither a directory nor a file here and is skipped.
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let path = entry.path();
        if kind.is_dir() {
            if !name.starts_with('.') && !SKIP_DIRS.contains(&name.as_ref()) {
                walk(root, &path, depth + 1, found);
            }
        } else if kind.is_file() && is_sqlite_candidate(root, &path) {
            found.push(path);
        }
    }
}

/// Whether `path` has a database extension, is not a secret, and starts with
/// the SQLite header.
fn is_sqlite_candidate(root: &Path, path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if !DB_EXTENSIONS.contains(&extension.as_str()) {
        return false;
    }
    if is_secret(path.strip_prefix(root).unwrap_or(path)) {
        return false;
    }
    let mut header = [0u8; 16];
    fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut header))
        .is_ok_and(|()| &header == SQLITE_HEADER)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh, canonical scratch directory for one test.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("vix-mcp-ws-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    #[test]
    fn resolves_relative_and_absolute_paths_inside_the_root() {
        let root = scratch("inside");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();
        assert_eq!(resolve(&root, "src/a.rs").unwrap(), root.join("src/a.rs"));
        assert_eq!(
            resolve(&root, "./src/../src/a.rs").unwrap(),
            root.join("src/a.rs")
        );
        let abs = root.join("src/a.rs");
        assert_eq!(resolve(&root, abs.to_str().unwrap()).unwrap(), abs);
    }

    #[test]
    fn refuses_paths_that_leave_the_root() {
        let root = scratch("outside");
        fs::write(root.join("a.txt"), "x").unwrap();
        assert!(resolve(&root, "../a.txt").is_err());
        assert!(resolve(&root, "/etc/hosts").is_err());
        assert!(resolve(&root, "").is_err());
        assert!(resolve(&root, ".").is_err());
        assert!(resolve(&root, "missing.txt").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_symlink_that_points_outside() {
        let root = scratch("symlink");
        let outside = scratch("symlink-target");
        fs::write(outside.join("secret.txt"), "x").unwrap();
        std::os::unix::fs::symlink(outside.join("secret.txt"), root.join("link.txt")).unwrap();
        assert!(resolve(&root, "link.txt").is_err());
    }

    #[test]
    fn secret_files_are_recognized() {
        for name in [
            ".env",
            ".env.local",
            "config/server.pem",
            "tls.KEY",
            "id_rsa",
            "id_ed25519.pub",
            ".netrc",
            ".pgpass",
            "credentials.json",
            ".git/config",
            "home/.ssh/known_hosts",
        ] {
            assert!(is_secret(Path::new(name)), "{name}");
        }
        for name in ["src/main.rs", "README.md", "environment.md", "keys.rs"] {
            assert!(!is_secret(Path::new(name)), "{name}");
        }
    }

    #[test]
    fn read_text_refuses_secrets_binaries_and_large_files() {
        let root = scratch("read");
        fs::write(root.join(".env"), "TOKEN=x").unwrap();
        fs::write(root.join("bin.dat"), [0u8, 1, 2]).unwrap();
        fs::write(root.join("ok.md"), "# Hi\n").unwrap();
        assert!(read_text(&root, ".env").is_err());
        assert!(read_text(&root, "bin.dat").is_err());
        assert_eq!(
            read_text(&root, "ok.md").unwrap(),
            ("ok.md".to_string(), "# Hi\n".to_string())
        );
        let big = vec![b'a'; usize::try_from(MAX_FILE_BYTES).unwrap() + 1];
        fs::write(root.join("big.txt"), big).unwrap();
        assert!(read_text(&root, "big.txt").is_err());
    }

    #[test]
    fn find_sqlite_checks_the_header_and_skips_hidden_dirs() {
        let root = scratch("sqlite");
        let mut header = SQLITE_HEADER.to_vec();
        header.extend_from_slice(&[0; 84]);
        fs::create_dir_all(root.join("data")).unwrap();
        fs::create_dir_all(root.join(".hidden")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("data/app.db"), &header).unwrap();
        fs::write(root.join("data/notes.sqlite3"), &header).unwrap();
        fs::write(root.join("data/fake.db"), b"not a database at all").unwrap();
        fs::write(root.join("data/app.txt"), &header).unwrap();
        fs::write(root.join(".hidden/x.db"), &header).unwrap();
        fs::write(root.join("target/y.db"), &header).unwrap();
        assert_eq!(
            find_sqlite(&root),
            vec![root.join("data/app.db"), root.join("data/notes.sqlite3")]
        );
    }

    #[test]
    fn relative_uses_forward_slashes() {
        let root = Path::new("/w");
        assert_eq!(relative(root, Path::new("/w/a/b.rs")), "a/b.rs");
    }
}
