# System Information Panel

A read-only snapshot of host system information, plus the panel's selection:
the row-selection and scroll state.

Vix's Tools menu offers a *System Information* panel: a scrollable table of
facts about the host — operating system, CPU, memory, swap, disks, uptime,
and the current environment — gathered once when the panel opens (a static
snapshot, not a live monitor). The user browses with the arrow keys (or the
mouse) and can insert any value into the active editor. This crate gathers the
data (via [`sysinfo`]) and tracks the highlighted row and scroll offset; the
host renders the table, maps clicks to rows, and inserts the chosen value.

A fact the platform can't report is shown as *unknown* (a name, version,
hostname or uptime), shown as zero (load average), or left out (the CPU
counts, the Memory section, the Storage section) — the panel always opens.

## See also

- [file-information-panel spec](../../vix-file-information-panel/spec/) — shared info-panel behavior
