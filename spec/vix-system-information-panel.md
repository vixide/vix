# vix-system-information-panel

**Status:** Shipped (first cut) — **Tools → System Information** opens a
scrolling, read-only overlay holding a **static snapshot** (gathered once when
the panel opens, via the [`sysinfo`] crate) of: operating system (name, version,
kernel, hostname, architecture), CPU (model, vendor, physical/logical cores),
memory and swap (total/used/available), per-mount storage (free of total),
system uptime, and environment (user, home, working directory, shell). Arrow
keys / PageUp / PageDown / Home / End move the highlight; Enter or a left click
inserts the highlighted value into the active editor; Esc closes. The crate
gathers the data and tracks the highlighted row + scroll offset; the host renders
the table and performs the insertion.

Roadmap (not yet implemented): live refresh, per-process CPU/memory and the
process tree, hardware sensors (battery, temperatures, fan speeds), and network
interfaces / traffic rates. These need a refresh loop and are more
platform-fragile, so they are deferred.

[`sysinfo`]: https://crates.io/crates/sysinfo

Because Rust is a low-level systems programming language with a powerful standard library, it can gather almost any information the operating system and hardware have to offer.

Using community crates like sysinfo or OS-specific bindings, Rust can profile a system in granular detail.

Hardware & Physical Info:

- CPU: Model name, architecture, number of cores (physical and logical), and vendor information.

- Memory: Total physical RAM, available memory, and swap space.

- Storage: Drive models, partitions, total disk space, and available disk space.

- Sensors: Battery state (health, charge level), motherboard/CPU temperatures, and fan speeds.

OS & Environment:

- Platform Details: Operating system name, kernel version, hostname, and architecture (e.g., \(x86\_64\), AArch64).

- Environment: Current user information (username, UID/GID), home directory, and environmental variables.

- Time & Locale: System uptime, current timezone, and date.

Running Processes & System Activity:

- Process Tree: A list of all actively running processes and their child processes.

- Resource Usage: Per-process CPU and memory consumption.

- Network: Active network connections, open ports, network interfaces, and incoming/outgoing traffic rates.

- I/O Operations: Disk read/write activity, open files, and system-wide process statuses.