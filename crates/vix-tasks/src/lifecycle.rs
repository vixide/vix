//! Lifecycle command resolution: project-type defaults, per-project
//! overrides, and a resolved-command cache, merged by a documented
//! precedence.

use crate::project_type::ProjectType;

/// The six lifecycle command slots: configure, compile, test, install,
/// package, and run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LifecycleCommands {
    /// Command that prepares the project to build.
    pub configure: Option<String>,
    /// Command that builds/compiles the project.
    pub compile: Option<String>,
    /// Command that runs the project's test suite.
    pub test: Option<String>,
    /// Command that installs the project.
    pub install: Option<String>,
    /// Command that produces a distributable package/artifact.
    pub package: Option<String>,
    /// Command that runs the project.
    pub run: Option<String>,
}

/// Merge the default commands of every detected project type for a
/// directory. When more than one type defines the same slot, the first type
/// in `types` wins for that slot (`types` is typically
/// [`detect_project_types`](crate::project_type::detect_project_types)'s
/// output, whose order matches [`PROJECT_TYPES`](crate::project_type::PROJECT_TYPES)).
#[must_use]
pub fn default_lifecycle(types: &[&ProjectType]) -> LifecycleCommands {
    LifecycleCommands {
        configure: first_some(types, |t| t.configure_cmd),
        compile: first_some(types, |t| t.compile_cmd),
        test: first_some(types, |t| t.test_cmd),
        install: first_some(types, |t| t.install_cmd),
        package: first_some(types, |t| t.package_cmd),
        run: first_some(types, |t| t.run_cmd),
    }
}

fn first_some(
    types: &[&ProjectType],
    slot: impl Fn(&ProjectType) -> Option<&'static str>,
) -> Option<String> {
    types.iter().find_map(|t| slot(t)).map(ToOwned::to_owned)
}

/// Apply override precedence: for each slot, `cached` wins if set, else
/// `project_override`, else `default` — a previously resolved/edited command
/// always takes precedence over a per-project override, which in turn takes
/// precedence over the project type's default.
///
/// `project_override` is a plain [`LifecycleCommands`] the host builds
/// however it stores per-project configuration (fields present = configured,
/// `None` = not configured) — e.g. from a per-project override file. `cached`
/// is the host's per-project command cache (fields present = previously
/// resolved/edited and cached).
#[must_use]
pub fn effective_lifecycle(
    default: &LifecycleCommands,
    project_override: &LifecycleCommands,
    cached: &LifecycleCommands,
) -> LifecycleCommands {
    LifecycleCommands {
        configure: resolve(
            default.configure.as_deref(),
            project_override.configure.as_deref(),
            cached.configure.as_deref(),
        ),
        compile: resolve(
            default.compile.as_deref(),
            project_override.compile.as_deref(),
            cached.compile.as_deref(),
        ),
        test: resolve(
            default.test.as_deref(),
            project_override.test.as_deref(),
            cached.test.as_deref(),
        ),
        install: resolve(
            default.install.as_deref(),
            project_override.install.as_deref(),
            cached.install.as_deref(),
        ),
        package: resolve(
            default.package.as_deref(),
            project_override.package.as_deref(),
            cached.package.as_deref(),
        ),
        run: resolve(
            default.run.as_deref(),
            project_override.run.as_deref(),
            cached.run.as_deref(),
        ),
    }
}

fn resolve(
    default: Option<&str>,
    project_override: Option<&str>,
    cached: Option<&str>,
) -> Option<String> {
    cached
        .or(project_override)
        .or(default)
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_type::{PROJECT_TYPES, detect_project_types};

    #[test]
    fn default_lifecycle_first_type_wins_per_slot() {
        let types = detect_project_types(&["Cargo.toml", "package.json"]);
        let merged = default_lifecycle(&types);
        // cargo is registered before npm, so cargo's compile command wins.
        assert_eq!(merged.compile.as_deref(), Some("cargo build"));
    }

    #[test]
    fn default_lifecycle_falls_through_to_second_type_when_first_has_none() {
        // `make` has no package_cmd; pair it with `just`, which has no
        // package_cmd either, then `cargo`, which does.
        let make = PROJECT_TYPES.iter().find(|t| t.name == "make").unwrap();
        let cargo = PROJECT_TYPES.iter().find(|t| t.name == "cargo").unwrap();
        let merged = default_lifecycle(&[make, cargo]);
        assert_eq!(merged.package.as_deref(), Some("cargo build --release"));
        assert_eq!(merged.compile.as_deref(), Some("make"));
    }

    #[test]
    fn default_lifecycle_empty_types_is_all_none() {
        assert_eq!(default_lifecycle(&[]), LifecycleCommands::default());
    }

    #[test]
    fn effective_lifecycle_precedence_cached_over_override_over_default() {
        let default = LifecycleCommands {
            compile: Some("default build".into()),
            ..Default::default()
        };
        let over = LifecycleCommands {
            compile: Some("override build".into()),
            ..Default::default()
        };
        let cached = LifecycleCommands {
            compile: Some("cached build".into()),
            ..Default::default()
        };
        let empty = LifecycleCommands::default();

        assert_eq!(
            effective_lifecycle(&default, &empty, &empty)
                .compile
                .as_deref(),
            Some("default build")
        );
        assert_eq!(
            effective_lifecycle(&default, &over, &empty)
                .compile
                .as_deref(),
            Some("override build")
        );
        assert_eq!(
            effective_lifecycle(&default, &over, &cached)
                .compile
                .as_deref(),
            Some("cached build")
        );
        assert_eq!(
            effective_lifecycle(&default, &empty, &cached)
                .compile
                .as_deref(),
            Some("cached build")
        );
    }

    #[test]
    fn effective_lifecycle_all_none_stays_none() {
        let empty = LifecycleCommands::default();
        assert_eq!(effective_lifecycle(&empty, &empty, &empty), empty);
    }
}
