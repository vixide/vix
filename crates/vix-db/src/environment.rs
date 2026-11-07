//! Connection environments (T781): the badge a saved connection wears in the
//! connections list and the workbench editor title, and its label.
//!
//! The [`Environment`] type itself lives in `vix-settings` (it is persisted, by
//! connection name, in `Settings::db_environments`) and is re-exported from
//! [`crate::connect`]. What an environment *does* — production confirms every
//! write — is [`crate::editor::needs_confirmation`].
//!
//! Colors follow the Vix theme rules: the built-in themes are monochrome, so
//! there the badge is plain text (dim for `local`, full intensity otherwise);
//! with a custom JSON theme the badge borrows the theme's `syntax` colors —
//! `string` for development, `keyword` for testing, `number` for staging,
//! `comment` for local — and production is always red, the one color every
//! palette reads as danger. Owning spec: `crates/vix-db/spec/index.md`.

use ratatui::style::{Color, Style};

pub use crate::connect::Environment;

/// The i18n key of `env`'s display label (`ui.db_env_local`, …).
///
/// # Examples
///
/// ```
/// use vix_db::connect::Environment;
/// assert_eq!(vix_db::environment::label_key(Environment::Production), "ui.db_env_production");
/// ```
#[must_use]
pub fn label_key(env: Environment) -> &'static str {
    match env {
        Environment::Development => "ui.db_env_development",
        Environment::Testing => "ui.db_env_testing",
        Environment::Staging => "ui.db_env_staging",
        Environment::Production => "ui.db_env_production",
        // `Local`, and any environment a later version adds.
        _ => "ui.db_env_local",
    }
}

/// The translated label of `env`, in brackets, as the badge text.
#[must_use]
pub fn badge(env: Environment) -> String {
    format!("[{}]", t!(label_key(env)))
}

/// The badge style for `env` under the active theme (see the module docs).
#[must_use]
pub fn style(env: Environment) -> Style {
    if vix_theme::custom_name().is_none() {
        return if env == Environment::Local {
            vix_theme::dim()
        } else {
            vix_theme::base()
        };
    }
    let themed = |token: &str, fallback: Color| {
        Style::default().fg(vix_theme::syntax_color(token).unwrap_or(fallback))
    };
    match env {
        Environment::Development => themed("string", Color::Green),
        Environment::Testing => themed("keyword", Color::Cyan),
        Environment::Staging => themed("number", Color::Yellow),
        Environment::Production => Style::default().fg(Color::Red),
        _ => themed("comment", Color::DarkGray),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_environment_has_a_distinct_label_key_and_a_badge() {
        let keys: std::collections::BTreeSet<_> =
            Environment::ALL.iter().map(|e| label_key(*e)).collect();
        assert_eq!(keys.len(), Environment::ALL.len());
        for env in Environment::ALL {
            assert!(badge(env).starts_with('['));
        }
    }

    #[test]
    fn environments_cycle_through_all_five_and_default_to_local() {
        assert_eq!(Environment::default(), Environment::Local);
        let mut env = Environment::Local;
        for expected in Environment::ALL.iter().skip(1) {
            env = env.next();
            assert_eq!(env, *expected);
        }
        assert_eq!(env.next(), Environment::Local);
        assert!(Environment::Production.is_production());
        assert!(!Environment::Staging.is_production());
    }
}
