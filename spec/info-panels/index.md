# Information Panels

Vix groups several read-only, at-a-glance overlays under the umbrella of
*Information Panels*: the **Workspace Dashboard**, **System Information**, **File
Information**, **Outline**, and **Welcome** panels. Each opens as a modal overlay
on top of the editor, shows facts about your workspace, host, file, or code, and
closes with `Esc`. None of them edit your buffers; two of them (System
Information and File Information) let you copy a value *into* the editor.

Each panel follows the Vix split: a small, pure-state crate holds the data and
the selection/scroll state, while the host (`src/app.rs`, `src/ui.rs`) gathers
the raw values, spawns any background work, renders the overlay, and routes keys
and mouse clicks.

| Panel              | Opens from                                            | Crate                              |
| ------------------ | ----------------------------------------------------- | ---------------------------------- |
| Workspace Dashboard | Tools → Workspace Dashboard                           | `vix-workspace-dashboard-panel`    |
| System Information | Tools → System Information                            | `vix-system-information-panel`     |
| File Information   | Tools → File Information                              | `vix-file-information-panel`       |
| Outline            | `Ctrl+Shift+B`, or the command palette ("Outline")    | `vix-outline-panel`                |
| Welcome            | First run, or Help → Welcome…                         | `vix-welcome-panel`                |

## Workspace Dashboard

The Workspace Dashboard shows live metrics about the current workspace folder. It
opens from **Tools → Workspace Dashboard** (action `tools.dashboard`).

The folder name is known immediately and appears at once. The remaining
metrics are computed in **background threads** so the panel never blocks: each
metric stays `None` (rendered as `computing…`) until its thread finishes, then
fills in live. While any metric is still pending, the run loop ticks faster so
results appear promptly.

| Metric      | How it is computed                                                        |
| ----------- | ------------------------------------------------------------------------ |
| Folder      | The top-level workspace folder name (shown immediately)                  |
| Disk usage  | Human-readable size (e.g. `12M`) from a `du -sh` subprocess on the root  |
| File count  | A recursive walk of the workspace, skipping `.git` and `target`          |
| Commit count | Commits reachable from `HEAD` via `vix-git`; `0` (None) when not a repo  |

The dashboard is read-only; `Esc` (or `Enter`) closes it. The crate
(`Dashboard`) holds the four fields and exposes `is_complete()`; the host's
`open_dashboard` spawns the three threads and wires up an `mpsc` channel, and
`poll_dashboard` drains finished results into the open panel each frame. See
[`vix-workspace-dashboard-panel`](../../vix-workspace-dashboard-panel/spec/index.md).

## System Information

The System Information panel is a scrollable, read-only table of facts about the
host machine. It opens from **Tools → System Information** (action
`tools.system_info`).

The data is a **static snapshot** gathered once when the panel opens (via the
[`sysinfo`](https://crates.io/crates/sysinfo) crate plus a few environment
variables) — it is not a live monitor. The table is organized into sections, each
introduced by a heading row (a row with no value):

| Section          | Rows                                                                 |
| ---------------- | ------------------------------------------------------------------- |
| Operating System | Name, Version, Kernel, Hostname, Architecture                       |
| CPU              | Model, Vendor, Physical cores, Logical cores                        |
| Memory           | Total RAM, Used RAM, Available RAM, Total swap, Used swap           |
| Storage          | One row per mount point: free space of total                        |
| Uptime           | System uptime, Load average (1 / 5 / 15 min)                        |
| Environment      | User, Home, Working dir, Shell                                      |

Byte counts are formatted as `16.0 GiB`-style values and uptime as `Dd Hh Mm`.
Arrow keys (plus `PageUp`/`PageDown`/`Home`/`End`, and the mouse) move the
highlight; `Enter` or a left click inserts the highlighted row's **value** into
the active editor (heading rows have no value and insert nothing); `Esc` closes.
The crate (`Panel`) gathers the rows and tracks the highlighted row and scroll
offset. See
[`vix-system-information-panel`](../../vix-system-information-panel/spec/index.md).

## File Information

The File Information panel is a small table of facts about the file in the active
editor tab. It opens from **Tools → File Information** (action
`tools.file_info`).

The host's `gather_file_info` collects the values: the character / word / line
counts come from the buffer, and size / permissions / last-modified come from a
filesystem stat — but only when the file has been saved. The rows that need a
saved file on disk are omitted for an unsaved buffer.

| Row           | Source                                                          |
| ------------- | -------------------------------------------------------------- |
| Name          | File name, or `(unsaved)` for a never-saved buffer            |
| Path          | Full path, or `(unsaved)`                                     |
| Language      | The editor's detected language id (e.g. `rust`, `text`)      |
| Modified      | Whether the buffer has unsaved changes                        |
| Characters    | Character count of the buffer                                 |
| Words         | Whitespace-separated word count                              |
| Lines         | Line count                                                    |
| Size          | On-disk size (human-readable + exact bytes); saved files only |
| Permissions   | Unix mode as `rwxr-xr-x` + octal; Unix + saved files only     |
| Last modified | File mtime as `YYYY-MM-DD HH:MM:SS UTC`; saved files only     |

Arrow keys (plus `PageUp`/`PageDown`/`Home`/`End`, and the mouse) move the
highlight; `Enter` or a left click inserts the highlighted row's value into the
editor; `Esc` closes. The crate (`Panel`) formats the rows from a `FileInfo` and
tracks the selection. See
[`vix-file-information-panel`](../../vix-file-information-panel/spec/index.md).

## Outline

The Outline panel lists the symbols (declarations) in the active buffer. It opens
with **`Ctrl+Shift+B`** or from the command palette ("Outline") via the
`nav.outline` action.

Each row shows a symbol's structural kind prefix (`fn`, `struct`, `mod`, `impl`,
…) and its name. The symbols come from the same fast, offline, language-agnostic
heuristic used by go-to-symbol (`palette::symbols`), so no language server is
required. On open the panel selects the symbol the cursor is currently inside
(`select_nearest`). When the buffer has no symbols (or is an image tab), the panel
does not open and a status message reports it instead.

Arrow keys (plus `PageUp`/`PageDown`/`Home`/`End`, and the mouse) move the
highlight; `Enter` or a click **jumps the cursor to that symbol's line** and
closes the panel; `Esc` closes without jumping. The crate (`Outline`) holds the
entries and the selection/scroll state and reports the selected line. See
[`vix-outline-panel`](../../vix-outline-panel/spec/index.md).

## Welcome

The Welcome panel is a friendly, novice-oriented screen explaining what Vix is,
how to get started, what it can do, and how to send feedback. It appears
automatically the **first time Vix runs**, and can be reopened any time from
**Help → Welcome…** (action `help.welcome`).

First-run behavior is gated by the `welcomed` setting (default `false`):
`maybe_show_welcome` opens the panel once on first launch and then sets
`welcomed` (persisted on exit) so it does not return on later launches.

The panel is **scrollable** text. The content lives in the host's i18n catalog
under the `welcome.body` locale key so it is translatable; the host splits it into
lines, soft-wraps to the panel width, renders with a scrollbar, and forwards
scroll keys. The crate (`Panel`) is pure state — it holds the lines and tracks
the scroll offset.

| Key / action            | Effect                   |
| ----------------------- | ------------------------ |
| `↑` / `↓`               | Scroll one line          |
| `PgUp` / `PgDn` / `Space` | Scroll one page        |
| `Home` / `End`          | Jump to the top / bottom |
| mouse wheel             | Scroll                   |
| `Esc` / `Enter` / `q`   | Close the panel          |

See [`vix-welcome-panel`](../../vix-welcome-panel/spec/index.md).

## As implemented in Vix

All five panels are **shipped**. Each is a small pure-state crate paired with
host glue in `src/app.rs` and `src/ui.rs`:

- **Workspace Dashboard** — `vix-workspace-dashboard-panel` (`Dashboard`).
  `open_dashboard` (action `tools.dashboard`) names the folder, spawns three
  background threads (`du -sh`, a recursive `count_files` skipping `.git` and
  `target`, and `vix_git::commit_count`), and feeds an `mpsc` channel that
  `poll_dashboard` drains each frame; `dashboard_loading` keeps the run loop
  ticking while metrics compute. Read-only; `Esc`/`Enter` closes.
- **System Information** — `vix-system-information-panel` (`Panel`, `gather`).
  `open_system_info` (action `tools.system_info`) takes a one-time `sysinfo`
  snapshot. Arrow keys move the highlight; `Enter`/click inserts the value; `Esc`
  closes.
- **File Information** — `vix-file-information-panel` (`Panel`, `FileInfo`,
  `rows`). `open_file_info` (action `tools.file_info`) calls `gather_file_info`,
  which reads buffer counts and a filesystem stat (size / Unix permissions /
  mtime) for saved files. Arrow keys move; `Enter`/click inserts; `Esc` closes.
- **Outline** — `vix-outline-panel` (`Outline`, `Entry`). `open_outline` (action
  `nav.outline`, bound to `Ctrl+Shift+B` and offered in the command palette)
  builds entries from `palette::symbols` and calls `select_nearest`. `Enter`/click
  jumps to the symbol; `Esc` closes.
- **Welcome** — `vix-welcome-panel` (`Panel`). `maybe_show_welcome` shows it once
  on first run (gated by the `welcomed` setting); `open_welcome` (action
  `help.welcome`, Help → Welcome…) reopens it from `welcome.body`. Scroll keys
  and the mouse wheel scroll; `Esc`/`Enter`/`q` closes.

The Tools menu lists Workspace Dashboard, System Information, and File
Information (`src/menu.rs`); the Outline panel is reached by keybinding or the
command palette; the Welcome panel is reached on first run or from the Help menu.
