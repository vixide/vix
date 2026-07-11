# System Information Panel

A read-only snapshot of host system information and the panel's row-selection
+ scroll state.

Vix's Tools menu offers a *System Information* panel: a scrollable table of
facts about the host — operating system, CPU, memory, swap, disks, uptime,
and the current environment — gathered once when the panel opens (a static
snapshot, not a live monitor). The user browses with the arrow keys (or the
mouse) and can insert any value into the active editor. This crate gathers the
data (via [`sysinfo`]) and tracks the highlighted row and scroll offset; the
host renders the table, maps clicks to rows, and inserts the chosen value.

## See also

- [file-information-panel spec](../../vix-file-information-panel/spec/) — shared info-panel behavior
