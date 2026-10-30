//! Container workspaces from the **OC** menu (T718): **Open Workspace in
//! Container…** (`w` on a running container in the list) and **Reopen in Dev
//! Container**.
//!
//! Both end in the same place as **File → Open Remote…**: a
//! `ContainerAuthority` installed in the authority slot by
//! `App::poll_remote`, with the status-bar name, connection-lost state,
//! reconnect and trust of any remote workspace
//! (`tui/src/app/remote_session.rs`). See `spec/remote-editing`.
//!
//! The dev container is built and started by the `devcontainer` CLI (`up
//! --workspace-folder <root>`); Vix only reads its result line and two
//! fields of `devcontainer.json` as fallbacks. Building runs the project's
//! Dockerfile, features and lifecycle commands, so it is asked for first
//! (the T714 trust gate for a workspace that is still local), and the
//! container workspace that results counts as trusted.

use std::path::PathBuf;
use std::time::Instant;

use vix_authority::container::{ContainerTarget, ExecKind};
use vix_authority::remote::RemoteTarget;
use vix_input::{KeyCode, KeyEvent};

use crate::oci::devcontainer;
use crate::oci::{Engine, OciError};

use super::super::remote_session::connect_worker;
use super::super::{App, Confirm, ConfirmKind, Prompt, PromptKind};

/// The exec kind an OCI engine's containers are opened with, if any.
fn exec_kind(engine: Engine) -> Option<ExecKind> {
    match engine {
        Engine::Docker => Some(ExecKind::Docker),
        Engine::Podman => Some(ExecKind::Podman),
        _ => None,
    }
}

impl App {
    /// **OC → Open Workspace in Container…**: the container list, where `w`
    /// on a running container opens a workspace in it.
    pub(in crate::app) fn oc_open_workspace(&mut self) {
        self.open_oc(crate::oci::argv::Kind::Container, None);
        if self.oc.is_some() {
            self.status = t!("status.oc_pick_workspace").to_string();
        }
    }

    /// `w` on a running container: the Open Remote prompt, filled in with
    /// the container's name and `/`, for the user to finish the path.
    pub(in crate::app) fn oc_workspace_prompt(&mut self, name: &str) {
        let Some(engine) = self.oc.as_ref().map(|p| p.engine) else {
            return;
        };
        let Some(kind) = exec_kind(engine) else {
            self.status = t!("status.oc_workspace_engine", name = engine.binary()).to_string();
            return;
        };
        let target = match ContainerTarget::new(kind, name).and_then(|t| t.with_path("/")) {
            Ok(t) => t,
            Err(e) => {
                self.status = t!("msg.oc_invalid", error = e).to_string();
                return;
            }
        };
        self.oc = None;
        self.prompt = Some(
            Prompt::new(PromptKind::OpenRemote, t!("prompt.open_remote").to_string())
                .with_input(target.to_url()),
        );
    }

    /// **OC → Reopen in Dev Container**: check for a configuration and the
    /// CLI, then ask before building.
    pub(in crate::app) fn oc_devcontainer_reopen(&mut self) {
        if self.remote.session.is_some() {
            self.status = t!("status.devcontainer_remote").to_string();
            return;
        }
        if self.remote_busy() {
            self.status = t!("status.remote_busy").to_string();
            return;
        }
        let auth = crate::authority::current();
        let Some(file) = devcontainer::CONFIG_FILES
            .iter()
            .find(|f| auth.is_file(&self.root.join(f)))
        else {
            self.status = t!("status.devcontainer_none").to_string();
            return;
        };
        if self.devcontainer_cli().is_none() {
            self.messages
                .error(t!("msg.devcontainer_cli_missing").to_string());
            return;
        }
        if self.editor.tabs.iter().any(|t| t.dirty) {
            self.status = t!("status.remote_unsaved_first").to_string();
            return;
        }
        self.remote.devcontainer_prompt_at = Some(Instant::now());
        self.confirm = Some(Confirm {
            message: t!("confirm.devcontainer_up", file = *file).to_string(),
            paths: vec![self.root.join(file)],
            kind: ConfirmKind::DevcontainerUp,
        });
    }

    /// The `devcontainer` CLI: the one a test set, else `PATH`'s.
    fn devcontainer_cli(&self) -> Option<PathBuf> {
        self.remote
            .devcontainer_cli
            .clone()
            .or_else(|| crate::fileops::which_on_path("devcontainer"))
    }

    /// `y` builds and opens the dev container; `n` or Esc doesn't. Keys
    /// within [`super::super::SCRIPT_TRUST_KEY_GUARD`] of the question are
    /// ignored (T655), since `y` runs the project's own code.
    pub(in crate::app) fn devcontainer_confirm_key(&mut self, key: KeyEvent) {
        let opened = self.remote.devcontainer_prompt_at;
        if opened.is_some_and(|t| t.elapsed() < super::super::SCRIPT_TRUST_KEY_GUARD) {
            return;
        }
        match key.code {
            KeyCode::Char('y' | 'Y') | KeyCode::Enter => {
                let config = self.confirm.take().and_then(|c| c.paths.into_iter().next());
                self.remote.devcontainer_prompt_at = None;
                if let Some(config) = config {
                    self.devcontainer_up(&config);
                }
            }
            KeyCode::Char('n' | 'N') | KeyCode::Esc => {
                self.confirm = None;
                self.remote.devcontainer_prompt_at = None;
                self.status = t!("status.devcontainer_cancelled").to_string();
            }
            _ => {}
        }
    }

    /// Run `devcontainer up` and connect to the container it reports, in
    /// the background.
    fn devcontainer_up(&mut self, config_path: &std::path::Path) {
        let Some(cli) = self.devcontainer_cli() else {
            self.messages
                .error(t!("msg.devcontainer_cli_missing").to_string());
            return;
        };
        let engine = Engine::from_name(&self.settings.oc_engine).or_else(|| {
            crate::oci::detect("", crate::fileops::which_on_path)
                .ok()
                .map(|d| d.engine)
        });
        let kind = if engine == Some(Engine::Podman) {
            ExecKind::Podman
        } else {
            ExecKind::Docker
        };
        let docker_path = (kind == ExecKind::Podman).then(|| {
            crate::fileops::which_on_path("podman")
                .map_or_else(|| "podman".to_string(), |p| p.display().to_string())
        });
        let config = crate::authority::current()
            .read_to_string(config_path)
            .ok()
            .and_then(|text| devcontainer::parse_config(&text).ok())
            .unwrap_or_default();
        let root = self.root.display().to_string();
        let basename = self
            .root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let options = self.connect_options();
        let label = config
            .name
            .clone()
            .unwrap_or_else(|| t!("ui.devcontainer").to_string());
        self.start_remote_job(label, false, move || {
            let args = devcontainer::up_argv(&root, docker_path.as_deref())
                .map_err(|e| t!("msg.devcontainer_failed", error = e).to_string())?;
            let out = crate::oci::run(&cli, &args, None).map_err(|e| up_error(&e))?;
            let up = devcontainer::parse_up(&out)
                .map_err(|e| t!("msg.devcontainer_failed", error = e).to_string())?;
            let folder = devcontainer::workspace_folder(&up, &config, &basename);
            let user = up.remote_user.clone().or(config.remote_user);
            let bad = |e: vix_authority::ssh::TargetError| {
                t!("msg.devcontainer_failed", error = e).to_string()
            };
            let mut target = ContainerTarget::new(kind, &up.container_id).map_err(bad)?;
            if let Some(user) = user {
                target = target.with_user(&user).map_err(bad)?;
            }
            let target = target.with_path(&folder).map_err(bad)?;
            connect_worker(RemoteTarget::Container(target), &options, true)
        });
        self.status = t!("status.devcontainer_building").to_string();
    }
}

/// A failed `devcontainer up`, in one line: its last line of output (the
/// CLI logs to stderr and ends with the reason).
fn up_error(e: &OciError) -> String {
    match e {
        OciError::NotInstalled => t!("msg.devcontainer_cli_missing").to_string(),
        OciError::Failed { stderr, .. } => {
            let last = stderr
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or_default();
            let last: String = last.chars().take(300).collect();
            t!("msg.devcontainer_failed", error = last).to_string()
        }
        other => t!("msg.devcontainer_failed", error = other).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oci_panel::{Panel, Request};
    use crate::settings::Settings;

    fn app(engine: Engine) -> App {
        let dir = std::env::temp_dir();
        let mut app = App::new(dir, Settings::default());
        app.oc = Some(Panel::new(engine, crate::oci::argv::Kind::Container, false));
        app
    }

    /// `w` on a running container fills in the Open Remote prompt with its
    /// name, for the user to finish the path.
    #[test]
    fn w_prefills_the_open_remote_prompt() {
        let mut a = app(Engine::Podman);
        a.oc_request(Request::OpenWorkspace("web".into()));
        assert!(a.oc.is_none());
        let prompt = a.prompt.as_ref().unwrap();
        assert!(prompt.kind == PromptKind::OpenRemote);
        assert_eq!(prompt.input, "podman://web/");
        // nerdctl has no exec authority; a bad name never gets as far.
        let mut a = app(Engine::Nerdctl);
        a.oc_request(Request::OpenWorkspace("web".into()));
        assert!(a.prompt.is_none());
        let mut a = app(Engine::Docker);
        a.oc_request(Request::OpenWorkspace("-it".into()));
        assert!(a.prompt.is_none());
    }
}
