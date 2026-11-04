//! The workspace trust prompt (T132, T627): one persisted decision per
//! workspace that both `.vix/scripts/` and `.vix/settings.toml` wait on.
//!
//! Moved out of `scripts.rs` (T627) so the prompt exists in every build: a
//! build without the `script` capability still has project settings to
//! trust. The script half (`project_script_count`, `load_scripts`) is a
//! no-op in such a build (`scripts_off.rs`).

use vix_input::{KeyCode, KeyEvent};

use super::{App, ScriptTrustPrompt};

impl App {
    /// Whether this workspace's project scripts and settings are trusted:
    /// the persisted decision from a prior [`App::accept_script_trust`]/
    /// [`App::decline_script_trust`], or `false` if never yet asked.
    pub(super) fn project_scripts_trusted(&self) -> bool {
        self.load_session()
            .workspace(&self.session_key())
            .is_some_and(|w| w.scripts_trusted == Some(true))
    }

    /// Queue the trust prompt if this workspace has project scripts or a
    /// project settings file and it has never been decided either way —
    /// called once at startup ([`App::start`], right after
    /// [`App::load_scripts`]). A prior explicit decline is deliberately
    /// **not** re-asked here (that would defeat the point of persisting "no"
    /// at all, asking again every single launch) — `script.reload` re-checks
    /// even after a decline instead, so a user who changes their mind has a
    /// way to be asked again without every ordinary startup re-asking too. A
    /// no-op when there's nothing to ask about or the answer is already on
    /// file (either way).
    pub fn maybe_prompt_script_trust(&mut self) {
        let decided = self
            .load_session()
            .workspace(&self.session_key())
            .is_some_and(|w| w.scripts_trusted.is_some());
        if !decided {
            self.queue_script_trust_prompt_if_any();
        }
    }

    /// Re-check trust even after a prior explicit decline — called by
    /// `script.reload`, so a user who changes their mind about an already-
    /// declined workspace has a way to be asked again (just reload)
    /// without every ordinary startup re-asking too.
    #[cfg(feature = "script")]
    pub(super) fn maybe_reprompt_script_trust(&mut self) {
        if !self.project_scripts_trusted() {
            self.queue_script_trust_prompt_if_any();
        }
    }

    /// Queue the trust prompt if `.vix/scripts/` has at least one script or
    /// `.vix/settings.toml` exists, unconditionally — the shared tail of
    /// [`App::maybe_prompt_script_trust`]/`maybe_reprompt_script_trust`,
    /// which differ only in *when* they're willing to call this. Never for
    /// a remote workspace, whose project files are not read (T714).
    fn queue_script_trust_prompt_if_any(&mut self) {
        if self.remote_host().is_some() {
            return;
        }
        let count = self.project_script_count();
        let settings = vix_settings::project_settings_path(&self.root).is_file();
        let env = vix_project_env::needs_trust(&self.root, Self::user_env_file().as_deref());
        if count > 0 || settings || env {
            self.script_trust = Some(ScriptTrustPrompt {
                count,
                settings,
                env,
            });
            self.script_trust_opened_at = Some(std::time::Instant::now());
        }
    }

    /// Trust this workspace: persist the decision, then load its project
    /// scripts and apply its project settings (both were skipped so far,
    /// since [`App::project_scripts_trusted`] was false until now).
    fn accept_script_trust(&mut self) {
        if self.script_trust.take().is_none() {
            return;
        }
        self.set_scripts_trusted(Some(true));
        self.reload_project_env_quietly();
        self.load_scripts();
        self.resolve_key_overrides();
        self.load_project_settings();
        self.status = t!("status.scripts_trusted").to_string();
    }

    /// Decline this workspace's project scripts and settings: persist the
    /// decision (so startup doesn't ask again every launch) and leave them
    /// unloaded.
    fn decline_script_trust(&mut self) {
        if self.script_trust.take().is_none() {
            return;
        }
        self.set_scripts_trusted(Some(false));
        self.status = t!("status.scripts_not_trusted").to_string();
    }

    /// Persist this workspace's trust decision to the session store,
    /// without disturbing anything else already saved for it (open files,
    /// project command history, …) or counting as a workspace "open".
    fn set_scripts_trusted(&mut self, trusted: Option<bool>) {
        let key = self.session_key();
        let mut session = self.load_session();
        session.set_scripts_trusted(&key, trusted);
        self.store_session_or_warn(&session);
    }

    /// Keys for the trust prompt: only an explicit `y` trusts and loads;
    /// `n`/`Esc` declines; anything else (Enter included) is ignored. Keys
    /// within [`SCRIPT_TRUST_KEY_GUARD`](super::SCRIPT_TRUST_KEY_GUARD) of the
    /// prompt opening are dropped too (T655): the prompt appears at startup
    /// or on reload, so an Enter or `y` typed ahead of it must not approve a
    /// hostile clone's `.vix/scripts/` or `.vix/settings.toml`.
    pub(super) fn script_trust_key(&mut self, key: KeyEvent) {
        if self
            .script_trust_opened_at
            .is_some_and(|t| t.elapsed() < super::SCRIPT_TRUST_KEY_GUARD)
        {
            return;
        }
        match key.code {
            KeyCode::Char('y' | 'Y') => self.accept_script_trust(),
            KeyCode::Char('n' | 'N') | KeyCode::Esc => self.decline_script_trust(),
            _ => {}
        }
    }
}
