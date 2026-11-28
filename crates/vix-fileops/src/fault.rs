//! Save-failure injection (T865), for tests only.
//!
//! The save path calls [`hit`] at each step that can fail on a real disk:
//! writing and syncing the temp file, renaming it over the target, taking the
//! recovery copy and writing in place. Normally every call is `Ok`. With the
//! `fault-injection` feature (enabled only by dev-dependencies, like
//! `isolated-user-dirs`) a test arms a fault with [`inject`]; each hit of that
//! step on the same thread then fails until the returned guard is dropped.
//! Thread-local, so parallel tests cannot trip each other's saves.

use std::io;

/// A step of a save that a test can make fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SaveFault {
    /// Writing the replacement data to the temp file (a full disk).
    TempWrite,
    /// Flushing the temp file to disk (`fsync` failed).
    TempSync,
    /// Renaming the temp file over the target (the rename is refused).
    Rename,
    /// Taking the recovery copy before an in-place write (no usable
    /// recovery directory).
    Backup,
    /// Writing in place, after the target was truncated (a full disk).
    InPlaceWrite,
}

impl SaveFault {
    /// This fault's bit in the thread's armed set.
    #[cfg(any(test, feature = "fault-injection"))]
    const fn bit(self) -> u8 {
        match self {
            Self::TempWrite => 1,
            Self::TempSync => 2,
            Self::Rename => 4,
            Self::Backup => 8,
            Self::InPlaceWrite => 16,
        }
    }

    /// The error an injected failure of this step reports.
    #[cfg(any(test, feature = "fault-injection"))]
    fn error(self) -> io::Error {
        let kind = match self {
            Self::TempWrite | Self::InPlaceWrite => io::ErrorKind::StorageFull,
            Self::TempSync => io::ErrorKind::Other,
            Self::Rename | Self::Backup => io::ErrorKind::PermissionDenied,
        };
        io::Error::new(kind, format!("injected failure: {self:?}"))
    }
}

#[cfg(any(test, feature = "fault-injection"))]
thread_local! {
    /// The faults armed on this thread, as `SaveFault::bit`s.
    static ARMED: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

/// Arm `fault` on this thread until the returned guard is dropped.
#[cfg(any(test, feature = "fault-injection"))]
#[must_use = "the fault is disarmed when the guard is dropped"]
pub fn inject(fault: SaveFault) -> Injected {
    ARMED.with(|a| a.set(a.get() | fault.bit()));
    Injected { fault }
}

/// An armed fault; dropping it disarms the fault.
#[cfg(any(test, feature = "fault-injection"))]
#[derive(Debug)]
pub struct Injected {
    /// The armed fault.
    fault: SaveFault,
}

#[cfg(any(test, feature = "fault-injection"))]
impl Drop for Injected {
    fn drop(&mut self) {
        ARMED.with(|a| a.set(a.get() & !self.fault.bit()));
    }
}

/// `Err` when `fault` is armed on this thread, else `Ok`.
#[cfg(any(test, feature = "fault-injection"))]
pub(crate) fn hit(fault: SaveFault) -> io::Result<()> {
    if ARMED.with(std::cell::Cell::get) & fault.bit() == 0 {
        Ok(())
    } else {
        Err(fault.error())
    }
}

/// Without injection every step goes ahead.
#[cfg(not(any(test, feature = "fault-injection")))]
#[expect(
    clippy::unnecessary_wraps,
    reason = "the same signature as the injecting build's `hit`"
)]
pub(crate) const fn hit(_fault: SaveFault) -> io::Result<()> {
    Ok(())
}
