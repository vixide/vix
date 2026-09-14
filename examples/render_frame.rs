//! Render a frame of the real TUI to a `TestBackend` and print it as plain
//! text -- no real terminal needed. The same technique `tests/snapshots.rs`
//! uses for its golden-screen tests (improvement plan T004/T005), pulled out
//! here as its own minimal, standalone example (T502).
//!
//! Run with: `cargo run --example render_frame -- [file]`

#![warn(clippy::pedantic)]

use std::path::PathBuf;

use ratatui::{Terminal, backend::TestBackend};
use vix::app::App;
use vix::settings::Settings;

fn main() {
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut app = App::new(root, Settings::default());

    // An 80x24 viewport, the classic terminal default -- large enough that a
    // real editor pane has room to draw in.
    app.layout.editor = ratatui::layout::Rect::new(0, 0, 80, 24);

    // Optionally open a real file, so the frame shows syntax-highlighted
    // content instead of the empty-workspace welcome screen.
    if let Some(path) = std::env::args().nth(1) {
        app.open_initial(&PathBuf::from(path));
    }

    // `TestBackend` renders into an in-memory cell buffer instead of a real
    // terminal -- `vix::ui::draw` (the same function the real event loop
    // calls every frame) doesn't know the difference.
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("test backend");
    terminal
        .draw(|frame| vix::ui::draw(&mut app, frame))
        .expect("draw a frame");

    // Flatten the cell buffer to plain text, one line per row, trailing
    // whitespace trimmed.
    let buffer = terminal.backend().buffer();
    for y in 0..buffer.area.height {
        let line: String = (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        println!("{}", line.trim_end());
    }
}
