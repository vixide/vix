//! The scripted render smoke test (`crate::startup::Script`): wait for
//! startup, run actions, type keystrokes through the same key handling real
//! input takes, capture the window with `Window::take_snapshot`, save a PNG,
//! and quit. Also prints how long a frame takes, for comparing toolkits.

use std::time::{Duration, Instant};

use crate::keys::{self, Mods};
use crate::startup::Script;
use slint::ComponentHandle;
use slint_interpreter::ComponentInstance;
use vix::app::App;

/// Whether Slint reports Control as `meta` here (see `crate::keys`).
const APPLE: bool = cfg!(any(target_os = "macos", target_os = "ios"));

/// Where a scripted run is up to.
enum Stage {
    /// Waiting for startup to settle.
    Waiting,
    /// Actions run and keys typed; waiting for the frames to show them.
    Typed(Instant),
}

/// A scripted run in progress.
pub struct Run {
    script: Script,
    started: Instant,
    stage: Stage,
}

/// What one step asks of the window.
pub struct Step {
    /// Scripted key presses to handle now, as Slint would deliver them.
    pub keys: Vec<(String, Mods)>,
    /// Time to capture the window and quit.
    pub capture: bool,
}

impl Run {
    /// Start a scripted run.
    #[must_use]
    pub fn new(script: Script) -> Self {
        Self {
            script,
            started: Instant::now(),
            stage: Stage::Waiting,
        }
    }

    /// Advance the run; call on every timer tick.
    pub fn step(&mut self, app: &mut App) -> Step {
        let mut step = Step {
            keys: Vec::new(),
            capture: false,
        };
        match self.stage {
            Stage::Waiting if self.started.elapsed() > Duration::from_secs(2) => {
                for id in &self.script.actions {
                    app.run_action(id);
                }
                step.keys = self
                    .script
                    .keys
                    .iter()
                    .map(|k| keys::scripted(k, APPLE))
                    .collect();
                self.stage = Stage::Typed(Instant::now());
            }
            Stage::Typed(at) if at.elapsed() > Duration::from_secs(1) => step.capture = true,
            _ => {}
        }
        step
    }

    /// Save the window as the script's PNG, reporting how long the capture
    /// (a full re-render of the scene) took for `spans` text spans.
    pub fn capture(&self, ui: &ComponentInstance, spans: usize) {
        let path = &self.script.path;
        let at = Instant::now();
        let shot = match ui.window().take_snapshot() {
            Ok(shot) => shot,
            Err(e) => {
                eprintln!("vix-slint: could not capture the window: {e}");
                return;
            }
        };
        eprintln!(
            "vix-slint: full render of {spans} text spans took {:?}",
            at.elapsed()
        );
        if let Err(e) = image::save_buffer(
            path,
            shot.as_bytes(),
            shot.width(),
            shot.height(),
            image::ExtendedColorType::Rgba8,
        ) {
            eprintln!("vix-slint: could not save {path}: {e}");
        }
    }
}
