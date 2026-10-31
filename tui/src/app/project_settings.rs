//! Project and per-language settings (T627): merging a trusted workspace's
//! `<root>/.vix/settings.toml` over the user settings, keeping its values out
//! of the user file on save, and resolving `[languages.<id>]` overrides for
//! the active buffer. The merge itself is `vix_settings`' (`merge_layer`,
//! `unmerge`, `Settings::for_language`); this is the host side.

use std::borrow::Cow;
use std::path::Path;

use super::App;
use crate::settings::Settings;

/// A project settings layer in effect: the user settings it was merged over,
/// and the merge result as first installed (so in-app changes made since can
/// be told apart from the project's own values when saving).
pub(super) struct ProjectLayer {
    /// The user settings as loaded, before the project file was merged.
    user: Settings,
    /// The user settings with the project file merged in, as installed.
    merged: Settings,
}

impl App {
    /// The settings to write to the user file: [`App::settings`] itself, or,
    /// with a project layer in effect, the user settings plus only the
    /// in-app changes made since (`vix_settings::unmerge`).
    pub(super) fn user_settings(&self) -> Cow<'_, Settings> {
        match &self.project_settings {
            Some(layer) => Cow::Owned(vix_settings::unmerge(
                &layer.user,
                &layer.merged,
                &self.settings,
            )),
            None => Cow::Borrowed(&self.settings),
        }
    }

    /// The settings for the active buffer: [`App::settings`] with the
    /// buffer's `[languages.<id>]` overrides applied (T627).
    pub(super) fn buffer_settings(&self) -> Cow<'_, Settings> {
        match self.editor.active_tab() {
            Some(tab) => self.settings.for_language(tab.editor.language()),
            None => Cow::Borrowed(&self.settings),
        }
    }

    /// Whether `path` is this workspace's project settings file.
    pub(super) fn is_project_settings_file(&self, path: &Path) -> bool {
        let canon = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        canon(path) == canon(&vix_settings::project_settings_path(&self.root))
    }

    /// Re-read this workspace's `.vix/settings.toml` and install the user
    /// settings with it merged over them (or the user settings alone, when
    /// there is no usable project file), re-applying what changed (T627).
    /// Called at startup, when the workspace is trusted, and on a workspace
    /// switch.
    pub fn load_project_settings(&mut self) {
        let user = self.user_settings().into_owned();
        let had_layer = self.project_settings.is_some();
        let effective = self.layer_project_settings(user);
        if !had_layer && self.project_settings.is_none() {
            return;
        }
        let old = std::mem::take(&mut self.settings);
        self.apply_settings(&old, effective);
    }

    /// `user` with this workspace's project settings merged over it, when
    /// the workspace is local and trusted and the file exists and is valid;
    /// records the layer in [`App::project_settings`]. Otherwise `user`
    /// unchanged, with any read, parse or merge error reported. Unknown keys
    /// in the project file and non-language keys in any `languages` table
    /// are reported as warnings.
    pub(super) fn layer_project_settings(&mut self, user: Settings) -> Settings {
        self.project_settings = None;
        let effective = self.merge_project_file(user);
        for key in vix_settings::language_key_problems(&effective) {
            self.messages
                .warn(t!("msg.settings_language_key_ignored", key = key).to_string());
        }
        effective
    }

    /// [`App::layer_project_settings`]' merge step.
    fn merge_project_file(&mut self, user: Settings) -> Settings {
        if self.remote_host().is_some() || !self.project_scripts_trusted() {
            return user;
        }
        let path = vix_settings::project_settings_path(&self.root);
        let layer = match vix_settings::read_layer(&path) {
            Ok(Some(layer)) => layer,
            Ok(None) => return user,
            Err(e) => {
                self.report_project_settings_error(&path, &e);
                return user;
            }
        };
        for key in vix_settings::unknown_keys(&layer) {
            self.messages.warn(
                t!(
                    "msg.project_settings_unknown_key",
                    key = key,
                    path = path.display()
                )
                .to_string(),
            );
        }
        match vix_settings::merge_layer(&user, &layer) {
            Ok(merged) => {
                self.project_settings = Some(Box::new(ProjectLayer {
                    user,
                    merged: merged.clone(),
                }));
                merged
            }
            Err(e) => {
                self.report_project_settings_error(&path, &e);
                user
            }
        }
    }

    /// Post a project settings load failure to the message drawer.
    fn report_project_settings_error(&mut self, path: &Path, e: &vix_settings::LayerError) {
        self.messages.error(
            t!(
                "msg.project_settings_load_failed",
                path = path.display(),
                error = e
            )
            .to_string(),
        );
    }
}
