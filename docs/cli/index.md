# Command line

`vix [OPTIONS] [FILES]...` — see `vix --help` for the definitive, always-current
list; this page adds usage detail and the `git` integration snippets.

## Arguments

| Form | Effect |
| ---- | ------ |
| `vix` | Open the editor rooted at the current directory, restoring the previous session for this workspace (if `restore_session` is on). |
| `vix FILE...` | Open one or more files; the last one is focused. |
| `vix FILE:LINE:COL` | Open `FILE` and jump straight to `LINE`, column `COL` (both 1-based). |
| `vix -` | Read standard input into an unsaved scratch buffer instead of opening a file — e.g. `git show HEAD:path/to/file \| vix -`. |

## Options

| Flag | Effect |
| ---- | ------ |
| `-l, --locale <LOCALE>` | UI language for this run only (e.g. `en`, `es`, `fr`, `de`, `cy`) — overrides the saved `locale` setting without persisting the change. |
| `--diff <OLD> <NEW>` | Open a read-only unified-diff overlay comparing the two files directly, independent of any open buffer. This is the shape a `git difftool` driver invokes with (`$LOCAL $REMOTE`) — see below. |
| `--version` | Print the version and exit. |
| `--version --json` | Print `{"name":"vix","version":"…"}` instead of plain text, for scripts/tooling. |
| `--help` | Full flag reference (clap-generated, always in sync with the binary). |

## `git difftool`

Point `git difftool` at `vix --diff`:

```ini
# ~/.gitconfig
[difftool "vix"]
	cmd = vix --diff \"$LOCAL\" \"$REMOTE\"
[diff]
	tool = vix
```

```sh
git difftool                # every changed file, one at a time
git difftool HEAD~1          # against a specific revision
git difftool --dir-diff       # not applicable — vix compares one file pair
```

Vix's diff overlay is read-only (`Esc`/`q` closes it, `↑`/`↓` scroll) — it's a
viewer, not an editor for the diff itself. To edit, open the file normally
(`vix "$LOCAL"`) instead.

## `git mergetool`

Merge-conflict resolution doesn't need a dedicated flag: a file with
unresolved `<<<<<<<`/`=======`/`>>>>>>>` markers opens normally, and Vix's
built-in conflict tool (`crates/vix-conflict-tool/spec/index.md`) resolves
the block under the cursor with ours/theirs/both — no special CLI shape
required, just point `mergetool` at the merged file:

```ini
# ~/.gitconfig
[mergetool "vix"]
	cmd = vix \"$MERGED\"
	trustExitCode = false
[merge]
	tool = vix
```

```sh
git mergetool
```

`trustExitCode = false` is intentional: Vix doesn't report a resolved/
unresolved outcome via its exit code (a TUI editor's exit code reflects
whether it launched and ran, not whether the user finished resolving
anything), so `git` should ask whether the merge succeeded rather than
trust the process result — its normal behavior for `trustExitCode = false`.
