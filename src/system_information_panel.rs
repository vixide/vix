#![allow(clippy::pedantic)] // folded subcrate: kept at its original (non-pedantic) lint level
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

#![forbid(unsafe_code)]
#![deny(missing_docs)]

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
        Row { label: label.to_string(), value: String::new() }
    }

    fn pair(label: &str, value: impl Into<String>) -> Self {
        Row { label: label.to_string(), value: value.into() }
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
        Panel { rows: gather(), selected: 0, scroll: 0 }
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
        self.selected = self.selected.saturating_sub(1);
    }

    /// Move the highlight down one row, stopping at the bottom.
    pub fn down(&mut self) {
        if self.selected + 1 < self.rows.len() {
            self.selected += 1;
        }
    }

    /// Move the highlight up one page (`page` rows), stopping at the top.
    pub fn page_up(&mut self, page: usize) {
        self.selected = self.selected.saturating_sub(page.max(1));
    }

    /// Move the highlight down one page (`page` rows), stopping at the bottom.
    pub fn page_down(&mut self, page: usize) {
        if !self.rows.is_empty() {
            self.selected = (self.selected + page.max(1)).min(self.rows.len() - 1);
        }
    }

    /// Select a row directly (e.g. from a click); returns whether `idx` was real.
    pub fn select_index(&mut self, idx: usize) -> bool {
        if idx < self.rows.len() {
            self.selected = idx;
            true
        } else {
            false
        }
    }

    /// Adjust [`scroll`](Self::scroll) so the highlighted row stays within a
    /// window of `height` visible rows.
    pub fn ensure_visible(&mut self, height: usize) {
        let height = height.max(1);
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + height {
            self.scroll = self.selected + 1 - height;
        }
        let max_scroll = self.rows.len().saturating_sub(height);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }
    }

    /// The highlighted row's value (empty for a section heading).
    #[must_use]
    pub fn selected_value(&self) -> String {
        self.rows.get(self.selected).map(|r| r.value.clone()).unwrap_or_default()
    }
}

/// Format a byte count as a human-readable size (`16.0 GiB`).
#[must_use]
pub fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
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
        if let Ok(v) = std::env::var(key) {
            if !v.is_empty() {
                return v;
            }
        }
    }
    fallback.to_string()
}

/// Gather a fresh snapshot of host system information into display rows.
#[must_use]
#[allow(clippy::vec_init_then_push)] // rows are appended conditionally below
pub fn gather() -> Vec<Row> {
    let mut sys = System::new_all();
    sys.refresh_all();
    let unknown = "unknown";
    let mut rows = Vec::new();

    rows.push(Row::heading("Operating System"));
    rows.push(Row::pair("Name", System::name().unwrap_or_else(|| unknown.into())));
    rows.push(Row::pair("Version", System::long_os_version().unwrap_or_else(|| unknown.into())));
    rows.push(Row::pair("Kernel", System::kernel_version().unwrap_or_else(|| unknown.into())));
    rows.push(Row::pair("Hostname", System::host_name().unwrap_or_else(|| unknown.into())));
    rows.push(Row::pair("Architecture", std::env::consts::ARCH));

    rows.push(Row::heading("CPU"));
    if let Some(cpu) = sys.cpus().first() {
        let brand = cpu.brand().trim();
        if !brand.is_empty() {
            rows.push(Row::pair("Model", brand));
        }
        rows.push(Row::pair("Vendor", cpu.vendor_id()));
    }
    if let Some(physical) = sys.physical_core_count() {
        rows.push(Row::pair("Physical cores", physical.to_string()));
    }
    rows.push(Row::pair("Logical cores", sys.cpus().len().to_string()));

    rows.push(Row::heading("Memory"));
    rows.push(Row::pair("Total RAM", human_bytes(sys.total_memory())));
    rows.push(Row::pair("Used RAM", human_bytes(sys.used_memory())));
    rows.push(Row::pair("Available RAM", human_bytes(sys.available_memory())));
    rows.push(Row::pair("Total swap", human_bytes(sys.total_swap())));
    rows.push(Row::pair("Used swap", human_bytes(sys.used_swap())));

    let disks = Disks::new_with_refreshed_list();
    if !disks.is_empty() {
        rows.push(Row::heading("Storage"));
        for disk in &disks {
            let mount = disk.mount_point().display().to_string();
            let value = format!(
                "{} free of {}",
                human_bytes(disk.available_space()),
                human_bytes(disk.total_space()),
            );
            rows.push(Row::pair(&mount, value));
        }
    }

    rows.push(Row::heading("Uptime"));
    rows.push(Row::pair("System uptime", human_uptime(System::uptime())));
    let load = System::load_average();
    rows.push(Row::pair(
        "Load average",
        format!("{:.2} / {:.2} / {:.2} (1/5/15 min)", load.one, load.five, load.fifteen),
    ));

    rows.push(Row::heading("Environment"));
    rows.push(Row::pair("User", env_or(&["USER", "USERNAME"], unknown)));
    rows.push(Row::pair("Home", env_or(&["HOME", "USERPROFILE"], unknown)));
    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| unknown.into());
    rows.push(Row::pair("Working dir", cwd));
    rows.push(Row::pair("Shell", env_or(&["SHELL", "COMSPEC"], unknown)));

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
        assert!(p.rows.iter().any(|r| r.is_heading()), "has section headings");
        assert!(
            p.rows.iter().any(|r| r.label == "Logical cores" && !r.value.is_empty()),
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
