//! Native Codex and Claude conversations shared by the TUI and GUI front ends.
mod worktree;

use super::ide_parity::{ChoiceList, ChoicePurpose};
use super::{App, KeyCode, KeyEvent};
use crate::ai_panel::{Panel, Role};
use crate::geo::Geo;
use std::collections::VecDeque;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use vix_acp::{display::sanitize, process::AgentProcess};
use vix_ai_agents::Provider;
use vix_ai_agents::conversation::{Conversation, Entry, Message, Store};
use vix_ai_agents::protocol::{Approval, Event, Field, Turn};

/// How long a stopped provider gets to exit on its own before it is killed.
const STOP_GRACE: Duration = Duration::from_millis(500);
/// How long an exited provider's stderr reader gets to catch up.
const EXIT_GRACE: Duration = Duration::from_millis(200);
/// Saved messages shown when a conversation opens; older ones stay on disk.
const SHOWN_MESSAGES: usize = 200;
/// The stderr lines quoted when a provider fails or exits.
const STDERR_LINES: usize = 20;

#[derive(Clone, Debug)]
pub(crate) enum Choice {
    New(Provider),
    #[cfg(feature = "git")]
    NewWorktree(Provider),
    Resume(Entry),
}

struct Running {
    turn: Turn,
    process: AgentProcess,
    /// Requests waiting for a decision, oldest (the one shown) first.
    approvals: VecDeque<(Approval, Panel)>,
}

/// A provider process that was told to stop, killed at `deadline` if it has
/// not exited by then.
struct Stopping {
    process: AgentProcess,
    deadline: Instant,
}

impl Stopping {
    /// Whether the process exited or ran out of time (dropping it kills it).
    fn done(&mut self) -> bool {
        Instant::now() >= self.deadline || !matches!(self.process.try_wait(), Ok(None))
    }
}

pub(crate) struct NativePanel {
    pub(crate) panel: Panel,
    pub(crate) conversation: Conversation,
    root: PathBuf,
    create_worktree: bool,
    running: Option<Running>,
    stopping: Option<Stopping>,
    /// Esc was pressed once during a turn; a second Esc stops the provider.
    pub(crate) stop_armed: bool,
}

/// The instant `wait` from now (now itself if the clock cannot go that far).
fn after(wait: Duration) -> Instant {
    let now = Instant::now();
    now.checked_add(wait).unwrap_or(now)
}

/// The translated label of an approval line; a provider key is shown as is.
fn field_label(field: &Field) -> String {
    match field {
        Field::Tool => t!("ui.agent_field_tool").to_string(),
        Field::Command => t!("ui.agent_field_command").to_string(),
        Field::Directory => t!("ui.agent_field_directory").to_string(),
        Field::Reason => t!("ui.agent_field_reason").to_string(),
        Field::File => t!("ui.agent_field_file").to_string(),
        Field::Old => t!("ui.agent_field_old").to_string(),
        Field::New => t!("ui.agent_field_new").to_string(),
        Field::Content => t!("ui.agent_field_content").to_string(),
        Field::Change => t!("ui.agent_field_change").to_string(),
        Field::Other(key) => sanitize(key),
        _ => String::new(),
    }
}

/// The request as readable, sanitized lines: one turn per field, a
/// multi-line value under its label.
fn approval_panel(ask: &Approval) -> Panel {
    let mut panel = Panel::open();
    for (field, text) in ask.summary() {
        let label = field_label(&field);
        let text = sanitize(&text);
        if text.contains('\n') {
            panel.push(Role::Assistant, format!("{label}:\n{text}"));
        } else {
            panel.push(Role::Assistant, format!("{label}: {text}"));
        }
    }
    panel.scroll = usize::MAX;
    panel
}

/// The last lines of the provider's stderr, sanitized, after `message`.
fn with_stderr(message: &str, process: &AgentProcess) -> String {
    let tail = sanitize(&process.stderr_tail());
    let lines: Vec<&str> = tail.trim_end().lines().collect();
    let skip = lines.len().minus(STDERR_LINES);
    let quoted = lines.get(skip..).unwrap_or_default().join("\n");
    if quoted.trim().is_empty() {
        message.to_owned()
    } else {
        format!("{message}\n{quoted}")
    }
}

impl NativePanel {
    pub(crate) fn awaiting_approval(&self) -> bool {
        self.running
            .as_ref()
            .is_some_and(|r| !r.approvals.is_empty())
    }

    /// Whether a provider process is still running or being stopped.
    pub(crate) fn active(&self) -> bool {
        self.running.is_some() || self.stopping.is_some()
    }

    /// The oldest permission request stays visible until the user answers it.
    pub(crate) fn displayed_panel(&mut self) -> &mut Panel {
        if let Some((_, panel)) = self.running.as_mut().and_then(|r| r.approvals.front_mut()) {
            panel
        } else {
            &mut self.panel
        }
    }

    fn open(conversation: Conversation, root: PathBuf, create_worktree: bool) -> Self {
        let mut panel = Panel::open();
        let messages = &conversation.transcript.messages;
        let skip = messages.len().minus(SHOWN_MESSAGES);
        for message in messages.iter().skip(skip) {
            match message {
                Message::User(s) => panel.push(Role::User, sanitize(s)),
                Message::Reply(s) => panel.push(Role::Assistant, sanitize(s)),
            }
        }
        Self {
            panel,
            conversation,
            root,
            create_worktree,
            running: None,
            stopping: None,
            stop_armed: false,
        }
    }

    fn start(
        &mut self,
        spawn: impl FnOnce(&[String], &std::path::Path) -> io::Result<AgentProcess>,
    ) -> io::Result<()> {
        if self.running.is_some() || self.panel.input.trim().is_empty() {
            return Ok(());
        }
        // A stopped turn still winding down gives way to the new one.
        self.stopping = None;
        let cwd = if let Some(checkout) = self.conversation.worktree() {
            if self.create_worktree {
                // Persist ownership before Git can create files or a branch.
                self.conversation.save()?;
            }
            worktree::directory(&self.root, &checkout, &mut self.create_worktree)?
        } else {
            self.root.clone()
        };
        let transcript = &self.conversation.transcript;
        let provider = transcript.provider;
        let turn = Turn::new(
            provider,
            cwd.display().to_string(),
            transcript.session.clone(),
            self.panel.input.clone(),
        );
        let mut process = spawn(&turn.argv(), &cwd).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                io::Error::new(
                    e.kind(),
                    t!("status.agent_not_installed", provider = provider.name()).to_string(),
                )
            } else {
                e
            }
        })?;
        process.send(&turn.initialize())?;
        let prompt = self.panel.input.clone();
        self.conversation
            .transcript
            .messages
            .push(Message::User(prompt.clone()));
        if let Err(e) = self.conversation.save() {
            self.conversation.transcript.messages.pop();
            return Err(e);
        }
        self.panel.input.clear();
        self.panel.push(Role::User, sanitize(&prompt));
        self.panel.busy = true;
        self.running = Some(Running {
            turn,
            process,
            approvals: VecDeque::new(),
        });
        Ok(())
    }

    /// Let a finished turn's provider exit on its own (stdin closed), killing
    /// it if it has not within [`STOP_GRACE`].
    fn retire(&mut self, mut process: AgentProcess) {
        self.stop_armed = false;
        process.close_stdin();
        self.stopping = Some(Stopping {
            process,
            deadline: after(STOP_GRACE),
        });
    }

    /// Stop the running turn: ask the provider to interrupt it, close its
    /// stdin, and kill it if it has not exited within [`STOP_GRACE`]. Open
    /// requests are dropped unanswered; the draft is kept.
    fn stop(&mut self) {
        self.stop_armed = false;
        let Some(mut run) = self.running.take() else {
            return;
        };
        if let Some(interrupt) = run.turn.interrupt() {
            // Best effort: a provider that stopped reading is killed anyway.
            let _ = run.process.send(&interrupt);
        }
        self.retire(run.process);
        self.panel.busy = false;
        self.panel
            .push(Role::Error, t!("status.agent_stopped").to_string());
    }

    fn poll(&mut self) -> io::Result<()> {
        if self.stopping.as_mut().is_some_and(Stopping::done) {
            self.stopping = None;
        }
        let Some(mut run) = self.running.take() else {
            return Ok(());
        };
        for msg in run.process.drain() {
            for event in run.turn.receive(&msg) {
                match event {
                    Event::Send(out) => run.process.send(&out)?,
                    Event::Session(id) => {
                        self.conversation.transcript.session = Some(id);
                        self.conversation.save()?;
                    }
                    Event::Text(s) => {
                        let s = sanitize(&s);
                        self.panel.push(Role::Assistant, s.clone());
                        self.conversation
                            .transcript
                            .messages
                            .push(Message::Reply(s));
                        self.conversation.save()?;
                    }
                    Event::Tool(s) => self.panel.push(Role::Assistant, sanitize(&s)),
                    Event::Ask(ask) => {
                        let panel = approval_panel(&ask);
                        run.approvals.push_back((ask, panel));
                    }
                    Event::Cancelled(id) => run.approvals.retain(|(ask, _)| ask.id() != &id),
                    Event::Declined(q) => self.panel.push(
                        Role::Error,
                        t!("ui.agent_declined", request = sanitize(&q)).to_string(),
                    ),
                    Event::Done => {
                        self.panel.busy = false;
                        self.retire(run.process);
                        return Ok(());
                    }
                    Event::Interrupted => {
                        self.panel.busy = false;
                        self.panel
                            .push(Role::Error, t!("status.agent_interrupted").to_string());
                        self.retire(run.process);
                        return Ok(());
                    }
                    Event::Failed(s) => {
                        return Err(io::Error::other(with_stderr(&sanitize(&s), &run.process)));
                    }
                    _ => {}
                }
            }
        }
        if run.process.is_closed() {
            // Give the stderr reader a moment to collect the last words.
            let until = after(EXIT_GRACE);
            while matches!(run.process.try_wait(), Ok(None)) && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(10));
            }
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                with_stderr(&t!("status.acp_exited"), &run.process),
            ));
        }
        self.running = Some(run);
        Ok(())
    }

    fn fail(&mut self, error: &io::Error) {
        self.running = None;
        self.stop_armed = false;
        self.panel.busy = false;
        self.panel.push(
            Role::Error,
            t!("status.acp_failed", error = sanitize(&error.to_string())).to_string(),
        );
    }

    fn answer(&mut self, allow: bool) -> io::Result<()> {
        if let Some(run) = &mut self.running
            && let Some((ask, _)) = run.approvals.pop_front()
        {
            run.process.send(&ask.answer(allow))?;
        }
        Ok(())
    }
}

impl Drop for NativePanel {
    /// Closing the panel (or quitting) stops a running turn the same way Esc
    /// does, then waits at most [`STOP_GRACE`] for the provider to exit
    /// before killing it and its tool processes.
    fn drop(&mut self) {
        self.stop();
        if let Some(mut stopping) = self.stopping.take() {
            while !stopping.done() {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

impl App {
    fn agent_store(&self) -> io::Result<Store> {
        let path = self
            .session_path
            .clone()
            .or_else(|| vix_fileops::user_dirs::config_file("vix", "session"))
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        Ok(Store::for_workspace(&path, &self.session_key()))
    }

    fn native_allowed(&mut self) -> bool {
        if self.ai_refused() {
            return false;
        }
        if !crate::authority::current().is_local() || self.remote_session_key().is_some() {
            self.status = t!("status.agent_local_only").to_string();
            return false;
        }
        if !self.workspace_trusted() {
            self.status = t!("status.acp_untrusted").to_string();
            return false;
        }
        true
    }

    pub(super) fn agent_conversations(&mut self) {
        if !self.native_allowed() {
            if self.settings.ai_enabled()
                && crate::authority::current().is_local()
                && self.remote_session_key().is_none()
                && self.script_trust.is_none()
            {
                self.script_trust = Some(super::ScriptTrustPrompt {
                    count: self.project_script_count(),
                    settings: vix_settings::project_settings_path(&self.root).is_file(),
                    env: vix_project_env::needs_trust(&self.root, Self::user_env_file().as_deref()),
                    language_server: None,
                });
                self.script_trust_opened_at = Some(std::time::Instant::now());
            }
            return;
        }
        if self.native_panel.is_some() {
            return;
        }
        let entries = match self.agent_store().and_then(|s| s.entries()) {
            Ok(entries) => entries,
            Err(e) => {
                self.status = t!("status.acp_failed", error = sanitize(&e.to_string())).to_string();
                return;
            }
        };
        let mut choices = vec![Choice::New(Provider::Codex), Choice::New(Provider::Claude)];
        #[cfg(feature = "git")]
        if crate::git::is_repo(&self.root) {
            choices.extend([
                Choice::NewWorktree(Provider::Codex),
                Choice::NewWorktree(Provider::Claude),
            ]);
        }
        choices.extend(entries.into_iter().map(Choice::Resume));
        let rows = choices
            .iter()
            .map(|choice| match choice {
                Choice::New(p) => t!("ui.agent_new", provider = p.name()).to_string(),
                #[cfg(feature = "git")]
                Choice::NewWorktree(p) => {
                    t!("ui.agent_new_worktree", provider = p.name()).to_string()
                }
                Choice::Resume(e) => t!(
                    "ui.agent_saved_row",
                    provider = e.provider.name(),
                    title = e.worktree_branch().map_or_else(
                        || sanitize(&e.title),
                        |b| format!("{} · {b}", sanitize(&e.title))
                    )
                )
                .to_string(),
            })
            .collect();
        self.choice_list = Some(ChoiceList {
            panel: vix_choice_list::Panel::new(
                t!("menu.item.ai.agent_conversations").to_string(),
                t!("ui.remote_pick_hint").to_string(),
                rows,
            ),
            purpose: ChoicePurpose::AgentConversations {
                root: self.root.clone(),
                choices,
            },
        });
    }

    pub(super) fn select_agent_conversation(&mut self, root: &std::path::Path, choice: &Choice) {
        if self.root != root || !self.native_allowed() || self.native_panel.is_some() {
            return;
        }
        match self.agent_store().and_then(|s| match choice {
            Choice::New(p) => s.create(*p).map(|c| (c, false)),
            #[cfg(feature = "git")]
            Choice::NewWorktree(p) => s.create_worktree(*p).map(|c| (c, true)),
            Choice::Resume(entry) => s.resume(entry).map(|c| (c, false)),
        }) {
            Ok((c, create)) => {
                self.native_panel = Some(NativePanel::open(c, self.root.clone(), create));
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.status = t!("status.agent_conversation_locked").to_string();
            }
            Err(e) => {
                self.status = t!("status.acp_failed", error = sanitize(&e.to_string())).to_string();
            }
        }
    }

    pub(super) fn poll_native_agents(&mut self) {
        if self
            .native_panel
            .as_ref()
            .is_some_and(|p| p.root != self.root)
            || !self.settings.ai_enabled()
        {
            self.native_panel = None;
        }
        if let Some(native) = &mut self.native_panel
            && let Err(e) = native.poll()
        {
            native.fail(&e);
        }
    }

    /// Stop any native provider before Vix exits (see [`NativePanel`]'s drop).
    pub(super) fn close_native_agents(&mut self) {
        self.native_panel = None;
    }

    pub(super) fn native_agent_key(&mut self, key: KeyEvent) {
        if (key.code == KeyCode::Enter || (key.code == KeyCode::Char('y') && Self::ctrl(&key)))
            && !self.native_allowed()
        {
            self.native_panel = None;
            return;
        }
        let Some(native) = &mut self.native_panel else {
            return;
        };
        if native.root != self.root {
            self.native_panel = None;
            return;
        }
        let plain = !Self::ctrl(&key) && !Self::alt(&key);
        let armed = std::mem::take(&mut native.stop_armed);
        let result = match key.code {
            KeyCode::Up => {
                native.displayed_panel().scroll_up();
                Ok(())
            }
            KeyCode::Down => {
                native.displayed_panel().scroll_down();
                Ok(())
            }
            KeyCode::PageUp => {
                for _ in 0..10 {
                    native.displayed_panel().scroll_up();
                }
                Ok(())
            }
            KeyCode::PageDown => {
                for _ in 0..10 {
                    native.displayed_panel().scroll_down();
                }
                Ok(())
            }
            KeyCode::Char('y') if native.awaiting_approval() && Self::ctrl(&key) => {
                native.answer(true)
            }
            KeyCode::Char('n' | 'N') if native.awaiting_approval() && plain => native.answer(false),
            KeyCode::Esc if native.awaiting_approval() => native.answer(false),
            KeyCode::Esc if native.running.is_some() && armed => {
                native.stop();
                Ok(())
            }
            KeyCode::Esc if native.running.is_some() => {
                native.stop_armed = true;
                self.status = t!("status.agent_esc_again").to_string();
                Ok(())
            }
            KeyCode::Esc => {
                self.native_panel = None;
                return;
            }
            _ if native.awaiting_approval() => Ok(()),
            KeyCode::Enter => native.start(AgentProcess::spawn_isolated),
            KeyCode::Char('v') if Self::ctrl(&key) => {
                if let Ok(text) = crate::clipboard::get() {
                    native.panel.input.push_str(&text.replace("\r\n", "\n"));
                }
                Ok(())
            }
            KeyCode::Backspace => {
                native.panel.input.pop();
                Ok(())
            }
            KeyCode::Char(c) if plain => {
                native.panel.input.push(c);
                Ok(())
            }
            _ => Ok(()),
        };
        if let Err(e) = result {
            native.fail(&e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::cov4::{app_at, code, temp};
    use std::path::Path;
    use vix_input::KeyModifiers;

    /// Tick the app every 10 ms until `ready`, for up to 30 s: the gate
    /// runs many test binaries at once.
    fn wait(app: &mut App, what: &str, ready: impl Fn(&NativePanel) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            app.poll_native_agents();
            let panel = app.native_panel.as_ref().unwrap();
            if ready(panel) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}: {:?}",
                panel.panel.turns
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn eventually(what: &str, ready: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !ready() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// A trusted workspace with the chooser reachable.
    fn trusted(name: &str) -> (PathBuf, App) {
        let dir = temp(name);
        let mut app = app_at(&dir);
        trust(&mut app);
        (dir, app)
    }

    fn trust(app: &mut App) {
        app.run_action("ai.agent_conversations");
        assert!(app.choice_list.is_none());
        assert!(app.script_trust.is_some());
        app.script_trust_opened_at = None;
        app.on_key(code(KeyCode::Char('y')));
        assert!(app.workspace_trusted());
    }

    /// Open the chooser and pick row `row` (0 new Codex, 1 new Claude, 2.. saved).
    fn choose(app: &mut App, row: usize) {
        app.run_action("ai.agent_conversations");
        for _ in 0..row {
            app.on_key(code(KeyCode::Down));
        }
        app.on_key(code(KeyCode::Enter));
    }

    /// Send "one\ntwo" to the scripted provider in `mode`.
    fn send_to_fake(app: &mut App, mode: &str) {
        let script =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/vix-ai-agents/tests/provider.py");
        app.on_paste("one\ntwo");
        let native = app.native_panel.as_mut().unwrap();
        let provider = native.conversation.transcript.provider;
        native
            .start(|argv, cwd| {
                let mut args = vec![
                    if cfg!(windows) { "python" } else { "python3" }.to_owned(),
                    script.display().to_string(),
                    provider.executable().to_owned(),
                    mode.to_owned(),
                ];
                args.extend_from_slice(&argv[1..]);
                AgentProcess::spawn_isolated(&args, cwd)
            })
            .unwrap();
    }

    fn screen(app: &mut App) -> String {
        let mut screen =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 32)).unwrap();
        screen.draw(|f| crate::ui::draw(app, f)).unwrap();
        let buffer = screen.backend().buffer();
        buffer
            .content()
            .chunks(usize::from(buffer.area.width))
            .map(|row| {
                row.iter()
                    .map(ratatui::buffer::Cell::symbol)
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join(
                "
",
            )
    }

    fn errors(native: &NativePanel) -> Vec<&str> {
        native
            .panel
            .turns
            .iter()
            .filter(|t| t.role == Role::Error)
            .map(|t| t.text.as_str())
            .collect()
    }

    fn exercise(provider: Provider) {
        let (dir, mut app) = trusted(provider.executable());
        for mode in ["new", "resume"] {
            // The saved conversation is the first row after the two new ones.
            let row = if mode == "resume" {
                2
            } else {
                usize::from(provider == Provider::Claude)
            };
            choose(&mut app, row);
            let native = app.native_panel.as_mut().unwrap();
            assert_eq!(native.conversation.transcript.provider, provider);
            if mode == "resume" {
                assert!(native.panel.last_assistant().is_some());
            }
            send_to_fake(&mut app, mode);
            wait(&mut app, "the approval", NativePanel::awaiting_approval);
            let shown = screen(&mut app);
            assert!(
                shown.contains(provider.name()) && shown.contains("Ctrl+Y"),
                "{shown}"
            );
            if provider == Provider::Claude {
                assert!(
                    shown.contains("Tool: Edit") && shown.contains("x: 1"),
                    "{shown}"
                );
                app.on_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL));
            } else {
                assert!(shown.contains("Command: test command"), "{shown}");
                app.on_key(code(KeyCode::Esc));
            }
            wait(&mut app, "the reply", |p| !p.panel.busy);
            assert_eq!(
                app.native_panel.as_ref().unwrap().panel.last_assistant(),
                Some(format!("{} reply", provider.executable()).as_str())
            );
            app.on_key(code(KeyCode::Esc));
            assert!(app.native_panel.is_none());
            app = app_at(&dir);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn native_codex_conversation_and_resume() {
        exercise(Provider::Codex);
    }

    #[test]
    fn native_claude_conversation_and_resume() {
        exercise(Provider::Claude);
    }

    #[cfg(feature = "git")]
    #[test]
    fn isolated_providers_resume_in_distinct_checkouts_of_the_open_subfolder() {
        use crate::app::git::tests::{git, repo};
        use std::fs;
        let repo = repo("agents", &[("sub/worktree-test.txt", "base\n")]);
        let sub = repo.join("sub");
        fs::write(sub.join("worktree-test.txt"), "my draft").unwrap();
        fs::write(sub.join("untracked"), "private draft").unwrap();
        let config = temp("agent-config");
        let app_for_sub = || app_at(&sub).with_session_path(config.join("session.toml"));
        let mut app = app_for_sub();
        trust(&mut app);
        let mut checkouts = Vec::new();
        for (row, provider) in [(2, Provider::Codex), (3, Provider::Claude)] {
            choose(&mut app, row);
            let checkout = app
                .native_panel
                .as_ref()
                .unwrap()
                .conversation
                .worktree()
                .unwrap();
            assert!(
                !checkout.exists(),
                "opening the panel is side-effect free for Git"
            );
            for mode in ["new", "resume"] {
                if mode == "resume" {
                    app = app_for_sub();
                    choose(&mut app, 4); // most recently used conversation
                }
                assert_eq!(
                    app.native_panel.as_ref().unwrap().conversation.worktree(),
                    Some(checkout.clone())
                );
                send_to_fake(&mut app, mode);
                wait(
                    &mut app,
                    "worktree approval",
                    NativePanel::awaiting_approval,
                );
                if provider == Provider::Codex {
                    app.on_key(code(KeyCode::Esc));
                } else {
                    app.on_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL));
                }
                wait(&mut app, "worktree reply", |p| !p.panel.busy);
                assert_eq!(
                    errors(app.native_panel.as_ref().unwrap()),
                    Vec::<&str>::new()
                );
                app.on_key(code(KeyCode::Esc));
            }
            let name = provider.executable();
            assert_eq!(
                fs::read_to_string(checkout.join("sub/worktree-test.txt")).unwrap(),
                format!("base\n{name}\n{name}\n")
            );
            assert!(!checkout.join("sub/untracked").exists());
            assert_eq!(
                git(&checkout, &["branch", "--show-current"]),
                checkout.file_name().unwrap().to_str().unwrap()
            );
            checkouts.push(checkout);
        }
        assert_ne!(checkouts[0], checkouts[1]);
        assert_eq!(
            fs::read_to_string(sub.join("worktree-test.txt")).unwrap(),
            "my draft"
        );
        assert_eq!(git(&repo, &["branch", "--show-current"]), "main");

        // Changing the branch after opening a saved conversation blocks a
        // turn, as does moving the checkout away between consecutive attempts.
        choose(&mut app, 4);
        let panel = app.native_panel.as_mut().unwrap();
        panel.panel.input = "keep this draft".into();
        git(&checkouts[1], &["switch", "-c", "other"]);
        assert!(
            panel
                .start(|_, _| panic!("provider started on another branch"))
                .is_err()
        );
        fs::rename(&checkouts[1], config.join("moved")).unwrap();
        assert!(
            panel
                .start(|_, _| panic!("provider started without its worktree"))
                .is_err()
        );
        assert_eq!(panel.panel.input, "keep this draft");
        assert!(!checkouts[1].exists());
        drop(app);
        fs::remove_dir_all(config).unwrap();
        fs::remove_dir_all(repo).unwrap();
    }

    #[cfg(not(feature = "git"))]
    #[test]
    fn a_build_without_git_can_read_an_isolated_chat_but_cannot_start_its_provider() {
        let (dir, mut app) = trusted("without-git");
        let mut chat = app
            .agent_store()
            .unwrap()
            .create_worktree(Provider::Claude)
            .unwrap();
        chat.transcript
            .messages
            .push(Message::Reply("saved reply".into()));
        chat.save().unwrap();
        drop(chat);
        choose(&mut app, 2);
        let panel = app.native_panel.as_mut().unwrap();
        assert_eq!(panel.panel.last_assistant(), Some("saved reply"));
        panel.panel.input = "draft".into();
        assert!(panel.start(|_, _| panic!("started without Git")).is_err());
        drop(app);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn concurrent_approvals_are_queued_and_a_cancelled_one_is_dropped() {
        let (dir, mut app) = trusted("queue");
        choose(&mut app, 1);
        send_to_fake(&mut app, "queue");
        // Three requests and a cancel arrive together: the cancelled one goes.
        wait(&mut app, "the queue", |p| {
            p.running.as_ref().is_some_and(|r| r.approvals.len() == 2)
        });
        let shown = screen(&mut app);
        let lines: Vec<&str> = shown.lines().map(str::trim).collect();
        assert!(
            shown.contains("Command:") && lines.iter().any(|l| l.contains("echo two")),
            "{shown}"
        );
        assert!(
            !shown.contains("rm -rf"),
            "only the oldest is shown: {shown}"
        );
        app.on_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL));
        let shown = screen(&mut app);
        assert!(shown.contains("rm -rf /tmp/x"), "{shown}");
        // Shift+N declines as well as n.
        app.on_key(KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT));
        wait(&mut app, "the reply", |p| !p.panel.busy);
        let native = app.native_panel.as_ref().unwrap();
        assert!(errors(native).is_empty(), "{:?}", native.panel.turns);
        assert_eq!(native.panel.last_assistant(), Some("claude reply"));
        drop(app);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn codex_file_changes_are_shown_with_their_approval() {
        let (dir, mut app) = trusted("filechange");
        choose(&mut app, 0);
        send_to_fake(&mut app, "filechange");
        wait(&mut app, "the approval", NativePanel::awaiting_approval);
        let shown = screen(&mut app);
        assert!(
            shown.contains("Change:") && shown.contains("src/b.rs") && shown.contains("+new"),
            "{shown}"
        );
        assert!(shown.contains("Reason: apply the patch"), "{shown}");
        app.on_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL));
        wait(&mut app, "the reply", |p| !p.panel.busy);
        let native = app.native_panel.as_ref().unwrap();
        assert!(
            native
                .panel
                .turns
                .iter()
                .any(|t| t.text == "src/a.rs\n+hello"),
            "the started item is a tool line: {:?}",
            native.panel.turns
        );
        drop(app);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Run `mode` for `row`'s provider and return the panel's error lines.
    fn failure(name: &str, row: usize, mode: &str) -> Vec<String> {
        let (dir, mut app) = trusted(name);
        choose(&mut app, row);
        send_to_fake(&mut app, mode);
        wait(&mut app, "the end of the turn", |p| !p.panel.busy);
        let native = app.native_panel.as_ref().unwrap();
        let out = errors(native).into_iter().map(str::to_owned).collect();
        drop(app);
        std::fs::remove_dir_all(dir).unwrap();
        out
    }

    #[test]
    fn failed_interrupted_and_refused_turns_say_why() {
        let failed = failure("failed", 0, "failed");
        assert!(failed[0].contains("quota exceeded"), "{failed:?}");
        let interrupted = failure("interrupted", 0, "interrupted");
        assert_eq!(interrupted, [t!("status.agent_interrupted").to_string()]);
        let refused = failure("is-error", 1, "is_error");
        assert!(
            refused[0].contains("Credit balance is too low"),
            "{refused:?}"
        );
    }

    #[test]
    fn a_provider_exiting_mid_turn_reports_its_stderr_tail() {
        for row in [0, 1] {
            let errors = failure("exit", row, "exit");
            assert!(
                errors[0].contains(t!("status.acp_exited").as_ref())
                    && errors[0].contains("fatal: boom"),
                "{errors:?}"
            );
            assert!(errors[0].lines().count() <= STDERR_LINES + 1, "{errors:?}");
        }
    }

    #[test]
    fn non_json_and_oversized_lines_are_skipped() {
        for (row, name) in [(0, "codex"), (1, "claude")] {
            let (dir, mut app) = trusted(&format!("garbage-{name}"));
            choose(&mut app, row);
            send_to_fake(&mut app, "garbage");
            wait(&mut app, "the reply", |p| !p.panel.busy);
            let native = app.native_panel.as_ref().unwrap();
            assert!(errors(native).is_empty(), "{:?}", native.panel.turns);
            assert_eq!(
                native.panel.last_assistant(),
                Some(format!("{name} reply").as_str())
            );
            drop(app);
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn a_missing_provider_is_named() {
        let (dir, mut app) = trusted("missing");
        choose(&mut app, 0);
        app.on_paste("hello");
        let native = app.native_panel.as_mut().unwrap();
        let e = native
            .start(|_, cwd| AgentProcess::spawn_isolated(&["vix-no-such-provider".to_owned()], cwd))
            .unwrap_err();
        assert_eq!(
            e.to_string(),
            t!("status.agent_not_installed", provider = "Codex")
        );
        assert_eq!(native.panel.input, "hello", "the draft is kept");
        drop(app);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn esc_twice_interrupts_the_turn_and_keeps_the_draft() {
        for (row, name) in [(0, "codex"), (1, "claude")] {
            let (dir, mut app) = trusted(&format!("stop-{name}"));
            choose(&mut app, row);
            send_to_fake(&mut app, "stop");
            wait(&mut app, "the turn to start", |p| {
                p.panel.turns.iter().any(|t| t.text == "working")
            });
            for c in "draft".chars() {
                app.on_key(code(KeyCode::Char(c)));
            }
            app.on_key(code(KeyCode::Esc));
            assert_eq!(app.status, t!("status.agent_esc_again"));
            assert!(screen(&mut app).contains(t!("status.agent_esc_again").as_ref()));
            let native = app.native_panel.as_ref().unwrap();
            assert!(native.panel.busy && native.running.is_some());
            app.on_key(code(KeyCode::Esc));
            let native = app.native_panel.as_ref().unwrap();
            assert!(!native.panel.busy && native.running.is_none());
            assert_eq!(native.panel.input, "draft");
            assert_eq!(errors(native), [t!("status.agent_stopped").as_ref()]);
            // The provider saw the interrupt, then end of input, and exits.
            eventually("the interrupt", || dir.join("fake-interrupted").exists());
            eventually("end of input", || dir.join("fake-eof").exists());
            wait(&mut app, "the provider to exit", |p| !p.active());
            app.on_key(code(KeyCode::Esc));
            assert!(app.native_panel.is_none());
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn closing_during_a_turn_interrupts_it() {
        let (dir, mut app) = trusted("close-stop");
        choose(&mut app, 1);
        send_to_fake(&mut app, "stop");
        wait(&mut app, "the turn to start", |p| {
            p.panel.turns.iter().any(|t| t.text == "working")
        });
        app.on_exit();
        assert!(app.native_panel.is_none());
        eventually("the interrupt", || dir.join("fake-interrupted").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_locked_conversation_is_refused_and_a_corrupt_one_skipped() {
        let (dir, mut app) = trusted("locked");
        let store = app.agent_store().unwrap();
        let mut held = store.create(Provider::Codex).unwrap();
        held.transcript
            .messages
            .push(Message::User("held elsewhere".into()));
        held.save().unwrap();
        std::fs::write(
            store
                .dir()
                .join("00000000-0000-4000-8000-000000000000.json"),
            b"{corrupt",
        )
        .unwrap();
        app.run_action("ai.agent_conversations");
        let rows = &app.choice_list.as_ref().unwrap().panel.rows;
        assert_eq!(rows.len(), 3, "{rows:?}");
        assert_eq!(rows[2], "Codex · held elsewhere");
        app.on_key(code(KeyCode::Down));
        app.on_key(code(KeyCode::Down));
        app.on_key(code(KeyCode::Enter));
        assert!(app.native_panel.is_none());
        assert_eq!(app.status, t!("status.agent_conversation_locked"));
        drop(held);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_prompt_and_draft_are_sanitized_on_screen() {
        let (dir, mut app) = trusted("sanitize");
        choose(&mut app, 0);
        app.on_paste("safe\u{1b}[31mred\u{202e}txt");
        let shown = screen(&mut app);
        assert!(
            shown.contains("safered") && !shown.contains('\u{202e}'),
            "{shown}"
        );
        drop(app);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
