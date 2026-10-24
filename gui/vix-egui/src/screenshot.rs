//! The scripted render smoke test (`vix_gui_common::startup::Script`): wait
//! for startup, run actions, type keystrokes through the same event handling
//! real input takes, capture the window with egui's screenshot command, save
//! a PNG, and quit.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use eframe::egui::{Color32, ColorImage, Context, Event, Modifiers};
use vix::app::App;
use vix_gui_common::startup::Script;

/// Where a scripted run is up to.
enum Stage {
    /// Waiting for startup to settle.
    Waiting,
    /// Actions run and keys typed; waiting for the frames to show them.
    Typed(Instant),
    /// Screenshot requested; waiting for it to be saved.
    Capturing,
}

/// A scripted run in progress.
pub struct Run {
    script: Script,
    started: Instant,
    stage: Stage,
    saved: Arc<AtomicBool>,
}

/// What one step asks of the window.
pub struct Step {
    /// Scripted input events to handle now, with the modifiers held.
    pub input: Vec<(Event, Modifiers)>,
    /// The screenshot is saved: close the window.
    pub quit: bool,
}

impl Run {
    /// Start a scripted run.
    #[must_use]
    pub fn new(script: Script) -> Self {
        Self {
            script,
            started: Instant::now(),
            stage: Stage::Waiting,
            saved: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Advance the run; call once per frame.
    pub fn step(&mut self, ctx: &Context, app: &mut App) -> Step {
        ctx.request_repaint();
        let mut step = Step {
            input: Vec::new(),
            quit: false,
        };
        match self.stage {
            Stage::Waiting if self.started.elapsed() > Duration::from_secs(2) => {
                for id in &self.script.actions {
                    app.run_action(id);
                }
                for k in &self.script.keys {
                    let (events, held) = crate::keys::scripted(k);
                    step.input.extend(events.into_iter().map(|e| (e, held)));
                }
                self.stage = Stage::Typed(Instant::now());
            }
            Stage::Typed(at) if at.elapsed() > Duration::from_secs(1) => {
                let path = self.script.path.clone();
                let saved = Arc::clone(&self.saved);
                ctx.request_screenshot(move |image| {
                    if let Err(e) = save(&image, &path) {
                        eprintln!("vix-egui: could not save {path}: {e}");
                    }
                    saved.store(true, Ordering::SeqCst);
                });
                self.stage = Stage::Capturing;
            }
            Stage::Capturing => step.quit = self.saved.load(Ordering::SeqCst),
            _ => {}
        }
        step
    }
}

/// Write an egui screenshot as PNG.
fn save(image: &ColorImage, path: &str) -> Result<(), image::ImageError> {
    let [w, h] = image.size;
    let bytes: Vec<u8> = image
        .pixels
        .iter()
        .flat_map(Color32::to_srgba_unmultiplied)
        .collect();
    let (Ok(w), Ok(h)) = (u32::try_from(w), u32::try_from(h)) else {
        return Ok(());
    };
    image::save_buffer(path, &bytes, w, h, image::ExtendedColorType::Rgba8)
}
