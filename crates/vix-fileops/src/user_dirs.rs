//! Where Vix keeps the user's configuration and data, in one place.
//!
//! Settings, the saved session, DB history and the saved queries live in the
//! user's configuration directory (`~/.config/vix/` on Linux and macOS), and
//! IM history and crash reports in the data directory. Every lookup goes through
//! this module so a test build can swap the real directories for a throwaway
//! one: the `tui` tests build an `App` that persists settings, the session and
//! history, and before this module an ordinary `cargo test` rewrote the
//! developer's real configuration.
//!
//! The `isolated-user-dirs` feature does that swap. Only dev-dependencies turn it
//! on, so a normal build never has it; with it on, every path below is inside a
//! per-process directory under the system temporary directory and the real home
//! is never read or written. Owning spec: `crates/vix-fileops/spec/index.md`.

use std::path::PathBuf;

use serde::Serialize;
use serde::de::DeserializeOwned;

// With `isolated-user-dirs` (tests only) the real directories are never looked
// up, so `etcetera` is unused in that build; name it so Cargo's
// unused-dependency check does not flag the normal dependency.
#[cfg(feature = "isolated-user-dirs")]
use etcetera as _;

/// Whether the user directories are isolated from the real home (the
/// `isolated-user-dirs` feature). Tests that write configuration check this
/// before they do.
///
/// # Examples
///
/// ```
/// // Without the feature the real directories are used.
/// assert_eq!(vix_fileops::user_dirs::is_isolated(), cfg!(feature = "isolated-user-dirs"));
/// ```
#[must_use]
pub const fn is_isolated() -> bool {
    cfg!(feature = "isolated-user-dirs")
}

/// The per-process root that stands in for the home directory when isolated.
#[cfg(feature = "isolated-user-dirs")]
fn isolated_root() -> PathBuf {
    std::env::temp_dir().join(format!("vix-isolated-home-{}", std::process::id()))
}

/// The configuration file `<config dir>/<app>/<name>.toml`, or `None` when the
/// platform has no configuration directory.
///
/// # Examples
///
/// ```
/// let path = vix_fileops::user_dirs::config_file("vix", "config").unwrap();
/// assert!(path.ends_with("vix/config.toml"));
/// ```
#[must_use]
pub fn config_file(app: &str, name: &str) -> Option<PathBuf> {
    #[cfg(feature = "isolated-user-dirs")]
    {
        Some(
            isolated_root()
                .join("config")
                .join(app)
                .join(format!("{name}.toml")),
        )
    }
    #[cfg(not(feature = "isolated-user-dirs"))]
    {
        confy::get_configuration_file_path(app, Some(name)).ok()
    }
}

/// The data directory for `app` (`~/.local/share/<app>` on Linux and macOS), or
/// `None` when the home directory cannot be found.
///
/// # Examples
///
/// ```
/// let dir = vix_fileops::user_dirs::data_dir("vix").unwrap();
/// assert!(dir.ends_with("vix"));
/// ```
#[must_use]
pub fn data_dir(app: &str) -> Option<PathBuf> {
    #[cfg(feature = "isolated-user-dirs")]
    {
        Some(isolated_root().join("data").join(app))
    }
    #[cfg(not(feature = "isolated-user-dirs"))]
    {
        use etcetera::BaseStrategy as _;
        etcetera::choose_base_strategy()
            .ok()
            .map(|s| s.data_dir().join(app))
    }
}

/// Load `<config dir>/<app>/<name>.toml`, writing the default out when the file
/// is missing (as `confy::load` does).
///
/// # Errors
///
/// A [`confy::ConfyError`] when the configuration directory cannot be found or
/// the file cannot be read or parsed.
///
/// # Examples
///
/// ```
/// use serde::{Deserialize, Serialize};
/// #[derive(Default, Serialize, Deserialize)]
/// struct Demo { n: u32 }
/// // A missing file yields the default (this writes it under the user dir).
/// let _ = vix_fileops::user_dirs::load::<Demo>("vix-doctest", "demo");
/// ```
pub fn load<T: Serialize + DeserializeOwned + Default>(
    app: &str,
    name: &str,
) -> Result<T, confy::ConfyError> {
    match config_file(app, name) {
        Some(path) => confy::load_path(path),
        None => Err(confy::ConfyError::BadConfigDirectory(
            "no configuration directory".to_string(),
        )),
    }
}

/// Store `value` as `<config dir>/<app>/<name>.toml`, creating the directory.
///
/// # Errors
///
/// A [`confy::ConfyError`] when the configuration directory cannot be found or
/// the file cannot be written.
pub fn store<T: Serialize>(app: &str, name: &str, value: &T) -> Result<(), confy::ConfyError> {
    match config_file(app, name) {
        Some(path) => confy::store_path(path, value),
        None => Err(confy::ConfyError::BadConfigDirectory(
            "no configuration directory".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_feature_flag_matches_the_build() {
        assert_eq!(is_isolated(), cfg!(feature = "isolated-user-dirs"));
    }

    #[test]
    fn config_file_has_the_confy_shape() {
        let path = config_file("vix", "config").expect("a config dir");
        assert!(path.ends_with("vix/config.toml"), "{path:?}");
    }

    #[test]
    fn data_dir_ends_in_the_app_name() {
        let dir = data_dir("vix").expect("a data dir");
        assert!(dir.ends_with("vix"), "{dir:?}");
    }

    #[cfg(feature = "isolated-user-dirs")]
    #[test]
    fn isolated_dirs_never_touch_the_real_home() {
        let tmp = std::env::temp_dir();
        for path in [
            config_file("vix", "config").expect("config"),
            config_file("vix", "session").expect("session"),
            data_dir("vix").expect("data"),
        ] {
            assert!(path.starts_with(&tmp), "{path:?} is outside {tmp:?}");
            assert!(
                path.to_string_lossy().contains("vix-isolated-home-"),
                "{path:?}"
            );
        }
    }

    #[cfg(feature = "isolated-user-dirs")]
    #[test]
    fn isolated_store_and_load_round_trip_in_the_temp_dir() {
        use serde::Deserialize;
        #[derive(Default, Debug, PartialEq, Serialize, Deserialize)]
        struct Demo {
            n: u32,
        }
        store("vix-test", "demo", &Demo { n: 7 }).expect("store");
        let back: Demo = load("vix-test", "demo").expect("load");
        assert_eq!(back, Demo { n: 7 });
        let path = config_file("vix-test", "demo").expect("path");
        assert!(path.is_file());
        assert!(path.starts_with(std::env::temp_dir()));
    }
}
