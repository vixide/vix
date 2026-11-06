//! The scripted render smoke test (`crate::startup::Script`): open an
//! off-screen BUI window, run the actions, type the keystrokes through BUI's
//! real input path, save the window's pixels as a PNG, and quit.
//!
//! BUI renders headless windows with its software raster, so unlike the other
//! front ends this works anywhere — a CI runner without a display included.

use std::cell::RefCell;
use std::fs::File;
use std::io::BufWriter;
use std::rc::Rc;

use blenderui::prelude::Application;

use crate::keys::scripted;
use crate::model::Model;
use crate::startup::Script;
use crate::view::{self, Shared};

/// Logical size of the off-screen window.
const WIDTH: f32 = 1200.0;
/// Logical size of the off-screen window.
const HEIGHT: f32 = 800.0;

/// Run `script` against `model` and save the PNG it asks for.
///
/// # Errors
/// Returns a message when the window cannot be opened or the PNG written.
pub fn run(model: Model, script: &Script) -> Result<(), String> {
    let shared = Shared::new(model);
    let outcome: Rc<RefCell<Result<(), String>>> = Rc::new(RefCell::new(Ok(())));
    let result = Rc::clone(&outcome);
    Application::headless().run(|cx| {
        let win = match view::open(cx, &shared, "Vix".into(), WIDTH, HEIGHT) {
            Ok(w) => w,
            Err(e) => {
                *result.borrow_mut() = Err(e);
                return;
            }
        };
        // One frame first, so the App has a grid of the window's size.
        cx.draw(win.id);
        // BUI routes a key to the node under the pointer when nothing is
        // focused, so put the pointer over the window as a user's would be.
        cx.simulate_mouse_move(win.id, WIDTH / 2.0, HEIGHT / 2.0);
        for id in &script.actions {
            shared.model.borrow_mut().run_action(id);
        }
        for stroke in &script.keys {
            let k = scripted(stroke);
            cx.simulate_key_with(win.id, &k.text, &k.keysym, k.control, k.shift);
        }
        cx.notify_all();
        let (w, h, pixels) = cx.draw(win.id);
        *result.borrow_mut() = write_png(&script.path, w, h, &pixels);
    });
    // No `App::on_exit`: a scripted run must not write the user's session.
    outcome.replace(Ok(()))
}

/// Encode `0x00RRGGBB` pixels as an opaque RGBA PNG.
fn write_png(path: &str, columns: usize, lines: usize, pixels: &[u32]) -> Result<(), String> {
    let width = u32::try_from(columns).map_err(|e| e.to_string())?;
    let height = u32::try_from(lines).map_err(|e| e.to_string())?;
    let rgba: Vec<u8> = pixels
        .iter()
        .flat_map(|p| {
            let [_, red, green, blue] = p.to_be_bytes();
            [red, green, blue, 255]
        })
        .collect();
    let file = File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&rgba).map_err(|e| e.to_string())
}
