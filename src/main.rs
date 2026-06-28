//! Vix: Simple Terminal Rust IDE — binary entry point.
//!
//! Sets up the terminal (with mouse capture), runs the event loop, and restores
//! on exit. All of the application logic lives in the `vix` library crate.

// Always start with high quality coding conventions.
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

// When we build for MUSL static, use faster memory allocator.
#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::execute;

use vix::app::App;
use vix::settings::Settings;
use vix::ui;

/// Command-line interface for Vix.
#[derive(Parser, Debug)]
#[command(name = "vix", version, about = "Vix: Simple Terminal Rust IDE")]
struct Cli {
    /// File(s) to open on startup; the last one is focused.
    files: Vec<PathBuf>,

    /// UI language as a locale code (e.g. en, es, fr, de, cy). Overrides the
    /// saved `locale` setting for this run only.
    #[arg(short, long)]
    locale: Option<String>,
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();
    let settings = Settings::load();

    // A `--locale` flag wins over the persisted setting, but is not saved back.
    let locale = cli.locale.clone().unwrap_or_else(|| settings.locale.clone());
    rust_i18n::set_locale(&locale);

    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut app = App::new(root, settings);
    app.refresh_git();
    // First-run welcome screen (no-op after it has been seen once).
    app.maybe_show_welcome();

    // Optional file argument(s): open each, focusing the last. With no file
    // given, reopen the previous session for this workspace (if enabled).
    if cli.files.is_empty() {
        app.restore_session();
    } else {
        for path in cli.files {
            app.open_initial(&path);
        }
    }

    let mut terminal = ratatui::init();
    let _ = execute!(io::stdout(), EnableMouseCapture);
    // Also request any-motion mouse tracking (xterm mode 1003) so plain hover is
    // reported, not just clicks and drags. This drives the menu mouseover; every
    // other pane ignores button-less motion. Best-effort — terminals without
    // support simply won't send motion events.
    let _ = write!(io::stdout(), "\x1b[?1003h");
    let _ = io::stdout().flush();
    // Query the terminal for graphics support so images can be displayed. Use a
    // short timeout (the default is 2s) so startup never stalls on terminals
    // that don't answer the capability query.
    let query = ratatui_image::picker::cap_parser::QueryStdioOptions {
        timeout: Duration::from_millis(250),
        ..Default::default()
    };
    app.picker = ratatui_image::picker::Picker::from_query_stdio_with_options(query).ok();
    let result = run(&mut terminal, &mut app);
    let _ = write!(io::stdout(), "\x1b[?1003l");
    let _ = execute!(io::stdout(), DisableMouseCapture);
    let _ = io::stdout().flush();
    ratatui::restore();
    app.on_exit();
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> io::Result<()> {
    loop {
        // Drain any streamed output from a running command into the bottom dock,
        // and any finished dashboard metrics into the dashboard panel.
        app.poll_command();
        app.poll_ai_replace();
        app.poll_dashboard();
        app.poll_pomodoro();
        app.poll_terminal();
        app.refresh_inline_blame();
        app.refresh_outline_dock();
        // Drain language-server messages (diagnostics, hover/definition/completion
        // responses) and sync the active document.
        app.poll_lsp();
        app.poll_dap();
        terminal.draw(|frame| ui::draw(app, frame))?;
        if app.should_quit {
            return Ok(());
        }
        // Poll with a timeout so the calendar clock refreshes while idle; poll
        // faster while a command is streaming, dashboard metrics are computing,
        // or a language-server request is in flight so output appears promptly.
        let timeout = if app.command_running() || app.ai_replace_running() || app.dashboard_loading() || app.lsp_busy() || app.pomodoro_running() || app.terminal_running() || app.dap_busy() {
            Duration::from_millis(50)
        } else {
            Duration::from_millis(500)
        };
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) => app.on_key(key),
                Event::Mouse(mouse) => app.on_mouse(mouse),
                _ => {}
            }
        }
        if app.suspend_requested {
            app.suspend_requested = false;
            suspend(terminal);
        }
    }
}

/// Suspend the process to the shell (`Ctrl+Z` style): tear down the terminal,
/// raise `SIGTSTP`, then re-initialize when resumed with `fg`. A no-op off Unix.
fn suspend(terminal: &mut ratatui::DefaultTerminal) {
    #[cfg(unix)]
    {
        let _ = write!(io::stdout(), "\x1b[?1003l");
        let _ = execute!(io::stdout(), DisableMouseCapture);
        let _ = io::stdout().flush();
        ratatui::restore();
        let _ = nix::sys::signal::raise(nix::sys::signal::Signal::SIGTSTP);
        *terminal = ratatui::init();
        let _ = execute!(io::stdout(), EnableMouseCapture);
        let _ = write!(io::stdout(), "\x1b[?1003h");
        let _ = io::stdout().flush();
    }
    #[cfg(not(unix))]
    let _ = terminal;
}
