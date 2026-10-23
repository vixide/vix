//! AI: the transform actions (summarize/explain/define/annotate/improve/
//! doc-comment), the background CLI/HTTP runner, the chat panel, and the
//! accept/reject diff review.

use super::{
    AiDest, AiDiffState, AiMsg, AiReplace, AiTarget, App, Focus, KeyCode, KeyEvent, Prompt,
    PromptKind, spawn_shell,
};

impl App {
    /// Dispatch an `ai.*` action (the AI menu / palette). Returns `true` if handled.
    pub(super) fn run_ai_action(&mut self, action: &str) -> bool {
        match action {
            "ai.chat" => self.open_ai_panel(),
            "ai.summarize" => self.ai_summarize(),
            "ai.explain" => self.ai_explain(),
            "ai.define" => self.ai_define(),
            "ai.annotate" => self.ai_annotate(),
            "ai.improve" => self.ai_improve(),
            "ai.edit_with_instruction" => self.ai_begin_edit_with_instruction(),
            "ai.generate_doc_comment" => self.ai_generate_doc_comment(),
            _ => return false,
        }
        true
    }

    // ----- AI -------------------------------------------------------------

    /// Summarize the selection (or the whole file when nothing is selected) with
    /// `claude`; the result opens in a new editor tab.
    pub(super) fn ai_summarize(&mut self) {
        let text = self.selected_or_all_text();
        self.ai_to_new_tab("Summarize this text.", &text, &t!("menu.item.ai.summarize"));
    }

    /// Explain the selection (or the whole file when nothing is selected) with
    /// `claude`; the result opens in a new editor tab.
    pub(super) fn ai_explain(&mut self) {
        let text = self.selected_or_all_text();
        self.ai_to_new_tab("Explain this text.", &text, &t!("menu.item.ai.explain"));
    }

    /// Define a word with `claude`; the result opens in a new editor tab. The
    /// input is the selection if there is one; otherwise the word under the
    /// cursor, or the next word when the cursor sits between words. Never the
    /// whole buffer.
    pub(super) fn ai_define(&mut self) {
        let text = self.selected_or_word_text();
        self.ai_to_new_tab("Define this text.", &text, &t!("menu.item.ai.define"));
    }

    /// The selection, else the word at the cursor, else the next word after it.
    /// Returns an empty string when there is no editable buffer or no word ahead.
    pub(super) fn selected_or_word_text(&mut self) -> String {
        let Some(tab) = self.editor.active_tab_mut() else {
            return String::new();
        };
        if let Some(sel) = tab.editor.get_selection_text()
            && !sel.trim().is_empty()
        {
            return sel;
        }
        let cursor = tab.editor.get_cursor();
        if let Some((_, _, word)) = tab.editor.word_at(cursor) {
            return word;
        }
        // Cursor is between words: scan forward to the next word.
        let len = tab.editor.get_content().chars().count();
        for pos in cursor..=len {
            if let Some((_, _, word)) = tab.editor.word_at(pos) {
                return word;
            }
        }
        String::new()
    }

    /// Annotate the selection (or the whole file when nothing is selected) with
    /// `claude`, replacing it with the result.
    pub(super) fn ai_annotate(&mut self) {
        self.ai_replace_text("Annotate this text.", &t!("menu.item.ai.annotate"));
    }

    /// Improve the selection (or the whole file when nothing is selected) with
    /// `claude`, replacing it with the result.
    pub(super) fn ai_improve(&mut self) {
        self.ai_replace_text("Improve this text.", &t!("menu.item.ai.improve"));
    }

    /// Launch `claude -p <prompt>` over the selection (or the whole buffer when
    /// nothing is selected), capturing its full output to replace that text when
    /// it finishes. The transform runs in the background; [`Self::poll_ai_replace`]
    /// applies the result.
    pub(super) fn ai_replace_text(&mut self, prompt: &str, label: &str) {
        if self.ai_replace.is_some() {
            self.status = t!("status.ai_busy").to_string();
            return;
        }
        let tab_idx = self.editor.active;
        let (text, target) = {
            let Some(tab) = self.editor.active_tab_mut() else {
                return;
            };
            if tab.is_image() {
                self.status = t!("status.ai_no_input").to_string();
                return;
            }
            match tab.editor.get_selection() {
                Some(sel) if !sel.is_empty() => (
                    tab.editor.get_content_slice(sel.start, sel.end),
                    AiTarget::Range(sel.start, sel.end),
                ),
                _ => (tab.editor.get_content(), AiTarget::Whole),
            }
        };
        let dest = self.ai_replace_or_diff_dest(tab_idx, target);
        if let Some(rx) = self.spawn_ai(prompt, &text) {
            self.ai_replace = Some(AiReplace {
                rx,
                dest,
                label: label.to_string(),
            });
            self.status = t!("status.ai_running", action = label).to_string();
        }
    }

    /// `AiDest::Diff` when `Settings::ai_diff_review` is on (the default),
    /// else `AiDest::Replace` — the choice every fixed-instruction AI text
    /// transform (Annotate/Improve/generate-doc-comment) makes the same way.
    pub(super) fn ai_replace_or_diff_dest(&self, tab: usize, target: AiTarget) -> AiDest {
        if self.settings.misc.ai_diff_review {
            AiDest::Diff { tab, target }
        } else {
            AiDest::Replace { tab, target }
        }
    }

    /// Begin "Edit selection with instruction" (T125): prompts for a
    /// free-text instruction to apply to the current selection. Requires an
    /// actual selection (unlike Annotate/Improve, this never silently
    /// widens to the whole buffer -- a user-authored instruction is
    /// unpredictable enough without also guessing the target). The result
    /// always opens as a reviewable diff, regardless of `ai_diff_review`:
    /// free-text instructions carry more risk than the fixed prompts the
    /// setting was designed to let power users skip past.
    pub(super) fn ai_begin_edit_with_instruction(&mut self) {
        if self.ai_replace.is_some() {
            self.status = t!("status.ai_busy").to_string();
            return;
        }
        let tab_idx = self.editor.active;
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        if tab.is_image() {
            self.status = t!("status.ai_no_input").to_string();
            return;
        }
        let Some(sel) = tab.editor.get_selection().filter(|s| !s.is_empty()) else {
            self.status = t!("status.ai_selection_required").to_string();
            return;
        };
        self.ai_instruction_target = Some((tab_idx, AiTarget::Range(sel.start, sel.end)));
        self.prompt = Some(Prompt::new(
            PromptKind::AiInstruction,
            t!("prompt.ai_instruction").to_string(),
        ));
    }

    /// Run the instruction typed into the `AiInstruction` prompt over the
    /// selection captured by [`Self::ai_begin_edit_with_instruction`].
    pub(super) fn submit_ai_instruction(&mut self, instruction: &str) {
        let Some((tab_idx, target)) = self.ai_instruction_target.take() else {
            return;
        };
        let instruction = instruction.trim();
        if instruction.is_empty() {
            self.status = t!("status.ai_no_input").to_string();
            return;
        }
        let Some(tab) = self.editor.tabs.get_mut(tab_idx) else {
            return;
        };
        let text = match target {
            AiTarget::Whole => tab.editor.get_content(),
            AiTarget::Range(start, end) => tab.editor.get_content_slice(start, end),
        };
        let label = t!("menu.item.ai.edit_with_instruction").to_string();
        if let Some(rx) = self.spawn_ai(instruction, &text) {
            self.ai_replace = Some(AiReplace {
                rx,
                dest: AiDest::Diff {
                    tab: tab_idx,
                    target,
                },
                label: label.clone(),
            });
            self.status = t!("status.ai_running", action = label).to_string();
        }
    }

    /// Generate a doc comment for the symbol under the cursor (T125): sends
    /// the cursor's line plus a following window of context (enough for the
    /// assistant to see the signature/body without sending the whole file),
    /// and proposes the reply as a *pure insertion* right before that line
    /// (`AiDest::InsertBeforeLine`) -- it never touches existing text, it
    /// only adds a new one above it. Requires no selection; explicit-invoke
    /// only, same as every other AI menu action.
    pub(super) fn ai_generate_doc_comment(&mut self) {
        // A generous but bounded context window -- enough for a typical
        // function/type/impl block without risking sending the whole file.
        const CONTEXT_LINES: usize = 40;
        if self.ai_replace.is_some() {
            self.status = t!("status.ai_busy").to_string();
            return;
        }
        let tab_idx = self.editor.active;
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        if tab.is_image() {
            self.status = t!("status.ai_no_input").to_string();
            return;
        }
        let code = tab.editor.code_ref();
        let cursor = tab.editor.get_cursor();
        let line = code.char_to_line(cursor);
        let line_start = code.line_to_char(line);
        let context_end_line = (line + CONTEXT_LINES).min(code.len_lines().saturating_sub(1));
        let context_end = code.line_to_char(context_end_line);
        let context = code.slice(line_start, context_end);
        if context.trim().is_empty() {
            self.status = t!("status.ai_no_input").to_string();
            return;
        }
        let dest = AiDest::InsertBeforeLine {
            tab: tab_idx,
            at: line_start,
        };
        let instruction = "Write a documentation comment for the symbol (function, type, \
             or similar) that begins on the first line below, in the idiomatic comment \
             style for this code's language, matching the indentation of that first line. \
             Output only the comment block, with a trailing newline -- no explanation, no \
             code fence, and none of the code itself.";
        let label = t!("menu.item.ai.generate_doc_comment").to_string();
        if let Some(rx) = self.spawn_ai(instruction, &context) {
            self.ai_replace = Some(AiReplace {
                rx,
                dest,
                label: label.clone(),
            });
            self.status = t!("status.ai_running", action = label).to_string();
        }
    }

    /// Launch `claude -p <prompt>` over `text`, capturing its full output to open
    /// in a new editor tab when it finishes (Summarize/Explain/Define).
    pub(super) fn ai_to_new_tab(&mut self, prompt: &str, text: &str, label: &str) {
        if self.ai_replace.is_some() {
            self.status = t!("status.ai_busy").to_string();
            return;
        }
        if let Some(rx) = self.spawn_ai(prompt, text) {
            self.ai_replace = Some(AiReplace {
                rx,
                dest: AiDest::NewTab,
                label: label.to_string(),
            });
            self.status = t!("status.ai_running", action = label).to_string();
        }
    }

    /// Spawn the configured AI command (see [`Settings::ai_command`]) over `text`,
    /// returning a receiver for its captured stdout (or `None` after reporting an
    /// empty input or a spawn failure). The reader thread sends one [`AiMsg`] when
    /// the CLI exits. The command is built from the `ai_command` template so the
    /// AI menu can drive any assistant CLI, not just `claude`.
    pub(super) fn spawn_ai(
        &mut self,
        prompt: &str,
        text: &str,
    ) -> Option<std::sync::mpsc::Receiver<AiMsg>> {
        if text.trim().is_empty() {
            self.status = t!("status.ai_no_input").to_string();
            return None;
        }
        self.spawn_ai_cmd(prompt, text)
    }

    /// The spawn core shared by [`Self::spawn_ai`] and the chat panel: dispatch
    /// on [`Settings::ai_provider`] (T124) to either the original CLI shell-out
    /// ([`Self::spawn_ai_cli`], the `"cli"` default — every existing call site
    /// is unaffected) or a direct HTTP provider ([`Self::spawn_ai_http`]).
    /// Unlike [`Self::spawn_ai`] this does **not** reject empty input — a chat
    /// turn may carry no editor context — so callers must guard that themselves.
    pub(super) fn spawn_ai_cmd(
        &mut self,
        prompt: &str,
        text: &str,
    ) -> Option<std::sync::mpsc::Receiver<AiMsg>> {
        match vix_ai_core::Provider::parse(&self.settings.ai_provider) {
            Some(provider) => self.spawn_ai_http(provider, prompt, text),
            None => self.spawn_ai_cli(prompt, text),
        }
    }

    /// Run the configured CLI (see [`Settings::ai_command`]) over `text`,
    /// returning a receiver for its captured stdout (or `None` after reporting a
    /// spawn failure). The reader thread sends one [`AiMsg`] when the CLI exits.
    /// The command is built from the `ai_command` template so the AI menu can
    /// drive any assistant CLI, not just `claude`. This is the original AI
    /// spawn path (predates T124's HTTP providers) and its behavior is
    /// unchanged — it is still what `ai_provider = "cli"` (the default) uses.
    #[expect(
        clippy::expect_used,
        reason = "stdout was configured as piped when the child was spawned"
    )]
    pub(super) fn spawn_ai_cli(
        &mut self,
        prompt: &str,
        text: &str,
    ) -> Option<std::sync::mpsc::Receiver<AiMsg>> {
        // A private, unpredictable temp file (0600, O_EXCL) so a local attacker
        // can't pre-plant a symlink at the name and redirect the write, nor read
        // the buffer contents left behind in a shared /tmp.
        let Ok(tmp) = crate::fileops::write_private_temp("vix-ai", text.as_bytes()) else {
            self.status = t!("status.ai_no_input").to_string();
            return None;
        };
        let path = tmp.display().to_string();
        let cmd = self.settings.ai_command_line(prompt, &path);
        // `ai_command_line` always redirects the shell's own stdin from
        // `path` (`< "…"`) unless the template uses `{file}` instead, so
        // this process's own stdin is never actually read either way --
        // explicitly closed rather than left to the default of inheriting
        // this process's own stdin, which is never what a background
        // command should be reading from regardless of platform (T547).
        let mut child = match spawn_shell(&cmd, &self.root, false) {
            Ok(c) => c,
            Err(e) => {
                self.messages
                    .error(t!("msg.command_failed", error = e).to_string());
                return None;
            }
        };
        let stdout = child.stdout.take().expect("piped stdout");
        let child = std::sync::Arc::new(std::sync::Mutex::new(child));
        let (tx, rx) = std::sync::mpsc::channel();
        let reader_child = child;
        std::thread::spawn(move || {
            use std::io::Read;
            let mut out = String::new();
            let ok = std::io::BufReader::new(stdout)
                .read_to_string(&mut out)
                .is_ok();
            let status = reader_child
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .wait()
                .ok();
            // The input file is no longer needed once the CLI has exited.
            let _ = std::fs::remove_file(&tmp);
            let success = ok && status.and_then(|s| s.code()) == Some(0);
            // Normalize CRLF to LF unconditionally, not `cfg(windows)`-gated:
            // a Windows console command's text-mode output can be
            // CRLF-terminated regardless of what actually produced it (T547),
            // and no downstream consumer (the diff view, the buffer) wants a
            // visible `\r` in text that's conceptually just lines.
            let out = out.replace("\r\n", "\n");
            let _ = tx.send(if success {
                AiMsg::Done(out)
            } else {
                AiMsg::Failed(None)
            });
        });
        Some(rx)
    }

    /// Call `provider`'s HTTP API directly (T124) over `instruction`/`input`,
    /// returning a receiver for its reply. Resolves the endpoint/model
    /// (`Settings::ai_endpoint`/`ai_model`, falling back to the provider's own
    /// defaults) and the API key (`vix_ai_core::secret::resolve`), then runs
    /// the blocking request on a background thread — same shape as
    /// [`Self::spawn_ai_cli`], so every downstream caller (`poll_ai_replace`)
    /// handles both paths identically.
    pub(super) fn spawn_ai_http(
        &mut self,
        provider: vix_ai_core::Provider,
        instruction: &str,
        input: &str,
    ) -> Option<std::sync::mpsc::Receiver<AiMsg>> {
        let endpoint = if self.settings.ai_endpoint.trim().is_empty() {
            provider.default_endpoint().to_string()
        } else {
            self.settings.ai_endpoint.clone()
        };
        let model = if self.settings.ai_model.trim().is_empty() {
            provider.default_model().to_string()
        } else {
            self.settings.ai_model.clone()
        };
        let api_key =
            vix_ai_core::secret::resolve(provider.name(), &self.settings.ai_api_key_command)
                .unwrap_or_default();
        if provider.requires_api_key() && api_key.is_empty() {
            self.messages
                .error(t!("msg.ai_no_api_key", provider = provider.name()).to_string());
            return None;
        }
        let instruction = instruction.to_string();
        let input = input.to_string();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result =
                vix_ai_core::complete(provider, &endpoint, &model, &api_key, &instruction, &input);
            let _ = tx.send(match result {
                Ok(text) => AiMsg::Done(text),
                Err(reason) => AiMsg::Failed(Some(reason)),
            });
        });
        Some(rx)
    }

    /// Drain a finished AI task and apply its result. Called once per event-loop
    /// iteration; cheap when none is running.
    pub fn poll_ai_replace(&mut self) {
        let msg = {
            let Some(ar) = self.ai_replace.as_ref() else {
                return;
            };
            match ar.rx.try_recv() {
                Ok(m) => m,
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => AiMsg::Failed(None),
            }
        };
        let Some(ar) = self.ai_replace.take() else {
            return;
        };
        match msg {
            AiMsg::Done(out) => {
                let text = out.trim_end_matches('\n');
                if text.is_empty() {
                    self.report_ai_failure(&ar, None);
                    return;
                }
                match ar.dest {
                    AiDest::Replace { tab, target } => self.apply_ai_replace(tab, target, text),
                    AiDest::NewTab => {
                        self.editor.new_tab_with_content(text);
                        self.focus = Focus::Editor;
                    }
                    AiDest::Panel => {
                        if let Some(panel) = self.ai_panel.as_mut() {
                            panel.busy = false;
                            panel.push(crate::ai_panel::Role::Assistant, text);
                        }
                    }
                    AiDest::Diff { tab, target } => self.open_ai_diff(tab, target, text),
                    #[cfg(feature = "sql")]
                    AiDest::Db => {
                        if let Some(b) = self.db.as_mut() {
                            b.apply_ai_reply(text);
                        }
                    }
                    #[cfg(feature = "git")]
                    AiDest::GitCommitMessage => {
                        self.git_panel = None;
                        self.prompt = Some(
                            Prompt::new(PromptKind::GitCommit, t!("prompt.git_commit").to_string())
                                .with_input(text.trim().to_string()),
                        );
                    }
                    AiDest::InsertBeforeLine { tab, at } => {
                        let mut insert = text.to_string();
                        insert.push('\n');
                        let target = AiTarget::Range(at, at);
                        if self.settings.misc.ai_diff_review {
                            self.open_ai_diff(tab, target, &insert);
                        } else {
                            self.apply_ai_replace(tab, target, &insert);
                        }
                    }
                }
                self.status = t!("status.ai_done", action = &ar.label).to_string();
                if !ar.dest.is_inline() {
                    self.messages.info(t!("status.ai_done", action = ar.label));
                }
            }
            AiMsg::Failed(reason) => self.report_ai_failure(&ar, reason.as_deref()),
        }
    }

    /// Tell every surface an AI task can report through that it failed:
    /// the chat panel and DB workbench get an inline message, everything
    /// else gets the status line plus a `Messages` entry. `reason` (T124)
    /// is an HTTP provider's error text when there is one -- `None` for the
    /// original CLI path (its stderr is discarded, so there is nothing more
    /// specific than "failed" to show, matching its behavior before T124).
    pub(super) fn report_ai_failure(&mut self, ar: &AiReplace, reason: Option<&str>) {
        let message = match reason {
            Some(reason) => t!(
                "status.ai_failed_detail",
                action = &ar.label,
                reason = reason
            )
            .to_string(),
            None => t!("status.ai_failed", action = &ar.label).to_string(),
        };
        if let (AiDest::Panel, Some(panel)) = (ar.dest, self.ai_panel.as_mut()) {
            panel.busy = false;
            panel.push(crate::ai_panel::Role::Error, message.clone());
        }
        #[cfg(feature = "sql")]
        if let (AiDest::Db, Some(b)) = (ar.dest, self.db.as_mut()) {
            b.ai_failed();
        }
        self.status.clone_from(&message);
        if !ar.dest.is_inline() {
            self.messages.error(message);
        }
    }

    /// Write an AI transform's `text` back into tab `tab_idx`, replacing either
    /// the whole buffer or the recorded character range.
    pub(super) fn apply_ai_replace(&mut self, tab_idx: usize, target: AiTarget, text: &str) {
        let Some(tab) = self.editor.tabs.get_mut(tab_idx) else {
            return;
        };
        let new = match target {
            AiTarget::Whole => text.to_string(),
            AiTarget::Range(start, end) => {
                let chars: Vec<char> = tab.editor.get_content().chars().collect();
                let n = chars.len();
                let start = start.min(n);
                let end = end.min(n).max(start);
                let mut out: String = chars[..start].iter().collect();
                out.push_str(text);
                out.extend(&chars[end..]);
                out
            }
        };
        tab.editor.set_content(&new);
        tab.editor.set_selection(None);
        // Keep the caret in range: set_content leaves the old cursor untouched,
        // which can point past the end when the replacement is shorter.
        let caret = tab.editor.get_cursor();
        tab.editor.set_cursor(caret);
        tab.dirty = true;
    }

    /// Whether a background AI transform is in progress.
    #[must_use]
    pub fn ai_replace_running(&self) -> bool {
        self.ai_replace.is_some()
    }

    // ----- AI chat panel --------------------------------------------------

    /// Open the AI chat panel (a persistent conversation with the configured
    /// assistant), or focus it if already open. Seeds the input with the current
    /// selection so "ask about this" is one keystroke away.
    pub(super) fn open_ai_panel(&mut self) {
        if self.ai_panel.is_none() {
            let mut panel = crate::ai_panel::Panel::open();
            if let Some(sel) = self
                .editor
                .active_tab_mut()
                .and_then(|t| t.editor.get_selection_text())
                && !sel.trim().is_empty()
            {
                panel.input = sel;
            }
            self.ai_panel = Some(panel);
        }
    }

    /// Handle a key while the AI chat panel is open. Enter sends the current line;
    /// Esc closes; arrows / `PageUp` / `PageDown` scroll the transcript; Alt+T opens
    /// the last reply in a new tab; Alt+C copies it to the clipboard.
    pub(super) fn ai_panel_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.ai_panel = None,
            KeyCode::Enter => self.ai_panel_send(),
            KeyCode::Up => {
                if let Some(p) = self.ai_panel.as_mut() {
                    p.scroll_up();
                }
            }
            KeyCode::Down => {
                if let Some(p) = self.ai_panel.as_mut() {
                    p.scroll_down();
                }
            }
            KeyCode::PageUp => {
                if let Some(p) = self.ai_panel.as_mut() {
                    for _ in 0..10 {
                        p.scroll_up();
                    }
                }
            }
            KeyCode::PageDown => {
                if let Some(p) = self.ai_panel.as_mut() {
                    for _ in 0..10 {
                        p.scroll_down();
                    }
                }
            }
            KeyCode::Char('t') if Self::alt(&key) => self.ai_panel_last_to_tab(),
            KeyCode::Char('c') if Self::alt(&key) => self.ai_panel_copy_last(),
            KeyCode::Backspace => {
                if let Some(p) = self.ai_panel.as_mut() {
                    p.input.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(p) = self.ai_panel.as_mut() {
                    p.input.push(c);
                }
            }
            _ => {}
        }
    }

    /// Send the panel's current input line to the assistant, with the prior
    /// conversation supplied as stdin context, and mark the panel busy until the
    /// reply lands in [`Self::poll_ai_replace`].
    pub(super) fn ai_panel_send(&mut self) {
        if self.ai_replace.is_some() {
            self.status = t!("status.ai_busy").to_string();
            return;
        }
        let Some(panel) = self.ai_panel.as_mut() else {
            return;
        };
        let prompt = panel.input.trim().to_string();
        if prompt.is_empty() {
            return;
        }
        let context = panel.context();
        panel.input.clear();
        panel.push(crate::ai_panel::Role::User, prompt.clone());
        panel.busy = true;
        if let Some(rx) = self.spawn_ai_cmd(&prompt, &context) {
            let label = t!("menu.ai").to_string();
            self.ai_replace = Some(AiReplace {
                rx,
                dest: AiDest::Panel,
                label,
            });
        } else if let Some(panel) = self.ai_panel.as_mut() {
            panel.busy = false;
            panel.push(
                crate::ai_panel::Role::Error,
                t!("msg.command_failed", error = "spawn").to_string(),
            );
        }
    }

    /// Open the panel's most recent assistant reply in a new editor tab.
    pub(super) fn ai_panel_last_to_tab(&mut self) {
        if let Some(text) = self
            .ai_panel
            .as_ref()
            .and_then(|p| p.last_assistant())
            .map(str::to_string)
        {
            self.editor.new_tab_with_content(&text);
            self.ai_panel = None;
            self.focus = Focus::Editor;
        }
    }

    /// Copy the panel's most recent assistant reply to the system clipboard.
    pub(super) fn ai_panel_copy_last(&mut self) {
        let Some(text) = self
            .ai_panel
            .as_ref()
            .and_then(|p| p.last_assistant())
            .map(str::to_string)
        else {
            return;
        };
        if let Some(tab) = self.editor.active_tab_mut() {
            tab.editor.set_clipboard(&text);
            self.status = t!("status.ai_copied").to_string();
        }
    }

    // ----- AI diff review (Annotate / Improve) ----------------------------

    /// The open AI diff review, if any (for rendering).
    #[must_use]
    pub fn ai_diff_review(&self) -> Option<&crate::ai_diff::Review> {
        self.ai_diff.as_ref().map(|s| &s.review)
    }

    /// Open an accept/reject diff review for an AI transform whose `new_text`
    /// would replace `target` in tab `tab_idx`. No-ops (with a status note) when
    /// the assistant proposed no change.
    pub(super) fn open_ai_diff(&mut self, tab_idx: usize, target: AiTarget, new_text: &str) {
        let Some(tab) = self.editor.tabs.get(tab_idx) else {
            return;
        };
        let old = match target {
            AiTarget::Whole => tab.editor.get_content(),
            AiTarget::Range(start, end) => tab.editor.get_content_slice(start, end),
        };
        match crate::ai_diff::Review::from_texts(&old, new_text) {
            Some(review) => {
                self.ai_diff = Some(AiDiffState {
                    review,
                    tab: tab_idx,
                    target,
                });
            }
            None => self.status = t!("status.ai_no_change").to_string(),
        }
    }

    /// Handle a key while the AI diff review is open. Enter applies the accepted
    /// hunks; Esc discards the whole proposal; ↑/↓ move between hunks; Space
    /// toggles the highlighted hunk; `a`/`r` accept/reject all.
    pub(super) fn ai_diff_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.ai_diff = None,
            KeyCode::Enter => self.ai_diff_apply(),
            KeyCode::Up => {
                if let Some(s) = self.ai_diff.as_mut() {
                    s.review.prev();
                }
            }
            KeyCode::Down => {
                if let Some(s) = self.ai_diff.as_mut() {
                    s.review.next();
                }
            }
            KeyCode::Char(' ') => {
                if let Some(s) = self.ai_diff.as_mut() {
                    s.review.toggle();
                }
            }
            KeyCode::Char('a') => {
                if let Some(s) = self.ai_diff.as_mut() {
                    s.review.set_all(true);
                }
            }
            KeyCode::Char('r') => {
                if let Some(s) = self.ai_diff.as_mut() {
                    s.review.set_all(false);
                }
            }
            _ => {}
        }
    }

    /// Apply the reviewed result (accepted hunks applied, rejected ones reverted)
    /// to its target as a single undoable edit, then close the review.
    pub(super) fn ai_diff_apply(&mut self) {
        let Some(state) = self.ai_diff.take() else {
            return;
        };
        let text = state.review.result();
        self.apply_ai_replace(state.tab, state.target, &text);
        self.status = t!("status.ai_done", action = t!("menu.ai")).to_string();
    }
}
