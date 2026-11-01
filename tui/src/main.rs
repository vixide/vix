//! Vix: Simple Terminal Rust IDE — binary entry point.
//!
//! Sets up the terminal (with mouse capture), runs the event loop, and restores
//! on exit. All of the application logic lives in the `vix` library crate.

// Always start with high quality coding conventions.

// When we build for MUSL static, use faster memory allocator.
#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use crossterm::event::{
    self, DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
    EnableFocusChange, EnableMouseCapture, Event,
};
#[cfg(target_os = "macos")]
use crossterm::event::{
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
use crossterm::{execute, queue};

use vix::app::{App, AppFlags};
use vix::cli::Cli;
use vix::settings::Settings;
use vix::ui;

/// Ask the terminal for its background colour (OSC 11) and classify it,
/// falling back to `COLORFGBG`, then to "unknown" (the caller treats that as
/// dark). The reply arrives as a burst of key events, which are consumed here;
/// a terminal that does not answer costs at most [`BACKGROUND_QUERY_TIMEOUT`].
fn detect_terminal_background() -> Option<vix::theme_model::background::Background> {
    use vix::theme_model::background as bg;
    let from_terminal = query_background_colour().map(bg::classify);
    from_terminal.or_else(|| {
        std::env::var("COLORFGBG")
            .ok()
            .and_then(|v| bg::from_colorfgbg(&v))
    })
}

/// How long to wait for a terminal's answer to the OSC 11 query.
const BACKGROUND_QUERY_TIMEOUT: Duration = Duration::from_millis(200);

/// Send the OSC 11 query and collect the reply's characters until its
/// terminator (BEL, or ESC `\`) or the timeout; `None` when there is no answer.
fn query_background_colour() -> Option<(u8, u8, u8)> {
    use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
    crossterm::terminal::enable_raw_mode().ok()?;
    let mut out = io::stdout();
    if write!(out, "\x1b]11;?\x07")
        .and_then(|()| out.flush())
        .is_err()
    {
        let _ = crossterm::terminal::disable_raw_mode();
        return None;
    }
    let mut reply = String::new();
    let deadline = std::time::Instant::now() + BACKGROUND_QUERY_TIMEOUT;
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        if !crossterm::event::poll(left).unwrap_or(false) {
            break;
        }
        let Ok(Event::Key(key)) = crossterm::event::read() else {
            continue;
        };
        if key.kind == KeyEventKind::Release {
            continue;
        }
        match key.code {
            // BEL arrives as Ctrl+G; the string terminator as ESC then `\`.
            KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
            KeyCode::Char('\\') if reply.contains("rgb:") => break,
            KeyCode::Char(c) => reply.push(c),
            _ => {}
        }
    }
    let _ = crossterm::terminal::disable_raw_mode();
    vix::theme_model::background::parse_osc11_reply(&reply)
}

/// The modes that print or serve and exit before anything else runs (no
/// settings load, which would create `config.toml`; no terminal, clipboard
/// or keyring): `--version`, `--remote-agent` and `--mcp`. Returns whether
/// one ran; a failed server exits with status 1.
fn run_early_modes(cli: &Cli) -> bool {
    if cli.version {
        if cli.json {
            let capabilities = vix::capability::enabled_names()
                .iter()
                .map(|c| format!("\"{c}\""))
                .collect::<Vec<_>>()
                .join(",");
            println!(
                r#"{{"name":"vix","version":"{}","capabilities":[{capabilities}]}}"#,
                env!("CARGO_PKG_VERSION")
            );
        } else {
            println!("vix {}", env!("CARGO_PKG_VERSION"));
        }
        return true;
    }
    // `--remote-agent` (T717): a Vix on another machine drives this process
    // over the SSH channel. Exits when its input ends.
    if cli.remote_agent {
        if vix::authority::agent::serve_stdio().is_err() {
            std::process::exit(1);
        }
        return true;
    }
    // `--mcp` (T731).
    if cli.mcp {
        if !vix::mcp::run(&cli.files, cli.locale.as_deref()) {
            std::process::exit(1);
        }
        return true;
    }
    false
}

fn main() -> io::Result<()> {
    let mut cli = Cli::parse();

    if run_early_modes(&cli) {
        return Ok(());
    }

    // Opt this process into the real system clipboard. Everything that does not
    // call this — the test suite above all — copies and pastes through an
    // in-memory clipboard instead, so running the tests cannot overwrite what
    // the user had on their pasteboard.
    vix::clipboard::use_system();
    // Likewise the OS keyring for saved database passwords (T639).
    vix::db::secret::use_system_keyring();

    // T537 (Run H): `Settings::load()`'s own fallback silently discarded a
    // genuine load failure (a missing file is *not* one -- `confy` already
    // returns/writes `Settings::default()` transparently for that case, so
    // an `Err` here always means something real, e.g. a TOML syntax error in
    // an existing file). Caught here instead, so `app.warn_settings_load_
    // failed` can report it once the `App` (and its message drawer) exists.
    let (settings, settings_load_error) = match Settings::try_load() {
        Ok(s) => (s, None),
        Err(e) => (Settings::default(), Some(e.to_string())),
    };
    // T625: pick the clipboard provider (system / OSC 52) from the setting and
    // the real environment (SSH, tmux) — read here once, never set.
    vix::clipboard::configure(
        vix::clipboard::Provider::parse(&settings.clipboard_provider),
        vix::clipboard::Environment::current(),
    );

    // A `--locale` flag wins over the persisted setting, but is not saved
    // back. Set *before* `--doctor`/`--export-settings`/`--import-settings`
    // below (T561: once `vix-doctor` actually called `t!()`, leaving this
    // until after those early-return paths would have made `--locale es
    // --doctor` silently ignore `--locale` -- worth fixing now that it
    // would otherwise be a real, avoidable inconsistency, not "the CLI
    // path just doesn't localize" the way `--version`'s own plain text
    // still reasonably doesn't).
    let locale = cli
        .locale
        .clone()
        .unwrap_or_else(|| settings.locale.clone());
    rust_i18n::set_locale(&locale);

    if cli.doctor {
        let checks = vix::capability::doctor_checks(&settings);
        for line in vix::doctor::format_report(&checks) {
            println!("{line}");
        }
        return Ok(());
    }

    // Same early-return shape as `--doctor` above: both exit before the TUI
    // starts, so neither needs a running `App`/terminal at all. Grouped into
    // one helper (rather than inlined here) to keep `main` under clippy's
    // line-count limit.
    if handle_settings_bundle_flags(&cli, &settings) {
        return Ok(());
    }
    if let Some(args) = &cli.plugin {
        handle_plugin_command(args);
    }

    // `theme = "auto"`: find out whether the terminal is light or dark before the
    // App applies its theme (T579). Nothing is asked for any other theme value.
    if settings
        .theme
        .eq_ignore_ascii_case(vix::theme_model::background::AUTO)
    {
        vix::theme_model::background::set_detected(detect_terminal_background());
    }

    // `vix ssh://user@host/path` (T712), or a container name such as
    // `vix docker://web/srv` (T718): connect now, while `ssh` can still
    // prompt on the plain terminal, and work on the remote directory.
    let remote = match vix::remote::connect_cli(&mut cli.files, &settings.remote_agent) {
        Ok(target) => target,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    let root = remote.as_ref().map_or_else(
        || std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        |(t, _)| PathBuf::from(t.path()),
    );
    let mut app = App::new(root, settings);
    // T663: opt-in (never in `App::new`) so tests do not read the real home.
    app.notice_interrupted_saves();
    // The status-bar host, conflict-checked saves and the trust prompt
    // (T713, T714), before `start` loads scripts or restores the session.
    if let Some((target, agent_note)) = remote {
        app.adopt_remote(target);
        if let Some(note) = agent_note {
            app.messages.info(note);
        }
    }
    if let Some(e) = settings_load_error {
        app.warn_settings_load_failed(&e);
    }
    // Graphics support is probed the first time an image is shown, not here:
    // the probe is a round trip of terminal queries every launch would pay
    // for (T749). Installed before `start`, which may open an image given
    // on the command line.
    #[cfg(feature = "image")]
    {
        app.image_probe = Some(probe_terminal_graphics);
    }
    // Color depth and hyperlink support, from the environment only: no
    // terminal query, so startup pays nothing for it (T628, T749).
    app.terminal_caps = vix::term_caps::TerminalCaps::from_process_env();
    // Scripts, key overrides, the welcome screen, and the initial files /
    // session / stdin buffer / `--diff` — shared with the GUI front end.
    app.start(&cli)?;

    let mut terminal = ratatui::init();
    let _ = execute!(io::stdout(), EnableMouseCapture);
    // Ask the terminal to bracket pastes, so pasted text arrives as one
    // `Event::Paste` instead of as one key event per character: that is what
    // lets a paste be a single edit (and a single undo step) and keeps
    // auto-indent from re-indenting each pasted line. Best-effort — terminals
    // without support simply keep sending keys.
    let _ = execute!(io::stdout(), EnableBracketedPaste);
    // Ask for focus reports (xterm mode 1004): gaining focus checks open files
    // for outside changes at once, and losing it can auto-save (T628).
    // Best-effort — terminals without support never send them.
    let _ = execute!(io::stdout(), EnableFocusChange);
    // macOS: ask for the kitty keyboard protocol's disambiguated key encoding.
    // It is the only way a terminal can report the `Command` modifier at all —
    // crossterm sets `KeyModifiers::SUPER` only with these flags on — and Vix
    // folds Command into Control (`App::command_as_control`). Best-effort: a
    // terminal without support ignores the request and keeps its old encoding,
    // and one that keeps `Cmd` for its own menus never forwards it.
    #[cfg(target_os = "macos")]
    let _ = execute!(
        io::stdout(),
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    );
    // Also request any-motion mouse tracking (xterm mode 1003) so plain hover is
    // reported, not just clicks and drags. This drives the menu mouseover; every
    // other pane ignores button-less motion. Best-effort — terminals without
    // support simply won't send motion events.
    let _ = write!(io::stdout(), "\x1b[?1003h");
    let _ = io::stdout().flush();
    // Draw once before the (comparatively expensive, git-subprocess-based)
    // initial refresh, so the terminal shows real content immediately
    // rather than sitting on whatever was on screen before Vix started for
    // however long that refresh takes (T122). `run`'s own loop redraws with
    // the real git state on its first iteration.
    let _ = draw(&mut terminal, &mut app);
    app.refresh_git();
    let result = run(&mut terminal, &mut app);
    let _ = write!(io::stdout(), "\x1b[?1003l");
    #[cfg(target_os = "macos")]
    let _ = execute!(io::stdout(), PopKeyboardEnhancementFlags);
    let _ = execute!(
        io::stdout(),
        DisableFocusChange,
        DisableBracketedPaste,
        DisableMouseCapture
    );
    let _ = io::stdout().flush();
    ratatui::restore();
    app.on_exit();
    result
}

/// Ask the terminal which image protocol it speaks (kitty, iTerm2, Sixel, or
/// half blocks) and its cell size, for [`App::image_probe`]. The timeout is
/// short (the library's default is 2 s) so a terminal that never answers
/// the queries costs at most a quarter second, once.
#[cfg(feature = "image")]
fn probe_terminal_graphics() -> Option<ratatui_image::picker::Picker> {
    let query = ratatui_image::picker::cap_parser::QueryStdioOptions {
        timeout: Duration::from_millis(250),
        ..Default::default()
    };
    ratatui_image::picker::Picker::from_query_stdio_with_options(query).ok()
}

/// Draw one frame inside a synchronized update (DEC mode 2026): the terminal
/// holds the screen until the whole frame has arrived, so a large redraw
/// never shows half-painted (T628). Terminals without the mode ignore it.
fn draw(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> io::Result<()> {
    queue!(io::stdout(), BeginSynchronizedUpdate)?;
    let drawn = terminal.draw(|frame| ui::draw(app, frame)).map(|_| ());
    // End the update even when the draw failed, or the screen stays frozen.
    execute!(io::stdout(), EndSynchronizedUpdate)?;
    drawn
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> io::Result<()> {
    loop {
        // Background work shared with the GUI front end (`App::tick`).
        app.tick();
        draw(terminal, app)?;
        if app.flags.contains(AppFlags::SHOULD_QUIT) {
            return Ok(());
        }
        // Poll with a timeout so the calendar clock refreshes while idle, and
        // faster while something is streaming or in flight.
        let timeout = app.poll_interval();
        if event::poll(timeout)? {
            match event::read()? {
                // The one place crossterm events become backend-neutral
                // `vix-input` events (the GUI front end converts its own).
                Event::Key(key) => app.on_key(key.into()),
                Event::Mouse(mouse) => app.on_mouse(mouse.into()),
                Event::Paste(text) => app.on_paste(&text),
                Event::FocusGained => app.on_focus_gained(),
                Event::FocusLost => app.on_focus_lost(),
                Event::Resize(..) => {}
            }
        }
        if app.flags.contains(AppFlags::SUSPEND_REQUESTED) {
            app.flags.remove(AppFlags::SUSPEND_REQUESTED);
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
        #[cfg(target_os = "macos")]
        let _ = execute!(io::stdout(), PopKeyboardEnhancementFlags);
        let _ = execute!(
            io::stdout(),
            DisableFocusChange,
            DisableBracketedPaste,
            DisableMouseCapture
        );
        let _ = io::stdout().flush();
        ratatui::restore();
        let _ = nix::sys::signal::raise(nix::sys::signal::Signal::SIGTSTP);
        *terminal = ratatui::init();
        let _ = execute!(
            io::stdout(),
            EnableMouseCapture,
            EnableBracketedPaste,
            EnableFocusChange
        );
        #[cfg(target_os = "macos")]
        let _ = execute!(
            io::stdout(),
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        );
        let _ = write!(io::stdout(), "\x1b[?1003h");
        let _ = io::stdout().flush();
    }
    #[cfg(not(unix))]
    let _ = terminal;
}

/// Handle `vix --plugin …` (T637): print the report and exit — with status 1
/// when the command failed, so a script driving it can tell.
fn handle_plugin_command(args: &[String]) -> ! {
    let report = vix::plugin_cli::run_cli(args);
    for line in &report.lines {
        if report.ok {
            println!("{line}");
        } else {
            eprintln!("{line}");
        }
    }
    std::process::exit(i32::from(!report.ok));
}

/// Handle `--export-settings`/`--import-settings` (T555), printing a short
/// report to stdout/stderr. Returns `true` if either flag was present (the
/// caller should exit without starting the TUI); `false` otherwise.
fn handle_settings_bundle_flags(cli: &Cli, settings: &Settings) -> bool {
    if let Some(path) = &cli.export_settings {
        let bundle = vix::settings_bundle::collect(settings);
        match vix::settings_bundle::write(&bundle, path) {
            Ok(()) => println!("Wrote settings bundle to {}", path.display()),
            Err(e) => eprintln!("Failed to write settings bundle to {}: {e}", path.display()),
        }
        return true;
    }
    if let Some(path) = &cli.import_settings {
        match vix::settings_bundle::read(path) {
            Ok(bundle) => {
                for (name, outcome, preserved) in vix::settings_bundle::apply(&bundle) {
                    let note = match outcome {
                        vix::settings_bundle::EntryOutcome::Written => "written",
                        vix::settings_bundle::EntryOutcome::WrittenAfterBackup => {
                            "written (previous file backed up to .bak)"
                        }
                        vix::settings_bundle::EntryOutcome::Skipped => "skipped",
                    };
                    println!("{name}: {note}");
                    if preserved {
                        println!(
                            "  (ai_command/ai_api_key_command/test_command/lsp_servers differed \
                             from your current config and were NOT imported -- edit config.toml \
                             by hand if you really want them)"
                        );
                    }
                }
            }
            Err(e) => eprintln!(
                "Failed to read settings bundle from {}: {e}",
                path.display()
            ),
        }
        return true;
    }
    false
}
