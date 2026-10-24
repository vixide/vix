//! Compile the window's markup (`ui/window.slint` plus the generated menu
//! bar from `crate::menus`) with Slint's interpreter and create the window.
//!
//! The interpreter rather than `slint-build`: the Rust that `slint-build`
//! generates needs `#[allow(unsafe_code)]`, which this workspace's
//! `forbid(unsafe_code)` rejects, and the menu bar is markup generated from
//! the Vix menu model at startup anyway.

use std::future::Future;
use std::path::PathBuf;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use crate::font_choice::{FONT_SIZE, cell_height, primary_family};
use slint_interpreter::{Compiler, ComponentInstance, DiagnosticLevel, Value};

/// The window's markup, before the menu bar is filled in.
const TEMPLATE: &str = include_str!("../ui/window.slint");

/// The line in [`TEMPLATE`] the menu bar replaces.
const MARKER: &str = "//@MENUBAR@";

/// The component `ui/window.slint` exports.
const COMPONENT: &str = "VixWindow";

/// Run a future that never waits to completion. The interpreter's compiler is
/// `async` only for custom file loaders, which this front end does not use.
fn block_on<F: Future>(fut: F) -> F::Output {
    let mut fut = pin!(fut);
    let mut cx = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(out) = fut.as_mut().poll(&mut cx) {
            return out;
        }
        std::thread::yield_now();
    }
}

/// The window's full markup: the template with `menubar` in place.
#[must_use]
pub fn source(menubar: &str) -> String {
    TEMPLATE.replace(MARKER, menubar)
}

/// Compile `source` and return the window component, or the compiler's
/// errors.
///
/// # Errors
/// The compiler's error diagnostics, one per line.
pub fn compile(source: String) -> Result<slint_interpreter::ComponentDefinition, String> {
    let result =
        block_on(Compiler::default().build_from_source(source, PathBuf::from("window.slint")));
    let errors: Vec<String> = result
        .diagnostics()
        .filter(|d| d.level() == DiagnosticLevel::Error)
        .map(|d| d.to_string())
        .collect();
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    result
        .component(COMPONENT)
        .ok_or_else(|| format!("window.slint does not export {COMPONENT}"))
}

/// Create the window, titled `title`, with its menu bar and font set up.
///
/// # Errors
/// Compiler errors in the markup, or the platform failing to create a window.
pub fn create(title: &str) -> Result<ComponentInstance, String> {
    let definition = compile(source(&crate::menus::markup()))?;
    let ui = definition.create().map_err(|e| e.to_string())?;
    let set = |name: &str, value: Value| {
        ui.set_property(name, value)
            .map_err(|e| format!("{name}: {e}"))
    };
    set("window-title", Value::String(title.into()))?;
    set("family", Value::String(primary_family().into()))?;
    set("font-px", Value::Number(f64::from(FONT_SIZE)))?;
    set("cell-h", Value::Number(f64::from(cell_height())))?;
    set("native-menus", Value::Bool(crate::menus::NATIVE))?;
    Ok(ui)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_and_its_generated_menu_bar_compile() {
        assert!(TEMPLATE.contains(MARKER));
        let def = compile(source(&crate::menus::markup())).unwrap();
        let props: Vec<String> = def.properties().map(|(name, _)| name).collect();
        for p in ["rows", "cell-w", "grid-width", "dark", "native-menus"] {
            assert!(props.iter().any(|n| n == p), "missing property {p}");
        }
        let callbacks: Vec<String> = def.callbacks().collect();
        for c in ["key", "pointer", "scroll", "menu-activated", "resized"] {
            assert!(callbacks.iter().any(|n| n == c), "missing callback {c}");
        }
    }
}
