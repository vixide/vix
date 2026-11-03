//! What the terminal can show (T628): its color depth and whether to emit
//! OSC 8 hyperlinks.
//!
//! Decided once at startup from `COLORTERM`, `TERM` and `TERM_PROGRAM`. The
//! environment is only read, never changed, and the terminal is never
//! queried: a startup round trip is what T749 removed for speed. The
//! decision itself is pure ([`TerminalCaps::detect`] over a
//! [`TermEnv`]), so tests pass a fake environment.

use crate::theme::color_depth::{self, ColorDepth, TermEnv};

/// The terminal features the renderer adapts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCaps {
    /// How many colors to draw with; every frame is downsampled to this.
    pub color_depth: ColorDepth,
    /// Whether to wrap URLs in OSC 8 hyperlink escapes (the markdown preview
    /// and the messages dock).
    pub hyperlinks: bool,
}

impl Default for TerminalCaps {
    /// Truecolor with no hyperlinks: what an `App` built without a terminal
    /// (tests, the GUI front ends) draws with.
    fn default() -> Self {
        TerminalCaps {
            color_depth: ColorDepth::TrueColor,
            hyperlinks: false,
        }
    }
}

impl TerminalCaps {
    /// Decide the capabilities from `env`.
    #[must_use]
    pub fn detect(env: &TermEnv<'_>) -> Self {
        TerminalCaps {
            color_depth: color_depth::detect(env),
            hyperlinks: hyperlinks_supported(env),
        }
    }

    /// [`TerminalCaps::detect`] over this process's environment.
    #[must_use]
    pub fn from_process_env() -> Self {
        Self::detect(&ProcessTermEnv::read().as_env())
    }
}

/// Whether the terminal described by `env` should get OSC 8 hyperlinks.
/// Terminals without support ignore the sequence, so only the consoles known
/// to print it as text (`dumb`, `linux`, `vt*`, `cons*`) are excluded.
#[must_use]
pub fn hyperlinks_supported(env: &TermEnv<'_>) -> bool {
    let term = env.term.unwrap_or("").trim().to_ascii_lowercase();
    !(term == "dumb" || term == "linux" || term.starts_with("vt") || term.starts_with("cons"))
}

/// An owned copy of the environment variables [`TermEnv`] borrows.
#[derive(Debug, Clone, Default)]
pub struct ProcessTermEnv {
    /// `COLORTERM`.
    pub colorterm: Option<String>,
    /// `TERM`.
    pub term: Option<String>,
    /// `TERM_PROGRAM`.
    pub term_program: Option<String>,
}

impl ProcessTermEnv {
    /// Read the three variables from this process's environment.
    #[must_use]
    pub fn read() -> Self {
        let var = |name: &str| std::env::var(name).ok();
        ProcessTermEnv {
            colorterm: var("COLORTERM"),
            term: var("TERM"),
            term_program: var("TERM_PROGRAM"),
        }
    }

    /// Borrow the values as a [`TermEnv`].
    #[must_use]
    pub fn as_env(&self) -> TermEnv<'_> {
        TermEnv {
            colorterm: self.colorterm.as_deref(),
            term: self.term.as_deref(),
            term_program: self.term_program.as_deref(),
        }
    }
}

/// A `--doctor` / Help → Run Diagnostics row naming the color depth Vix
/// draws with and the variables it was chosen from.
#[must_use]
pub fn doctor_check(env: &TermEnv<'_>) -> vix_doctor::Check {
    let unset = || t!("doctor.env_unset").to_string();
    let show = |v: Option<&str>| v.map_or_else(unset, str::to_string);
    vix_doctor::Check {
        name: t!("doctor.name_color_depth").to_string(),
        status: vix_doctor::Status::Pass,
        detail: t!(
            "doctor.color_depth_detail",
            depth = color_depth::detect(env).name(),
            colorterm = show(env.colorterm),
            term = show(env.term),
            program = show(env.term_program)
        )
        .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Detection is driven by an injected [`TermEnv`], never the real process
    /// environment, so these cases are deterministic.
    #[test]
    fn detects_depth_and_hyperlinks_from_a_fake_environment() {
        let apple = TermEnv {
            colorterm: None,
            term: Some("xterm-256color"),
            term_program: Some("Apple_Terminal"),
        };
        let caps = TerminalCaps::detect(&apple);
        assert_eq!(caps.color_depth, ColorDepth::Ansi256);
        assert!(caps.hyperlinks);

        let console = TermEnv {
            term: Some("linux"),
            ..TermEnv::default()
        };
        let caps = TerminalCaps::detect(&console);
        assert_eq!(caps.color_depth, ColorDepth::Ansi16);
        assert!(!caps.hyperlinks);

        let modern = TermEnv {
            colorterm: Some("truecolor"),
            term: Some("xterm-256color"),
            term_program: None,
        };
        assert_eq!(
            TerminalCaps::detect(&modern).color_depth,
            ColorDepth::TrueColor
        );
    }

    /// An `App` built without a terminal draws in truecolor with plain
    /// (non-hyperlink) URLs.
    #[test]
    fn the_default_is_truecolor_without_hyperlinks() {
        let caps = TerminalCaps::default();
        assert_eq!(caps.color_depth, ColorDepth::TrueColor);
        assert!(!caps.hyperlinks);
    }

    /// The `doctor` row passes and shows both the depth and the `TERM` value.
    #[test]
    fn doctor_row_names_the_depth() {
        let env = TermEnv {
            term: Some("xterm-256color"),
            ..TermEnv::default()
        };
        let check = doctor_check(&env);
        assert_eq!(check.status, vix_doctor::Status::Pass);
        assert!(check.detail.contains("256"), "{}", check.detail);
        assert!(check.detail.contains("xterm-256color"), "{}", check.detail);
    }
}
