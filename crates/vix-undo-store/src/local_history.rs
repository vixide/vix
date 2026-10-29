//! Local History: automatic per-file snapshots taken on save, independent of
//! version control and of the undo tree (T720).
//!
//! Each saved file gets a directory `<config>/history/<hash of its path>/`
//! holding one `<id>.snap` file per snapshot (the whole text) and an optional
//! `<id>.label` file (a user-given name). The `id` is the snapshot's creation
//! time in milliseconds since the Unix epoch, so ids sort chronologically.
//!
//! Everything that touches the disk lives on [`LocalHistory`]; the text
//! transforms ([`unified_diff`], [`revert_range`]) are pure.

use std::fmt;
use std::path::{Path, PathBuf};

use similar::{ChangeTag, DiffOp, TextDiff};

use crate::sha;

/// Snapshots kept per file by default; the oldest are pruned beyond this.
pub const MAX_SNAPSHOTS: usize = 50;

/// Files larger than this (bytes) are not snapshotted.
pub const MAX_SNAPSHOT_BYTES: usize = 5 * 1024 * 1024;

/// Why a Local History operation failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum HistoryError {
    /// The snapshot does not exist (it was pruned or never taken).
    NotFound(u64),
    /// Reading or writing a snapshot failed.
    Io(std::io::Error),
}

impl fmt::Display for HistoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "no snapshot {id}"),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for HistoryError {}

impl From<std::io::Error> for HistoryError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// One saved snapshot of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// Creation time, milliseconds since the Unix epoch; unique per file.
    pub id: u64,
    /// The user's label, if one was given.
    pub label: Option<String>,
    /// Size of the snapshot text in bytes.
    pub bytes: u64,
}

/// A Local History store rooted at a directory.
#[derive(Debug, Clone)]
pub struct LocalHistory {
    root: PathBuf,
    keep: usize,
}

impl LocalHistory {
    /// A store rooted at `root` (tests use a temp directory).
    #[must_use]
    pub fn at(root: PathBuf) -> Self {
        Self {
            root,
            keep: MAX_SNAPSHOTS,
        }
    }

    /// The same store keeping at most `keep` snapshots per file (at least one).
    #[must_use]
    pub fn with_keep(mut self, keep: usize) -> Self {
        self.keep = keep.max(1);
        self
    }

    /// The store under the user's config directory (`<config>/history/`), or
    /// `None` when there is no config directory.
    #[must_use]
    pub fn default_location() -> Option<Self> {
        let dir = vix_settings::Settings::config_path()?
            .parent()?
            .join("history");
        Some(Self::at(dir))
    }

    /// The directory holding `file`'s snapshots.
    fn dir_for(&self, file: &Path) -> PathBuf {
        let canon = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
        self.root.join(sha(&canon.to_string_lossy()))
    }

    /// Take a snapshot of `content` for `file`. Returns `Ok(None)` when it is
    /// identical to the newest snapshot and unlabelled (nothing new to keep),
    /// or when it is too large.
    ///
    /// # Errors
    /// [`HistoryError::Io`] when the snapshot cannot be written.
    pub fn snapshot(
        &self,
        file: &Path,
        content: &str,
        label: Option<&str>,
    ) -> Result<Option<Snapshot>, HistoryError> {
        if content.len() > MAX_SNAPSHOT_BYTES {
            return Ok(None);
        }
        let label = label.filter(|l| !l.is_empty());
        if label.is_none()
            && let Some(newest) = self.list(file).first()
            && self.read(file, newest.id).is_ok_and(|t| t == content)
        {
            return Ok(None);
        }
        let dir = self.dir_for(file);
        std::fs::create_dir_all(&dir)?;
        let mut id = now_ms();
        while dir.join(format!("{id}.snap")).exists() {
            id += 1;
        }
        vix_fileops::write_atomic_private(&dir.join(format!("{id}.snap")), content.as_bytes())?;
        if let Some(l) = label {
            vix_fileops::write_atomic_private(&dir.join(format!("{id}.label")), l.as_bytes())?;
        }
        self.prune(file);
        Ok(Some(Snapshot {
            id,
            label: label.map(str::to_string),
            bytes: content.len() as u64,
        }))
    }

    /// The snapshots of `file`, newest first.
    #[must_use]
    pub fn list(&self, file: &Path) -> Vec<Snapshot> {
        let Ok(rd) = std::fs::read_dir(self.dir_for(file)) else {
            return Vec::new();
        };
        let mut out: Vec<Snapshot> = rd
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                if path.extension()? != "snap" {
                    return None;
                }
                let id: u64 = path.file_stem()?.to_str()?.parse().ok()?;
                let bytes = e.metadata().map_or(0, |m| m.len());
                let label = std::fs::read_to_string(path.with_extension("label")).ok();
                Some(Snapshot { id, label, bytes })
            })
            .collect();
        out.sort_by_key(|s| std::cmp::Reverse(s.id));
        out
    }

    /// The text of snapshot `id`.
    ///
    /// # Errors
    /// [`HistoryError::NotFound`] when there is no such snapshot.
    pub fn read(&self, file: &Path, id: u64) -> Result<String, HistoryError> {
        std::fs::read_to_string(self.dir_for(file).join(format!("{id}.snap")))
            .map_err(|_| HistoryError::NotFound(id))
    }

    /// Set (or, with an empty `label`, clear) snapshot `id`'s label.
    ///
    /// # Errors
    /// [`HistoryError::NotFound`] for an unknown id, [`HistoryError::Io`] when
    /// the label cannot be written.
    pub fn set_label(&self, file: &Path, id: u64, label: &str) -> Result<(), HistoryError> {
        let dir = self.dir_for(file);
        if !dir.join(format!("{id}.snap")).exists() {
            return Err(HistoryError::NotFound(id));
        }
        let path = dir.join(format!("{id}.label"));
        if label.is_empty() {
            let _ = std::fs::remove_file(path);
        } else {
            vix_fileops::write_atomic_private(&path, label.as_bytes())?;
        }
        Ok(())
    }

    /// Drop the oldest snapshots beyond the keep limit, unlabelled ones
    /// first (a labelled snapshot is a milestone the user named).
    fn prune(&self, file: &Path) {
        let all = self.list(file);
        if all.len() <= self.keep {
            return;
        }
        let excess = all.len() - self.keep;
        let dir = self.dir_for(file);
        let mut victims: Vec<&Snapshot> = all.iter().rev().filter(|s| s.label.is_none()).collect();
        victims.extend(all.iter().rev().filter(|s| s.label.is_some()));
        for s in victims.into_iter().take(excess) {
            let _ = std::fs::remove_file(dir.join(format!("{}.snap", s.id)));
            let _ = std::fs::remove_file(dir.join(format!("{}.label", s.id)));
        }
    }
}

/// Milliseconds since the Unix epoch.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// A unified-style line diff from `old` to `new`: ` ` context, `-` removed,
/// `+` added, with a `@@ -a +b @@` marker before each change group. Empty when
/// the texts are equal.
#[must_use]
pub fn unified_diff(old: &str, new: &str) -> String {
    use std::fmt::Write as _;
    let diff = TextDiff::from_lines(old, new);
    let mut out = String::new();
    for group in diff.grouped_ops(3) {
        let Some(first) = group.first() else {
            continue;
        };
        let _ = writeln!(
            out,
            "@@ -{} +{} @@",
            first.old_range().start + 1,
            first.new_range().start + 1
        );
        for op in &group {
            for change in diff.iter_changes(op) {
                out.push(match change.tag() {
                    ChangeTag::Delete => '-',
                    ChangeTag::Insert => '+',
                    ChangeTag::Equal => ' ',
                });
                out.push_str(change.value());
                if !change.value().ends_with('\n') {
                    out.push('\n');
                }
            }
        }
    }
    out
}

/// Replace the 0-based lines `start..end` of `current` with what `snapshot`
/// has there: every difference that touches the range is reverted, the rest of
/// `current` is left alone.
#[must_use]
pub fn revert_range(current: &str, snapshot: &str, start: usize, end: usize) -> String {
    let diff = TextDiff::from_lines(snapshot, current);
    let old: Vec<&str> = snapshot.split_inclusive('\n').collect();
    let new: Vec<&str> = current.split_inclusive('\n').collect();
    let end = end.max(start + 1);
    let mut out = String::new();
    for op in diff.ops() {
        let nr = op.new_range();
        let touches = match op {
            DiffOp::Equal { .. } => false,
            // A pure deletion has an empty new range: it touches when its
            // position falls inside the selection.
            DiffOp::Delete { new_index, .. } => *new_index >= start && *new_index < end,
            _ => nr.start < end && nr.end > start,
        };
        let src = if touches {
            old.get(op.old_range()).unwrap_or(&[])
        } else {
            new.get(nr).unwrap_or(&[])
        };
        for l in src {
            out.push_str(l);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(tag: &str) -> (LocalHistory, PathBuf) {
        let dir = std::env::temp_dir().join(format!("vix-lh-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        (LocalHistory::at(dir.clone()), dir)
    }

    #[test]
    fn snapshots_list_newest_first_and_skip_duplicates() {
        let (h, dir) = store("a");
        let f = Path::new("/nonexistent/a.txt");
        assert!(h.snapshot(f, "one\n", None).unwrap().is_some());
        assert!(
            h.snapshot(f, "one\n", None).unwrap().is_none(),
            "dup skipped"
        );
        assert!(h.snapshot(f, "two\n", None).unwrap().is_some());
        let l = h.list(f);
        assert_eq!(l.len(), 2);
        assert!(l[0].id > l[1].id);
        assert_eq!(h.read(f, l[0].id).unwrap(), "two\n");
        assert_eq!(h.read(f, l[1].id).unwrap(), "one\n");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn labels_round_trip_and_force_a_snapshot() {
        let (h, dir) = store("b");
        let f = Path::new("/nonexistent/b.txt");
        let s = h.snapshot(f, "x\n", Some("before")).unwrap().unwrap();
        assert_eq!(h.list(f)[0].label.as_deref(), Some("before"));
        // A labelled snapshot is kept even when the text is unchanged.
        assert!(h.snapshot(f, "x\n", Some("again")).unwrap().is_some());
        h.set_label(f, s.id, "").unwrap();
        assert!(h.list(f).iter().any(|x| x.id == s.id && x.label.is_none()));
        assert!(matches!(
            h.set_label(f, 1, "x"),
            Err(HistoryError::NotFound(1))
        ));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn prune_keeps_the_newest_and_labelled() {
        let (h, dir) = store("c");
        let f = Path::new("/nonexistent/c.txt");
        h.snapshot(f, "keep me\n", Some("milestone")).unwrap();
        for i in 0..(MAX_SNAPSHOTS + 5) {
            h.snapshot(f, &format!("v{i}\n"), None).unwrap();
        }
        let l = h.list(f);
        assert_eq!(l.len(), MAX_SNAPSHOTS);
        assert!(l.iter().any(|s| s.label.is_some()), "labelled one survives");
        let newest = format!("v{}\n", MAX_SNAPSHOTS + 4);
        assert_eq!(h.read(f, l[0].id).unwrap(), newest);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn with_keep_lowers_the_limit() {
        let (h, dir) = store("d");
        let h = h.with_keep(2);
        let f = Path::new("/nonexistent/d.txt");
        for i in 0..5 {
            h.snapshot(f, &format!("v{i}\n"), None).unwrap();
        }
        assert_eq!(h.list(f).len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn unified_diff_marks_changes() {
        assert_eq!(unified_diff("a\nb\n", "a\nb\n"), "");
        let d = unified_diff("a\nb\nc\n", "a\nB\nc\n");
        assert!(d.starts_with("@@ -1 +1 @@\n"));
        assert!(d.contains("-b\n") && d.contains("+B\n") && d.contains(" a\n"));
    }

    #[test]
    fn revert_range_only_touches_the_selection() {
        let snap = "a\nb\nc\nd\n";
        let cur = "A\nb\nC\nd\n";
        assert_eq!(revert_range(cur, snap, 2, 3), "A\nb\nc\nd\n");
        assert_eq!(revert_range(cur, snap, 0, 4), snap);
        assert_eq!(revert_range(cur, snap, 1, 2), cur);
    }

    #[test]
    fn revert_range_restores_a_deleted_line_and_drops_an_added_one() {
        let snap = "a\nb\nc\n";
        assert_eq!(
            revert_range("a\nc\n", snap, 1, 2),
            snap,
            "deleted line returns"
        );
        assert_eq!(
            revert_range("a\nb\nX\nc\n", snap, 2, 3),
            snap,
            "added line goes"
        );
    }
}
