//! Oil/wdired-style directory editing (T636): render a directory as an
//! editable text listing, diff the edited text back into renames, creates
//! and deletes, and order those into filesystem steps that are safe to run
//! one after another — including rename cycles such as two names swapped.
//!
//! Pure: nothing here touches the filesystem. The host lists the directory,
//! hands the entries to [`render`], diffs the saved buffer with [`diff`],
//! orders the result with [`schedule`] and runs each [`Step`] itself.
//!
//! Each listed line is `/<id> <name>`, where `<id>` is the entry's 1-based
//! position in the listing, zero-padded to at least three digits, and a
//! directory's name ends in `/`. The id is how a line keeps its identity
//! when its name is edited; a line without one is a new entry.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write as _};

/// One entry of the directory being edited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    /// The entry's file name (no path).
    pub name: String,
    /// Whether the entry is a directory.
    pub is_dir: bool,
}

impl Listed {
    /// An entry named `name`, a directory when `is_dir`.
    #[must_use]
    pub fn new(name: impl Into<String>, is_dir: bool) -> Self {
        Self {
            name: name.into(),
            is_dir,
        }
    }
}

/// Why an edited listing could not be turned into a plan. Line numbers are
/// 1-based.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiffError {
    /// The same id appears on two lines (copying an entry isn't supported).
    DuplicateId {
        /// The repeated id.
        id: usize,
        /// The line it appears on the second time.
        line: usize,
    },
    /// An id that no listed entry has.
    UnknownId {
        /// The id.
        id: usize,
        /// The line it appears on.
        line: usize,
    },
    /// A name that can't be a single directory entry: empty, `.`, `..`, or
    /// containing a path separator or NUL.
    InvalidName {
        /// The name as written.
        name: String,
        /// The line it appears on.
        line: usize,
    },
    /// Two lines would end up with the same name.
    DuplicateName {
        /// The name.
        name: String,
    },
}

impl fmt::Display for DiffError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateId { id, line } => write!(f, "duplicate id {id} on line {line}"),
            Self::UnknownId { id, line } => write!(f, "unknown id {id} on line {line}"),
            Self::InvalidName { name, line } => write!(f, "invalid name {name:?} on line {line}"),
            Self::DuplicateName { name } => write!(f, "duplicate name {name:?}"),
        }
    }
}

impl std::error::Error for DiffError {}

/// What an edited listing asks for, before ordering.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// `(from, to)` for each entry whose name changed.
    pub renames: Vec<(String, String)>,
    /// `(name, is_dir)` for each new line.
    pub creates: Vec<(String, bool)>,
    /// The listed entries whose lines were removed.
    pub deletes: Vec<Listed>,
}

impl Plan {
    /// Whether the listing was saved unchanged.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.renames.is_empty() && self.creates.is_empty() && self.deletes.is_empty()
    }

    /// How many changes the plan makes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.renames.len() + self.creates.len() + self.deletes.len()
    }
}

/// One filesystem step, with names relative to the edited directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Delete (or trash) an entry.
    Delete {
        /// The entry's name.
        name: String,
        /// Whether it is a directory.
        is_dir: bool,
    },
    /// Rename `from` to `to`; `to` is free when the step runs.
    Rename {
        /// The current name.
        from: String,
        /// The new name.
        to: String,
    },
    /// Create an empty file or a directory.
    Create {
        /// The new entry's name.
        name: String,
        /// Whether to create a directory.
        is_dir: bool,
    },
}

/// Render `entries` as the editable listing, one `/<id> <name>` line each
/// (ids are 1-based positions in `entries`), a directory's name ending in
/// `/`.
#[must_use]
pub fn render(entries: &[Listed]) -> String {
    let width = id_width(entries.len());
    let mut out = String::new();
    for (i, e) in entries.iter().enumerate() {
        let slash = if e.is_dir { "/" } else { "" };
        let _ = writeln!(out, "/{:0width$} {}{slash}", i + 1, e.name);
    }
    out
}

/// The id field's width: the digits of the largest id, at least three.
fn id_width(count: usize) -> usize {
    count.to_string().len().max(3)
}

/// Split a line into its id (when it starts with `/<digits>` and a space or
/// tab) and the rest.
fn split_id(line: &str) -> (Option<usize>, &str) {
    if let Some(rest) = line.strip_prefix('/') {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 {
            let after = &rest[digits..];
            if after.starts_with([' ', '\t'])
                && let Ok(id) = rest[..digits].parse::<usize>()
            {
                return (Some(id), after.trim_start_matches([' ', '\t']));
            }
        }
    }
    (None, line)
}

/// Strip a name's trailing `/` (a directory marker) and check it can be a
/// single directory entry. Returns the bare name and whether it had the
/// marker.
fn parse_name(text: &str, line: usize) -> Result<(String, bool), DiffError> {
    let text = text.trim_end_matches(['\r', ' ', '\t']);
    let (name, dir) = match text.strip_suffix('/') {
        Some(n) => (n, true),
        None => (text, false),
    };
    let bad = name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\0'])
        || (cfg!(windows) && name.contains('\\'));
    if bad {
        return Err(DiffError::InvalidName {
            name: text.to_string(),
            line,
        });
    }
    Ok((name.to_string(), dir))
}

/// Diff the edited `buffer` against the `entries` it was rendered from.
///
/// Blank lines are ignored. A line with a known id keeps that entry (a
/// changed name is a rename; whether the entry is a directory can't change,
/// so its trailing `/` is optional); a line without an id is a new entry (a
/// trailing `/` makes it a directory); an entry whose id no longer appears
/// is deleted. Renames and creates are listed in buffer order, deletes in
/// listing order.
///
/// # Errors
///
/// Returns a [`DiffError`] for a repeated or unknown id, a name that isn't a
/// single entry, or two lines that end up with the same name.
pub fn diff(entries: &[Listed], buffer: &str) -> Result<Plan, DiffError> {
    let mut seen: BTreeSet<usize> = BTreeSet::new();
    let mut plan = Plan::default();
    let mut finals: BTreeSet<String> = BTreeSet::new();
    for (i, raw) in buffer.lines().enumerate() {
        let line = i + 1;
        if raw.trim().is_empty() {
            continue;
        }
        let (id, rest) = split_id(raw);
        let (name, dir) = parse_name(rest, line)?;
        if !finals.insert(name.clone()) {
            return Err(DiffError::DuplicateName { name });
        }
        match id {
            Some(id) => {
                let Some(entry) = id.checked_sub(1).and_then(|k| entries.get(k)) else {
                    return Err(DiffError::UnknownId { id, line });
                };
                if !seen.insert(id) {
                    return Err(DiffError::DuplicateId { id, line });
                }
                if name != entry.name {
                    plan.renames.push((entry.name.clone(), name));
                }
            }
            None => plan.creates.push((name, dir)),
        }
    }
    plan.deletes = entries
        .iter()
        .enumerate()
        .filter(|(k, _)| !seen.contains(&(k + 1)))
        .map(|(_, e)| e.clone())
        .collect();
    Ok(plan)
}

/// Order `plan` into steps that can run one at a time without one step
/// clobbering an entry a later step still needs.
///
/// Deletes run first (so a rename or create may reuse a deleted name), then
/// renames, then creates. A rename runs only once its target is free; when
/// every remaining rename waits on another (a cycle, such as `a` ↔ `b`), one
/// source is first moved aside to a temporary name built from `temp_tag`
/// that no entry in `entries` or the plan uses, which breaks the cycle.
#[must_use]
pub fn schedule(entries: &[Listed], plan: &Plan, temp_tag: &str) -> Vec<Step> {
    let mut steps: Vec<Step> = plan
        .deletes
        .iter()
        .map(|e| Step::Delete {
            name: e.name.clone(),
            is_dir: e.is_dir,
        })
        .collect();
    // Names on disk as the steps run.
    let mut occupied: BTreeSet<String> = entries.iter().map(|e| e.name.clone()).collect();
    for e in &plan.deletes {
        occupied.remove(&e.name);
    }
    let mut taken: BTreeSet<String> = occupied.clone();
    taken.extend(plan.renames.iter().map(|(_, to)| to.clone()));
    taken.extend(plan.creates.iter().map(|(n, _)| n.clone()));
    // Pending renames, from -> to, in a stable order.
    let mut pending: BTreeMap<String, String> = plan.renames.iter().cloned().collect();
    let mut temp_n = 0usize;
    while !pending.is_empty() {
        let ready = pending
            .iter()
            .find(|(_, to)| !occupied.contains(*to))
            .map(|(from, to)| (from.clone(), to.clone()));
        if let Some((from, to)) = ready {
            pending.remove(&from);
            occupied.remove(&from);
            occupied.insert(to.clone());
            steps.push(Step::Rename { from, to });
            continue;
        }
        // Every pending rename waits on another: move one source aside.
        let Some((from, to)) = pending.pop_first() else {
            break;
        };
        let temp = loop {
            temp_n += 1;
            let candidate = format!(".{temp_tag}-{temp_n}");
            if !taken.contains(&candidate) {
                break candidate;
            }
        };
        taken.insert(temp.clone());
        occupied.remove(&from);
        occupied.insert(temp.clone());
        steps.push(Step::Rename {
            from,
            to: temp.clone(),
        });
        pending.insert(temp, to);
    }
    steps.extend(plan.creates.iter().map(|(name, is_dir)| Step::Create {
        name: name.clone(),
        is_dir: *is_dir,
    }));
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing() -> Vec<Listed> {
        vec![
            Listed::new("docs", true),
            Listed::new("a.txt", false),
            Listed::new("b.txt", false),
        ]
    }

    /// Run the steps against a set of names, panicking if a step would
    /// clobber or miss an entry.
    fn simulate(entries: &[Listed], steps: &[Step]) -> BTreeSet<String> {
        let mut names: BTreeSet<String> = entries.iter().map(|e| e.name.clone()).collect();
        for s in steps {
            match s {
                Step::Delete { name, .. } => assert!(names.remove(name), "delete {name}"),
                Step::Rename { from, to } => {
                    assert!(!names.contains(to), "rename onto existing {to}");
                    assert!(names.remove(from), "rename missing {from}");
                    names.insert(to.clone());
                }
                Step::Create { name, .. } => assert!(names.insert(name.clone()), "create {name}"),
            }
        }
        names
    }

    #[test]
    fn render_pads_ids_and_marks_directories() {
        assert_eq!(render(&listing()), "/001 docs/\n/002 a.txt\n/003 b.txt\n");
    }

    #[test]
    fn an_unchanged_listing_is_an_empty_plan() {
        let plan = diff(&listing(), &render(&listing())).unwrap();
        assert!(plan.is_empty());
        assert_eq!(plan.len(), 0);
    }

    #[test]
    fn renames_creates_and_deletes() {
        let edited = "/001 manual/\n/003 b.txt\nnew.txt\nsub/\n\n";
        let plan = diff(&listing(), edited).unwrap();
        assert_eq!(plan.renames, vec![("docs".into(), "manual".into())]);
        assert_eq!(
            plan.creates,
            vec![("new.txt".into(), false), ("sub".into(), true)]
        );
        assert_eq!(plan.deletes, vec![Listed::new("a.txt", false)]);
        assert_eq!(plan.len(), 4);
    }

    #[test]
    fn a_swap_is_a_cycle_broken_through_a_temporary_name() {
        let edited = "/001 docs/\n/002 b.txt\n/003 a.txt\n";
        let plan = diff(&listing(), edited).unwrap();
        assert_eq!(
            plan.renames,
            vec![
                ("a.txt".into(), "b.txt".into()),
                ("b.txt".into(), "a.txt".into())
            ]
        );
        let steps = schedule(&listing(), &plan, "vix-tmp");
        assert_eq!(steps.len(), 3, "{steps:?}");
        assert_eq!(
            steps[0],
            Step::Rename {
                from: "a.txt".into(),
                to: ".vix-tmp-1".into()
            }
        );
        let names = simulate(&listing(), &steps);
        let want: BTreeSet<String> = ["docs", "a.txt", "b.txt"].map(String::from).into();
        assert_eq!(names, want);
    }

    #[test]
    fn a_three_way_rotation_and_a_chain_need_no_extra_temporaries() {
        let entries = vec![
            Listed::new("a", false),
            Listed::new("b", false),
            Listed::new("c", false),
            Listed::new("x", false),
        ];
        // a->b, b->c, c->a (cycle), and x->y (free).
        let plan = diff(&entries, "/001 b\n/002 c\n/003 a\n/004 y\n").unwrap();
        let steps = schedule(&entries, &plan, "t");
        let temps = steps
            .iter()
            .filter(|s| matches!(s, Step::Rename { to, .. } if to.starts_with(".t-")))
            .count();
        assert_eq!(temps, 1, "{steps:?}");
        let names = simulate(&entries, &steps);
        let want: BTreeSet<String> = ["a", "b", "c", "y"].map(String::from).into();
        assert_eq!(names, want);
        // A chain a->b where b is deleted runs after the delete.
        let plan = diff(&entries, "/001 b\n/003 c\n/004 x\n").unwrap();
        let steps = schedule(&entries, &plan, "t");
        assert!(matches!(&steps[0], Step::Delete { name, .. } if name == "b"));
        simulate(&entries, &steps);
    }

    #[test]
    fn the_temporary_name_avoids_existing_names() {
        let entries = vec![
            Listed::new("a", false),
            Listed::new("b", false),
            Listed::new(".t-1", false),
        ];
        let plan = diff(&entries, "/001 b\n/002 a\n/003 .t-1\n").unwrap();
        let steps = schedule(&entries, &plan, "t");
        assert!(
            steps
                .iter()
                .any(|s| matches!(s, Step::Rename { to, .. } if to == ".t-2"))
        );
        simulate(&entries, &steps);
    }

    #[test]
    fn a_create_may_reuse_a_name_freed_by_a_rename_or_delete() {
        let plan = diff(&listing(), "/001 docs/\n/002 c.txt\na.txt\n").unwrap();
        let steps = schedule(&listing(), &plan, "t");
        let names = simulate(&listing(), &steps);
        let want: BTreeSet<String> = ["docs", "a.txt", "c.txt"].map(String::from).into();
        assert_eq!(names, want);
    }

    #[test]
    fn errors() {
        let e = listing();
        assert_eq!(
            diff(&e, "/002 a.txt\n/002 b.txt\n"),
            Err(DiffError::DuplicateId { id: 2, line: 2 })
        );
        assert_eq!(
            diff(&e, "/009 z\n"),
            Err(DiffError::UnknownId { id: 9, line: 1 })
        );
        assert_eq!(
            diff(&e, "/001 docs/\n/002 b.txt\n/003 b.txt\n"),
            Err(DiffError::DuplicateName {
                name: "b.txt".into()
            })
        );
        assert_eq!(
            diff(&e, "/002 a.txt\n/003 b.txt\nb.txt\n"),
            Err(DiffError::DuplicateName {
                name: "b.txt".into()
            })
        );
        for bad in ["..", ".", "x/y", "/"] {
            assert!(
                matches!(diff(&e, bad), Err(DiffError::InvalidName { line: 1, .. })),
                "{bad}"
            );
        }
        assert_eq!(
            DiffError::UnknownId { id: 9, line: 1 }.to_string(),
            "unknown id 9 on line 1"
        );
    }

    #[test]
    fn ids_tolerate_tabs_and_a_dropped_directory_slash() {
        let plan = diff(&listing(), "/1\tdocs\n/02 a.txt\n/003   b.txt\r\n").unwrap();
        assert!(plan.is_empty(), "{plan:?}");
    }
}
