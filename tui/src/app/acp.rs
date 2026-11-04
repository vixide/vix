//! The Agent Client Protocol host (T635): start a coding agent for a trusted
//! workspace, prompt it, and answer what it asks. See `crates/vix-acp/spec`.
//!
//! The agent is a guest. It is only spawned when [`App::workspace_trusted`];
//! permission requests are answered through a prompt that defaults to deny;
//! file writes arrive as a pending `vix-ai-diff` review and never touch the disk;
//! paths outside the workspace root are refused.

use std::collections::VecDeque;

use serde_json::Value;
use vix_acp::process::AgentProcess;
use vix_acp::{Client, Event, PermOption, PlanEntry, policy};

use super::{AiTarget, App, Prompt, PromptKind};
use crate::ai_panel::Role;

/// The largest file the agent may read in one go (8 MiB).
const MAX_READ: u64 = 8 * 1024 * 1024;

/// The synthetic path (so title) of the read-only plan tab.
const PLAN_TAB: &str = "*agent plan*";

/// A running agent and the conversation state around it.
pub(super) struct AcpHost {
    /// The command line it was started from (an entry of `ai_agents`).
    command: String,
    /// The agent's latest plan.
    plan: Vec<PlanEntry>,
    /// Tool calls so far as `(id, title, status)`, in first-seen order.
    tools: Vec<(String, String, String)>,
    /// The child process.
    proc: AgentProcess,
    /// The protocol state machine.
    client: Client,
    /// Reply text streamed so far, flushed to the panel as one turn.
    reply: String,
    /// The permission request currently shown as a prompt.
    asking: Option<(Value, Vec<PermOption>)>,
    /// Permission requests waiting for the prompt to be free.
    queue: VecDeque<(Value, String, Vec<PermOption>)>,
}

impl AcpHost {
    /// Write each message, ignoring a broken pipe (the next poll sees the
    /// agent is gone).
    fn send_all<'a>(&mut self, msgs: impl IntoIterator<Item = &'a Value>) {
        for m in msgs {
            let _ = self.proc.send(m);
        }
    }

    /// Answer the open permission request: `allow` picks an allow-once option,
    /// anything else rejects (or cancels).
    fn answer_permission(&mut self, allow: bool) {
        let Some((id, options)) = self.asking.take() else {
            return;
        };
        let choice = if allow {
            policy::allow_once_option(&options)
        } else {
            policy::reject_option(&options)
        };
        let msg = Client::answer_permission(&id, &options, choice);
        self.send_all([&msg]);
    }
}

impl App {
    /// Dispatch an `ai.agent_*` action. Returns `true` if handled.
    pub(super) fn run_acp_action(&mut self, action: &str) -> bool {
        match action {
            "ai.agent_start" => self.acp_start(),
            "ai.agent_resume" => self.acp_resume(),
            "ai.agent_plan" => self.acp_show_plan(),
            "ai.agent_prompt" => self.acp_begin_prompt(),
            "ai.agent_stop" => self.acp_stop(),
            _ => return false,
        }
        true
    }

    /// Whether an agent process is running (for tests and the status line).
    #[must_use]
    pub fn acp_running(&self) -> bool {
        self.acp.is_some()
    }

    /// Start a configured agent, if the workspace is trusted. One entry in
    /// `ai_agents` starts it directly; several ask which (a number).
    fn acp_start(&mut self) {
        if self.acp.is_some() {
            return;
        }
        // Trust first: an untrusted workspace never gets a process, whatever
        // else is wrong with the request.
        if !self.workspace_trusted() {
            self.status = t!("status.acp_untrusted").to_string();
            return;
        }
        match self.settings.ai_agents.len() {
            0 => self.status = t!("status.acp_no_agent").to_string(),
            1 => {
                let command = self.settings.ai_agents[0].clone();
                self.acp_spawn(&command, None);
            }
            _ => {
                let list: Vec<String> = self
                    .settings
                    .ai_agents
                    .iter()
                    .enumerate()
                    .map(|(i, c)| format!("{}) {c}", i + 1))
                    .collect();
                self.prompt = Some(Prompt::new(
                    PromptKind::AcpAgent,
                    t!("prompt.acp_agent", agents = list.join("  ")).to_string(),
                ));
            }
        }
    }

    /// The agent chooser was submitted: a 1-based number into `ai_agents`
    /// (empty means the first). Anything else starts nothing.
    pub(super) fn submit_acp_agent(&mut self, input: &str) {
        let input = input.trim();
        let index = if input.is_empty() {
            Some(0)
        } else {
            input.parse::<usize>().ok().and_then(|n| n.checked_sub(1))
        };
        match index.and_then(|i| self.settings.ai_agents.get(i).cloned()) {
            Some(command) => self.acp_spawn(&command, None),
            None => self.status = t!("status.acp_bad_choice").to_string(),
        }
    }

    /// Start the agent that ran last, asking it to load its previous session.
    fn acp_resume(&mut self) {
        if self.acp.is_some() {
            return;
        }
        if !self.workspace_trusted() {
            self.status = t!("status.acp_untrusted").to_string();
            return;
        }
        let Some((command, session)) = self.acp_last_session.clone().or_else(|| {
            // After a restart the in-memory copy is empty: read the one saved
            // for this workspace (see `acp_remember_session`).
            let saved = self.load_session();
            let ws = saved.workspace(&self.session_key())?;
            Some((ws.acp_agent.clone()?, ws.acp_session.clone()?))
        }) else {
            self.status = t!("status.acp_no_session").to_string();
            return;
        };
        // The command must still be one the user configured: a remembered
        // session never widens what may be executed.
        if !self.settings.ai_agents.contains(&command) {
            self.status = t!("status.acp_no_session").to_string();
            return;
        }
        self.acp_spawn(&command, Some(&session));
    }

    /// Spawn `command` (split on whitespace, no shell) in the workspace root.
    /// Callers have already checked [`App::workspace_trusted`]; it is checked
    /// again here so no path can reach the spawn without it.
    fn acp_spawn(&mut self, command: &str, resume: Option<&str>) {
        if !self.workspace_trusted() {
            self.status = t!("status.acp_untrusted").to_string();
            return;
        }
        let argv: Vec<String> = command.split_whitespace().map(str::to_string).collect();
        let Some(name) = argv.first().cloned() else {
            self.status = t!("status.acp_no_agent").to_string();
            return;
        };
        let mut proc = match AgentProcess::spawn(&argv, &self.root) {
            Ok(p) => p,
            Err(e) => {
                self.status = t!("status.acp_spawn_failed", error = e.to_string()).to_string();
                return;
            }
        };
        let cwd = self.root.display().to_string();
        let mut client = match resume {
            Some(sid) => Client::resuming(&cwd, sid),
            None => Client::new(&cwd),
        };
        for m in client.start() {
            let _ = proc.send(&m);
        }
        self.acp = Some(AcpHost {
            command: command.to_string(),
            plan: Vec::new(),
            tools: Vec::new(),
            proc,
            client,
            reply: String::new(),
            asking: None,
            queue: VecDeque::new(),
        });
        self.open_ai_panel();
        self.status = t!("status.acp_started", name = name).to_string();
    }

    /// Remember the agent and session id, in memory and in the workspace's
    /// saved session so **Resume Agent** works after Vix restarts.
    fn acp_remember_session(&mut self, command: &str, session_id: &str) {
        self.acp_last_session = Some((command.to_string(), session_id.to_string()));
        let key = self.session_key();
        let mut session = self.load_session();
        session.set_acp_session(&key, command, session_id);
        self.store_session_or_warn(&session);
    }

    /// The agent's plan and tool calls as the text of the plan tab.
    fn acp_plan_text(host: &AcpHost) -> String {
        let mut lines = vec![t!("ui.acp_plan_heading").to_string()];
        lines.extend(
            host.plan
                .iter()
                .map(|e| format!("- [{}] {}", e.status, e.content)),
        );
        lines.push(String::new());
        lines.push(t!("ui.acp_tools_heading").to_string());
        lines.extend(
            host.tools
                .iter()
                .map(|(_, title, status)| format!("- [{status}] {title}")),
        );
        lines.push(String::new());
        lines.join("\n")
    }

    /// Open (or refresh) the read-only plan tab: the plan and every tool call.
    fn acp_show_plan(&mut self) {
        let Some(host) = self.acp.as_ref() else {
            self.status = t!("status.acp_not_running").to_string();
            return;
        };
        let text = Self::acp_plan_text(host);
        if let Some(i) = self.acp_plan_tab() {
            self.editor.active = i;
            self.acp_refresh_plan_tab(&text);
            self.focus = super::Focus::Editor;
        } else {
            self.open_readonly_text_tab(PLAN_TAB, &text);
        }
    }

    /// The plan tab's index, if open.
    fn acp_plan_tab(&self) -> Option<usize> {
        self.editor
            .tabs
            .iter()
            .position(|t| t.path.as_deref() == Some(std::path::Path::new(PLAN_TAB)))
    }

    /// Replace the plan tab's text (it is read-only to the user, not to us).
    fn acp_refresh_plan_tab(&mut self, text: &str) {
        if let Some(i) = self.acp_plan_tab()
            && let Some(tab) = self.editor.tabs.get_mut(i)
        {
            tab.editor.set_content(text);
            tab.dirty = false;
        }
    }

    /// Stop the agent (its process is killed on drop).
    fn acp_stop(&mut self) {
        if let Some(panel) = self.acp_parked_panel.take() {
            self.ai_panel = Some(panel);
        }
        if self.acp.take().is_some() {
            if let Some(p) = self.acp_panel() {
                p.busy = false;
            }
            self.status = t!("status.acp_stopped").to_string();
        } else {
            self.status = t!("status.acp_not_running").to_string();
        }
    }

    /// Ask for a prompt to send the agent.
    fn acp_begin_prompt(&mut self) {
        if self.acp.is_none() {
            self.status = t!("status.acp_not_running").to_string();
            return;
        }
        self.acp_park_panel();
        self.prompt = Some(Prompt::new(
            PromptKind::AcpPrompt,
            t!("prompt.acp_prompt").to_string(),
        ));
    }

    /// Send `text` to the running agent (from the prompt or the chat panel).
    /// Returns `false`, leaving a status note, when nothing was sent.
    pub(super) fn acp_send_prompt(&mut self, text: &str) -> bool {
        let text = text.trim();
        let Some(host) = self.acp.as_mut() else {
            self.status = t!("status.acp_not_running").to_string();
            return false;
        };
        if text.is_empty() {
            return false;
        }
        let Some(msg) = host.client.prompt(text) else {
            self.status = t!("status.acp_not_ready").to_string();
            return false;
        };
        host.send_all([&msg]);
        if let Some(panel) = self.acp_panel() {
            panel.push(Role::User, text);
            panel.busy = true;
        }
        true
    }

    /// The prompt opened by **AI → Prompt Agent…** was submitted.
    pub(super) fn submit_acp_prompt(&mut self, input: &str) {
        self.acp_send_prompt(input);
    }

    /// The permission prompt was submitted: only `y`/`yes` allows.
    pub(super) fn submit_acp_permission(&mut self, input: &str) {
        let allow = matches!(input.trim().to_lowercase().as_str(), "y" | "yes");
        if let Some(host) = self.acp.as_mut() {
            host.answer_permission(allow);
        }
    }

    /// The chat panel, wherever it is (shown, or parked behind one of our prompts).
    fn acp_panel(&mut self) -> Option<&mut crate::ai_panel::Panel> {
        self.ai_panel.as_mut().or(self.acp_parked_panel.as_mut())
    }

    /// Move the chat panel out of the way of a prompt (see [`Self::poll_acp`]).
    fn acp_park_panel(&mut self) {
        if let Some(panel) = self.ai_panel.take() {
            self.acp_parked_panel = Some(panel);
        }
    }

    /// Add a transcript line (when the panel is open; otherwise the status line).
    fn acp_note(&mut self, role: Role, text: String) {
        match self.acp_panel() {
            Some(p) => p.push(role, text),
            None => self.status = text.lines().next().unwrap_or_default().to_string(),
        }
    }

    /// Move streamed reply text into the transcript as one turn.
    fn acp_flush_reply(&mut self, host: &mut AcpHost) {
        if !host.reply.trim().is_empty() {
            let text = std::mem::take(&mut host.reply);
            self.acp_note(Role::Assistant, text);
        }
    }

    /// Drain the agent's messages and act on them (called every tick).
    pub fn poll_acp(&mut self) {
        let Some(mut host) = self.acp.take() else {
            return;
        };
        // The chat panel sits above prompts in the overlay table, so it is parked
        // while one of ours is open and put back once it closes.
        if self.prompt.is_none()
            && let Some(panel) = self.acp_parked_panel.take()
        {
            self.ai_panel = Some(panel);
        }
        // A permission prompt that is no longer open (Esc, or replaced) is a
        // denial: the agent must never be left waiting, and silence is not consent.
        let prompt_open = self
            .prompt
            .as_ref()
            .is_some_and(|p| p.kind == PromptKind::AcpPermission);
        if host.asking.is_some() && !prompt_open {
            host.answer_permission(false);
        }
        for msg in host.proc.drain() {
            let step = host.client.handle(&msg);
            host.send_all(&step.out);
            for event in step.events {
                self.acp_event(&mut host, event);
            }
        }
        if host.proc.is_closed() {
            self.acp_flush_reply(&mut host);
            if let Some(p) = self.acp_panel() {
                p.busy = false;
            }
            self.status = t!("status.acp_exited").to_string();
            return;
        }
        if host.asking.is_none()
            && self.prompt.is_none()
            && let Some((id, title, options)) = host.queue.pop_front()
        {
            self.prompt = Some(Prompt::new(
                PromptKind::AcpPermission,
                t!("prompt.acp_permission", title = title).to_string(),
            ));
            host.asking = Some((id, options));
            self.acp_park_panel();
        }
        self.acp = Some(host);
    }

    /// Record a tool call, merging an update into the call it names (an empty
    /// title or status on an update means "unchanged").
    fn acp_track_tool(host: &mut AcpHost, id: &str, title: &str, status: &str) {
        if let Some(t) = host.tools.iter_mut().find(|t| t.0 == id) {
            if !title.is_empty() {
                title.clone_into(&mut t.1);
            }
            if !status.is_empty() {
                status.clone_into(&mut t.2);
            }
        } else {
            host.tools
                .push((id.to_string(), title.to_string(), status.to_string()));
        }
    }

    /// Keep an open plan tab current as the agent works.
    fn acp_sync_plan_tab(&mut self, host: &AcpHost) {
        if self.acp_plan_tab().is_some() {
            let text = Self::acp_plan_text(host);
            self.acp_refresh_plan_tab(&text);
        }
    }

    /// React to one protocol event.
    fn acp_event(&mut self, host: &mut AcpHost, event: Event) {
        match event {
            Event::Ready { session_id } => {
                self.acp_remember_session(&host.command, &session_id);
                self.status = if host.client.resumed() {
                    t!("status.acp_resumed").to_string()
                } else {
                    t!("status.acp_ready").to_string()
                };
            }
            Event::AgentText(t) => host.reply.push_str(&t),
            Event::Thought(_) => {}
            Event::ToolCall { id, title, status } => {
                Self::acp_track_tool(host, &id, &title, &status);
                self.acp_sync_plan_tab(host);
                if !title.is_empty() {
                    self.acp_flush_reply(host);
                    let line = t!("ui.acp_tool_call", title = title, status = status).to_string();
                    self.acp_note(Role::Assistant, line);
                }
            }
            Event::Plan(entries) => {
                host.plan.clone_from(&entries);
                self.acp_sync_plan_tab(host);
                self.acp_flush_reply(host);
                let steps: Vec<String> = entries
                    .iter()
                    .map(|e| format!("- [{}] {}", e.status, e.content))
                    .collect();
                let text = t!("ui.acp_plan", steps = steps.join("\n")).to_string();
                self.acp_note(Role::Assistant, text);
            }
            Event::Permission {
                req_id,
                title,
                options,
            } => host.queue.push_back((req_id, title, options)),
            Event::ReadFile {
                req_id,
                path,
                line,
                limit,
            } => {
                let answer = self.acp_read(&path, line, limit);
                let msg = match &answer {
                    Ok(text) => Client::answer_read(&req_id, Ok(text)),
                    Err(why) => Client::answer_read(&req_id, Err(why)),
                };
                host.send_all([&msg]);
            }
            Event::WriteFile {
                req_id,
                path,
                content,
            } => {
                let result = self.acp_write(&path, &content);
                let msg = Client::answer_write(&req_id, result.as_deref().map_or(Ok(()), Err));
                host.send_all([&msg]);
            }
            Event::TurnEnded { stop_reason } => {
                self.acp_flush_reply(host);
                if let Some(p) = self.acp_panel() {
                    p.busy = false;
                }
                self.status = t!("status.acp_turn_ended", reason = stop_reason).to_string();
            }
            Event::Failed(error) => {
                self.acp_flush_reply(host);
                if let Some(p) = self.acp_panel() {
                    p.busy = false;
                }
                let text = t!("status.acp_failed", error = error).to_string();
                self.acp_note(Role::Error, text);
            }
        }
    }

    /// The open tab showing `path`, matched as written or by real path (a tab
    /// may hold the canonical spelling of a path the agent got from the root).
    fn acp_tab_index(&self, path: &std::path::Path) -> Option<usize> {
        let real = path.canonicalize().ok();
        self.editor.tabs.iter().position(|t| {
            t.path.as_deref().is_some_and(|tp| {
                tp == path
                    || real
                        .as_deref()
                        .is_some_and(|r| tp.canonicalize().is_ok_and(|c| c == r))
            })
        })
    }

    /// The text the agent asked for: the open buffer's (including unsaved
    /// edits), else the file. Outside the workspace is refused.
    fn acp_read(
        &mut self,
        path: &str,
        line: Option<u64>,
        limit: Option<u64>,
    ) -> Result<String, String> {
        let Ok(p) = policy::contain_on_disk(&self.root, path) else {
            self.status = t!("status.acp_refused", path = path).to_string();
            return Err("path is outside the workspace".into());
        };
        let text = if let Some(tab) = self.acp_tab_index(&p).and_then(|i| self.editor.tabs.get(i)) {
            tab.text()
        } else {
            let too_big = std::fs::metadata(&p).is_ok_and(|m| m.len() > MAX_READ);
            if too_big {
                return Err("file is too large".into());
            }
            std::fs::read_to_string(&p).map_err(|e| e.to_string())?
        };
        Ok(vix_acp::client::slice_lines(&text, line, limit))
    }

    /// Queue the agent's write as a pending diff on the target buffer. Nothing
    /// is written to disk. `None` is success; `Some(reason)` is a refusal.
    fn acp_write(&mut self, path: &str, content: &str) -> Option<String> {
        let Ok(p) = policy::contain_on_disk(&self.root, path) else {
            self.status = t!("status.acp_refused", path = path).to_string();
            return Some("path is outside the workspace".into());
        };
        let exists = p.exists();
        if exists && !p.is_file() {
            return Some("only files can be edited".into());
        }
        if !exists && !p.parent().is_some_and(std::path::Path::is_dir) {
            return Some("the parent directory does not exist".into());
        }
        if self.ai_diff.is_some() {
            return Some("another edit is awaiting review".into());
        }
        if !exists && self.acp_tab_index(&p).is_none() {
            // A new file: an empty tab that carries the path but exists only in
            // memory. The content arrives as a pending review, and nothing is
            // on disk until the user accepts it and saves.
            let name = p.file_name().and_then(|n| n.to_str()).map(str::to_string);
            self.editor.new_tab_with_content_as("", name.as_deref());
            if let Some(t) = self.editor.active_tab_mut() {
                t.path = Some(p.clone());
                t.dirty = false;
            }
        } else if self.acp_tab_index(&p).is_none() {
            self.open_path(&p, false);
        }
        let Some(idx) = self.acp_tab_index(&p) else {
            return Some("could not open the file".into());
        };
        self.editor.active = idx;
        self.open_ai_diff(idx, AiTarget::Whole, content);
        if self.ai_diff.is_some() {
            self.status = t!("status.acp_edit_pending", path = path).to_string();
        }
        None
    }
}
