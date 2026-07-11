//! Live workspace metrics for the dashboard overlay (Tools → Workspace Dashboard).
//!
//! Pure state. The host fills these fields from background computations — disk
//! usage via `du`, a recursive file count, and the git commit count — and each
//! metric stays `None` until its computation finishes, so the panel can show a
//! "computing…" placeholder. The host owns the threads and rendering.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Workspace metrics shown in the dashboard. Each `Option` is `None` while its
/// background computation is still running.
pub struct Dashboard {
    /// Top-level workspace folder name.
    pub folder: String,
    /// Human-readable disk usage (e.g. `12M`), from `du`.
    pub disk_usage: Option<String>,
    /// Number of files under the workspace root.
    pub file_count: Option<u64>,
    /// Number of commits reachable from HEAD (`None` also when not a git repo).
    pub commit_count: Option<u64>,
}

impl Dashboard {
    /// A dashboard for `folder` with every metric still pending.
    #[must_use]
    pub fn new(folder: impl Into<String>) -> Self {
        Dashboard {
            folder: folder.into(),
            disk_usage: None,
            file_count: None,
            commit_count: None,
        }
    }

    /// Whether every metric has finished computing.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.disk_usage.is_some() && self.file_count.is_some() && self.commit_count.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_pending_and_completes_when_filled() {
        let mut d = Dashboard::new("vix");
        assert_eq!(d.folder, "vix");
        assert!(!d.is_complete());
        d.disk_usage = Some("12M".into());
        d.file_count = Some(42);
        assert!(!d.is_complete(), "commit count still pending");
        d.commit_count = Some(7);
        assert!(d.is_complete());
    }
}
