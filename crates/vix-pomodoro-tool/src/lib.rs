//! A Pomodoro work/break countdown as a small state machine.
//!
//! The timer has three phases: **Idle** (set the work length, default 25
//! minutes), **Work** (counting down; reaching zero starts the break), and
//! **Break** (a 5-minute countdown shown as an alert; reaching zero, or
//! cancelling, returns to Idle). Time advances only through [`Timer::tick`],
//! which the host calls with the number of whole seconds elapsed since the last
//! tick. Keeping the clock out of this crate makes every transition
//! deterministic and unit-testable.
//!
//! State diagram: `Idle --start--> Work --(remaining == 0)--> Break
//! --(remaining == 0)--> Idle`; [`Timer::stop`] returns to `Idle` from any phase.
//! The host owns the wall clock, the bell, and rendering (it shows
//! [`Timer::label`], and an alert while [`Timer::is_break`]).
//!
//! Spec: `crates/vix-pomodoro-tool/spec/index.md`.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

/// Default work length, in minutes.
pub const DEFAULT_WORK_MINUTES: u64 = 25;
/// Break length, in minutes.
pub const BREAK_MINUTES: u64 = 5;

/// The allowed work length, in minutes: the length is refused outside it.
const MIN_WORK_MINUTES: i64 = 1;
const MAX_WORK_MINUTES: i64 = 180;

/// Minutes as seconds.
fn minutes_to_secs(minutes: u64) -> u64 {
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "RCA-EXC-2: the work length is kept within 1..=180 minutes and the break is 5, so the product is tiny"
    )]
    let secs = minutes * 60;
    secs
}

/// The countdown after `secs` more seconds; only called with `secs < remaining`.
fn remaining_after(remaining: u64, secs: u64) -> u64 {
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "RCA-EXC-2: the caller checked secs < remaining, so the difference is positive"
    )]
    let left = remaining - secs;
    left
}

/// Why a work-length adjustment was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TimerError {
    /// The step added to the current length does not fit `i64`.
    Overflow {
        /// The figure that overflowed.
        figure: &'static str,
    },
    /// The new length is outside the allowed range.
    OutOfRange {
        /// The figure that is out of range.
        figure: &'static str,
        /// The valid range.
        range: &'static str,
    },
}

impl std::fmt::Display for TimerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Overflow { figure } => write!(f, "{figure}: the step does not fit"),
            Self::OutOfRange { figure, range } => write!(f, "{figure}: must be in {range}"),
        }
    }
}

impl std::error::Error for TimerError {}

/// The timer's current phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Stopped; the user can set the work length and start.
    Idle,
    /// Counting down the work interval.
    Work,
    /// Counting down the break interval (shown as an alert).
    Break,
}

/// What a [`Timer::tick`] caused, so the host can react (e.g. ring a bell).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    /// Nothing notable; the countdown advanced (or the timer is idle).
    None,
    /// The work interval just hit zero and the break began.
    BreakStarted,
    /// The break interval just hit zero and the timer returned to idle.
    Finished,
}

/// A Pomodoro timer.
#[derive(Debug, Clone)]
pub struct Timer {
    /// Configured work length, in minutes (1–180). Kept in range by
    /// [`Timer::adjust_minutes`]; writing the field directly bypasses the clamp.
    work_minutes: u64,
    /// The current phase.
    pub phase: Phase,
    /// Seconds remaining in the current Work/Break countdown (0 while Idle shows
    /// the configured work length instead).
    pub remaining: u64,
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

impl Timer {
    /// A new idle timer with the default 25-minute work length.
    #[must_use]
    pub fn new() -> Self {
        Timer {
            work_minutes: DEFAULT_WORK_MINUTES,
            phase: Phase::Idle,
            remaining: 0,
        }
    }

    /// The configured work length, in minutes (always within 1 to 180).
    #[must_use]
    pub fn work_minutes(&self) -> u64 {
        self.work_minutes
    }

    /// Change the work length by `delta` minutes. Only allowed while idle (a
    /// running countdown ignores it). A new length outside 1 to 180 minutes is
    /// refused, never moved to the nearest end.
    ///
    /// # Errors
    /// [`TimerError::Overflow`] if adding `delta` to the current length does not
    /// fit `i64`, and [`TimerError::OutOfRange`] if the new length is outside 1 to
    /// 180; the length is left unchanged either way.
    ///
    /// # Examples
    /// ```
    /// let mut t = vix_pomodoro_tool::Timer::new();
    /// t.adjust_minutes(5).unwrap();
    /// assert_eq!(t.work_minutes(), 30);
    /// assert!(t.adjust_minutes(-1000).is_err()); // below the 1-minute floor
    /// assert_eq!(t.work_minutes(), 30); // unchanged
    /// assert!(t.adjust_minutes(1000).is_err()); // above the 180-minute ceiling
    /// assert!(t.adjust_minutes(i64::MAX).is_err()); // 30 + i64::MAX does not fit
    /// ```
    pub fn adjust_minutes(&mut self, delta: i64) -> Result<(), TimerError> {
        if self.phase != Phase::Idle {
            return Ok(());
        }
        // The stored length is within 1..=180, so it always fits `i64`.
        let current = i64::try_from(self.work_minutes).map_err(|_| TimerError::Overflow {
            figure: "work_minutes",
        })?;
        let sum = current.checked_add(delta).ok_or(TimerError::Overflow {
            figure: "work_minutes",
        })?;
        if !(MIN_WORK_MINUTES..=MAX_WORK_MINUTES).contains(&sum) {
            return Err(TimerError::OutOfRange {
                figure: "work_minutes",
                range: "1 ..= 180",
            });
        }
        self.work_minutes = u64::try_from(sum).map_err(|_| TimerError::Overflow {
            figure: "work_minutes",
        })?;
        Ok(())
    }

    /// Start the work countdown from the configured length. Restarts from the
    /// full length if a countdown (even a break) was already running.
    pub fn start(&mut self) {
        self.phase = Phase::Work;
        self.remaining = minutes_to_secs(self.work_minutes);
    }

    /// Stop the countdown and reset to idle at the configured work length.
    pub fn stop(&mut self) {
        self.phase = Phase::Idle;
        self.remaining = 0;
    }

    /// Whether a countdown is currently running (Work or Break).
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.phase != Phase::Idle
    }

    /// Whether the break alert should be shown.
    #[must_use]
    pub fn is_break(&self) -> bool {
        self.phase == Phase::Break
    }

    /// Advance the countdown by `secs` whole seconds, performing phase
    /// transitions, and report what happened.
    ///
    /// A single tick causes at most one transition: if `secs` overshoots the
    /// end of the work interval, the surplus is discarded and the break starts
    /// from its full length. Ticking while idle does nothing.
    ///
    /// # Examples
    /// ```
    /// use vix_pomodoro_tool::{Tick, Timer};
    ///
    /// let mut t = Timer::new();
    /// t.start();
    /// assert_eq!(t.tick(10), Tick::None);
    /// assert_eq!(t.label(), "24:50");
    /// assert_eq!(t.tick(25 * 60), Tick::BreakStarted);
    /// assert!(t.is_break());
    /// ```
    pub fn tick(&mut self, secs: u64) -> Tick {
        match self.phase {
            Phase::Idle => Tick::None,
            Phase::Work => {
                if secs >= self.remaining {
                    // Work finished: roll into the break. Any overshoot in `secs`
                    // is intentionally dropped rather than carried into the break.
                    self.phase = Phase::Break;
                    self.remaining = minutes_to_secs(BREAK_MINUTES);
                    Tick::BreakStarted
                } else {
                    self.remaining = remaining_after(self.remaining, secs);
                    Tick::None
                }
            }
            Phase::Break => {
                if secs >= self.remaining {
                    // Break finished: back to Idle at the configured work length.
                    self.stop();
                    Tick::Finished
                } else {
                    self.remaining = remaining_after(self.remaining, secs);
                    Tick::None
                }
            }
        }
    }

    /// The seconds shown on the display: the live countdown while running, or the
    /// configured work length while idle.
    #[must_use]
    pub fn display_seconds(&self) -> u64 {
        match self.phase {
            Phase::Idle => minutes_to_secs(self.work_minutes),
            _ => self.remaining,
        }
    }

    /// The display formatted as `MM:SS`. Minutes are not capped at two digits,
    /// so the 180-minute maximum renders as `180:00`.
    #[must_use]
    pub fn label(&self) -> String {
        let s = self.display_seconds();
        #[expect(
            clippy::integer_division,
            reason = "RCA-EXC-2: the divisor is the constant 60"
        )]
        let (minutes, seconds) = (s / 60, s % 60);
        format!("{minutes:02}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phases_report_running_and_break_and_the_break_counts_down() {
        let mut t = Timer::new();
        assert!(!t.is_running());
        t.start();
        assert!(t.is_running());
        assert!(!t.is_break());
        assert_eq!(t.tick(25 * 60), Tick::BreakStarted);
        assert!(t.is_running());
        assert_eq!(t.tick(10), Tick::None);
        assert_eq!(t.display_seconds(), BREAK_MINUTES * 60 - 10);
    }

    #[test]
    fn defaults_to_25_minutes_idle() {
        let t = Timer::new();
        assert_eq!(t.phase, Phase::Idle);
        assert_eq!(t.label(), "25:00");
        assert!(!t.is_running());
    }

    #[test]
    fn adjust_only_while_idle() {
        let mut t = Timer::new();
        t.adjust_minutes(5).unwrap();
        assert_eq!(t.work_minutes(), 30);
        t.adjust_minutes(-29).unwrap();
        assert_eq!(t.work_minutes(), 1);
        t.start();
        t.adjust_minutes(10).unwrap(); // ignored while running
        assert_eq!(t.work_minutes(), 1);
    }

    #[test]
    fn a_length_outside_one_to_180_is_refused_not_clamped() {
        let mut t = Timer::new();
        let out = |figure| TimerError::OutOfRange {
            figure,
            range: "1 ..= 180",
        };
        // One past each end is refused and leaves the length alone.
        assert_eq!(
            t.adjust_minutes(-25),
            Err(out("work_minutes")),
            "25 - 25 = 0"
        );
        assert_eq!(t.work_minutes(), 25);
        assert_eq!(
            t.adjust_minutes(156),
            Err(out("work_minutes")),
            "25 + 156 = 181"
        );
        assert_eq!(t.work_minutes(), 25);
        // The ends themselves are accepted.
        t.adjust_minutes(-24).unwrap();
        assert_eq!(t.work_minutes(), 1);
        t.adjust_minutes(179).unwrap();
        assert_eq!(t.work_minutes(), 180);
        // A step that overflows `i64` is its own error.
        assert_eq!(
            t.adjust_minutes(i64::MAX),
            Err(TimerError::Overflow {
                figure: "work_minutes"
            })
        );
        assert_eq!(t.work_minutes(), 180);
        assert_eq!(
            TimerError::OutOfRange {
                figure: "work_minutes",
                range: "1 ..= 180"
            }
            .to_string(),
            "work_minutes: must be in 1 ..= 180"
        );
        assert_eq!(
            TimerError::Overflow {
                figure: "work_minutes"
            }
            .to_string(),
            "work_minutes: the step does not fit"
        );
    }

    #[test]
    fn start_then_tick_counts_down() {
        let mut t = Timer::new();
        t.adjust_minutes(-24).unwrap(); // 1 minute
        t.start();
        assert_eq!(t.label(), "01:00");
        assert_eq!(t.tick(20), Tick::None);
        assert_eq!(t.label(), "00:40");
    }

    #[test]
    fn work_zero_starts_break() {
        let mut t = Timer::new();
        t.adjust_minutes(-24).unwrap(); // 1 minute work
        t.start();
        assert_eq!(t.tick(60), Tick::BreakStarted);
        assert!(t.is_break());
        assert_eq!(t.label(), "05:00");
    }

    #[test]
    fn break_zero_finishes_and_resets() {
        let mut t = Timer::new();
        t.adjust_minutes(-24).unwrap(); // 1 minute work
        t.start();
        t.tick(60); // into break (5:00)
        assert_eq!(t.tick(5 * 60), Tick::Finished);
        assert_eq!(t.phase, Phase::Idle);
        assert_eq!(t.label(), "01:00"); // back to the configured work length
    }

    #[test]
    fn stop_resets_to_configured_length() {
        let mut t = Timer::new();
        t.start();
        t.tick(30);
        t.stop();
        assert_eq!(t.phase, Phase::Idle);
        assert_eq!(t.label(), "25:00");
    }
}
