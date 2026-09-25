//! A read-only snapshot of host system information and the panel's row-selection
//! + scroll state.
//!
//! Vix's Tools menu offers a *System Information* panel: a scrollable table of
//! facts about the host — operating system, CPU, memory, swap, disks, uptime,
//! and the current environment — gathered once when the panel opens (a static
//! snapshot, not a live monitor). The user browses with the arrow keys (or the
//! mouse) and can insert any value into the active editor. This crate gathers the
//! data (via [`sysinfo`]) and tracks the highlighted row and scroll offset; the
//! host renders the table, maps clicks to rows, and inserts the chosen value.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

// Shared workspace i18n: brings `t!` into scope unqualified and surfaces the
// translation lookup fns at this crate root (see the vix_i18n crate). T563:
// this crate originally called `t!` zero times -- every row label was
// hardcoded English, shown in the real Tools -> System Information panel.
#[macro_use]
extern crate vix_i18n;
vix_i18n::surface!();

use sysinfo::{Disks, System};

/// One row of the table: a `label` and its `value`. Section headings are rows
/// with an empty `value` (and so insert nothing).
#[derive(Clone, Debug)]
pub struct Row {
    /// Left-hand label (e.g. `Total memory`) or a section heading.
    pub label: String,
    /// Right-hand value (e.g. `16.0 GiB`), or empty for a section heading.
    pub value: String,
}

impl Row {
    fn heading(label: &str) -> Self {
        Row {
            label: label.to_string(),
            value: String::new(),
        }
    }

    fn pair(label: &str, value: impl Into<String>) -> Self {
        Row {
            label: label.to_string(),
            value: value.into(),
        }
    }

    /// Whether this row is a section heading (no value to insert).
    #[must_use]
    pub fn is_heading(&self) -> bool {
        self.value.is_empty()
    }
}

/// Selection + scroll state for the System Information overlay, over a snapshot
/// of [`Row`]s gathered when the panel opens.
pub struct Panel {
    /// The gathered rows, in display order.
    pub rows: Vec<Row>,
    /// Index of the highlighted row.
    pub selected: usize,
    /// First visible row, kept in sync by [`Panel::ensure_visible`].
    pub scroll: usize,
}

impl Default for Panel {
    fn default() -> Self {
        Panel::open()
    }
}

impl Panel {
    /// Gather a fresh snapshot and open the panel on its first row.
    #[must_use]
    pub fn open() -> Self {
        Panel {
            rows: gather(),
            selected: 0,
            scroll: 0,
        }
    }

    /// Number of rows in the table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether the table has no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Move the highlight up one row, stopping at the top.
    pub fn up(&mut self) {
        self.selected = vix_list_state::up(self.selected);
    }

    /// Move the highlight down one row, stopping at the bottom.
    pub fn down(&mut self) {
        self.selected = vix_list_state::down(self.selected, self.rows.len());
    }

    /// Move the highlight up one page (`page` rows), stopping at the top.
    pub fn page_up(&mut self, page: usize) {
        self.selected = vix_list_state::page_up(self.selected, page);
    }

    /// Move the highlight down one page (`page` rows), stopping at the bottom.
    pub fn page_down(&mut self, page: usize) {
        self.selected = vix_list_state::page_down(self.selected, page, self.rows.len());
    }

    /// Select a row directly (e.g. from a click); returns whether `idx` was real.
    pub fn select_index(&mut self, idx: usize) -> bool {
        match vix_list_state::select_index(idx, self.rows.len()) {
            Some(i) => {
                self.selected = i;
                true
            }
            None => false,
        }
    }

    /// Adjust [`scroll`](Self::scroll) so the highlighted row stays within a
    /// window of `height` visible rows.
    pub fn ensure_visible(&mut self, height: usize) {
        self.scroll =
            vix_list_state::ensure_visible(self.selected, self.scroll, height, self.rows.len());
    }

    /// The highlighted row's value (empty for a section heading).
    #[must_use]
    pub fn selected_value(&self) -> String {
        self.rows
            .get(self.selected)
            .map(|r| r.value.clone())
            .unwrap_or_default()
    }
}

/// Format a byte count as a human-readable size (`16.0 GiB`), using the
/// active locale's decimal-separator convention (T567).
///
/// Delegates to `vix-byte-size` (Run H, T521) — this crate and
/// `vix-file-information-panel` used to each hand-roll an identical copy.
#[must_use]
pub fn human_bytes(n: u64) -> String {
    vix_byte_size::human_bytes_for_locale(n, &rust_i18n::locale())
}

/// Format a duration in seconds as `Dd Hh Mm`.
#[must_use]
pub fn human_uptime(secs: u64) -> String {
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3_600;
    let mins = (secs % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h {mins}m")
    } else if hours > 0 {
        format!("{hours}h {mins}m")
    } else {
        format!("{mins}m")
    }
}

fn env_or(keys: &[&str], fallback: &str) -> String {
    for key in keys {
        if let Ok(v) = std::env::var(key)
            && !v.is_empty()
        {
            return v;
        }
    }
    fallback.to_string()
}

/// The "Operating System" and "CPU" sections (host identity + processor facts).
fn os_and_cpu_rows(sys: &System, unknown: &str) -> Vec<Row> {
    let mut rows = vec![
        Row::heading(&t!("info.operating_system")),
        Row::pair(
            &t!("info.name"),
            System::name().unwrap_or_else(|| unknown.to_string()),
        ),
        Row::pair(
            &t!("info.version"),
            System::long_os_version().unwrap_or_else(|| unknown.to_string()),
        ),
        Row::pair(
            &t!("info.kernel"),
            System::kernel_version().unwrap_or_else(|| unknown.to_string()),
        ),
        Row::pair(
            &t!("info.hostname"),
            System::host_name().unwrap_or_else(|| unknown.to_string()),
        ),
        Row::pair(&t!("info.architecture"), std::env::consts::ARCH),
        Row::heading(&t!("info.cpu")),
    ];
    if let Some(cpu) = sys.cpus().first() {
        let brand = cpu.brand().trim();
        if !brand.is_empty() {
            rows.push(Row::pair(&t!("info.model"), brand));
        }
        rows.push(Row::pair(&t!("info.vendor"), cpu.vendor_id()));
    }
    if let Some(physical) = sysinfo::System::physical_core_count() {
        rows.push(Row::pair(&t!("info.physical_cores"), physical.to_string()));
    }
    rows.push(Row::pair(
        &t!("info.logical_cores"),
        sys.cpus().len().to_string(),
    ));
    rows
}

/// The "Memory" and "Storage" sections (RAM/swap totals + per-disk free space).
fn memory_and_storage_rows(sys: &System) -> Vec<Row> {
    let mut rows = vec![
        Row::heading(&t!("info.memory")),
        Row::pair(&t!("info.total_ram"), human_bytes(sys.total_memory())),
        Row::pair(&t!("info.used_ram"), human_bytes(sys.used_memory())),
        Row::pair(
            &t!("info.available_ram"),
            human_bytes(sys.available_memory()),
        ),
        Row::pair(&t!("info.total_swap"), human_bytes(sys.total_swap())),
        Row::pair(&t!("info.used_swap"), human_bytes(sys.used_swap())),
    ];

    let disks = Disks::new_with_refreshed_list();
    if !disks.is_empty() {
        rows.push(Row::heading(&t!("info.storage")));
        for disk in &disks {
            let mount = disk.mount_point().display().to_string();
            let value = t!(
                "info.disk_free_of",
                free = human_bytes(disk.available_space()),
                total = human_bytes(disk.total_space())
            )
            .to_string();
            rows.push(Row::pair(&mount, value));
        }
    }
    rows
}

/// The "Uptime" and "Environment" sections (load average + shell env vars).
fn uptime_and_environment_rows(unknown: &str) -> Vec<Row> {
    let load = System::load_average();
    let cwd =
        std::env::current_dir().map_or_else(|_| unknown.to_string(), |p| p.display().to_string());
    vec![
        Row::heading(&t!("info.uptime")),
        Row::pair(&t!("info.system_uptime"), human_uptime(System::uptime())),
        Row::pair(
            &t!("info.load_average"),
            t!(
                "info.load_average_value",
                one = format!("{:.2}", load.one),
                five = format!("{:.2}", load.five),
                fifteen = format!("{:.2}", load.fifteen)
            )
            .to_string(),
        ),
        Row::heading(&t!("info.environment")),
        Row::pair(&t!("info.user"), env_or(&["USER", "USERNAME"], unknown)),
        Row::pair(&t!("info.home"), env_or(&["HOME", "USERPROFILE"], unknown)),
        Row::pair(&t!("info.working_dir"), cwd),
        Row::pair(&t!("info.shell"), env_or(&["SHELL", "COMSPEC"], unknown)),
    ]
}

/// Gather a fresh snapshot of host system information into display rows.
#[must_use]
pub fn gather() -> Vec<Row> {
    let mut sys = System::new_all();
    sys.refresh_all();
    let unknown = t!("info.unknown").to_string();

    let mut rows = os_and_cpu_rows(&sys, &unknown);
    rows.extend(memory_and_storage_rows(&sys));
    rows.extend(uptime_and_environment_rows(&unknown));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_bytes_scales_units() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1024), "1.0 KiB");
        assert_eq!(human_bytes(1024 * 1024), "1.0 MiB");
        assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.0 GiB");
    }

    #[test]
    fn human_uptime_formats_ranges() {
        assert_eq!(human_uptime(0), "0m");
        assert_eq!(human_uptime(90), "1m");
        assert_eq!(human_uptime(3 * 3600 + 25 * 60), "3h 25m");
        assert_eq!(human_uptime(2 * 86_400 + 3600), "2d 1h 0m");
    }

    #[test]
    fn snapshot_has_headings_and_values() {
        let p = Panel::open();
        assert!(!p.is_empty(), "a snapshot produces rows");
        assert!(
            p.rows.iter().any(super::Row::is_heading),
            "has section headings"
        );
        // Never assert on translated text directly (the active locale is
        // process-global and races other tests) -- compare against the same
        // t! call the code under test made.
        let logical_cores_label = t!("info.logical_cores").to_string();
        assert!(
            p.rows
                .iter()
                .any(|r| r.label == logical_cores_label && !r.value.is_empty()),
            "reports a logical core count"
        );
    }

    #[test]
    fn navigation_and_scroll_clamp() {
        let mut p = Panel::open();
        let last = p.len() - 1;
        p.up();
        assert_eq!(p.selected, 0, "up at the top stays put");
        p.page_down(10_000);
        assert_eq!(p.selected, last, "page down clamps to the last row");
        p.down();
        assert_eq!(p.selected, last, "down at the bottom stays put");
        p.ensure_visible(5);
        assert!(p.scroll <= last && last < p.scroll + 5);
        p.page_up(10_000);
        assert_eq!(p.selected, 0);
        p.ensure_visible(5);
        assert_eq!(p.scroll, 0);
    }

    #[test]
    fn select_index_guards_range() {
        let mut p = Panel::open();
        assert!(p.select_index(0));
        assert!(!p.select_index(p.len()));
    }
}
