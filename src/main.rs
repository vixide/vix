//! STRIDE: Simple Terminal Rust IDE — binary entry point.
//!
//! Sets up the terminal (with mouse capture), runs the event loop, and restores
//! on exit. All of the application logic lives in the `stride` library crate.

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::execute;

use stride::app::App;
use stride::ui;

fn main() -> io::Result<()> {
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut app = App::new(root);

    // Optional file argument(s): open each, focusing the last.
    for arg in std::env::args().skip(1) {
        app.open_initial(PathBuf::from(arg));
    }

    let mut terminal = ratatui::init();
    let _ = execute!(io::stdout(), EnableMouseCapture);
    // Query the terminal for graphics support so images can be displayed.
    app.picker = ratatui_image::picker::Picker::from_query_stdio().ok();
    let result = run(&mut terminal, &mut app);
    let _ = execute!(io::stdout(), DisableMouseCapture);
    let _ = io::stdout().flush();
    ratatui::restore();
    app.on_exit();
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(app, frame))?;
        if app.should_quit {
            return Ok(());
        }
        // Poll with a timeout so the calendar clock refreshes while idle.
        if event::poll(Duration::from_millis(500))? {
            match event::read()? {
                Event::Key(key) => app.on_key(key),
                Event::Mouse(mouse) => app.on_mouse(mouse),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }
}
