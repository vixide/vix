//! A Pomodoro work/break countdown as a small state machine.
//!
//! The timer has three phases: **Idle** (set the work length, default 25
//! minutes), **Work** (counting down; reaching zero starts the break), and
//! **Break** (a 5-minute countdown shown as an alert; reaching zero, or
//! cancelling, returns to Idle). Time advances only through [`Timer::tick`],
//! which the host calls with the number of whole seconds elapsed since the last
//! tick. Keeping the clock out of this crate makes every transition
//! deterministic and unit-testable.

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

/// Default work length, in minutes.
pub const DEFAULT_WORK_MINUTES: u64 = 25;
/// Break length, in minutes.
pub const BREAK_MINUTES: u64 = 5;

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
    /// Configured work length, in minutes (1–180).
    pub work_minutes: u64,
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

    /// Change the work length by `delta` minutes, clamped to 1–180. Only allowed
    /// while idle (a running countdown ignores it).
    pub fn adjust_minutes(&mut self, delta: i64) {
        if self.phase == Phase::Idle {
            let current = i64::try_from(self.work_minutes).unwrap_or(i64::MAX);
            let adjusted = current.saturating_add(delta).clamp(1, 180);
            self.work_minutes = u64::try_from(adjusted).unwrap_or(1);
        }
    }

    /// Start the work countdown from the configured length.
    pub fn start(&mut self) {
        self.phase = Phase::Work;
        self.remaining = self.work_minutes * 60;
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
    pub fn tick(&mut self, secs: u64) -> Tick {
        match self.phase {
            Phase::Idle => Tick::None,
            Phase::Work => {
                if secs >= self.remaining {
                    // Work finished: roll into the break.
                    self.phase = Phase::Break;
                    self.remaining = BREAK_MINUTES * 60;
                    Tick::BreakStarted
                } else {
                    self.remaining -= secs;
                    Tick::None
                }
            }
            Phase::Break => {
                if secs >= self.remaining {
                    self.stop();
                    Tick::Finished
                } else {
                    self.remaining -= secs;
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
            Phase::Idle => self.work_minutes * 60,
            _ => self.remaining,
        }
    }

    /// The display formatted as `MM:SS`.
    #[must_use]
    pub fn label(&self) -> String {
        let s = self.display_seconds();
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        t.adjust_minutes(5);
        assert_eq!(t.work_minutes, 30);
        t.adjust_minutes(-100); // clamps to 1
        assert_eq!(t.work_minutes, 1);
        t.start();
        t.adjust_minutes(10); // ignored while running
        assert_eq!(t.work_minutes, 1);
    }

    #[test]
    fn start_then_tick_counts_down() {
        let mut t = Timer::new();
        t.adjust_minutes(-24); // 1 minute
        t.start();
        assert_eq!(t.label(), "01:00");
        assert_eq!(t.tick(20), Tick::None);
        assert_eq!(t.label(), "00:40");
    }

    #[test]
    fn work_zero_starts_break() {
        let mut t = Timer::new();
        t.adjust_minutes(-24); // 1 minute work
        t.start();
        assert_eq!(t.tick(60), Tick::BreakStarted);
        assert!(t.is_break());
        assert_eq!(t.label(), "05:00");
    }

    #[test]
    fn break_zero_finishes_and_resets() {
        let mut t = Timer::new();
        t.adjust_minutes(-24); // 1 minute work
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
