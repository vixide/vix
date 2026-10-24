//! The window's root view: owns the Vix `App`, renders it through
//! `ui::draw` into a [`GridBackend`] each frame, paints the grid, and feeds
//! keyboard / mouse / menu input back as `vix-input` events and action ids.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::{
    App, Context, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, PathPromptOptions, Pixels, Point,
    Render, ScrollDelta, ScrollWheelEvent, Size, Styled, Task, Window, WindowAppearance, canvas,
    div, px,
};
use ratatui::Terminal;
use vix::app::{App as VixApp, AppFlags};
use vix_input::{KeyModifiers, MouseEvent, MouseEventKind};

use crate::grid::GridBackend;
use crate::keys;
use crate::menus::{OpenNative, RunAction};
use crate::paint::{Metrics, paint_grid};

/// Font size in the window, in logical pixels.
const FONT_SIZE: f32 = 14.0;
/// Line height as a multiple of the font size.
const LINE_HEIGHT: f32 = 1.3;
/// Wheel pixels per scrolled line, for trackpads reporting pixel deltas.
const PIXELS_PER_LINE: f32 = 20.0;

/// The monospace family for this platform, unless `VIX_GUI_FONT` names one.
fn font_family() -> String {
    std::env::var("VIX_GUI_FONT").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "Menlo".into()
        } else if cfg!(target_os = "windows") {
            "Consolas".into()
        } else {
            "DejaVu Sans Mono".into()
        }
    })
}

/// Installed families to fall back to for glyphs the primary font lacks.
///
/// Vix draws file, panel and status icons from the Nerd Font private-use
/// range (a terminal user gets them from a Nerd-patched terminal font), and a
/// stock monospace font has none of them. Prefer the icons-only "Symbols Nerd
/// Font", then any installed "… Nerd Font Mono" family; with none installed,
/// icons show the platform's missing-glyph box, as in a non-Nerd terminal.
fn nerd_fallbacks(installed: &[String]) -> Vec<String> {
    let mut out: Vec<String> = ["Symbols Nerd Font Mono", "Symbols Nerd Font"]
        .into_iter()
        .filter(|f| installed.iter().any(|i| i == f))
        .map(String::from)
        .collect();
    if let Some(mono) = installed.iter().find(|f| f.ends_with("Nerd Font Mono")) {
        out.push(mono.clone());
    }
    out
}

/// The root view of a Vix window.
pub struct VixView {
    app: VixApp,
    term: Terminal<GridBackend>,
    focus: FocusHandle,
    metrics: Option<Metrics>,
    /// Fractional wheel lines not yet delivered.
    scroll_accum: f32,
    /// The size the canvas was last painted at: the grid is sized from this,
    /// not the raw viewport, so it fills exactly the space layout gave it.
    canvas_size: Rc<Cell<Option<Size<Pixels>>>>,
    /// The cell the last motion event was reported for.
    last_move_cell: Option<(u16, u16)>,
    /// Keeps the background tick loop alive for the view's lifetime.
    _ticker: Task<()>,
}

impl VixView {
    /// Wrap a started `App` and begin ticking it.
    pub fn new(app: VixApp, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        #[cfg(feature = "screenshot")]
        Self::schedule_screenshot(window, cx);
        let ticker = cx.spawn(async move |this, cx| {
            loop {
                let interval = this
                    .update(cx, |view, _| view.app.poll_interval())
                    .unwrap_or(Duration::from_millis(500));
                cx.background_executor().timer(interval).await;
                // `tick` drains background work into App state; redraw so it
                // shows (the clock, streamed output, diagnostics, …).
                if this.update(cx, Self::after_input).is_err() {
                    break;
                }
            }
        });
        Self {
            app,
            term: Terminal::new(GridBackend::new(80, 24)).unwrap_or_else(|never| match never {}),
            focus,
            metrics: None,
            scroll_accum: 0.0,
            last_move_cell: None,
            canvas_size: Rc::default(),
            _ticker: ticker,
        }
    }

    /// With `VIX_GUI_SCREENSHOT=<path>` set: wait for startup to settle, run
    /// the comma-separated action ids in `VIX_GUI_SCREENSHOT_ACTIONS` (to open
    /// an overlay, say), then type the space-separated GPUI keystrokes in
    /// `VIX_GUI_SCREENSHOT_KEYS` (`cmd-p`, `h`, `shift-a`, `enter`) through the
    /// window's real key dispatch, render the window to `<path>` as PNG, and
    /// quit.
    #[cfg(feature = "screenshot")]
    fn schedule_screenshot(window: &Window, cx: &mut Context<Self>) {
        let Ok(path) = std::env::var("VIX_GUI_SCREENSHOT") else {
            return;
        };
        let actions = std::env::var("VIX_GUI_SCREENSHOT_ACTIONS").unwrap_or_default();
        let keys = std::env::var("VIX_GUI_SCREENSHOT_KEYS").unwrap_or_default();
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            let _ = this.update(cx, |view, cx| {
                for id in actions.split(',').filter(|a| !a.is_empty()) {
                    view.app.run_action(id);
                }
                view.after_input(cx);
            });
            let _ = cx.update(|window, cx| {
                for k in keys.split_whitespace() {
                    match gpui_kit::Keystroke::parse(k) {
                        Ok(ks) => {
                            window.dispatch_keystroke(ks, cx);
                        }
                        Err(e) => eprintln!("vix-gui: bad keystroke {k:?}: {e}"),
                    }
                }
            });
            cx.background_executor().timer(Duration::from_secs(1)).await;
            // Draw explicitly: macOS pauses frame callbacks for a window that
            // is not visible (a CI runner, or behind other windows), so the
            // last presented frame may predate the actions above.
            let _ = cx.update(|window, cx| {
                window.draw(cx).clear(cx);
                match window.render_to_image() {
                    Ok(image) => {
                        if let Err(e) = image.save(&path) {
                            eprintln!("vix-gui: could not save {path}: {e}");
                        }
                    }
                    Err(e) => eprintln!("vix-gui: could not render the window: {e}"),
                }
                cx.quit();
            });
        })
        .detach();
    }

    /// Refresh git state (repo, branch, status) and redraw. Deferred until
    /// after the window opens, as the terminal binary does (T122).
    pub fn refresh_git(&mut self, cx: &mut Context<Self>) {
        self.app.refresh_git();
        cx.notify();
    }

    /// Run background work, quit if the App asked to, and schedule a redraw.
    fn after_input(&mut self, cx: &mut Context<Self>) {
        self.app.tick();
        if self.app.flags.contains(AppFlags::SHOULD_QUIT) {
            self.app.on_exit();
            cx.quit();
            return;
        }
        // Suspending to a shell has no meaning in a window; drop the request.
        self.app.flags.remove(AppFlags::SUSPEND_REQUESTED);
        cx.notify();
    }

    /// Font metrics and default colors, measured once per window.
    fn metrics(&mut self, window: &mut Window) -> Metrics {
        if let Some(m) = &self.metrics {
            return m.clone();
        }
        let family = font_family();
        let font_size = px(FONT_SIZE);
        let ts = window.text_system();
        let font_id = ts.resolve_font(&gpui_kit::font(family.clone()));
        let fallbacks = nerd_fallbacks(&ts.all_font_names());
        let cell_width = ts
            .em_advance(font_id, font_size)
            .unwrap_or(px(FONT_SIZE * 0.6));
        let cell_height = px((FONT_SIZE * LINE_HEIGHT).round());
        let dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let (fg, bg) = if dark {
            (0xd4_d4d4, 0x1e_1e1e)
        } else {
            (0x1e_1e1e, 0xff_ffff)
        };
        let m = Metrics {
            family: family.into(),
            fallbacks: (!fallbacks.is_empty())
                .then(|| gpui_kit::FontFallbacks::from_fonts(fallbacks)),
            font_size,
            cell_width,
            cell_height,
            default_fg: gpui_kit::rgb(fg),
            default_bg: gpui_kit::rgb(bg),
        };
        self.metrics = Some(m.clone());
        m
    }

    /// The cell under a window position.
    fn cell_at(&self, pos: Point<Pixels>) -> (u16, u16) {
        let Some(m) = &self.metrics else {
            return (0, 0);
        };
        let area = self.term.backend().buffer().area;
        (
            cell_index(pos.x / m.cell_width, area.width),
            cell_index(pos.y / m.cell_height, area.height),
        )
    }

    fn mouse(&mut self, kind: MouseEventKind, pos: Point<Pixels>, mods: KeyModifiers) {
        let (column, row) = self.cell_at(pos);
        self.app.on_mouse(MouseEvent {
            kind,
            column,
            row,
            modifiers: mods,
        });
    }

    fn on_key_down(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        if let Some(key) = keys::key_event(
            &ks.key,
            ks.key_char.as_deref(),
            keys::modifiers(ks.modifiers),
        ) {
            self.app.on_key(key);
            cx.stop_propagation();
            self.after_input(cx);
        }
    }

    fn on_mouse_down(&mut self, ev: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(b) = keys::mouse_button(ev.button) {
            let mods = keys::modifiers(ev.modifiers);
            self.mouse(MouseEventKind::Down(b), ev.position, mods);
            self.after_input(cx);
        }
    }

    fn on_mouse_up(&mut self, ev: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(b) = keys::mouse_button(ev.button) {
            let mods = keys::modifiers(ev.modifiers);
            self.mouse(MouseEventKind::Up(b), ev.position, mods);
            self.after_input(cx);
        }
    }

    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let mods = keys::modifiers(ev.modifiers);
        let kind = match ev.pressed_button.and_then(keys::mouse_button) {
            Some(b) => MouseEventKind::Drag(b),
            None => MouseEventKind::Moved,
        };
        // A terminal reports motion per cell, not per pixel; so does this.
        let cell = self.cell_at(ev.position);
        if self.last_move_cell == Some(cell) {
            return;
        }
        self.last_move_cell = Some(cell);
        self.mouse(kind, ev.position, mods);
        self.after_input(cx);
    }

    fn on_scroll(&mut self, ev: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let dy = match ev.delta {
            ScrollDelta::Lines(p) => p.y,
            ScrollDelta::Pixels(p) => p.y / px(PIXELS_PER_LINE),
        };
        self.scroll_accum += dy;
        let mods = keys::modifiers(ev.modifiers);
        let mut delivered = false;
        while self.scroll_accum.abs() >= 1.0 {
            let kind = if self.scroll_accum > 0.0 {
                self.scroll_accum -= 1.0;
                MouseEventKind::ScrollUp
            } else {
                self.scroll_accum += 1.0;
                MouseEventKind::ScrollDown
            };
            self.mouse(kind, ev.position, mods);
            delivered = true;
        }
        if delivered {
            self.after_input(cx);
        }
    }

    fn on_run_action(&mut self, action: &RunAction, _: &mut Window, cx: &mut Context<Self>) {
        self.app.run_action(action.id);
        self.after_input(cx);
    }

    /// Ask for files with the platform's file dialog, then open them.
    fn open_native(cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let _ = this.update(cx, |view, cx| view.open_paths(&paths, cx));
        })
        .detach();
    }

    fn open_paths(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        for path in paths {
            self.app.open_initial(path);
        }
        self.after_input(cx);
    }
}

/// A fractional cell coordinate, clamped into `0..len`.
fn cell_index(v: f32, len: u16) -> u16 {
    let max = f32::from(len.saturating_sub(1));
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped into 0..=u16 range first"
    )]
    let i = v.floor().clamp(0.0, max) as u16;
    i
}

impl Render for VixView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let m = self.metrics(window);
        let vp = self
            .canvas_size
            .get()
            .unwrap_or_else(|| window.viewport_size());
        let cols = cell_index(vp.width / m.cell_width, u16::MAX).max(1);
        let rows = cell_index(vp.height / m.cell_height, u16::MAX).max(1);
        self.term.backend_mut().resize(cols, rows);
        let app = &mut self.app;
        let _ = self.term.draw(|frame| vix::ui::draw(app, frame));
        let buffer = self.term.backend().buffer().clone();
        let cursor = self.term.backend().cursor();
        let canvas_size = Rc::clone(&self.canvas_size);
        let bg = m.default_bg;

        div()
            .size_full()
            .bg(bg)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Right, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .on_action(cx.listener(Self::on_run_action))
            .on_action(cx.listener(|_, _: &OpenNative, _, cx| Self::open_native(cx)))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, (), window: &mut Window, cx: &mut App| {
                        // Layout gave the canvas a different size than the
                        // grid was drawn for (first frame, or a resize):
                        // redraw at the real size.
                        if canvas_size.replace(Some(bounds.size)) != Some(bounds.size) {
                            window.refresh();
                        }
                        paint_grid(&buffer, cursor, &m, bounds, window, cx);
                    },
                )
                .size_full(),
            )
    }
}
