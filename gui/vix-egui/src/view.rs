//! The eframe app: owns the Vix `App`, renders it through `ui::draw` into the
//! in-memory cell grid each frame, paints the grid with egui, and feeds keyboard,
//! mouse and menu input back as `vix-input` events and action ids.

use crate::font_choice::{FONT_SIZE, cell_height};
use crate::grid::GridBackend;
use crate::render::{Palette, Rgba8, cell_colors, row_backgrounds, row_text};
use eframe::egui::{
    Color32, Context, Event, FontFamily, FontId, IMEPurpose, Modifiers, MouseWheelUnit,
    PointerButton, Pos2, Rect, Stroke, TextFormat, Ui, Vec2, ViewportCommand, output::IMEOutput,
    pos2, text::LayoutJob, vec2,
};
use ratatui::Terminal;
use ratatui::style::Modifier;
use vix::app::{App as VixApp, AppFlags};
use vix_input::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::fonts::BOLD;
use crate::keys;

/// Wheel points per scrolled line, for trackpads reporting point deltas.
const POINTS_PER_LINE: f32 = 20.0;

/// The Vix window.
pub struct VixEgui {
    app: VixApp,
    term: Terminal<GridBackend>,
    palette: Palette,
    /// Cell size in points, measured on the first frame (after the fonts load).
    cell: Option<Vec2>,
    /// Fractional wheel lines not yet delivered.
    scroll_accum: f32,
    /// The button held down, so motion reports as a drag.
    pressed: Option<MouseButton>,
    /// The cell the last motion event was reported for.
    last_move_cell: Option<(u16, u16)>,
    /// The App asked to quit and the window is closing for real.
    quitting: bool,
    #[cfg(target_os = "macos")]
    _menu: crate::menus::NativeMenu,
    #[cfg(feature = "screenshot")]
    script: Option<crate::screenshot::Run>,
}

impl VixEgui {
    /// Wrap a started `App`, installing fonts (and, on macOS, the menu bar).
    pub fn new(cc: &eframe::CreationContext<'_>, app: VixApp, dark: bool) -> Self {
        cc.egui_ctx.set_fonts(crate::fonts::definitions());
        Self {
            app,
            term: Terminal::new(GridBackend::new(80, 24)).unwrap_or_else(|never| match never {}),
            palette: Palette::for_appearance(dark),
            cell: None,
            scroll_accum: 0.0,
            pressed: None,
            last_move_cell: None,
            quitting: false,
            #[cfg(target_os = "macos")]
            _menu: crate::menus::install(),
            #[cfg(feature = "screenshot")]
            script: crate::startup::Script::from_env().map(crate::screenshot::Run::new),
        }
    }

    /// Refresh git state; deferred until the window is up (T122).
    pub fn refresh_git(&mut self) {
        self.app.refresh_git();
    }

    /// The cell size, measured once the window's fonts are in place.
    fn cell(&mut self, ctx: &Context) -> Vec2 {
        if let Some(c) = self.cell {
            return c;
        }
        let width = ctx.fonts_mut(|f| f.glyph_width(&FontId::monospace(FONT_SIZE), 'm'));
        let c = vec2(width, cell_height());
        self.cell = Some(c);
        c
    }

    /// Quit when the App asked to; drop a suspend request (no meaning here).
    fn check_quit(&mut self, ctx: &Context) {
        self.app.flags.remove(AppFlags::SUSPEND_REQUESTED);
        if self.app.flags.contains(AppFlags::SHOULD_QUIT) && !self.quitting {
            self.quitting = true;
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
    }

    /// Run one action id from a menu.
    fn run_menu_action(&mut self, id: &str) {
        #[cfg(target_os = "macos")]
        if id == crate::menus::OPEN_NATIVE {
            if let Some(paths) = rfd::FileDialog::new().pick_files() {
                for p in paths {
                    self.app.open_initial(&p);
                }
            }
            return;
        }
        self.app.run_action(id);
    }

    /// The cell under a window position, relative to the grid's origin.
    fn cell_at(&self, origin: Pos2, pos: Pos2) -> (u16, u16) {
        let Some(c) = self.cell else {
            return (0, 0);
        };
        let area = self.term.backend().buffer().area;
        (
            cell_index((pos.x - origin.x) / c.x, area.width),
            cell_index((pos.y - origin.y) / c.y, area.height),
        )
    }

    fn mouse(&mut self, kind: MouseEventKind, cell: (u16, u16), mods: KeyModifiers) {
        self.app.on_mouse(MouseEvent {
            kind,
            column: cell.0,
            row: cell.1,
            modifiers: mods,
        });
    }

    /// Feed one egui event to the App.
    fn handle(&mut self, ev: &Event, held: Modifiers, origin: Pos2, hover: Option<Pos2>) {
        for key in keys::key_events(ev, held) {
            self.app.on_key(key);
        }
        match ev {
            Event::Paste(text) => self.app.on_paste(text),
            Event::PointerMoved(pos) => {
                // A terminal reports motion per cell, not per pixel; so does this.
                let cell = self.cell_at(origin, *pos);
                if self.last_move_cell != Some(cell) {
                    self.last_move_cell = Some(cell);
                    let kind = self
                        .pressed
                        .map_or(MouseEventKind::Moved, MouseEventKind::Drag);
                    self.mouse(kind, cell, keys::modifiers(held));
                }
            }
            Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers,
            } => {
                let b = match button {
                    PointerButton::Primary => MouseButton::Left,
                    PointerButton::Secondary => MouseButton::Right,
                    PointerButton::Middle => MouseButton::Middle,
                    _ => return,
                };
                let kind = if *pressed {
                    self.pressed = Some(b);
                    MouseEventKind::Down(b)
                } else {
                    self.pressed = None;
                    MouseEventKind::Up(b)
                };
                let cell = self.cell_at(origin, *pos);
                self.mouse(kind, cell, keys::modifiers(*modifiers));
            }
            Event::MouseWheel {
                unit,
                delta,
                modifiers,
                ..
            } => {
                let rows = f32::from(self.term.backend().buffer().area.height);
                self.scroll_accum += match unit {
                    MouseWheelUnit::Line => delta.y,
                    MouseWheelUnit::Point => delta.y / POINTS_PER_LINE,
                    MouseWheelUnit::Page => delta.y * rows,
                };
                let cell = hover.map_or((0, 0), |p| self.cell_at(origin, p));
                // Positive y moves the content down: the view scrolls up.
                while self.scroll_accum.abs() >= 1.0 {
                    let kind = if self.scroll_accum > 0.0 {
                        self.scroll_accum -= 1.0;
                        MouseEventKind::ScrollUp
                    } else {
                        self.scroll_accum += 1.0;
                        MouseEventKind::ScrollDown
                    };
                    self.mouse(kind, cell, keys::modifiers(*modifiers));
                }
            }
            _ => {}
        }
    }

    /// Paint the grid, its cursor, and tell the IME where the cursor is.
    fn paint(&self, ui: &Ui, rect: Rect, cell: Vec2) {
        let painter = ui.painter();
        let p = self.palette;
        painter.rect_filled(rect, 0.0, c32(p.bg));
        let buffer = self.term.backend().buffer();
        let at = |col: u16, row: u16| {
            pos2(
                rect.min.x + cell.x * f32::from(col),
                rect.min.y + cell.y * f32::from(row),
            )
        };
        for y in 0..buffer.area.height {
            for run in row_backgrounds(buffer, y, p) {
                let r =
                    Rect::from_min_size(at(run.col, y), vec2(cell.x * f32::from(run.len), cell.y));
                painter.rect_filled(r, 0.0, c32(run.color));
            }
            for span in row_text(buffer, y, p) {
                let m = span.style.modifier;
                let color = c32(span.style.fg);
                let line = |on: bool| {
                    if on {
                        Stroke::new(1.0, color)
                    } else {
                        Stroke::NONE
                    }
                };
                let family = if m.contains(Modifier::BOLD) {
                    FontFamily::Name(BOLD.into())
                } else {
                    FontFamily::Monospace
                };
                let job = LayoutJob::single_section(
                    span.text,
                    TextFormat {
                        font_id: FontId::new(FONT_SIZE, family),
                        color,
                        italics: m.contains(Modifier::ITALIC),
                        underline: line(m.contains(Modifier::UNDERLINED)),
                        strikethrough: line(m.contains(Modifier::CROSSED_OUT)),
                        line_height: Some(cell.y),
                        ..TextFormat::default()
                    },
                );
                painter.galley(at(span.col, y), painter.layout_job(job), color);
            }
        }
        let cursor = self.term.backend().cursor();
        if let Some(pos) = cursor
            && pos.x < buffer.area.width
            && pos.y < buffer.area.height
        {
            let (fg, _) = cell_colors(&buffer[(pos.x, pos.y)], p);
            painter.rect_filled(
                Rect::from_min_size(at(pos.x, pos.y), vec2(2.0, cell.y)),
                0.0,
                c32(fg),
            );
        }
        // Keep IME composition windows next to the text cursor.
        let caret = cursor.map_or(rect.min, |c| at(c.x, c.y));
        ui.ctx().output_mut(|o| {
            o.ime = Some(IMEOutput {
                purpose: IMEPurpose::Terminal,
                rect,
                cursor_rect: Rect::from_min_size(caret, cell),
                should_interrupt_composition: false,
            });
        });
    }
}

/// A planned color as an egui one.
fn c32(c: Rgba8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
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

impl eframe::App for VixEgui {
    fn logic(&mut self, ctx: &Context, _: &mut eframe::Frame) {
        // Background work (streamed output, LSP, file watching, …), exactly
        // as the terminal loop's `App::tick`.
        self.app.tick();
        #[cfg(target_os = "macos")]
        for id in crate::menus::clicked() {
            self.run_menu_action(&id);
        }
        // The close button runs Vix's own quit flow (unsaved changes, session)
        // instead of closing outright.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.run_menu_action("file.quit");
        }
        self.check_quit(ctx);
        // A scripted run must paint even when the window is not visible (a
        // CI runner, or a window behind others): eframe otherwise skips `ui`.
        #[cfg(feature = "screenshot")]
        if self.script.is_some() {
            ctx.send_viewport_cmd(ViewportCommand::RequestPaintWhileHidden);
        }
        ctx.request_repaint_after(self.app.poll_interval());
    }

    fn ui(&mut self, ui: &mut Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let rect = ui.max_rect();
        let cell = self.cell(&ctx);
        let (events, held, hover) =
            ctx.input(|i| (i.events.clone(), i.modifiers, i.pointer.hover_pos()));
        for ev in &events {
            self.handle(ev, held, rect.min, hover);
        }
        #[cfg(feature = "screenshot")]
        if let Some(mut run) = self.script.take() {
            let step = run.step(&ctx, &mut self.app);
            for (ev, held) in &step.input {
                self.handle(ev, *held, rect.min, None);
            }
            if step.quit {
                self.quitting = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
            self.script = Some(run);
        }
        self.check_quit(&ctx);

        let cols = cell_index(rect.width() / cell.x, u16::MAX).max(1);
        let rows = cell_index(rect.height() / cell.y, u16::MAX).max(1);
        self.term.backend_mut().resize(cols, rows);
        let app = &mut self.app;
        let _ = self.term.draw(|frame| vix::ui::draw(app, frame));
        self.paint(ui, rect, cell);
    }

    fn on_exit(&mut self) {
        self.app.on_exit();
    }
}
