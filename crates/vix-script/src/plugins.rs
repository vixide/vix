//! Rhai plugins (T637, § Plugins): git repositories of scripts installed
//! under the global scripts directory, pinned in `plugins.lock`, and loaded
//! only while trusted.
//!
//! Everything here except [`install`]/[`update`] works without starting a
//! process: reading and writing the lock, the trust digest, enabling,
//! disabling, removing, the symlink check, and discovering the scripts of
//! enabled plugins. [`install`]/[`update`] shell out to `git` through
//! `vix_git::run` and exist only with this crate's `plugins` feature.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::DiscoveredScript;

/// File name of the lock, next to `config.toml`.
pub const LOCK_FILE_NAME: &str = "plugins.lock";

/// Longest plugin name accepted (§ Plugins, "Commands").
const MAX_NAME_LEN: usize = 64;

/// Where plugins live: the global scripts directory (each plugin is a
/// subdirectory of it) and the lock file. The host resolves both
/// (`Settings::scripts_dir()`, `Settings::plugins_lock_path()`); tests point
/// them at a temp directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginPaths {
    /// The global scripts directory; plugin `<name>` is `<scripts_dir>/<name>/`.
    pub scripts_dir: PathBuf,
    /// The `plugins.lock` file.
    pub lock_path: PathBuf,
}

impl PluginPaths {
    /// The directory plugin `name` is (or would be) installed in.
    #[must_use]
    pub fn plugin_dir(&self, name: &str) -> PathBuf {
        self.scripts_dir.join(name)
    }
}

/// Why a plugin operation failed. The host turns each variant into a
/// translated message; [`fmt::Display`] is the untranslated detail.
#[derive(Debug)]
#[non_exhaustive]
pub enum PluginError {
    /// The `<git-url>[@rev]` argument is empty, option-shaped, or contains
    /// whitespace or control characters.
    InvalidSpec(String),
    /// The plugin name (given, or derived from the URL) is not a valid name.
    InvalidName(String),
    /// A plugin (or some other file) already occupies `<scripts dir>/<name>`.
    AlreadyInstalled(String),
    /// No plugin by this name is in the lock.
    NotInstalled(String),
    /// A symlink in the plugin resolves outside the plugin's directory (or
    /// doesn't resolve at all).
    SymlinkEscape(PathBuf),
    /// `plugins.lock` could not be parsed or serialized.
    Lockfile(String),
    /// A filesystem operation failed.
    Io(std::io::Error),
    /// `git` failed; the text is its error output.
    Git(String),
    /// This build has no `plugins` feature, so `git` can't be run.
    GitUnavailable,
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSpec(s) => write!(f, "invalid plugin source: {s}"),
            Self::InvalidName(s) => write!(f, "invalid plugin name: {s}"),
            Self::AlreadyInstalled(s) => write!(f, "already installed: {s}"),
            Self::NotInstalled(s) => write!(f, "not installed: {s}"),
            Self::SymlinkEscape(p) => {
                write!(f, "symlink escapes the plugin directory: {}", p.display())
            }
            Self::Lockfile(s) => write!(f, "plugins.lock: {s}"),
            Self::Io(e) => write!(f, "{e}"),
            Self::Git(s) => write!(f, "git: {s}"),
            Self::GitUnavailable => f.write_str("git is not included in this build"),
        }
    }
}

impl std::error::Error for PluginError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for PluginError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// One installed plugin, as pinned in `plugins.lock`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockEntry {
    /// The plugin's name: its directory under the scripts directory.
    pub name: String,
    /// The git URL it was installed from.
    pub url: String,
    /// The branch, tag or commit asked for at install, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    /// The full commit currently checked out.
    pub commit: String,
    /// SHA-256 [`digest`] of the plugin's files when the user enabled it;
    /// `None` while disabled. The plugin loads only while this equals the
    /// digest of its files now (§ Plugins, "Trust").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trusted: Option<String>,
}

/// The whole `plugins.lock`: one entry per installed plugin, in install
/// order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lockfile {
    /// Every installed plugin (`[[plugin]]` tables).
    #[serde(default, rename = "plugin")]
    pub plugins: Vec<LockEntry>,
}

impl Lockfile {
    /// Parse lock text.
    ///
    /// # Errors
    ///
    /// [`PluginError::Lockfile`] when `text` is not a valid lock.
    pub fn parse(text: &str) -> Result<Self, PluginError> {
        toml::from_str(text).map_err(|e| PluginError::Lockfile(e.to_string()))
    }

    /// Serialize to lock text.
    ///
    /// # Errors
    ///
    /// [`PluginError::Lockfile`] if serialization fails.
    pub fn to_toml(&self) -> Result<String, PluginError> {
        toml::to_string(self).map_err(|e| PluginError::Lockfile(e.to_string()))
    }

    /// Read the lock at `path`; a missing file is an empty lock.
    ///
    /// # Errors
    ///
    /// [`PluginError::Io`] when the file exists but can't be read, and
    /// [`PluginError::Lockfile`] when it can't be parsed.
    pub fn load(path: &Path) -> Result<Self, PluginError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Write the lock to `path` (creating its directory), through a
    /// temporary file renamed into place so a crash never leaves half a lock.
    ///
    /// # Errors
    ///
    /// [`PluginError::Io`] on a filesystem failure, or
    /// [`PluginError::Lockfile`] if serialization fails.
    pub fn save(&self, path: &Path) -> Result<(), PluginError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = self.to_toml()?;
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// The entry for `name`, if installed.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&LockEntry> {
        self.plugins.iter().find(|p| p.name == name)
    }

    /// The entry for `name`, mutably.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut LockEntry> {
        self.plugins.iter_mut().find(|p| p.name == name)
    }

    /// Remove and return the entry for `name` — its trust record goes with it.
    pub fn remove(&mut self, name: &str) -> Option<LockEntry> {
        let idx = self.plugins.iter().position(|p| p.name == name)?;
        Some(self.plugins.remove(idx))
    }
}

/// Whether an installed plugin loads (§ Plugins, "Trust").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginState {
    /// Trusted, and its files still match the trusted digest: it loads.
    Enabled,
    /// No trust record: it does not load.
    Disabled,
    /// Trusted once, but its files changed since: it does not load until
    /// enabled again.
    Changed,
    /// In the lock, but its directory is gone.
    Missing,
}

impl PluginState {
    /// The i18n key naming this state (`ui.plugin_state.*`).
    #[must_use]
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::Enabled => "ui.plugin_state.enabled",
            Self::Disabled => "ui.plugin_state.disabled",
            Self::Changed => "ui.plugin_state.changed",
            Self::Missing => "ui.plugin_state.missing",
        }
    }
}

/// Split `<git-url>[@rev]` into the URL and the optional rev: the rev is the
/// text after the last `@` that comes after the last `/` and `:`, so
/// `git@host:me/repo.git` has none and `file:///srv/repo.git@v1` has `v1`.
///
/// # Errors
///
/// [`PluginError::InvalidSpec`] when the URL or rev is empty, starts with
/// `-` (git would read it as an option), or contains whitespace or control
/// characters, or the URL uses git's `ext::` transport (which runs a
/// command).
pub fn parse_spec(spec: &str) -> Result<(String, Option<String>), PluginError> {
    let bad = || PluginError::InvalidSpec(spec.to_string());
    let last_sep = spec.rfind(['/', ':']).map_or(0, |i| i + 1);
    let (url, rev) = match spec[last_sep..].rfind('@') {
        Some(at) => {
            let at = last_sep + at;
            (&spec[..at], Some(&spec[at + 1..]))
        }
        None => (spec, None),
    };
    if !plain_arg(url) || url.starts_with("ext::") {
        return Err(bad());
    }
    if let Some(rev) = rev
        && !plain_arg(rev)
    {
        return Err(bad());
    }
    Ok((url.to_string(), rev.map(str::to_string)))
}

/// Non-empty, not option-shaped, no whitespace or control characters.
fn plain_arg(s: &str) -> bool {
    !s.is_empty() && !s.starts_with('-') && !s.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// The plugin name for `url`: its last path segment without a `.git`
/// suffix (`https://example.com/me/vix-case.git` → `vix-case`).
///
/// # Errors
///
/// [`PluginError::InvalidName`] when that segment is not a [`valid_name`].
pub fn name_from_url(url: &str) -> Result<String, PluginError> {
    let trimmed = url.trim_end_matches('/');
    let last = trimmed.rsplit(['/', ':']).next().unwrap_or(trimmed);
    let name = last.strip_suffix(".git").unwrap_or(last);
    if valid_name(name) {
        Ok(name.to_string())
    } else {
        Err(PluginError::InvalidName(name.to_string()))
    }
}

/// Whether `name` is a valid plugin name: ASCII letters, digits, `.`, `_`
/// and `-`, at most 64 characters, not starting with `.` or `-` — so it is
/// always one plain path component, never `..` or a hidden directory.
#[must_use]
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME_LEN
        && !name.starts_with(['.', '-'])
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// SHA-256 over every file and symlink under `dir` (the top-level `.git`
/// excluded), each recorded with its relative path and kind, in sorted
/// order — the code identity trust is pinned to. Symlinks are hashed by
/// their target text, never followed.
///
/// # Errors
///
/// [`PluginError::Io`] when `dir` or anything under it can't be read.
pub fn digest(dir: &Path) -> Result<String, PluginError> {
    let mut entries = Vec::new();
    walk(dir, dir, &mut entries)?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut h = Sha256::new();
    for (rel, kind, path) in entries {
        h.update(rel.as_bytes());
        h.update([0]);
        match kind {
            EntryKind::File => {
                let bytes = std::fs::read(&path)?;
                h.update(b"f");
                h.update((bytes.len() as u64).to_le_bytes());
                h.update(&bytes);
            }
            EntryKind::Symlink => {
                let target = std::fs::read_link(&path)?;
                let target = target.to_string_lossy();
                h.update(b"l");
                h.update((target.len() as u64).to_le_bytes());
                h.update(target.as_bytes());
            }
        }
    }
    Ok(hex(&h.finalize()))
}

/// Lowercase hex of `bytes`.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// What a walked entry is.
#[derive(Clone, Copy)]
enum EntryKind {
    File,
    Symlink,
}

/// Collect `(relative path, kind, path)` for every file and symlink under
/// `dir`, recursing into real directories only, skipping `root/.git`.
fn walk(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, EntryKind, PathBuf)>,
) -> Result<(), PluginError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if dir == root && entry.file_name() == ".git" {
            continue;
        }
        let kind = entry.file_type()?;
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if kind.is_symlink() {
            out.push((rel, EntryKind::Symlink, path));
        } else if kind.is_dir() {
            walk(root, &path, out)?;
        } else if kind.is_file() {
            out.push((rel, EntryKind::File, path));
        }
    }
    Ok(())
}

/// Fail if any symlink under `dir` (the top-level `.git` excluded) resolves
/// — following any chain — to a path outside `dir`, or doesn't resolve at
/// all (§ Plugins, "Trust": symlinks cannot escape).
///
/// # Errors
///
/// [`PluginError::SymlinkEscape`] naming the first offending link, or
/// [`PluginError::Io`] when `dir` can't be read.
pub fn check_symlinks(dir: &Path) -> Result<(), PluginError> {
    let root = dir.canonicalize()?;
    let mut entries = Vec::new();
    walk(dir, dir, &mut entries)?;
    for (_, kind, path) in entries {
        if !matches!(kind, EntryKind::Symlink) {
            continue;
        }
        let inside = path
            .canonicalize()
            .is_ok_and(|real| real.starts_with(&root));
        if !inside {
            return Err(PluginError::SymlinkEscape(path));
        }
    }
    Ok(())
}

/// The [`PluginState`] of `entry` right now.
#[must_use]
pub fn state(paths: &PluginPaths, entry: &LockEntry) -> PluginState {
    let dir = paths.plugin_dir(&entry.name);
    if !valid_name(&entry.name) || !dir.is_dir() {
        return PluginState::Missing;
    }
    match &entry.trusted {
        None => PluginState::Disabled,
        Some(trusted) => match digest(&dir) {
            Ok(now) if &now == trusted => PluginState::Enabled,
            _ => PluginState::Changed,
        },
    }
}

/// Trust plugin `name` as its files are now: record their [`digest`] in the
/// lock, so it loads.
///
/// # Errors
///
/// [`PluginError::NotInstalled`] when it isn't in the lock (or its directory
/// is gone), and any lock or filesystem error.
pub fn enable(paths: &PluginPaths, name: &str) -> Result<(), PluginError> {
    let mut lock = Lockfile::load(&paths.lock_path)?;
    let dir = paths.plugin_dir(name);
    let entry = lock
        .get_mut(name)
        .filter(|_| valid_name(name) && dir.is_dir())
        .ok_or_else(|| PluginError::NotInstalled(name.to_string()))?;
    entry.trusted = Some(digest(&dir)?);
    lock.save(&paths.lock_path)
}

/// Stop trusting plugin `name`: drop its trust record, so it no longer
/// loads and a later enable has to trust the files afresh.
///
/// # Errors
///
/// [`PluginError::NotInstalled`] when it isn't in the lock, and any lock
/// error.
pub fn disable(paths: &PluginPaths, name: &str) -> Result<(), PluginError> {
    let mut lock = Lockfile::load(&paths.lock_path)?;
    let entry = lock
        .get_mut(name)
        .ok_or_else(|| PluginError::NotInstalled(name.to_string()))?;
    entry.trusted = None;
    lock.save(&paths.lock_path)
}

/// Uninstall plugin `name`: delete its directory and its lock entry, trust
/// record included. Returns the removed entry.
///
/// # Errors
///
/// [`PluginError::NotInstalled`] when it isn't in the lock, and any lock or
/// filesystem error.
pub fn remove(paths: &PluginPaths, name: &str) -> Result<LockEntry, PluginError> {
    let mut lock = Lockfile::load(&paths.lock_path)?;
    if !valid_name(name) {
        return Err(PluginError::InvalidName(name.to_string()));
    }
    let entry = lock
        .remove(name)
        .ok_or_else(|| PluginError::NotInstalled(name.to_string()))?;
    remove_path(&paths.plugin_dir(name))?;
    lock.save(&paths.lock_path)?;
    Ok(entry)
}

/// Delete `path` without following it: a symlink is unlinked, a directory
/// removed recursively, a missing path ignored.
fn remove_path(path: &Path) -> Result<(), PluginError> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(path)?,
        Ok(_) => std::fs::remove_file(path)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

/// Every script of every [`PluginState::Enabled`] plugin, with stem
/// `<name>/<file stem>`: `.rhai` regular files at the plugin's top level
/// only (no subdirectories, no symlinks). In lock order, then stem order.
///
/// # Errors
///
/// Any error reading the lock — the host reports it and loads no plugin.
pub fn discover_enabled(paths: &PluginPaths) -> Result<Vec<DiscoveredScript>, PluginError> {
    let lock = Lockfile::load(&paths.lock_path)?;
    let mut found = Vec::new();
    for entry in &lock.plugins {
        if state(paths, entry) != PluginState::Enabled {
            continue;
        }
        let mut by_stem: BTreeMap<String, PathBuf> = BTreeMap::new();
        crate::discovery::collect_into(&paths.plugin_dir(&entry.name), &mut by_stem);
        found.extend(by_stem.into_iter().map(|(stem, path)| DiscoveredScript {
            stem: format!("{}/{stem}", entry.name),
            path,
        }));
    }
    Ok(found)
}

#[cfg(feature = "plugins")]
pub use git_ops::{UpdateOutcome, install, update};

#[cfg(feature = "plugins")]
mod git_ops {
    //! The two operations that run `git`.

    use std::path::Path;

    use super::{
        LockEntry, Lockfile, PluginError, PluginPaths, check_symlinks, digest, name_from_url,
        parse_spec, remove_path, valid_name,
    };

    /// `-c` options passed on every git call: never the `ext::` transport,
    /// which runs an arbitrary command.
    const SAFE: [&str; 2] = ["-c", "protocol.ext.allow=never"];

    /// What [`update`] did to one plugin.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct UpdateOutcome {
        /// The plugin's name.
        pub name: String,
        /// The commit before the update.
        pub old_commit: String,
        /// The commit now checked out (and pinned).
        pub new_commit: String,
        /// `true` when the plugin was trusted and its files changed, so its
        /// trust was dropped and it is disabled until enabled again.
        pub trust_dropped: bool,
    }

    /// Run git in `dir` with the safe options prepended.
    fn git(dir: &Path, args: &[&str]) -> Result<String, PluginError> {
        let mut all: Vec<&str> = SAFE.to_vec();
        all.extend_from_slice(args);
        vix_git::run(dir, &all).map_err(|e| PluginError::Git(e.to_string()))
    }

    /// The full commit `rev` names in the repository at `dir`, trying
    /// `origin/<rev>` first (a branch that may have moved) and then `rev`
    /// itself (a tag or commit).
    fn resolve(dir: &Path, rev: &str) -> Result<String, PluginError> {
        let remote = format!("origin/{rev}^{{commit}}");
        let local = format!("{rev}^{{commit}}");
        git(dir, &["rev-parse", "--verify", "--quiet", &remote])
            .or_else(|_| git(dir, &["rev-parse", "--verify", "--quiet", &local]))
    }

    /// Detach `dir`'s checkout at `commit`.
    fn checkout(dir: &Path, commit: &str) -> Result<(), PluginError> {
        git(dir, &["checkout", "--quiet", "--detach", commit]).map(drop)
    }

    /// Install the plugin `spec` (`<git-url>[@rev]`) into
    /// `<scripts dir>/<name>/` and pin it in the lock, **disabled**. The clone
    /// happens in a hidden staging directory, renamed into place only after
    /// the symlink check passes; on any failure it is deleted and the lock
    /// is untouched.
    ///
    /// # Errors
    ///
    /// [`PluginError::InvalidSpec`]/[`PluginError::InvalidName`] for a bad
    /// argument, [`PluginError::AlreadyInstalled`] when the name is taken,
    /// [`PluginError::Git`] when clone or checkout fails,
    /// [`PluginError::SymlinkEscape`] for a link out of the plugin, and any
    /// lock or filesystem error.
    pub fn install(paths: &PluginPaths, spec: &str) -> Result<LockEntry, PluginError> {
        let (url, rev) = parse_spec(spec)?;
        let name = name_from_url(&url)?;
        let mut lock = Lockfile::load(&paths.lock_path)?;
        let dest = paths.plugin_dir(&name);
        if lock.get(&name).is_some() || std::fs::symlink_metadata(&dest).is_ok() {
            return Err(PluginError::AlreadyInstalled(name));
        }
        std::fs::create_dir_all(&paths.scripts_dir)?;
        let staging_name = format!(".{name}.installing");
        let staging = paths.scripts_dir.join(&staging_name);
        remove_path(&staging)?;
        let result = clone_and_check(paths, &url, rev.as_deref(), &staging_name, &staging);
        let commit = match result {
            Ok(commit) => commit,
            Err(e) => {
                remove_path(&staging).ok();
                return Err(e);
            }
        };
        std::fs::rename(&staging, &dest)?;
        let entry = LockEntry {
            name,
            url,
            rev,
            commit,
            trusted: None,
        };
        lock.plugins.push(entry.clone());
        lock.save(&paths.lock_path)?;
        Ok(entry)
    }

    /// Clone `url` into `staging`, check out `rev` (or the default branch),
    /// verify its symlinks, and return the commit.
    fn clone_and_check(
        paths: &PluginPaths,
        url: &str,
        rev: Option<&str>,
        staging_name: &str,
        staging: &Path,
    ) -> Result<String, PluginError> {
        git(
            &paths.scripts_dir,
            &[
                "clone",
                "--quiet",
                "--no-recurse-submodules",
                "--",
                url,
                staging_name,
            ],
        )?;
        let commit = resolve(staging, rev.unwrap_or("HEAD"))?;
        checkout(staging, &commit)?;
        check_symlinks(staging)?;
        Ok(commit)
    }

    /// Fetch plugin `name` (or every plugin, for `None`) and move it to the
    /// newest commit of its rev (or the remote's default branch), re-pinning
    /// the lock. A plugin whose files changed loses its trust; one whose new
    /// commit has a symlink out of the plugin is put back on its old commit
    /// and reported.
    ///
    /// # Errors
    ///
    /// [`PluginError::NotInstalled`] for an unknown `name`, and the first
    /// git, symlink, lock or filesystem error; plugins updated before that
    /// error stay updated (the lock is saved after each).
    pub fn update(
        paths: &PluginPaths,
        name: Option<&str>,
    ) -> Result<Vec<UpdateOutcome>, PluginError> {
        let lock = Lockfile::load(&paths.lock_path)?;
        let names: Vec<String> = match name {
            Some(n) if lock.get(n).is_some() => vec![n.to_string()],
            Some(n) => return Err(PluginError::NotInstalled(n.to_string())),
            None => lock.plugins.iter().map(|p| p.name.clone()).collect(),
        };
        let mut outcomes = Vec::new();
        for name in names {
            outcomes.push(update_one(paths, &name)?);
        }
        Ok(outcomes)
    }

    /// [`update`] for one plugin.
    fn update_one(paths: &PluginPaths, name: &str) -> Result<UpdateOutcome, PluginError> {
        let mut lock = Lockfile::load(&paths.lock_path)?;
        let dir = paths.plugin_dir(name);
        let Some(entry) = lock
            .get_mut(name)
            .filter(|_| valid_name(name) && dir.is_dir())
        else {
            return Err(PluginError::NotInstalled(name.to_string()));
        };
        git(
            &dir,
            &[
                "fetch",
                "--quiet",
                "--tags",
                "--no-recurse-submodules",
                "origin",
            ],
        )?;
        let target = entry.rev.clone().unwrap_or_else(|| "HEAD".to_string());
        let new_commit = resolve(&dir, &target)?;
        let old_commit = entry.commit.clone();
        checkout(&dir, &new_commit)?;
        if let Err(e) = check_symlinks(&dir) {
            checkout(&dir, &old_commit).ok();
            return Err(e);
        }
        let mut trust_dropped = false;
        if let Some(trusted) = &entry.trusted
            && *trusted != digest(&dir)?
        {
            entry.trusted = None;
            trust_dropped = true;
        }
        entry.commit.clone_from(&new_commit);
        lock.save(&paths.lock_path)?;
        Ok(UpdateOutcome {
            name: name.to_string(),
            old_commit,
            new_commit,
            trust_dropped,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_splits_rev_only_after_the_last_separator() {
        assert_eq!(
            parse_spec("https://example.com/me/case.git@v1").unwrap(),
            ("https://example.com/me/case.git".into(), Some("v1".into()))
        );
        assert_eq!(
            parse_spec("git@example.com:me/case.git").unwrap(),
            ("git@example.com:me/case.git".into(), None)
        );
        assert_eq!(
            parse_spec("file:///srv/case.git").unwrap(),
            ("file:///srv/case.git".into(), None)
        );
    }

    #[test]
    fn spec_refuses_option_shaped_and_ext_input() {
        assert!(parse_spec("--upload-pack=evil").is_err());
        assert!(parse_spec("https://example.com/x.git@-b").is_err());
        assert!(parse_spec("ext::sh -c touch% /tmp/pwned").is_err());
        assert!(parse_spec("").is_err());
        assert!(parse_spec("https://example.com/x.git@").is_err());
    }

    #[test]
    fn name_comes_from_the_last_segment() {
        assert_eq!(
            name_from_url("https://example.com/me/vix-case.git").unwrap(),
            "vix-case"
        );
        assert_eq!(name_from_url("git@example.com:me/tools/").unwrap(), "tools");
        assert!(name_from_url("https://example.com/..").is_err());
        assert!(name_from_url("https://example.com/.hidden.git").is_err());
        assert!(valid_name("a.b_c-1"));
        assert!(!valid_name("a/b"));
        assert!(!valid_name(&"x".repeat(65)));
    }
}
