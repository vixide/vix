//! File operations and language servers (T623, `crates/vix-lsp/spec`): the
//! explorer's moves, copies, deletes and the directory editor's renames,
//! creates and deletes first ask servers for their `will*Files` edits (held
//! for at most [`FILE_OP_WAIT`]), then tell them with `did*Files`; and a
//! bounded recursive scan lets servers' registered `**/*.x` file watchers see
//! files nobody has open.
//!
//! One operation can be held at a time. While it waits, the work that would
//! follow is parked as a [`FileOpCont`] and run when the last server has
//! answered or the wait runs out; a second operation started meanwhile goes
//! ahead without waiting.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::lsp_core::file_ops::{
    self, FileChangeType, FileOp, FileOpItem, WatchSnapshot, diff_snapshots,
};

use super::App;
use super::dir_edit::DirEditConfirm;

/// How long a file operation waits for language servers' `will*Files` answers
/// before going ahead anyway.
const FILE_OP_WAIT: Duration = Duration::from_secs(3);

/// How often the recursive watcher scan runs.
const SCAN_EVERY: Duration = Duration::from_secs(3);

/// How many of the host's own announced file changes are remembered, to keep
/// the next scan from reporting them a second time.
const ANNOUNCED_KEPT: usize = 256;

/// The work parked while servers answer `will*Files`.
pub(crate) enum FileOpCont {
    /// Rename the active file `old` to `new` (`File → Rename`).
    Rename {
        /// The current path.
        old: PathBuf,
        /// The new path.
        new: PathBuf,
    },
    /// Carry on pasting the explorer clipboard (`App::paste`).
    Paste,
    /// Delete (or trash) these explorer paths.
    Delete(Vec<PathBuf>),
    /// Apply this confirmed directory-editor plan.
    DirEdit(DirEditConfirm),
}

/// A file operation held for servers' answers.
pub(crate) struct PendingFileOp {
    /// What to do once they have answered.
    cont: FileOpCont,
    /// When the servers were asked.
    since: Instant,
}

/// State of the recursive file-watcher scan.
#[derive(Default)]
pub(crate) struct WatchState {
    /// When the next scan is due; `None` until the first.
    next_scan: Option<Instant>,
    /// The covered files the last scan found.
    snapshot: Option<WatchSnapshot>,
    /// File changes the host already told servers about itself.
    announced: Vec<(PathBuf, FileChangeType)>,
}

impl App {
    /// Ask servers for the `will*Files` edits `groups` imply. Returns whether
    /// the operation must now wait for them (the caller then parks its work
    /// with [`App::hold_file_op`]); `false` when no server wants it or another
    /// operation is already held.
    pub(super) fn ask_servers_before(&mut self, groups: &[(FileOp, Vec<FileOpItem>)]) -> bool {
        self.pending_file_op.is_none() && self.lsp.will_file_operations(groups)
    }

    /// Park `cont` until servers have answered the request just sent by
    /// [`App::ask_servers_before`].
    pub(super) fn hold_file_op(&mut self, cont: FileOpCont) {
        self.pending_file_op = Some(PendingFileOp {
            cont,
            since: Instant::now(),
        });
    }

    /// A server answered `will*Files`: apply its edits, and once every server
    /// has answered, run the held operation.
    pub(super) fn on_will_file_ops_reply(&mut self, edits: &[crate::lsp::FileEdits], last: bool) {
        if self.pending_file_op.is_none() {
            return;
        }
        if !edits.is_empty() {
            self.apply_workspace_edit(edits);
        }
        if last {
            self.run_pending_file_op();
        }
    }

    /// Go ahead when servers have not answered within [`FILE_OP_WAIT`].
    pub(super) fn expire_pending_file_op(&mut self) {
        if self
            .pending_file_op
            .as_ref()
            .is_some_and(|p| p.since.elapsed() >= FILE_OP_WAIT)
        {
            self.run_pending_file_op();
        }
    }

    /// Carry out the held operation.
    fn run_pending_file_op(&mut self) {
        let Some(pending) = self.pending_file_op.take() else {
            return;
        };
        match pending.cont {
            FileOpCont::Rename { old, new } => self.finish_rename(&old, &new),
            FileOpCont::Paste => self.process_paste(),
            FileOpCont::Delete(paths) => self.delete_confirmed_now(&paths),
            FileOpCont::DirEdit(confirm) => self.apply_dir_edit_plan(&confirm),
        }
    }

    /// Tell servers the operations in `groups` happened (`did*Files`, plus
    /// watched-file events), and remember the file changes so the next scan
    /// does not repeat them.
    pub(super) fn announce_file_ops(&mut self, groups: &[(FileOp, Vec<FileOpItem>)]) {
        let changes = self.lsp.did_file_operations(groups);
        let kept = &mut self.watch_scan.announced;
        kept.extend(changes);
        if kept.len() > ANNOUNCED_KEPT {
            kept.drain(..kept.len() - ANNOUNCED_KEPT);
        }
    }

    /// Forget the scan's baseline (no server watches files any more).
    pub(super) fn reset_watch_scan(&mut self) {
        self.watch_scan.snapshot = None;
        self.watch_scan.next_scan = None;
    }

    /// Scan the workspace for files servers' registered watchers cover and add
    /// the created, changed and deleted ones to `events` (T623). Runs at most
    /// every [`SCAN_EVERY`], visits at most [`file_ops::SCAN_MAX_ENTRIES`]
    /// directory entries, and its first run only records the baseline.
    pub(super) fn scan_watched_files(&mut self, events: &mut Vec<(PathBuf, FileChangeType)>) {
        let now = Instant::now();
        if self.watch_scan.next_scan.is_some_and(|t| now < t) {
            return;
        }
        self.watch_scan.next_scan = Some(now + SCAN_EVERY);
        let roots = self.lsp.watch_roots();
        if roots.is_empty() {
            return;
        }
        let lsp = &self.lsp;
        let new =
            file_ops::scan_watched(&roots, &|p| lsp.watch_covers(p), file_ops::SCAN_MAX_ENTRIES);
        let found = self
            .watch_scan
            .snapshot
            .as_ref()
            .map(|old| diff_snapshots(old, &new))
            .unwrap_or_default();
        self.watch_scan.snapshot = Some(new);
        for change in found {
            if let Some(i) = self.watch_scan.announced.iter().position(|a| *a == change) {
                self.watch_scan.announced.remove(i);
            } else if !events.iter().any(|(p, _)| *p == change.0) {
                events.push(change);
            }
        }
    }
}

/// Collect `(operation, item)` pairs into one group per operation, in the order
/// renames, creates, deletes; operations with no items are left out.
pub(super) fn group_ops(
    ops: impl IntoIterator<Item = (FileOp, FileOpItem)>,
) -> Vec<(FileOp, Vec<FileOpItem>)> {
    let mut groups: Vec<(FileOp, Vec<FileOpItem>)> =
        [FileOp::Rename, FileOp::Create, FileOp::Delete]
            .into_iter()
            .map(|op| (op, Vec::new()))
            .collect();
    for (op, item) in ops {
        if let Some((_, items)) = groups.iter_mut().find(|(o, _)| *o == op) {
            items.push(item);
        }
    }
    groups.retain(|(_, items)| !items.is_empty());
    groups
}

/// The item for `path`, noting now whether it is a folder (a delete removes
/// that knowledge).
pub(super) fn item(path: &std::path::Path) -> FileOpItem {
    FileOpItem::single(path, path.is_dir())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn group_ops_orders_renames_creates_deletes_and_drops_empty_groups() {
        let one = |p: &str| FileOpItem::single(Path::new(p), false);
        let groups = group_ops([
            (FileOp::Delete, one("/w/d")),
            (FileOp::Create, one("/w/c")),
            (FileOp::Delete, one("/w/e")),
        ]);
        let ops: Vec<FileOp> = groups.iter().map(|(op, _)| *op).collect();
        assert_eq!(ops, [FileOp::Create, FileOp::Delete]);
        assert_eq!(groups[1].1.len(), 2);
        assert_eq!(group_ops([]).len(), 0);
    }
}
