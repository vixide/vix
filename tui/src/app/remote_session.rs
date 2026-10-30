//! Remote workspaces in the App (T713, T714): **File → Open Remote…**, the
//! status-bar host, the connection-lost state and reconnect, the save-conflict
//! prompt, and the trust prompt that keeps a remote workspace's own code from
//! running until the user says so.
//!
//! The authority itself (`vix_authority::ssh::SshAuthority`) is T712's; this
//! file is only the user surface around it. See `spec/remote-editing` and
//! `crates/vix-authority/spec/ssh`.
//!
//! **Connecting from the dialog never prompts.** The TUI owns the terminal,
//! so the dialog connects on a background thread with `ssh`'s `BatchMode`:
//! a host whose key isn't in `known_hosts` yet, or a key that needs a
//! passphrase not loaded into the agent, fails with a message saying how to
//! fix it (`ssh host` once in a terminal, `ssh-add`, or start Vix with `vix
//! ssh://…`, which connects before the UI and can prompt). Vix never accepts a
//! host key itself and never handles a password.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Instant;

use vix_authority::Authority;
use vix_authority::ssh::SshTarget;
use vix_input::{KeyCode, KeyEvent};

use super::ide_parity::{ChoiceList, ChoicePurpose};
use super::{App, Confirm, ConfirmKind, Focus, Prompt, PromptKind};

/// Actions that run code the workspace itself defines (its tasks, run
/// configurations, test commands, debugger and language servers). They wait
/// for the user to trust a remote workspace (T714).
const UNTRUSTED_BLOCKED: &[&str] = &[
    "project.compile",
    "project.configure",
    "project.install",
    "project.package",
    "project.run",
    "project.test",
    "project.test_at_point",
    "project.repeat_last_task",
    "project.run_config",
    "project.run_task",
    "project.subproject.compile",
    "project.subproject.configure",
    "project.subproject.install",
    "project.subproject.package",
    "project.subproject.run",
    "project.subproject.test",
    "run.start",
    "lsp.code_lens_run",
    "lsp.restart",
    "script.run",
];

/// The remote workspace that is open: where it is, whether the connection
/// is up, and whether the user trusts it.
#[derive(Debug, Clone)]
pub(crate) struct RemoteSession {
    /// The host (user, port) — its `path` is not kept up to date; the
    /// workspace root is `App::root`.
    target: SshTarget,
    /// `user@host:port` for the status bar.
    label: String,
    /// An operation failed with `NotConnected`; saves wait for a reconnect.
    lost: bool,
    /// The persisted trust decision: `None` not yet asked.
    trusted: Option<bool>,
    /// When the trust prompt opened (typed-ahead keys are ignored, T655).
    trust_prompt_at: Option<Instant>,
}

impl RemoteSession {
    fn new(target: SshTarget) -> Self {
        let mut label = String::new();
        if let Some(user) = &target.user {
            label.push_str(user);
            label.push('@');
        }
        label.push_str(&target.host);
        if let Some(port) = target.port {
            label.push(':');
            label.push_str(&port.to_string());
        }
        Self {
            target,
            label,
            lost: false,
            trusted: None,
            trust_prompt_at: None,
        }
    }

    /// The canonical `ssh://` name of `root` on this host: the session key,
    /// so state is per host and path.
    fn name_of(&self, root: &Path) -> String {
        let mut t = self.target.clone();
        t.path = root.to_string_lossy().into_owned();
        t.to_url()
    }
}

/// What a background connect produced.
struct Connected {
    /// The connected authority, ready to install.
    authority: Arc<dyn Authority>,
    /// The host, with `path` the workspace root.
    target: SshTarget,
    /// A file to open, when the name was a file (its directory is the root).
    file: Option<PathBuf>,
    /// What to say about the remote agent (T717), if anything.
    agent_note: Option<String>,
}

/// A connect running on a background thread.
struct Pending {
    rx: Receiver<Result<Connected, String>>,
    /// The host being connected to, for the status bar.
    label: String,
    /// Re-establishing the open workspace's connection, not opening a new one.
    reconnect: bool,
}

/// The App's remote state (one field, so `App` itself barely changes).
#[derive(Default)]
pub(crate) struct RemoteState {
    /// The open remote workspace, if any.
    pub(crate) session: Option<RemoteSession>,
    pending: Option<Pending>,
    /// How the dialog runs `ssh` (tests point `program` at a fake). `None`
    /// is the defaults, with the remote agent as the `remote_agent` setting
    /// says (T717).
    #[cfg(feature = "tasks")]
    pub(crate) ssh_options: Option<vix_authority::ssh::SshOptions>,
}

impl App {
    // ----- public surface ------------------------------------------------

    /// Run the Open Remote dialog's `ssh` as `options` says (an explicit
    /// `ssh` path for tests, or connection sharing off). Builder form, like
    /// `App::with_session_path`.
    #[cfg(feature = "tasks")]
    #[must_use]
    pub fn with_ssh_options(mut self, options: vix_authority::ssh::SshOptions) -> Self {
        self.remote.ssh_options = Some(options);
        self
    }

    /// The options the next connect runs `ssh` with.
    #[cfg(feature = "tasks")]
    fn ssh_options(&self) -> vix_authority::ssh::SshOptions {
        self.remote.ssh_options.clone().unwrap_or_else(|| {
            vix_authority::ssh::SshOptions::default().with_agent(
                vix_authority::ssh::AgentMode::from_setting(&self.settings.remote_agent),
            )
        })
    }

    /// Mark this App as working on `target`'s host, whose authority the
    /// caller already installed and whose `path` is the workspace root
    /// (`vix ssh://…` does this right after `App::new`). Turns on the
    /// save-conflict check, shows the host in the status bar, records the
    /// name in the recent remotes, and asks whether to trust the workspace
    /// unless that was already decided (T714). Until it is trusted, its
    /// tasks, run configurations, debugger, language servers and
    /// project-local tools don't run.
    pub fn adopt_remote(&mut self, target: SshTarget) {
        let mut session = RemoteSession::new(target);
        let key = session.name_of(&self.root);
        let mut store = self.load_session();
        session.trusted = store.workspace(&key).and_then(|w| w.remote_trusted);
        store.push_recent_remote(&key);
        self.store_session_quietly(&store);
        self.remote.session = Some(session);
        self.editor
            .flags
            .insert(crate::editor::Flags::CHECK_CONFLICTS);
        self.rebuild_lsp();
        match self.remote.session.as_ref().and_then(|s| s.trusted) {
            None => self.open_remote_trust_prompt(),
            Some(false) => {
                self.status = t!("status.remote_restricted").to_string();
            }
            Some(true) => {}
        }
    }

    /// The remote host (`user@host:port`) while a remote workspace is open.
    #[must_use]
    pub fn remote_host(&self) -> Option<&str> {
        self.remote.session.as_ref().map(|s| s.label.as_str())
    }

    /// Whether the remote connection was lost (saves wait for a reconnect).
    #[must_use]
    pub fn remote_connection_lost(&self) -> bool {
        self.remote.session.as_ref().is_some_and(|s| s.lost)
    }

    /// Whether a remote connect or reconnect is running in the background.
    #[must_use]
    pub fn remote_busy(&self) -> bool {
        self.remote.pending.is_some()
    }

    /// Whether the open workspace is a remote one the user has not trusted
    /// (never asked, or declined). Always `false` for a local workspace.
    #[must_use]
    pub fn remote_untrusted(&self) -> bool {
        self.remote
            .session
            .as_ref()
            .is_some_and(|s| s.trusted != Some(true))
    }

    /// Connect to the remote name `name` (`ssh://…` or `/ssh:…`) in the
    /// background and, once connected, make it the workspace (a directory)
    /// or open it in its directory (a file). The same as accepting
    /// **File → Open Remote…**. Refused while buffers have unsaved edits,
    /// since the switch closes them.
    pub fn open_remote(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        if self.remote.pending.is_some() {
            self.status = t!("status.remote_busy").to_string();
            return;
        }
        if self.editor.tabs.iter().any(|t| t.dirty) {
            self.status = t!("status.remote_unsaved_first").to_string();
            return;
        }
        let target = match SshTarget::parse(name) {
            Ok(t) => t,
            Err(e) => {
                self.messages
                    .error(t!("msg.remote_bad_name", name = name, error = e).to_string());
                return;
            }
        };
        self.start_connect(target, false);
    }

    /// Drain a finished background connect: install the authority and open
    /// the workspace, or report why it failed. Called every tick.
    pub fn poll_remote(&mut self) {
        let Some(pending) = self.remote.pending.as_ref() else {
            return;
        };
        let result = match pending.rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(t!(
                "msg.remote_connect_failed",
                host = pending.label.clone(),
                error = "?"
            )
            .to_string()),
        };
        let Some(pending) = self.remote.pending.take() else {
            return;
        };
        match result {
            Ok(connected) if pending.reconnect => self.finish_reconnect(connected),
            Ok(connected) => self.enter_remote(connected),
            Err(message) => {
                self.messages.error(message);
                self.status = t!("status.remote_connect_failed", host = pending.label).to_string();
            }
        }
    }

    // ----- actions ---------------------------------------------------------

    /// Dispatch the remote-workspace actions. Returns `true` if handled.
    pub(super) fn run_remote_action(&mut self, action: &str) -> bool {
        match action {
            "file.open_remote" => self.open_remote_dialog(),
            "file.reconnect_remote" => self.reconnect_remote(),
            "file.trust_remote" => {
                if self.remote.session.is_some() {
                    self.open_remote_trust_prompt();
                } else {
                    self.status = t!("status.remote_none").to_string();
                }
            }
            _ => return false,
        }
        true
    }

    /// Whether `action` must wait because the remote workspace is untrusted;
    /// says so on the status line when it does.
    pub(super) fn remote_trust_blocks(&mut self, action: &str) -> bool {
        if !self.remote_untrusted() || !UNTRUSTED_BLOCKED.contains(&action) {
            return false;
        }
        self.status = t!("status.remote_untrusted_blocked").to_string();
        true
    }

    /// **File → Open Remote…**: pick a recent remote, or type a name.
    fn open_remote_dialog(&mut self) {
        let recent = self.load_session().recent_remotes;
        if recent.is_empty() {
            self.open_remote_prompt();
            return;
        }
        let mut rows = vec![t!("ui.remote_new_connection").to_string()];
        rows.extend(recent.iter().cloned());
        self.choice_list = Some(ChoiceList {
            panel: vix_choice_list::Panel::new(
                t!("ui.remote_open_title").to_string(),
                t!("ui.remote_open_hint").to_string(),
                rows,
            )
            .with_selected(1),
            purpose: ChoicePurpose::RecentRemote { names: recent },
        });
    }

    /// The Open Remote name prompt.
    pub(super) fn open_remote_prompt(&mut self) {
        self.prompt = Some(Prompt::new(
            PromptKind::OpenRemote,
            t!("prompt.open_remote").to_string(),
        ));
    }

    /// **File → Reconnect Remote**: re-establish the open remote workspace's
    /// connection. Buffers stay as they are; saves resume once it is back,
    /// still checked against the version each buffer last saw.
    fn reconnect_remote(&mut self) {
        let Some(session) = self.remote.session.as_ref() else {
            self.status = t!("status.remote_none").to_string();
            return;
        };
        if self.remote.pending.is_some() {
            self.status = t!("status.remote_busy").to_string();
            return;
        }
        let mut target = session.target.clone();
        target.path = self.root.to_string_lossy().into_owned();
        self.start_connect(target, true);
    }

    // ----- connecting ----------------------------------------------------

    /// Connect to `target` on a background thread; [`App::poll_remote`]
    /// picks up the result.
    fn start_connect(&mut self, target: SshTarget, reconnect: bool) {
        let label = RemoteSession::new(target.clone()).label;
        let (tx, rx) = mpsc::channel();
        #[cfg(feature = "tasks")]
        let options = self.ssh_options();
        let spawned = std::thread::Builder::new()
            .name("vix-remote-connect".into())
            .spawn(move || {
                #[cfg(feature = "tasks")]
                let result = connect_worker(target, options);
                #[cfg(not(feature = "tasks"))]
                let result = connect_worker(&target);
                let _ = tx.send(result);
            });
        if let Err(e) = spawned {
            self.messages
                .error(t!("msg.remote_connect_failed", host = label, error = e).to_string());
            return;
        }
        self.status = t!("status.remote_connecting", host = label).to_string();
        self.remote.pending = Some(Pending {
            rx,
            label,
            reconnect,
        });
    }

    /// Make a freshly connected remote the workspace.
    fn enter_remote(&mut self, connected: Connected) {
        // The current workspace's session is saved under its own key before
        // the authority (which the key is computed through) changes.
        if let Err(e) = self.save_session() {
            self.messages
                .warn(t!("msg.session_save_failed", error = e).to_string());
        }
        vix_authority::install(connected.authority);
        let root = PathBuf::from(&connected.target.path);
        // Set before re-rooting, so the session key and the language-server
        // switch already see the remote workspace.
        self.remote.session = Some(RemoteSession::new(connected.target.clone()));
        self.reroot_workspace(&root, &[]);
        self.adopt_remote(connected.target);
        if let Some(note) = connected.agent_note {
            self.messages.info(note);
        }
        if let Some(file) = connected.file {
            self.open_path(&file, false);
        }
        self.focus = Focus::Editor;
        if self.remote.session.as_ref().and_then(|s| s.trusted) != Some(false) {
            self.status = t!(
                "status.remote_opened",
                host = self.remote_host().unwrap_or_default().to_string()
            )
            .to_string();
        }
    }

    /// The connection is back: install the new authority and resume saves.
    fn finish_reconnect(&mut self, connected: Connected) {
        vix_authority::install(connected.authority);
        if let Some(s) = self.remote.session.as_mut() {
            s.lost = false;
        }
        let host = self.remote_host().unwrap_or_default().to_string();
        self.messages
            .info(t!("msg.remote_reconnected", host = host).to_string());
        if let Some(note) = connected.agent_note {
            self.messages.info(note);
        }
        self.status = t!("status.remote_reconnected", host = host).to_string();
    }

    // ----- connection lost -------------------------------------------------

    /// Note an error from a workspace operation: `NotConnected` under a remote
    /// authority marks the connection lost (once) and says how to get it
    /// back. Returns whether it did.
    pub(super) fn note_remote_error(&mut self, e: &io::Error) -> bool {
        if e.kind() != io::ErrorKind::NotConnected {
            return false;
        }
        let Some(session) = self.remote.session.as_mut() else {
            return false;
        };
        if !session.lost {
            session.lost = true;
            let host = session.label.clone();
            self.messages
                .warn(t!("msg.remote_connection_lost", host = host).to_string());
            self.status = t!("status.remote_connection_lost", host = host).to_string();
        }
        true
    }

    /// Whether a save must wait for a reconnect; says so when it must.
    pub(super) fn remote_save_blocked(&mut self) -> bool {
        if !self.remote_connection_lost() {
            return false;
        }
        self.status = t!("status.remote_save_blocked").to_string();
        true
    }

    /// The status-bar segment: the remote host, with "connecting",
    /// "connection lost" or "restricted" when that applies. Empty locally.
    pub(crate) fn remote_segment(&self) -> String {
        if let Some(p) = &self.remote.pending {
            return format!(
                "{}   ",
                t!("ui.remote_segment_connecting", host = p.label.clone())
            );
        }
        let Some(s) = &self.remote.session else {
            return String::new();
        };
        let text = if s.lost {
            t!("ui.remote_segment_lost", host = s.label.clone())
        } else if s.trusted == Some(true) {
            t!("ui.remote_segment", host = s.label.clone())
        } else {
            t!("ui.remote_segment_restricted", host = s.label.clone())
        };
        format!("{text}   ")
    }

    /// The session key of a remote workspace (its `ssh://` name), or `None`
    /// for a local one.
    pub(super) fn remote_session_key(&self) -> Option<String> {
        self.remote.session.as_ref().map(|s| s.name_of(&self.root))
    }

    // ----- trust (T714) ----------------------------------------------------

    /// Ask whether to trust the open remote workspace.
    fn open_remote_trust_prompt(&mut self) {
        let Some(key) = self.remote_session_key() else {
            return;
        };
        if let Some(s) = self.remote.session.as_mut() {
            s.trust_prompt_at = Some(Instant::now());
        }
        self.confirm = Some(Confirm {
            message: t!("confirm.remote_trust", name = key).to_string(),
            paths: Vec::new(),
            kind: ConfirmKind::TrustRemote,
        });
    }

    /// Record the trust answer, persisted per host and path.
    fn set_remote_trust(&mut self, trusted: bool) {
        let Some(key) = self.remote_session_key() else {
            return;
        };
        if let Some(s) = self.remote.session.as_mut() {
            s.trusted = Some(trusted);
        }
        let mut store = self.load_session();
        store.set_remote_trusted(&key, Some(trusted));
        self.store_session_quietly(&store);
        self.rebuild_lsp();
        self.status = if trusted {
            t!("status.remote_trusted").to_string()
        } else {
            t!("status.remote_restricted").to_string()
        };
    }

    /// Whether language servers may start: the setting, and not in an
    /// untrusted remote workspace.
    pub(super) fn lsp_allowed(&self) -> bool {
        self.settings.subsystems.lsp_enabled && !self.remote_untrusted()
    }

    /// Replace the language-server manager to match [`App::lsp_allowed`].
    fn rebuild_lsp(&mut self) {
        self.lsp.shutdown();
        self.lsp_synced.clear();
        self.lsp = crate::lsp::Lsp::new(
            self.lsp_allowed(),
            self.settings.lsp_servers.clone(),
            &self.workspace_folders,
        );
    }

    /// Persist `session`, reporting a failure as a warning.
    fn store_session_quietly(&mut self, session: &crate::session::Session) {
        if let Err(e) = self.store_session(session) {
            self.messages
                .warn(t!("msg.session_save_failed", error = e).to_string());
        }
    }

    // ----- prompts -----------------------------------------------------------

    /// Keys for the remote confirmations (trust, save conflict). Returns
    /// whether the open confirmation was one of them.
    pub(super) fn remote_confirm_key(&mut self, key: KeyEvent) -> bool {
        let Some(kind) = self.confirm.as_ref().map(|c| c.kind) else {
            return false;
        };
        match kind {
            ConfirmKind::TrustRemote => {
                let opened = self.remote.session.as_ref().and_then(|s| s.trust_prompt_at);
                if opened.is_some_and(|t| t.elapsed() < super::SCRIPT_TRUST_KEY_GUARD) {
                    return true;
                }
                match key.code {
                    KeyCode::Char('y' | 'Y') => {
                        self.confirm = None;
                        self.set_remote_trust(true);
                    }
                    KeyCode::Char('n' | 'N') | KeyCode::Esc => {
                        self.confirm = None;
                        self.set_remote_trust(false);
                    }
                    _ => {}
                }
                true
            }
            ConfirmKind::SaveConflict => {
                self.save_conflict_key(key);
                true
            }
            _ => false,
        }
    }

    /// Ask what to do about a save that found the file changed on disk.
    pub(super) fn open_save_conflict(&mut self, path: PathBuf) {
        self.confirm = Some(Confirm {
            message: t!("confirm.save_conflict", path = path.display()).to_string(),
            paths: vec![path],
            kind: ConfirmKind::SaveConflict,
        });
    }

    /// `o` overwrites, `c` compares the buffer with the file on disk, `n`
    /// or Esc keeps both as they are.
    fn save_conflict_key(&mut self, key: KeyEvent) {
        let Some(path) = self.confirm.as_ref().and_then(|c| c.paths.first().cloned()) else {
            self.confirm = None;
            return;
        };
        match key.code {
            KeyCode::Char('o' | 'O') => {
                self.confirm = None;
                if let Some(i) = self
                    .editor
                    .tabs
                    .iter()
                    .position(|t| t.path.as_deref() == Some(path.as_path()))
                {
                    self.editor.active = i;
                    self.write_active_to_disk_with(true);
                }
            }
            KeyCode::Char('c' | 'C') => {
                self.confirm = None;
                self.compare_with_disk(&path);
            }
            KeyCode::Char('n' | 'N') | KeyCode::Esc => {
                self.confirm = None;
                self.status = t!("status.save_conflict_kept").to_string();
            }
            _ => {}
        }
    }

    /// Show the file on disk against the buffer, so the user can merge by
    /// hand and save again.
    fn compare_with_disk(&mut self, path: &Path) {
        let disk = match crate::authority::current().read_to_string_guarded(path) {
            Ok(text) => text,
            Err(e) => {
                self.note_remote_error(&e);
                self.messages
                    .error(t!("msg.open_failed", error = e).to_string());
                return;
            }
        };
        let Some(buffer) = self
            .editor
            .tabs
            .iter()
            .find(|t| t.path.as_deref() == Some(path))
            .map(crate::editor::Tab::text)
        else {
            return;
        };
        let lines = crate::diff_view::build(&disk, &buffer);
        if lines.is_empty() {
            self.status = t!("status.diff_identical").to_string();
            return;
        }
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        self.diff_view = Some(super::DiffViewState {
            title: t!("ui.save_conflict_compare", name = name).to_string(),
            lines,
            scroll: 0,
        });
    }

    /// A row of the recent-remotes chooser: row 0 types a new name.
    pub(super) fn choose_recent_remote(&mut self, names: &[String], row: usize) {
        match row.checked_sub(1).and_then(|i| names.get(i)) {
            Some(name) => self.open_remote(&name.clone()),
            None => self.open_remote_prompt(),
        }
    }
}

/// Connect to `target` without prompting (`BatchMode`), resolve its path,
/// and split it into a workspace root and an optional file to open.
#[cfg(feature = "tasks")]
fn connect_worker(
    target: SshTarget,
    options: vix_authority::ssh::SshOptions,
) -> Result<Connected, String> {
    use vix_authority::ssh::SshAuthority;
    let host = target.host.clone();
    let failed = |e: io::Error| {
        t!(
            "msg.remote_connect_failed_batch",
            host = host.clone(),
            error = e
        )
        .to_string()
    };
    let remote_path = PathBuf::from(&target.path);
    let auth = SshAuthority::with_options(target.clone(), options).map_err(failed)?;
    auth.connect(false).map_err(failed)?;
    let path = auth.canonicalize(&remote_path).map_err(failed)?;
    let (root, file) = if auth.is_dir(&path) {
        (path, None)
    } else {
        let root = path
            .parent()
            .map_or_else(|| PathBuf::from("/"), Path::to_path_buf);
        (root, Some(path))
    };
    let agent_note = crate::remote::agent_note(&host, &auth.agent_status());
    let mut target = target;
    target.path = root.to_string_lossy().into_owned();
    Ok(Connected {
        authority: Arc::new(auth),
        target,
        file,
        agent_note,
    })
}

/// Without `tasks` Vix starts no process, so it can't run `ssh`.
#[cfg(not(feature = "tasks"))]
fn connect_worker(_target: &SshTarget) -> Result<Connected, String> {
    Err(t!("msg.capability_missing", name = "tasks").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    fn app(tag: &str) -> (App, PathBuf) {
        let dir = std::env::temp_dir().join(format!("vix-remote-ui-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let app =
            App::new(dir.clone(), Settings::default()).with_session_path(dir.join("session.toml"));
        (app, dir)
    }

    fn key(c: char) -> KeyEvent {
        KeyEvent::from(KeyCode::Char(c))
    }

    /// Adopting a remote shows the host, keys the session by its `ssh://`
    /// name, turns on the conflict check, and asks about trust; until
    /// trusted, workspace-defined actions are refused and language servers
    /// stay off.
    #[test]
    fn an_adopted_remote_is_untrusted_until_the_user_trusts_it() {
        let (mut app, dir) = app("trust");
        let target = SshTarget::parse(&format!("ssh://me@box:2222{}", dir.display())).unwrap();
        app.adopt_remote(target);
        assert_eq!(app.remote_host(), Some("me@box:2222"));
        assert!(app.session_key().starts_with("ssh://me@box:2222/"));
        assert!(
            app.editor
                .flags
                .contains(crate::editor::Flags::CHECK_CONFLICTS)
        );
        assert!(app.remote_untrusted());
        assert!(!app.lsp_allowed());
        assert_eq!(
            app.confirm.as_ref().map(|c| c.kind),
            Some(ConfirmKind::TrustRemote)
        );
        assert!(app.remote_trust_blocks("project.test"));
        assert!(app.remote_trust_blocks("run.start"));
        assert!(!app.remote_trust_blocks("file.save"));
        assert!(!app.remote_trust_blocks("project.run_config_edit"));

        // A key typed ahead of the prompt is ignored (T655).
        app.remote_confirm_key(key('y'));
        assert!(app.remote_untrusted());
        if let Some(s) = app.remote.session.as_mut() {
            s.trust_prompt_at = None;
        }
        assert!(app.remote_confirm_key(key('y')));
        assert!(!app.remote_untrusted());
        assert!(app.confirm.is_none());
        assert!(!app.remote_trust_blocks("project.test"));

        // The decision is persisted per host and path: a new App on the same
        // name doesn't ask again, and one on another port does.
        let session = dir.join("session.toml");
        let mut again = App::new(dir.clone(), Settings::default()).with_session_path(&session);
        again.adopt_remote(
            SshTarget::parse(&format!("ssh://me@box:2222{}", dir.display())).unwrap(),
        );
        assert!(!again.remote_untrusted());
        assert!(again.confirm.is_none());
        let mut other = App::new(dir.clone(), Settings::default()).with_session_path(&session);
        other.adopt_remote(
            SshTarget::parse(&format!("ssh://me@box:2200{}", dir.display())).unwrap(),
        );
        assert!(other.remote_untrusted());
        assert_eq!(
            other
                .load_session()
                .recent_remotes
                .first()
                .map(String::as_str),
            Some(format!("ssh://me@box:2200{}", dir.display()).as_str())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `NotConnected` marks the connection lost once, blocks saves (the
    /// buffer stays dirty), and shows in the status segment; other errors
    /// and local workspaces are unaffected.
    #[test]
    fn a_lost_connection_keeps_buffers_and_blocks_saves() {
        let (mut app, dir) = app("lost");
        let lost = io::Error::new(io::ErrorKind::NotConnected, "gone");
        assert!(!app.note_remote_error(&lost), "local: nothing to lose");
        app.adopt_remote(SshTarget::parse(&format!("ssh://box{}", dir.display())).unwrap());
        app.confirm = None;
        assert!(!app.note_remote_error(&io::Error::from(io::ErrorKind::NotFound)));
        assert!(app.note_remote_error(&lost));
        assert!(app.remote_connection_lost());
        assert!(app.remote_segment().contains("box"));
        let file = dir.join("a.txt");
        std::fs::write(&file, "x\n").unwrap();
        app.open_path(&file, false);
        let area = ratatui::layout::Rect::new(0, 0, 80, 24);
        assert!(app.editor.insert_str("edit ", area));
        app.run_action("file.save");
        assert!(app.editor.active_tab().unwrap().dirty, "kept, not saved");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "x\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A save that finds the file changed on disk asks; Esc keeps both,
    /// `c` compares, `o` overwrites.
    #[test]
    fn a_save_conflict_asks_before_overwriting() {
        let (mut app, dir) = app("conflict");
        app.editor
            .flags
            .insert(crate::editor::Flags::CHECK_CONFLICTS);
        let file = dir.join("a.txt");
        std::fs::write(&file, "one\n").unwrap();
        app.open_path(&file, false);
        let area = ratatui::layout::Rect::new(0, 0, 80, 24);
        assert!(app.editor.insert_str("mine ", area));
        std::fs::write(&file, "theirs, longer\n").unwrap();
        app.run_action("file.save");
        assert_eq!(
            app.confirm.as_ref().map(|c| c.kind),
            Some(ConfirmKind::SaveConflict)
        );
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "theirs, longer\n");
        app.confirm_key(KeyEvent::from(KeyCode::Esc));
        assert!(app.confirm.is_none());
        assert!(app.editor.active_tab().unwrap().dirty);

        app.run_action("file.save");
        app.confirm_key(key('c'));
        assert!(app.diff_view.is_some(), "compare opens the diff");
        app.diff_view = None;

        app.run_action("file.save");
        app.confirm_key(key('o'));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "mine one\n");
        assert!(!app.editor.active_tab().unwrap().dirty);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
