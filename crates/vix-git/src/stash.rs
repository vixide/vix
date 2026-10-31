//! The stash list (T634): parse `git stash list`, and apply, pop, drop, or
//! show one entry by its index.
//!
//! Every runner names a stash as `stash@{N}`, built here from a `usize`, so
//! no caller-supplied text ever reaches git's argument list.

#[cfg(feature = "process")]
use std::path::Path;

#[cfg(feature = "process")]
use crate::GitError;

/// One `git stash list` row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StashEntry {
    /// The `N` in `stash@{N}`: 0 is the most recent stash.
    pub index: usize,
    /// The stash's description (`WIP on main: 1a2b3c4 subject`, or the
    /// message given to `git stash push -m`).
    pub message: String,
}

impl StashEntry {
    /// The entry's reflog name, `stash@{N}`.
    #[must_use]
    pub fn reference(&self) -> String {
        stash_ref(self.index)
    }
}

/// `stash@{index}`.
#[must_use]
pub fn stash_ref(index: usize) -> String {
    format!("stash@{{{index}}}")
}

/// Parse `git stash list`'s default output (`%gd: %gs`, one stash per line:
/// `stash@{0}: WIP on main: 1a2b3c4 subject`). A line that doesn't start
/// with `stash@{N}` is skipped, so a stray warning can't become a row.
#[must_use]
pub fn parse_stash_list(text: &str) -> Vec<StashEntry> {
    text.lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("stash@{")?;
            let (num, rest) = rest.split_once('}')?;
            let index = num.parse().ok()?;
            let message = rest.strip_prefix(':').unwrap_or(rest).trim().to_string();
            Some(StashEntry { index, message })
        })
        .collect()
}

#[cfg(feature = "process")]
/// Every stash, most recent first (**Git → Stash List…**). Empty when there
/// are none or `dir` isn't a repository.
#[must_use]
pub fn stash_list(dir: &Path) -> Vec<StashEntry> {
    crate::run(dir, &["stash", "list"])
        .map(|s| parse_stash_list(&s))
        .unwrap_or_default()
}

#[cfg(feature = "process")]
/// Apply stash `index` to the working tree, keeping it (`git stash apply`).
///
/// # Errors
/// git's stderr on failure (e.g. a conflict, or no such stash).
pub fn stash_apply(dir: &Path, index: usize) -> Result<(), GitError> {
    crate::run(dir, &["stash", "apply", &stash_ref(index)]).map(drop)
}

#[cfg(feature = "process")]
/// Apply stash `index` and drop it when that succeeds (`git stash pop`).
///
/// # Errors
/// git's stderr on failure (e.g. a conflict, or no such stash).
pub fn stash_pop_at(dir: &Path, index: usize) -> Result<(), GitError> {
    crate::run(dir, &["stash", "pop", &stash_ref(index)]).map(drop)
}

#[cfg(feature = "process")]
/// Delete stash `index` without applying it (`git stash drop`). The host
/// confirms first: a dropped stash is gone from the list.
///
/// # Errors
/// git's stderr on failure (e.g. no such stash).
pub fn stash_drop(dir: &Path, index: usize) -> Result<(), GitError> {
    crate::run(dir, &["stash", "drop", &stash_ref(index)]).map(drop)
}

#[cfg(feature = "process")]
/// Stash `index` as a unified diff against the commit it was made on
/// (`git stash show -p`). `None` on any failure.
#[must_use]
pub fn stash_show(dir: &Path, index: usize) -> Option<String> {
    crate::run(dir, &["stash", "show", "-p", &stash_ref(index)]).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_stash_list_lines() {
        let text = "stash@{0}: WIP on main: 1a2b3c4 add parser\n\
                    stash@{1}: On feature/x: before rebase\n";
        let got = parse_stash_list(text);
        assert_eq!(
            got,
            vec![
                StashEntry {
                    index: 0,
                    message: "WIP on main: 1a2b3c4 add parser".into()
                },
                StashEntry {
                    index: 1,
                    message: "On feature/x: before rebase".into()
                },
            ]
        );
        assert_eq!(got[1].reference(), "stash@{1}");
    }

    #[test]
    fn skips_lines_that_are_not_stash_entries() {
        assert_eq!(parse_stash_list(""), Vec::new());
        assert_eq!(
            parse_stash_list("warning: something\nstash@{x}: bad\n"),
            Vec::new()
        );
        let got = parse_stash_list("stash@{12}:no space\n");
        assert_eq!(got[0].index, 12);
        assert_eq!(got[0].message, "no space");
    }
}
