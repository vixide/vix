//! Project environment activation (T667): resolve the workspace's venv /
//! direnv / mise environment and publish it for everything Vix spawns.
//!
//! Trust gate: `direnv`/`mise` run project-controlled code, so they are
//! resolved only when the workspace's trust decision (T132, persisted in the
//! session store) is a recorded "yes". See `crates/vix-project-env/spec`.

use super::App;

impl App {
    /// The status-bar segment naming the active environment source (T667),
    /// empty when none is active.
    pub(crate) fn project_env_segment(&self) -> String {
        self.project_env_source
            .map(|s| format!("{}   ", t!("ui.project_env_indicator", source = s.label())))
            .unwrap_or_default()
    }

    /// Resolve and publish the environment; returns what was blocked, if
    /// anything, so the caller can say so.
    fn resolve_project_env(&mut self) -> Option<vix_project_env::Source> {
        let resolved = vix_project_env::resolve(&self.root, self.workspace_trusted());
        self.project_env_source = resolved.env.as_ref().map(|e| e.source);
        vix_project_env::set_active(resolved.env);
        resolved.blocked
    }

    /// Startup / trust-accepted variant: no status message unless something
    /// was activated.
    pub(crate) fn reload_project_env_quietly(&mut self) {
        self.resolve_project_env();
    }

    /// `project.reload_env`: re-resolve the environment for later spawns and
    /// report the outcome.
    pub(crate) fn reload_project_env(&mut self) {
        let blocked = self.resolve_project_env();
        self.status = match (self.project_env_source, blocked) {
            (_, Some(source)) => {
                t!("status.project_env_untrusted", source = source.label()).to_string()
            }
            (Some(source), None) => {
                t!("status.project_env_active", source = source.label()).to_string()
            }
            (None, None) => t!("status.project_env_none").to_string(),
        };
    }
}
