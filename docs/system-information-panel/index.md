# System Information Panel

The system information panel is a scrolling, read-only overlay that shows a
snapshot of your machine. You can browse the values and insert any one of them
into the active editor.

## Opening the panel

Open it from the menu bar: **Tools → System Information**. The panel appears as
a modal overlay over the editor.

## What it shows

The panel gathers a **static snapshot** once, at the moment it opens, using the
[`sysinfo`](https://crates.io/crates/sysinfo) crate. It covers:

- **Operating system** — name, version, kernel, hostname, architecture.
- **CPU** — model, vendor, physical and logical core counts.
- **Memory and swap** — total, used, and available.
- **Storage** — per-mount, shown as free of total.
- **Uptime** — how long the system has been running.
- **Environment** — user, home directory, working directory, shell.

Because the snapshot is taken once on open, the values do not refresh while the
panel is open; reopen the panel to gather fresh data.

## Keybindings

| Key                       | Action                                          |
| ------------------------- | ----------------------------------------------- |
| `↑` / `↓`                 | Move the highlight up / down one row            |
| `PageUp` / `PageDown`     | Move the highlight by a page                    |
| `Home` / `End`            | Jump to the first / last row                    |
| `Enter`                   | Insert the highlighted value into the editor    |
| `Esc`                     | Close the panel                                 |

## Mouse

A left click on a row inserts that row's value into the active editor.

## Example

To record your kernel version in a bug report: open **Tools → System
Information**, highlight the kernel row, and press `Enter` (or click it). The
value is inserted at the cursor.

## Roadmap

Live refresh, per-process CPU/memory and the process tree, hardware sensors
(battery, temperatures, fan speeds), and network interfaces / traffic rates are
planned but not yet implemented.
