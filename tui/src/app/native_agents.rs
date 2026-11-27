//! Native Codex and Claude conversations shared by the TUI and GUI front ends.
use super::ide_parity::{ChoiceList, ChoicePurpose};
use super::{App, KeyCode, KeyEvent};
use crate::ai_panel::{Panel, Role};
use std::io;
use std::path::PathBuf;
use vix_acp::{display::sanitize, process::AgentProcess};
use vix_ai_agents::Provider;
use vix_ai_agents::conversation::{Conversation, Entry, Message, Store};
use vix_ai_agents::protocol::{Approval, Event, Turn};

#[derive(Clone, Debug)]
pub(crate) enum Choice {
    New(Provider),
    Resume(Entry),
}

struct Running {
    turn: Turn,
    process: AgentProcess,
    approval: Option<(Approval, Panel)>,
}

pub(crate) struct NativePanel {
    pub(crate) panel: Panel,
    pub(crate) conversation: Conversation,
    root: PathBuf,
    running: Option<Running>,
}

impl NativePanel {
    pub(crate) fn awaiting_approval(&self) -> bool {
        self.running.as_ref().is_some_and(|r| r.approval.is_some())
    }

    /// The current permission request stays visible until the user answers it.
    pub(crate) fn displayed_panel(&mut self) -> &mut Panel {
        if let Some((_, panel)) = self.running.as_mut().and_then(|r| r.approval.as_mut()) {
            panel
        } else {
            &mut self.panel
        }
    }

    fn open(conversation: Conversation, root: PathBuf) -> Self {
        let mut panel = Panel::open();
        for message in &conversation.transcript.messages {
            match message {
                Message::User(s) => panel.push(Role::User, sanitize(s)),
                Message::Reply(s) => panel.push(Role::Assistant, sanitize(s)),
            }
        }
        Self {
            panel,
            conversation,
            root,
            running: None,
        }
    }

    fn start(
        &mut self,
        spawn: impl FnOnce(&[String], &std::path::Path) -> io::Result<AgentProcess>,
    ) -> io::Result<()> {
        if self.running.is_some() || self.panel.input.trim().is_empty() {
            return Ok(());
        }
        let transcript = &self.conversation.transcript;
        let turn = Turn::new(
            transcript.provider,
            self.root.display().to_string(),
            transcript.session.clone(),
            self.panel.input.clone(),
        );
        let mut process = spawn(&turn.argv(), &self.root)?;
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
        self.panel.push(Role::User, prompt);
        self.panel.busy = true;
        self.running = Some(Running {
            turn,
            process,
            approval: None,
        });
        Ok(())
    }

    fn poll(&mut self) -> io::Result<()> {
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
                    Event::Ask(ask) if run.approval.is_some() => {
                        run.process.send(&ask.answer(false))?;
                    }
                    Event::Ask(ask) => {
                        let mut request = Panel::open();
                        request.push(
                            Role::Assistant,
                            sanitize(&serde_json::to_string_pretty(&ask.details)?),
                        );
                        run.approval = Some((ask, request));
                    }
                    Event::Done => {
                        self.panel.busy = false;
                        return Ok(());
                    }
                    Event::Failed(s) => {
                        return Err(io::Error::other(sanitize(&s)));
                    }
                }
            }
        }
        if run.process.is_closed() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                t!("status.acp_exited").to_string(),
            ));
        }
        self.running = Some(run);
        Ok(())
    }

    fn fail(&mut self, error: &io::Error) {
        self.running = None;
        self.panel.busy = false;
        self.panel.push(
            Role::Error,
            t!("status.acp_failed", error = sanitize(&error.to_string())).to_string(),
        );
    }

    fn answer(&mut self, allow: bool) -> io::Result<()> {
        if let Some(run) = &mut self.running
            && let Some((ask, _)) = run.approval.take()
        {
            run.process.send(&ask.answer(allow))?;
        }
        Ok(())
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
                self.status = t!("status.acp_failed", error = e.to_string()).to_string();
                return;
            }
        };
        let mut choices = vec![Choice::New(Provider::Codex), Choice::New(Provider::Claude)];
        choices.extend(entries.into_iter().map(Choice::Resume));
        let rows = choices
            .iter()
            .map(|choice| match choice {
                Choice::New(p) => t!("ui.agent_new", provider = p.name()).to_string(),
                Choice::Resume(e) => format!("{} · {}", e.provider.name(), sanitize(&e.title)),
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
            Choice::New(p) => s.create(*p),
            Choice::Resume(entry) => s.resume(entry),
        }) {
            Ok(c) => self.native_panel = Some(NativePanel::open(c, self.root.clone())),
            Err(e) => self.status = t!("status.acp_failed", error = e.to_string()).to_string(),
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
            KeyCode::Esc | KeyCode::Char('n') if native.awaiting_approval() => native.answer(false),
            KeyCode::Esc => {
                self.native_panel = None;
                return;
            }
            _ if native.awaiting_approval() => Ok(()),
            KeyCode::Enter => native.start(AgentProcess::spawn),
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
            KeyCode::Char(c) if !Self::ctrl(&key) && !Self::alt(&key) => {
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
    use std::time::{Duration, Instant};
    use vix_input::KeyModifiers;

    fn wait(app: &mut App, ready: impl Fn(&NativePanel) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            app.poll_native_agents();
            let panel = app.native_panel.as_ref().unwrap();
            if ready(panel) {
                return;
            }
            assert!(
                panel.panel.busy && Instant::now() < deadline,
                "provider stopped or timed out: {:?}",
                panel.panel.turns
            );
            std::thread::yield_now();
        }
    }

    fn exercise(provider: Provider) {
        let dir = temp(provider.executable());
        let mut app = app_at(&dir);
        app.run_action("ai.agent_conversations");
        assert!(app.choice_list.is_none());
        assert!(app.script_trust.is_some());
        app.script_trust_opened_at = None;
        app.on_key(code(KeyCode::Char('y')));
        assert!(app.workspace_trusted());
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../crates/vix-ai-agents/tests/provider.py");
        for mode in ["new", "resume"] {
            app.run_action("ai.agent_conversations");
            if mode == "resume" || provider == Provider::Claude {
                app.on_key(code(KeyCode::Down));
            }
            if mode == "resume" {
                app.on_key(code(KeyCode::Down));
            }
            app.on_key(code(KeyCode::Enter));
            app.on_paste("one\ntwo");
            let native = app.native_panel.as_mut().unwrap();
            assert_eq!(native.conversation.transcript.provider, provider);
            if mode == "resume" {
                assert!(native.panel.last_assistant().is_some());
            }
            native
                .start(|argv, cwd| {
                    let mut args = vec![
                        if cfg!(windows) { "python" } else { "python3" }.to_owned(),
                        script.display().to_string(),
                        provider.executable().to_owned(),
                        mode.to_owned(),
                    ];
                    args.extend_from_slice(&argv[1..]);
                    AgentProcess::spawn(&args, cwd)
                })
                .unwrap();
            wait(&mut app, NativePanel::awaiting_approval);
            let mut screen =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 32)).unwrap();
            screen.draw(|f| crate::ui::draw(&mut app, f)).unwrap();
            let shown: String = screen
                .backend()
                .buffer()
                .content()
                .iter()
                .map(ratatui::buffer::Cell::symbol)
                .collect();
            assert!(
                shown.contains(provider.name()) && shown.contains("Ctrl+Y"),
                "{shown}"
            );
            if provider == Provider::Claude {
                app.on_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL));
            } else {
                app.on_key(code(KeyCode::Esc));
            }
            wait(&mut app, |p| !p.panel.busy);
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
}
