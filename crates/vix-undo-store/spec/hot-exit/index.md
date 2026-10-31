# Hot Exit

Unsaved text survives a crash, a killed terminal or a forced quit (T629).
While Vix is idle, and again on quit, each dirty buffer is backed up to
`<config>/backup/`; when its file is next opened, Vix asks **Recover unsaved
changes?** and can show the difference first. A backup is never applied on
its own, and a clean save deletes it. The `hot_exit` setting (default `true`)
turns the whole feature off.

There is no action id or menu item: like persistent undo, this rides on the
ordinary open, idle and save paths. Compare VS Code's `files.hotExit`, Vim's
swap files and Emacs auto-save files.

## When a backup is written

| Moment | What is written |
| --- | --- |
| Idle pass (every 2 s, from `App::tick`) | Each eligible buffer whose edit revision changed since its last backup. |
| Buffer over 4 MiB | As above, but at most once a minute: a large buffer is not copied every pass (T749). |
| Quit (`App::on_exit`) | Every eligible buffer not yet backed up at its current revision, large or not. |

An **eligible** buffer is dirty, holds text (not an image), is editable (not
read-only), is not the editable search-results buffer, and is owned here (not
a Cowork guest's shared-in buffer, which its host saves). The Large File
Viewer is read-only and never has unsaved text.

Only changed buffers are written: each tab remembers the revision it last
backed up (`Tab::backup`), so an untouched dirty buffer costs nothing per pass.
A failed write is reported once per session as a warning.

## How a backup is keyed

Backups reuse the persistent undo store's scheme: a SHA-256 of an identity
names the file, and a SHA-256 of the text guards it.

| Buffer | Identity | File name |
| --- | --- | --- |
| A local file | its canonical path | `<sha(identity)>.json` |
| A file in a remote (SSH or container) workspace | its `ssh://host/path` name | `<sha(identity)>.json` |
| An untitled buffer | a session id given on its first backup | `untitled-<workspace hash>-<id>.json` |

Backups always stay on the local machine, even for a remote buffer; keying a
remote file by its remote name keeps it apart from a local file of the same
path. An untitled buffer keeps its session id for as long as it is open, so
later backups replace its file; the workspace hash in the name means a
workspace only offers back its own untitled buffers.

Each file holds the identity, the text and the text's hash. A file whose hash
does not match its text (torn or edited by hand) is ignored. New files are
owner-only, since unsaved text may be secret.

## Recovering

- **Opening a file** (not a preview) that has a backup: if the backup equals
  the file, it is stale and is deleted silently. Otherwise the prompt offers
  `r` recover, `d` show the diff, `x` discard, `n`/`Esc` decide later.
- **Starting** in a workspace with untitled backups: one prompt offers them
  all; `r` reopens each as an untitled, unsaved buffer with its old session id.
- `r` puts the backup's text into the buffer and leaves it **dirty**: nothing
  is written to the file until the user saves.
- `d` opens the unified diff overlay (`vix-diff-view`), the file on disk
  against the backup; when it closes the prompt comes back.
- `x` deletes the backup. `n`/`Esc` keeps it, to be offered on the next open;
  a later edit of the buffer replaces it.
- Several offers queue and are shown one at a time, after any other open
  confirmation.

## When a backup is deleted

- A **clean save** of the buffer (Save, Save All, auto-save, the save half of
  the close/quit prompt). Save As deletes the backup kept under the old path
  or the untitled session id.
- **Discard** in the close or quit prompt: throwing the text away is a
  decision, so nothing should offer it back.
- `x` in the recovery prompt, or a backup found equal to its file on open.

A forced quit (Vi's `:q!`) keeps the backups: the next open offers them, and
`x` throws them away.

## As implemented in Vix

`vix_undo_store::hot_exit` is the store: `BackupStore` (`at`,
`default_location`, `write`, `read`, `exists`, `remove`, `untitled_ids`),
`BackupKey`, `local_identity` and `new_session_id`, unit tested against a
temporary directory. The host is `tui/src/app/hot_exit.rs`: `poll_hot_exit`
(the idle pass, from `App::tick`), `flush_hot_exit` (from `App::on_exit`),
`check_backup_on_open` (from `App::open_path_with`), `offer_untitled_recovery`
(from `App::start`), `drop_active_backup` (from the save, Save As and discard
paths) and the `ConfirmKind::RecoverBackup` prompt. The store directory is
`App::with_backup_dir` when set, else `backup/` beside an overridden settings
file, else beside an overridden session file, else `<config>/backup/`, so a
test run never writes to the user's config. `vix_editor::editor::BackupMark`
is the per-tab bookkeeping. Integration tests: `tui/tests/integration/hot_exit.rs`.
