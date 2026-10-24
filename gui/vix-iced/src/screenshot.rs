//! The scripted render smoke test (`crate::startup::Script`): wait for
//! startup, run actions, type keystrokes through the same event handling
//! real input takes, capture the window with iced's `window::screenshot`,
//! save a PNG, and quit.

use std::time::{Duration, Instant};

use crate::startup::Script;
use iced::keyboard;
use iced::window::Screenshot;
use vix::app::App;

/// How often a scripted run is stepped.
pub const STEP: Duration = Duration::from_millis(100);

/// Where a scripted run is up to.
enum Stage {
    /// Waiting for startup to settle.
    Waiting,
    /// Actions run and keys typed; waiting for the frames to show them.
    Typed(Instant),
    /// Screenshot requested; waiting for it to arrive.
    Capturing,
}

/// A scripted run in progress.
pub struct Run {
    script: Script,
    started: Instant,
    stage: Stage,
}

/// What one step asks of the window.
pub enum Step {
    /// Nothing yet.
    Wait,
    /// Handle these scripted key presses now.
    Type(Vec<keyboard::Event>),
    /// Capture the window.
    Capture,
}

impl Run {
    /// Start a scripted run (once the App has started).
    #[must_use]
    pub fn new(script: Script) -> Self {
        Self {
            script,
            started: Instant::now(),
            stage: Stage::Waiting,
        }
    }

    /// Advance the run; call every [`STEP`].
    pub fn step(&mut self, app: &mut App) -> Step {
        match self.stage {
            Stage::Waiting if self.started.elapsed() > Duration::from_secs(2) => {
                for id in &self.script.actions {
                    app.run_action(id);
                }
                let keys = self
                    .script
                    .keys
                    .iter()
                    .filter_map(|k| crate::keys::scripted(k))
                    .collect();
                self.stage = Stage::Typed(Instant::now());
                Step::Type(keys)
            }
            Stage::Typed(at) if at.elapsed() > Duration::from_secs(1) => {
                self.stage = Stage::Capturing;
                Step::Capture
            }
            _ => Step::Wait,
        }
    }

    /// Save a captured window as the run's PNG.
    pub fn save(&self, shot: &Screenshot) {
        let path = &self.script.path;
        let result = image::save_buffer(
            path,
            &shot.rgba,
            shot.size.width,
            shot.size.height,
            image::ExtendedColorType::Rgba8,
        );
        if let Err(e) = result {
            eprintln!("vix-iced: could not save {path}: {e}");
        }
    }
}
