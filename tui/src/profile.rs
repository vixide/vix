//! An opt-in frame profiler for the event loop: `VIX_PROFILE=<file> vix …`.
//!
//! When the `VIX_PROFILE` environment variable names a file, the terminal
//! event loop times every tick, every frame it draws, every input event it
//! handles, and the latency from reading an input event to finishing the
//! frame that shows its result (what a user waits for). On exit it writes a
//! plain-text summary to that file: per phase the count, median, 95th and
//! 99th percentile and maximum, how many inputs took longer than one 60 Hz
//! frame (16 ms) to reach the screen, and the slowest inputs. With the
//! variable unset nothing is timed. See `docs/performance/index.md`.

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

use crate::geo::Geo;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// The environment variable that turns profiling on and names the report file.
pub const ENV: &str = "VIX_PROFILE";

/// How long an input may take to reach the screen before it counts as slow:
/// one frame at 60 Hz.
pub const BUDGET: Duration = Duration::from_millis(16);

/// Samples kept per phase; later samples are counted but not kept, so a
/// long session cannot grow without bound.
const MAX_SAMPLES: usize = 1_000_000;

/// How many of the slowest inputs the report lists.
const SLOWEST: usize = 10;

/// What a sample timed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// `App::tick`: background work polled before each frame.
    Tick,
    /// Drawing one frame.
    Draw,
    /// Handling one input event (key, mouse, paste, focus).
    Input,
    /// From reading an input event to the end of the frame drawn after it.
    InputToFrame,
}

impl Phase {
    const ALL: [Phase; 4] = [Phase::InputToFrame, Phase::Input, Phase::Draw, Phase::Tick];

    fn index(self) -> usize {
        match self {
            Phase::Tick => 0,
            Phase::Draw => 1,
            Phase::Input => 2,
            Phase::InputToFrame => 3,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Phase::Tick => "tick",
            Phase::Draw => "draw",
            Phase::Input => "input handling",
            Phase::InputToFrame => "input to frame",
        }
    }
}

/// Summary statistics over one phase's samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    /// Samples taken (including any past the keep limit).
    pub count: u64,
    /// Median.
    pub p50: Duration,
    /// 95th percentile.
    pub p95: Duration,
    /// 99th percentile.
    pub p99: Duration,
    /// Slowest sample.
    pub max: Duration,
}

/// Percentile statistics for `micros` (each sample in microseconds), with
/// `count` the total number taken. Percentiles use the nearest-rank method.
#[must_use]
pub fn stats(micros: &[u32], count: u64) -> Stats {
    if micros.is_empty() {
        return Stats {
            count,
            ..Stats::default()
        };
    }
    let mut sorted = micros.to_vec();
    sorted.sort_unstable();
    let rank = |p: usize| {
        // Nearest rank: the smallest value with at least p% of samples at or
        // below it.
        let i = (p.times(sorted.len()).div_ceil(100).max(1)).minus(1);
        Duration::from_micros(u64::from(sorted[i]))
    };
    Stats {
        count,
        p50: rank(50),
        p95: rank(95),
        p99: rank(99),
        max: Duration::from_micros(u64::from(sorted[sorted.len().minus(1)])),
    }
}

/// The event-loop profiler. Build it with [`Profiler::from_env`]; it is `None`
/// (and costs nothing) unless `VIX_PROFILE` is set.
#[derive(Debug)]
pub struct Profiler {
    out: PathBuf,
    started: Instant,
    samples: [Vec<u32>; 4],
    counts: [u64; 4],
    /// The slowest inputs to reach the screen: (latency in µs, seconds into
    /// the session).
    slowest: Vec<(u32, f64)>,
    /// When the input waiting for its frame was read.
    pending_input: Option<Instant>,
}

impl Profiler {
    /// A profiler writing to the file `VIX_PROFILE` names, if it is set and
    /// not empty.
    #[must_use]
    pub fn from_env() -> Option<Profiler> {
        std::env::var_os(ENV)
            .filter(|v| !v.is_empty())
            .map(|v| Profiler::new(PathBuf::from(v)))
    }

    /// A profiler that will write its report to `out`.
    #[must_use]
    pub fn new(out: PathBuf) -> Profiler {
        Profiler {
            out,
            started: Instant::now(),
            samples: Default::default(),
            counts: [0; 4],
            slowest: Vec::new(),
            pending_input: None,
        }
    }

    /// Record that `phase` took `took`.
    pub fn record(&mut self, phase: Phase, took: Duration) {
        let i = phase.index();
        if let Some(count) = self.counts.get_mut(i) {
            *count = count.plus(1);
        }
        let micros = u32::try_from(took.as_micros()).unwrap_or(u32::MAX);
        if self.samples[i].len() < MAX_SAMPLES {
            self.samples[i].push(micros);
        }
        if phase == Phase::InputToFrame {
            let at = self.started.elapsed().as_secs_f64();
            self.slowest.push((micros, at));
            self.slowest
                .sort_unstable_by_key(|s| std::cmp::Reverse(s.0));
            self.slowest.truncate(SLOWEST);
        }
    }

    /// Mark that an input event was just read (call before handling it).
    /// Only the first input before a frame starts the clock, so a burst of
    /// keys handled before one redraw counts from its first key.
    pub fn input_read(&mut self, at: Instant) {
        self.pending_input.get_or_insert(at);
    }

    /// Mark that a frame finished drawing at `at`, closing any pending
    /// input's latency.
    pub fn frame_drawn(&mut self, at: Instant) {
        if let Some(start) = self.pending_input.take() {
            self.record(Phase::InputToFrame, at.saturating_duration_since(start));
        }
    }

    /// The summary report.
    #[must_use]
    pub fn report(&self) -> String {
        let mut out = String::new();
        let session = self.started.elapsed();
        let _ = writeln!(out, "Vix frame profile ({ENV})");
        let _ = writeln!(out, "session: {:.1} s", session.as_secs_f64());
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "{:<16} {:>9} {:>10} {:>10} {:>10} {:>10}",
            "phase", "count", "p50 ms", "p95 ms", "p99 ms", "max ms"
        );
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        for phase in Phase::ALL {
            let i = phase.index();
            let s = stats(&self.samples[i], self.counts[i]);
            let _ = writeln!(
                out,
                "{:<16} {:>9} {:>10.2} {:>10.2} {:>10.2} {:>10.2}",
                phase.label(),
                s.count,
                ms(s.p50),
                ms(s.p95),
                ms(s.p99),
                ms(s.max)
            );
        }
        let latencies = &self.samples[Phase::InputToFrame.index()];
        let budget = u32::try_from(BUDGET.as_micros()).unwrap_or(u32::MAX);
        let slow = latencies.iter().filter(|&&m| m > budget).count();
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "inputs slower than {} ms to reach the screen: {slow} of {}",
            BUDGET.as_millis(),
            latencies.len()
        );
        if !self.slowest.is_empty() {
            let _ = writeln!(out);
            let _ = writeln!(out, "slowest inputs (ms, at s into the session):");
            for (micros, at) in &self.slowest {
                let _ = writeln!(out, "  {:>9.2}  at {at:.1} s", f64::from(*micros) / 1000.0);
            }
        }
        if self
            .counts
            .iter()
            .zip(&self.samples)
            .any(|(c, s)| *c > s.len() as u64)
        {
            let _ = writeln!(
                out,
                "\n(only the first {MAX_SAMPLES} samples per phase are in the percentiles)"
            );
        }
        out
    }

    /// Write the report to the file `VIX_PROFILE` named.
    ///
    /// # Errors
    ///
    /// Returns the I/O error if the file cannot be written.
    pub fn finish(&self) -> std::io::Result<PathBuf> {
        std::fs::write(&self.out, self.report())?;
        Ok(self.out.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_use_nearest_rank_percentiles() {
        let micros: Vec<u32> = (1..=100).collect();
        let s = stats(&micros, 100);
        assert_eq!(s.count, 100);
        assert_eq!(s.p50, Duration::from_micros(50));
        assert_eq!(s.p95, Duration::from_micros(95));
        assert_eq!(s.p99, Duration::from_micros(99));
        assert_eq!(s.max, Duration::from_micros(100));
        let one = stats(&[7], 1);
        assert_eq!(
            (one.p50, one.p99, one.max),
            (
                Duration::from_micros(7),
                Duration::from_micros(7),
                Duration::from_micros(7)
            )
        );
        assert_eq!(stats(&[], 0), Stats::default());
        assert_eq!(stats(&[3, 1, 2], 5).count, 5);
        assert_eq!(stats(&[3, 1, 2], 3).p50, Duration::from_micros(2));
    }

    #[test]
    fn input_latency_runs_from_the_first_input_to_the_next_frame() {
        let mut p = Profiler::new(PathBuf::from("unused"));
        let t0 = Instant::now();
        p.input_read(t0);
        p.input_read(t0 + Duration::from_millis(5)); // same burst
        p.frame_drawn(t0 + Duration::from_millis(20));
        p.frame_drawn(t0 + Duration::from_millis(40)); // no input pending
        assert_eq!(p.counts[Phase::InputToFrame.index()], 1);
        assert_eq!(p.samples[Phase::InputToFrame.index()], vec![20_000]);
    }

    #[test]
    fn report_counts_slow_inputs_and_lists_the_slowest() {
        let mut p = Profiler::new(PathBuf::from("unused"));
        for ms in [1, 2, 30, 5, 17] {
            p.record(Phase::InputToFrame, Duration::from_millis(ms));
        }
        p.record(Phase::Draw, Duration::from_millis(3));
        let r = p.report();
        assert!(
            r.contains("inputs slower than 16 ms to reach the screen: 2 of 5"),
            "{r}"
        );
        assert!(r.contains("draw"), "{r}");
        let slowest = r.split("slowest inputs").nth(1).unwrap();
        let first = slowest.lines().nth(1).unwrap();
        assert!(first.trim_start().starts_with("30.00"), "{r}");
        assert_eq!(p.slowest.len(), 5);
    }

    #[test]
    fn slowest_keeps_only_the_top_ten() {
        let mut p = Profiler::new(PathBuf::from("unused"));
        for ms in 1..=30 {
            p.record(Phase::InputToFrame, Duration::from_millis(ms));
        }
        assert_eq!(p.slowest.len(), SLOWEST);
        assert_eq!(p.slowest[0].0, 30_000);
        assert_eq!(p.slowest[SLOWEST - 1].0, 21_000);
    }

    #[test]
    fn finish_writes_the_report() {
        let path = std::env::temp_dir().join(format!("vix-profile-{}.txt", std::process::id()));
        let mut p = Profiler::new(path.clone());
        p.record(Phase::Tick, Duration::from_micros(10));
        assert_eq!(p.finish().unwrap(), path);
        assert!(std::fs::read_to_string(&path).unwrap().contains("tick"));
        let _ = std::fs::remove_file(&path);
    }
}
