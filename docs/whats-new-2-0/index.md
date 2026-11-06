# What's new in Vix 2.0

> **Status: 2.0.0, 2026-11-06.** Every item below is built and tested; where something is partial, experimental
> or opt-in, it says so. The full, detailed record is the
> [changelog](../changelog.md) (its `[2.0.0]` section).

2.0 is a feature release for people who edit code in a terminal: it adds working
on other machines, working together, a code-aware chat, many IDE conveniences
and a much lighter large-file path. It is also a major version because it
makes breaking changes to the **library** API of the `vix-*` crates so later
additions can be non-breaking. The `vix` command and your settings files keep
working; see [Upgrading](#upgrading).

## Highlights

- **Remote editing over SSH (experimental).** `vix ssh://user@host/path`, or
  **File → Open Remote…**, opens a folder on another machine with nothing
  installed there. Search, Git and the terminal run on the remote machine. The
  workspace starts untrusted; once you trust it, language servers, the
  debugger and tasks run there too. A `vix` of the same version on the remote
  side is used as a faster remote agent when it is present. Docker and Podman
  containers (`docker://`, `podman://`) and dev containers work the same way;
  Kubernetes (`k8s://`) passed the conformance suite against one local `kind`
  cluster, and WSL (`wsl://`) has only been tested against a stand-in, so treat
  both as experimental. See [Remote editing](../remote-editing/index.md).
- **Cowork: edit together in real time (opt-in build).** Single-use invites,
  the host approves each guest, everyone's cursors in their own color,
  conflict-free merging, a session chat and shared notes, and a terminal that
  is shared read-only by default. Everything travels encrypted with no server
  in between. Cowork is **not** in the default build or the prebuilt releases:
  everyone in a session needs a build made with `--features cowork`. See
  [Cowork](../cowork/index.md).
- **IM menu: code-aware chat.** A local Notes conversation, IRC (IRCv3 over
  TLS), `path:line` references that open on Enter, and threads anchored to lines
  of your files, stored in `.vix/im/threads/` so Git can share them. Matrix is
  planned, not built. See [IM](../im/index.md).
- **IDE features.** Local History (a snapshot on every save, with diff and
  restore), Search Everywhere, a Refactor menu, Organize Imports and Code
  Cleanup, named run configurations in `.vix/run.toml`, Peek Definition,
  CodeLens and postfix completion. See [IDE parity](../ide-parity/index.md).
- **Large File Viewer.** **File → Open in Large File Viewer…** reads a
  multi-gigabyte file read-only without loading it. Measured on an Apple M4 Max
  with a warm cache: a 4 GiB log shows its first screen in under a millisecond
  and is fully indexed in about half a second, using under 5 MiB. See
  [Large File Viewer](../large-file-viewer/index.md) and
  [Performance](../performance/index.md).
- **Lighter everyday editing.** A 100 MB log now takes 126 MiB instead of
  1.1 GiB, and an idle Vix uses no CPU instead of about 7.5% of a core. The
  terminal-capability probe for images no longer delays the first screen.
  Conditions and numbers: [Performance](../performance/index.md).

## More that is new

**Containers.** The OC menu manages containers, images, volumes, networks and
Podman pods on Docker, Podman or nerdctl, with Compose and image builds. Every
removing or running command shows its exact command first, and
`oc_mode = "read-only"` limits the menu to viewing.

**AI, when you want it.** `ai = "off"` hides the AI menu and refuses every AI
request, with no rebuild. `vix --mcp` serves a read-only view of a workspace
(open buffers, symbols, outline, SQLite schemas) to an AI agent over the Model
Context Protocol. Inline ghost-text completion
(`inline_completion = "off" | "lsp" | "ai"`, off by default) shows a dimmed
suggestion; Tab accepts it. A first slice of an Agent Client Protocol client
(**AI → Start Agent**) works in trusted workspaces only; choosing between
several agents and session resume are not built yet.

**Editing.** Multiple carets gain word, line and buffer motions, **Split
Selection into Lines** and **Select Regex Matches in Selection**. Undo Tree
(**Edit → Undo Tree…**) shows and restores any state of a buffer. Hot exit backs
unsaved buffers up so a crash or closed terminal no longer loses them (and asks
before recovering). Safe saving keeps hard links and file owners and keeps a
recovery copy of the original. Legacy encodings (UTF-16, Shift_JIS, GBK,
windows-125x and more) open as editable text and are saved back the way they
came. Compare Files side by side (`vix --diff OLD NEW`) opens both files as
editable, synchronized tabs.

**Code intelligence and debugging.** Indexing and build progress in the status
bar, **View → LSP Status…**, snippet completions that follow the full LSP
snippet grammar, on-type formatting, document colors and links, call and type
hierarchies, and incremental sync. The debugger gains conditions, hit counts,
logpoints, exception filters, attach, threads and expandable variables.
Tree-sitter text objects add function and class motions.

**Git.** An interactive commit graph, interactive rebase and cherry-pick, stash
list and worktrees, a change-gutter base you can choose, a read-only multi-file
branch review with exportable line comments, and opening or discarding changes
from the Git panel. Blame now matches unsaved edits.

**Terminal.** Several terminals, scrollback, text selection, and `Ctrl`+click on
a `path:line:col` in output opens it in the editor.

**Data and tools.** A REST client with many requests per `.http` file,
variables, environments and Copy as curl; a FHIR client; an HL7 v2 view with
MLLP send; a Todo Tree; formatting with your project's own Prettier; piping the
selection through a shell command; and an opt-in Jupyter kernel
(`--features jupyter`).

**Project environments.** Vix detects a `.venv`, direnv or mise environment and
applies it to language servers, tasks and the terminal. direnv and mise run only
in a workspace you have trusted, and a project can add its own detectors in
`.vix/env.toml`, optionally with extra variables for the detector's own command.

**Packaging.** From 2.0, Windows on ARM gets its own `.zip` and `.msi`. Since
1.8.0 every release also carries `.deb`, `.rpm` and Arch packages. See
[Getting Started](../getting-started/index.md) for the install commands that work
today.

## Fixes worth knowing about

- Git from a repository subdirectory now uses the right file; staging a path
  such as `app/[id]/page.tsx` no longer stages a similar one.
- LSP formatting edits go to the file they were requested for; edits computed
  against text that has since changed are dropped, not applied.
- Replace in Files skips and reports a line that changed after the search.
- Save As asks before overwriting; reloading guards against devices, huge and
  binary files.
- The single-color (phosphor and safelight) themes are now truly one color, and
  were renamed with a "Monochrome" suffix (old saved names still resolve).
- The workspace script-trust prompt needs an explicit `y`.
- The Linux musl build is now configured to link statically. This is **not yet
  verified on a real release build**.

## Upgrading

- **The `vix` command and your settings files** keep working. For example
  `auto_save = true` and `false` still load.
- **Terminal.** **Tools → Terminal** and `Ctrl+]` now hide the terminal, which
  keeps running; **Tools → Terminal → Close Terminal** stops it.
- **Themes.** Saved theme names that point at the old single-color names still
  find their theme.
- **Using the crates as a library.** Breaking changes: public types that keep
  growing are `#[non_exhaustive]` (build them from `Default::default()` and
  assign fields), most `Result<_, String>` APIs return typed errors, `App`'s
  public fields shrink to the ones the GUIs use, and some LSP, debugger and
  terminal signatures changed. The complete list is the **Breaking** section of
  the [changelog](../changelog.md).
- **Minimum Rust** for building from source is 1.97.

## Not in 2.0 yet

So that nothing here is a surprise:

- The winget, Scoop, AUR, `cargo binstall`, Nix, mise and container-image
  install channels have files in the repository but are **not published or
  verified**; use Homebrew, the installer scripts, npm, a Linux package or
  Cargo.
- There is no apt or dnf repository; update by installing the next package.
- Matrix chat, WSL against a real Windows host (Kubernetes is verified on one local cluster only), and the full ACP agent
  experience are not done.
- Cowork and Jupyter need a custom build.

## Announcement drafts

Maintainer-facing drafts of the launch announcement live in
[`docs/launch/`](../launch/index.md). They are drafts; nothing has been posted.
