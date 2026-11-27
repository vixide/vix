//! How a file's size decides the way it opens (T830): straight into an editor
//! buffer, after a question, after a question with a memory warning, or in
//! the Large File Viewer with no question at all.
//!
//! A pure function of the size and the limits, so the host can test every
//! branch without writing a 500 MB file. See this crate's `spec/index.md`
//! ("Size thresholds").

/// Above this many bytes (500 MiB), the question before loading a file into
/// an editor buffer carries a memory warning, whatever the user's
/// `large_file_mb` limit is (even `0`, "never ask").
pub const WARN_BYTES: u64 = 524_288_000;

/// Above this many bytes (1 GiB, the ceiling an editor buffer may hold), a
/// file opens read-only in the Large File Viewer without asking.
pub const VIEWER_BYTES: u64 = 1_073_741_824;

/// What opening a file of a given size should do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum OpenPlan {
    /// Load it into an editor buffer.
    Load,
    /// Ask first: it is over the user's `large_file_mb` limit.
    Ask,
    /// Ask first, with a memory warning: it is over [`Thresholds::warn`].
    Warn,
    /// Open it in the Large File Viewer: it is over [`Thresholds::viewer`].
    Viewer,
}

/// The two fixed size thresholds. [`Thresholds::default`] is
/// [`WARN_BYTES`] and [`VIEWER_BYTES`]; tests inject small ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Thresholds {
    /// Above this many bytes, asking carries a memory warning.
    pub warn: u64,
    /// Above this many bytes, the file goes to the Large File Viewer.
    pub viewer: u64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            warn: WARN_BYTES,
            viewer: VIEWER_BYTES,
        }
    }
}

impl Thresholds {
    /// The largest file an open may load without asking, given the user's
    /// limit `ask` in bytes (`0`: no limit of their own): the smaller of
    /// `ask` and [`Thresholds::warn`].
    #[must_use]
    pub fn load_limit(self, ask: u64) -> u64 {
        match ask {
            0 => self.warn,
            n => n.min(self.warn),
        }
    }

    /// What opening a file of `size` bytes should do, given the user's
    /// limit `ask` in bytes (`0`: no limit of their own).
    #[must_use]
    pub fn plan(self, size: u64, ask: u64) -> OpenPlan {
        if size > self.viewer {
            OpenPlan::Viewer
        } else if size > self.warn {
            OpenPlan::Warn
        } else if ask != 0 && size > ask {
            OpenPlan::Ask
        } else {
            OpenPlan::Load
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{OpenPlan, Thresholds, VIEWER_BYTES, WARN_BYTES};

    const MIB: u64 = 1024 * 1024;

    #[test]
    fn the_constants_are_500_mib_and_1_gib() {
        assert_eq!(WARN_BYTES, 500 * MIB);
        assert_eq!(VIEWER_BYTES, 1024 * MIB);
        const { assert!(WARN_BYTES < VIEWER_BYTES) };
    }

    #[test]
    fn every_band_of_sizes_has_its_plan() {
        let t = Thresholds::default();
        let ask = 50 * MIB;
        assert_eq!(t.plan(0, ask), OpenPlan::Load);
        assert_eq!(t.plan(ask, ask), OpenPlan::Load, "at the limit loads");
        assert_eq!(t.plan(ask + 1, ask), OpenPlan::Ask);
        assert_eq!(t.plan(WARN_BYTES, ask), OpenPlan::Ask);
        assert_eq!(t.plan(WARN_BYTES + 1, ask), OpenPlan::Warn);
        assert_eq!(t.plan(VIEWER_BYTES, ask), OpenPlan::Warn);
        assert_eq!(t.plan(VIEWER_BYTES + 1, ask), OpenPlan::Viewer);
        assert_eq!(t.plan(u64::MAX, ask), OpenPlan::Viewer);
    }

    #[test]
    fn no_limit_of_ones_own_still_warns_and_still_degrades() {
        let t = Thresholds::default();
        assert_eq!(t.plan(400 * MIB, 0), OpenPlan::Load);
        assert_eq!(t.plan(600 * MIB, 0), OpenPlan::Warn);
        assert_eq!(t.plan(2048 * MIB, 0), OpenPlan::Viewer);
        // A limit above the warning threshold changes nothing past it.
        assert_eq!(t.plan(600 * MIB, 800 * MIB), OpenPlan::Warn);
    }

    #[test]
    fn the_load_limit_is_the_smaller_of_the_two() {
        let t = Thresholds::default();
        assert_eq!(t.load_limit(0), WARN_BYTES);
        assert_eq!(t.load_limit(50 * MIB), 50 * MIB);
        assert_eq!(t.load_limit(800 * MIB), WARN_BYTES);
        let small = Thresholds {
            warn: 10,
            viewer: 20,
        };
        assert_eq!(small.load_limit(0), 10);
        assert_eq!(small.plan(15, 0), OpenPlan::Warn);
        assert_eq!(small.plan(21, 0), OpenPlan::Viewer);
    }
}
