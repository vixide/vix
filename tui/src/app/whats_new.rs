//! **Help → What's New** (T826): the release notes for this version,
//! bundled from `CHANGELOG.md` at build time (`tui/build.rs`), shown in the
//! welcome overlay, and opened once on the first start after the version
//! changes. The last version seen is the `last_seen_version` key of
//! `session.toml` (`crates/vix-session/spec/index.md`). Spec:
//! `crates/vix-welcome-panel/spec/index.md`, § What's New.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

use super::{App, WelcomePanel};
use crate::messages::Level;

/// The changelog excerpt `build.rs` cut for this version (may be empty).
const EXCERPT: &str = include_str!(concat!(env!("OUT_DIR"), "/whats_new.md"));

/// The running version, the one **What's New** describes.
const VERSION: &str = env!("CARGO_PKG_VERSION");

impl App {
    /// Open **Help → What's New** in the welcome overlay.
    pub(super) fn open_whats_new(&mut self) {
        self.welcome = Some(WelcomePanel::open(Self::whats_new_lines()));
    }

    /// The overlay text: a translated title, then the bundled release notes
    /// (or a translated note when this build bundled none).
    fn whats_new_lines() -> Vec<String> {
        let mut lines = vec![
            t!("help.whats_new.title", version = VERSION).to_string(),
            String::new(),
        ];
        let notes =
            vix_welcome_panel::whats_new::paragraphs(EXCERPT, &t!("help.whats_new.unreleased"));
        if notes.is_empty() {
            lines.push(t!("help.whats_new.empty").to_string());
        } else {
            lines.extend(notes);
        }
        lines
    }

    /// On start: when `session.toml` records another version than this
    /// one (or none), open **What's New** once and record this version. The
    /// first launch shows the welcome screen instead, so then the version is
    /// only recorded. Called by `main` after [`App::maybe_show_welcome`].
    pub fn maybe_show_whats_new(&mut self) {
        let seen = match &self.session_path {
            Some(path) => crate::session::Session::last_seen_version_from(path),
            None => crate::session::Session::last_seen_version(),
        };
        if seen.as_deref() == Some(VERSION) {
            return;
        }
        if self.welcome.is_none() {
            self.open_whats_new();
        }
        let recorded = match &self.session_path {
            Some(path) => crate::session::Session::record_seen_version_to(path, VERSION),
            None => crate::session::Session::record_seen_version(VERSION),
        };
        if let Err(e) = recorded {
            self.messages.push(
                Level::Warn,
                t!("msg.session_save_failed", error = e).to_string(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    /// An app whose session lives in a fresh temporary `session.toml`.
    fn app(tag: &str) -> (App, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("vix-whats-new-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let session = dir.join("session.toml");
        (
            App::new(dir, Settings::default()).with_session_path(&session),
            session,
        )
    }

    #[test]
    fn help_whats_new_shows_the_bundled_notes_for_this_version() {
        let (mut a, _) = app("menu");
        a.run_action("help.whats_new");
        let w = a.welcome.as_mut().unwrap();
        w.wrap_to(500);
        let lines = w.lines().to_vec();
        assert_eq!(
            lines[0],
            t!("help.whats_new.title", version = VERSION).to_string()
        );
        // The workspace checkout carries CHANGELOG.md, so notes are bundled,
        // starting with [Unreleased] and including this version's section.
        assert_ne!(EXCERPT, "");
        assert!(lines.contains(&t!("help.whats_new.unreleased").to_string()));
        assert!(
            lines.iter().any(|l| l.starts_with(&format!("{VERSION} ("))),
            "{lines:?}"
        );
        assert!(!lines.iter().any(|l| l.starts_with("## ")));
    }

    #[test]
    fn whats_new_opens_once_after_a_version_change() {
        let (mut a, session) = app("once");
        crate::session::Session::record_seen_version_to(&session, "0.0.1").unwrap();
        a.maybe_show_whats_new();
        assert!(a.welcome.is_some(), "a new version opens What's New");
        assert_eq!(
            crate::session::Session::last_seen_version_from(&session).as_deref(),
            Some(VERSION)
        );
        a.welcome = None;
        a.maybe_show_whats_new();
        assert!(
            a.welcome.is_none(),
            "the same version does not open it again"
        );
    }

    #[test]
    fn the_first_launch_welcome_wins_and_the_version_is_still_recorded() {
        let (mut a, session) = app("first");
        a.run_action("help.welcome");
        let welcome_first = a.welcome.as_mut().map(|w| {
            w.wrap_to(500);
            w.lines().first().cloned()
        });
        a.maybe_show_whats_new();
        let after = a.welcome.as_mut().map(|w| {
            w.wrap_to(500);
            w.lines().first().cloned()
        });
        assert_eq!(after, welcome_first, "the welcome screen stays");
        assert_eq!(
            crate::session::Session::last_seen_version_from(&session).as_deref(),
            Some(VERSION)
        );
    }

    #[test]
    fn a_session_save_after_the_check_keeps_the_recorded_version() {
        let (mut a, session) = app("save");
        a.maybe_show_whats_new();
        a.on_exit();
        assert_eq!(
            crate::session::Session::last_seen_version_from(&session).as_deref(),
            Some(VERSION)
        );
    }
}
