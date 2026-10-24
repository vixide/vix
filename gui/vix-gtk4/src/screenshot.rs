//! The scripted render smoke test (`crate::startup::Script`): wait for
//! startup, run actions, type keystrokes through the same key mapping real
//! input takes (`crate::keys::scripted`), then render the window's grid into
//! a Cairo image with the same paint code the window uses, save it as a PNG,
//! and quit. Rendering into an image surface does not depend on the window
//! being visible (a CI runner, or a window behind others).

use std::time::Duration;

use gtk4::cairo;
use gtk4::glib;

use crate::startup::Script;
use crate::view::View;

/// Start a scripted run on the open window.
pub fn start(view: View, script: Script) {
    glib::timeout_add_local_once(Duration::from_secs(2), move || {
        for id in &script.actions {
            view.run_action(id);
        }
        for k in &script.keys {
            view.send_keys(crate::keys::scripted(k));
        }
        glib::timeout_add_local_once(Duration::from_secs(1), move || {
            if let Err(e) = capture(&view, &script.path) {
                eprintln!("vix-gtk4: could not save {}: {e}", script.path);
            }
            view.close_now();
        });
    });
}

/// Render the window's grid at its current size and scale into `path`.
fn capture(view: &View, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (w, h, scale) = view.area_size();
    let (w, h) = (w.max(1), h.max(1));
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w * scale, h * scale)?;
    {
        let cr = cairo::Context::new(&surface)?;
        cr.scale(f64::from(scale), f64::from(scale));
        view.state()
            .borrow_mut()
            .render(&cr, f64::from(w), f64::from(h));
    }
    let mut file = std::fs::File::create(path)?;
    surface.write_to_png(&mut file)?;
    Ok(())
}
