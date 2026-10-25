//! Startup — the same sequence as the terminal
//! binary (`tui/src/main.rs`), minus the terminal: the real system clipboard,
//! settings (a load failure is reported once the App exists), the locale
//! (`--locale` wins, unsaved), `theme = "auto"` from the OS appearance, then
//! `App::new` and `App::start`.

use std::io;
use std::path::PathBuf;

use vix::app::App;
use vix::cli::Cli;
use vix::settings::Settings;

/// Settings as loaded, plus the load error to report once the App exists.
pub struct Loaded {
    /// The settings (defaults if loading failed).
    pub settings: Settings,
    /// Why loading failed, if it did (a missing file is not a failure).
    pub error: Option<String>,
}

/// Opt into the system clipboard and OS keyring, load settings, and set the
/// locale. Call first, before anything translates text (the native menu bar
/// included).
#[must_use]
pub fn load(cli: &Cli) -> Loaded {
    vix::clipboard::use_system();
    vix::db::secret::use_system_keyring();
    let (settings, error) = match Settings::try_load() {
        Ok(s) => (s, None),
        Err(e) => (Settings::default(), Some(e.to_string())),
    };
    let locale = cli
        .locale
        .clone()
        .unwrap_or_else(|| settings.locale.clone());
    rust_i18n::set_locale(&locale);
    Loaded { settings, error }
}

/// The started App for this working directory, plus the window title.
///
/// `dark` is the OS appearance: with `theme = "auto"` it decides the theme,
/// as the terminal binary's OSC 11 background query does there. Git state is
/// not refreshed yet; call `App::refresh_git` once the window is up (T122).
///
/// # Errors
/// Returns an error only when `-` was given and stdin could not be read.
pub fn start(cli: &Cli, loaded: Loaded, dark: bool) -> io::Result<(App, String)> {
    use vix::theme_model::background::{self, Background};
    if loaded.settings.theme.eq_ignore_ascii_case(background::AUTO) {
        background::set_detected(Some(if dark {
            Background::Dark
        } else {
            Background::Light
        }));
    }
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let title = window_title(&root);
    let mut app = App::new(root, loaded.settings);
    if let Some(e) = &loaded.error {
        app.warn_settings_load_failed(e);
    }
    app.start(cli)?;
    Ok((app, title))
}

/// The window title for a workspace rooted at `root`.
#[must_use]
pub fn window_title(root: &std::path::Path) -> String {
    format!("Vix — {}", root.display())
}

#[cfg(feature = "screenshot")]
/// A scripted run for the render smoke test: with `VIX_GUI_SCREENSHOT=<path>`
/// set, a front end waits for startup, runs `actions`, types `keys` through
/// its real key handling, saves the window as a PNG at `path`, and quits.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Script {
    /// Where to save the PNG.
    pub path: String,
    /// Action ids from `VIX_GUI_SCREENSHOT_ACTIONS` (comma-separated).
    pub actions: Vec<String>,
    /// Keystrokes from `VIX_GUI_SCREENSHOT_KEYS` (space-separated, GPUI
    /// syntax: `ctrl-p`, `shift-t`, `enter`, `a`).
    pub keys: Vec<String>,
}

#[cfg(feature = "screenshot")]
impl Script {
    /// The scripted run the environment asks for, if any.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let path = std::env::var("VIX_GUI_SCREENSHOT").ok()?;
        let actions = std::env::var("VIX_GUI_SCREENSHOT_ACTIONS").unwrap_or_default();
        let keys = std::env::var("VIX_GUI_SCREENSHOT_KEYS").unwrap_or_default();
        Some(Self {
            path,
            actions: actions
                .split(',')
                .filter(|a| !a.is_empty())
                .map(String::from)
                .collect(),
            keys: keys.split_whitespace().map(String::from).collect(),
        })
    }
}
