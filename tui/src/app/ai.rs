//! AI: the transform actions (summarize/explain/define/annotate/improve/
//! doc-comment), the background CLI/HTTP runner, the chat panel, and the
//! accept/reject diff review.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

use crate::geo::Geo;

use super::{
    AiDest, AiDiffState, AiMsg, AiReplace, AiTarget, App, Focus, KeyCode, KeyEvent, Prompt,
    PromptKind, spawn_shell_on,
};

impl App {
    /// Dispatch an `ai.*` action (the AI menu / palette). Returns `true` if handled.
    pub(super) fn run_ai_action(&mut self, action: &str) -> bool {
        if self.run_acp_action(action) {
            return true;
        }
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
        // Lines of surrounding text sent to the assistant with the doc-comment prompt.
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
        let context_end_line = line.plus(CONTEXT_LINES).min(code.len_lines().minus(1));
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

    /// Spawn the configured AI command (see `Settings::ai_command`) over `text`,
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

    /// Start a fill-in-the-middle request for ghost text (T618) around the
    /// cursor (`before` and `after` it; `line_prefix` is the cursor line's text
    /// before it), through the same provider dispatch as every other AI
    /// request. The receiver yields the cleaned suggestion once (`None` when
    /// the request failed or the model said nothing). `None` back from this
    /// call means it could not start (the reason has been reported).
    pub(super) fn spawn_ghost_ai(
        &mut self,
        before: &str,
        after: &str,
        line_prefix: &str,
    ) -> Option<std::sync::mpsc::Receiver<Option<String>>> {
        let input = vix_ai_core::fim::input(before, after);
        let rx = self.spawn_ai_cmd(vix_ai_core::fim::INSTRUCTION, &input)?;
        let line_prefix = line_prefix.to_string();
        let (tx, out) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let reply = match rx.recv() {
                Ok(AiMsg::Done(text)) => {
                    Some(vix_ai_core::fim::clean(&text, &line_prefix)).filter(|t| !t.is_empty())
                }
                _ => None,
            };
            let _ = tx.send(reply);
        });
        Some(out)
    }

    /// The spawn core shared by [`Self::spawn_ai`] and the chat panel: dispatch
    /// on `Settings::ai_provider` (T124) to either the original CLI shell-out
    /// ([`Self::spawn_ai_cli`], the `"cli"` default — every existing call site
    /// is unaffected) or a direct HTTP provider ([`Self::spawn_ai_http`]).
    /// Unlike [`Self::spawn_ai`] this does **not** reject empty input — a chat
    /// turn may carry no editor context — so callers must guard that themselves.
    pub(super) fn spawn_ai_cmd(
        &mut self,
        prompt: &str,
        text: &str,
    ) -> Option<std::sync::mpsc::Receiver<AiMsg>> {
        // T730: every AI request ends here, so `ai = "off"` is enforced here
        // too, whatever route led in (defense in depth).
        if self.ai_refused() {
            return None;
        }
        match vix_ai_core::Provider::parse(&self.settings.ai_provider) {
            Some(provider) => self.spawn_ai_http(provider, prompt, text),
            None => self.spawn_ai_cli(prompt, text),
        }
    }

    /// Run the configured CLI (see `Settings::ai_command`) over `text`,
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
    /// Spawn the configured assistant CLI (`ai_command`) in a background thread and return the
    /// receiver its output streams over.
    ///
    /// Returns `None` (with a status message set) when the private temp input file cannot be
    /// written. `prompt` is the instruction and `text` the buffer or selection text passed to
    /// the command through a temp file.
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
        // The AI CLI is the user's own tool, signed in on this machine, and
        // its prompt file is a local temp file: it always runs here, even
        // when the workspace is remote (T712). There, it runs in the temp
        // directory, since the workspace root is not a local directory.
        let local = crate::authority::LocalAuthority;
        let dir = if crate::authority::current().name() == "local" {
            self.root.clone()
        } else {
            std::env::temp_dir()
        };
        let mut child = match spawn_shell_on(&local, &cmd, &dir, false) {
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
                Err(reason) => AiMsg::Failed(Some(reason.to_string())),
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
                let mut out: String = chars.get(..start).unwrap_or(&[]).iter().collect();
                out.push_str(text);
                out.extend(chars.get(end..).unwrap_or(&[]));
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
        // With an ACP agent running, the panel talks to it (T635).
        if self.acp.is_some() {
            let text = self
                .ai_panel
                .as_mut()
                .map_or_default(|p| std::mem::take(&mut p.input));
            self.acp_send_prompt(&text);
            return;
        }
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
            tab.editor.copy_to_clipboard(&text);
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
            Ok(Some(review)) => {
                self.ai_diff = Some(AiDiffState {
                    review,
                    tab: tab_idx,
                    target,
                });
            }
            Ok(None) => self.status = t!("status.ai_no_change").to_string(),
            Err(e) => self
                .messages
                .error(t!("msg.command_failed", error = e.to_string()).to_string()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::cov4::*;
    use crate::settings::Settings;
    use std::sync::mpsc;
    use vix_ai_panel::Role;

    /// Queue `msg` as the finished result of a fake background AI task headed
    /// for `dest`, without running any assistant.
    fn inject(app: &mut App, dest: AiDest, msg: AiMsg) {
        let (tx, rx) = mpsc::channel();
        tx.send(msg).unwrap();
        app.ai_replace = Some(AiReplace {
            rx,
            dest,
            label: "Fake".to_string(),
        });
    }

    fn done(text: &str) -> AiMsg {
        AiMsg::Done(text.to_string())
    }

    fn ai_off_app(dir: &std::path::Path) -> App {
        let mut s = Settings::default();
        s.ai = "off".to_string();
        app_with(dir, s)
    }

    #[test]
    fn polling_with_nothing_running_or_nothing_ready_changes_nothing() {
        let dir = temp("ai-poll-idle");
        let mut app = app_at(&dir);
        app.poll_ai_replace();
        assert!(!app.ai_replace_running());
        let (tx, rx) = mpsc::channel::<AiMsg>();
        app.ai_replace = Some(AiReplace {
            rx,
            dest: AiDest::NewTab,
            label: "x".into(),
        });
        app.poll_ai_replace();
        assert!(app.ai_replace_running(), "still waiting for the reply");
        drop(tx);
        app.poll_ai_replace();
        assert!(!app.ai_replace_running(), "a dropped sender ends the task");
        assert_eq!(app.status, t!("status.ai_failed", action = "x").to_string());
    }

    #[test]
    fn a_new_tab_reply_opens_trimmed_text_in_a_new_tab_and_reports_done() {
        let dir = temp("ai-newtab");
        let mut app = app_at(&dir);
        let tabs = app.editor.tabs.len();
        inject(&mut app, AiDest::NewTab, done("summary text\n\n"));
        app.poll_ai_replace();
        assert_eq!(app.editor.tabs.len(), tabs + 1);
        assert_eq!(content(&mut app), "summary text");
        assert_eq!(app.focus, Focus::Editor);
        let done_text = t!("status.ai_done", action = "Fake").to_string();
        assert_eq!(app.status, done_text);
        assert_eq!(last_message(&app), Some(done_text));
    }

    #[test]
    fn an_empty_reply_is_reported_as_a_failure_and_changes_nothing() {
        let dir = temp("ai-empty");
        let mut app = app_at(&dir);
        buffer_with(&mut app, "keep me", 0);
        inject(
            &mut app,
            AiDest::Replace {
                tab: 0,
                target: AiTarget::Whole,
            },
            done("\n\n"),
        );
        app.poll_ai_replace();
        assert_eq!(content(&mut app), "keep me");
        let failed = t!("status.ai_failed", action = "Fake").to_string();
        assert_eq!(app.status, failed);
        assert_eq!(last_message(&app), Some(failed));
    }

    #[test]
    fn a_replace_reply_swaps_the_range_the_whole_buffer_or_a_clamped_range() {
        let dir = temp("ai-replace");
        let mut app = app_at(&dir);
        buffer_with(&mut app, "hello world", 0);
        inject(
            &mut app,
            AiDest::Replace {
                tab: 0,
                target: AiTarget::Range(0, 5),
            },
            done("HOWDY"),
        );
        app.poll_ai_replace();
        assert_eq!(content(&mut app), "HOWDY world");
        assert!(app.editor.active_tab_mut().unwrap().dirty);

        inject(
            &mut app,
            AiDest::Replace {
                tab: 0,
                target: AiTarget::Range(6, 999),
            },
            done("there"),
        );
        app.poll_ai_replace();
        assert_eq!(content(&mut app), "HOWDY there");

        inject(
            &mut app,
            AiDest::Replace {
                tab: 0,
                target: AiTarget::Whole,
            },
            done("all new"),
        );
        app.poll_ai_replace();
        assert_eq!(content(&mut app), "all new");
        // A stale tab index is ignored rather than panicking.
        app.apply_ai_replace(99, AiTarget::Whole, "nope");
        assert_eq!(content(&mut app), "all new");
    }

    #[test]
    fn a_diff_reply_opens_a_review_that_applies_only_accepted_hunks() {
        let dir = temp("ai-diff");
        let mut app = app_at(&dir);
        buffer_with(&mut app, "one\ntwo\nthree\nfour\nfive\n", 0);
        inject(
            &mut app,
            AiDest::Diff {
                tab: 0,
                target: AiTarget::Whole,
            },
            done("ONE\ntwo\nthree\nfour\nFIVE\n"),
        );
        app.poll_ai_replace();
        let review = app.ai_diff_review().expect("review opens");
        assert_eq!(review.change_count(), 2);
        assert_eq!(review.accepted_count(), 2);
        assert!(app.ai_diff_review().is_some());

        app.ai_diff_key(code(KeyCode::Down));
        app.ai_diff_key(code(KeyCode::Up));
        app.ai_diff_key(key('r'));
        assert_eq!(app.ai_diff_review().unwrap().accepted_count(), 0);
        app.ai_diff_key(key('a'));
        assert_eq!(app.ai_diff_review().unwrap().accepted_count(), 2);
        app.ai_diff_key(key(' '));
        assert_eq!(app.ai_diff_review().unwrap().accepted_count(), 1);
        app.ai_diff_key(key('z')); // ignored
        app.ai_diff_key(code(KeyCode::Enter));
        assert!(app.ai_diff_review().is_none());
        // The reply's trailing newline is trimmed on arrival.
        assert_eq!(content(&mut app), "one\ntwo\nthree\nfour\nFIVE");
        assert_eq!(
            app.status,
            t!("status.ai_done", action = t!("menu.ai")).to_string()
        );
    }

    #[test]
    fn escape_discards_a_diff_review_and_an_unchanged_reply_says_no_change() {
        let dir = temp("ai-diff-esc");
        let mut app = app_at(&dir);
        buffer_with(&mut app, "a\nb\n", 0);
        app.open_ai_diff(0, AiTarget::Whole, "A\nb\n");
        assert!(app.ai_diff_review().is_some());
        app.ai_diff_key(code(KeyCode::Esc));
        assert!(app.ai_diff_review().is_none());
        assert_eq!(content(&mut app), "a\nb\n");

        app.open_ai_diff(0, AiTarget::Whole, "a\nb\n");
        assert!(app.ai_diff_review().is_none());
        assert_eq!(app.status, t!("status.ai_no_change").to_string());
        // A stale tab index opens nothing.
        app.open_ai_diff(42, AiTarget::Whole, "x");
        assert!(app.ai_diff_review().is_none());
        // Applying with no review open is a no-op.
        app.ai_diff_apply();
        assert_eq!(content(&mut app), "a\nb\n");
        // Range targets diff only that slice.
        app.open_ai_diff(0, AiTarget::Range(0, 1), "Z");
        assert_eq!(app.ai_diff_review().unwrap().change_count(), 1);
    }

    #[test]
    fn a_doc_comment_reply_inserts_a_line_above_directly_or_through_a_review() {
        let dir = temp("ai-insert");
        let mut app = app_at(&dir);
        app.settings.misc.ai_diff_review = false;
        buffer_with(&mut app, "fn a() {}\nfn b() {}\n", 10);
        inject(
            &mut app,
            AiDest::InsertBeforeLine { tab: 0, at: 10 },
            done("/// Docs for b."),
        );
        app.poll_ai_replace();
        assert_eq!(content(&mut app), "fn a() {}\n/// Docs for b.\nfn b() {}\n");

        app.settings.misc.ai_diff_review = true;
        buffer_with(&mut app, "fn a() {}\n", 0);
        inject(
            &mut app,
            AiDest::InsertBeforeLine { tab: 0, at: 0 },
            done("/// Docs."),
        );
        app.poll_ai_replace();
        assert!(app.ai_diff_review().is_some());
        assert_eq!(content(&mut app), "fn a() {}\n", "review not yet applied");
        app.ai_diff_key(code(KeyCode::Enter));
        assert_eq!(content(&mut app), "/// Docs.\nfn a() {}\n");
    }

    #[test]
    fn replace_or_diff_destination_follows_the_review_setting() {
        let dir = temp("ai-dest");
        let mut app = app_at(&dir);
        app.settings.misc.ai_diff_review = true;
        assert!(matches!(
            app.ai_replace_or_diff_dest(3, AiTarget::Whole),
            AiDest::Diff { tab: 3, .. }
        ));
        app.settings.misc.ai_diff_review = false;
        assert!(matches!(
            app.ai_replace_or_diff_dest(3, AiTarget::Whole),
            AiDest::Replace { tab: 3, .. }
        ));
    }

    #[cfg(feature = "git")]
    #[test]
    fn a_commit_message_reply_prefills_the_git_commit_prompt() {
        let dir = temp("ai-commitmsg");
        let mut app = app_at(&dir);
        inject(
            &mut app,
            AiDest::GitCommitMessage,
            done("  feat: add thing\n"),
        );
        app.poll_ai_replace();
        let prompt = app.prompt.as_ref().expect("commit prompt");
        assert!(matches!(prompt.kind, PromptKind::GitCommit));
        assert_eq!(prompt.input, "feat: add thing");
        assert!(app.git_panel.is_none());
    }

    #[test]
    fn panel_replies_and_failures_land_in_the_transcript_without_a_message_entry() {
        let dir = temp("ai-panel-reply");
        let mut app = app_at(&dir);
        app.open_ai_panel();
        let before = app.messages.items.len();
        app.ai_panel.as_mut().unwrap().busy = true;
        inject(&mut app, AiDest::Panel, done("pong"));
        app.poll_ai_replace();
        let panel = app.ai_panel.as_ref().unwrap();
        assert!(!panel.busy);
        assert_eq!(panel.last_assistant(), Some("pong"));
        assert_eq!(app.messages.items.len(), before, "panel replies are inline");

        app.ai_panel.as_mut().unwrap().busy = true;
        inject(
            &mut app,
            AiDest::Panel,
            AiMsg::Failed(Some("rate limited".into())),
        );
        app.poll_ai_replace();
        let panel = app.ai_panel.as_ref().unwrap();
        assert!(!panel.busy);
        let last = panel.turns.last().unwrap();
        assert_eq!(last.role, Role::Error);
        assert_eq!(
            last.text,
            t!(
                "status.ai_failed_detail",
                action = "Fake",
                reason = "rate limited"
            )
            .to_string()
        );
        assert_eq!(app.messages.items.len(), before);

        inject(&mut app, AiDest::Panel, AiMsg::Failed(None));
        app.poll_ai_replace();
        assert_eq!(
            app.status,
            t!("status.ai_failed", action = "Fake").to_string()
        );
    }

    #[test]
    fn a_failure_outside_the_panel_adds_an_error_message() {
        let dir = temp("ai-fail-msg");
        let mut app = app_at(&dir);
        inject(&mut app, AiDest::NewTab, AiMsg::Failed(Some("boom".into())));
        app.poll_ai_replace();
        let text = t!("status.ai_failed_detail", action = "Fake", reason = "boom").to_string();
        assert_eq!(app.status, text);
        let m = app.messages.items.last().unwrap();
        assert_eq!(m.text, text);
        assert!(matches!(m.level, crate::messages::Level::Error));
    }

    #[test]
    fn chat_panel_keys_edit_the_input_scroll_and_close() {
        let dir = temp("ai-panel-keys");
        let mut app = app_at(&dir);
        app.run_action("ai.chat");
        assert!(app.ai_panel.is_some());
        for c in "hey".chars() {
            app.ai_panel_key(key(c));
        }
        app.ai_panel_key(code(KeyCode::Backspace));
        assert_eq!(app.ai_panel.as_ref().unwrap().input, "he");
        app.ai_panel_key(code(KeyCode::Up));
        assert_eq!(app.ai_panel.as_ref().unwrap().scroll, 1);
        app.ai_panel_key(code(KeyCode::PageUp));
        assert_eq!(app.ai_panel.as_ref().unwrap().scroll, 11);
        app.ai_panel_key(code(KeyCode::PageDown));
        assert_eq!(app.ai_panel.as_ref().unwrap().scroll, 1);
        app.ai_panel_key(code(KeyCode::Down));
        assert_eq!(app.ai_panel.as_ref().unwrap().scroll, 0);
        app.ai_panel_key(code(KeyCode::Tab)); // ignored
        assert_eq!(app.ai_panel.as_ref().unwrap().input, "he");
        app.ai_panel_key(code(KeyCode::Esc));
        assert!(app.ai_panel.is_none());
        // Keys with no panel open do nothing.
        app.ai_panel_key(key('x'));
        app.ai_panel_key(code(KeyCode::Up));
        assert!(app.ai_panel.is_none());
    }

    #[test]
    fn opening_the_chat_panel_seeds_the_selection_and_keeps_an_open_panel() {
        let dir = temp("ai-panel-seed");
        let mut app = app_at(&dir);
        buffer_with(&mut app, "explain this", 0);
        app.editor
            .active_tab_mut()
            .unwrap()
            .editor
            .set_selection_range(0, 7);
        app.open_ai_panel();
        assert_eq!(app.ai_panel.as_ref().unwrap().input, "explain");
        app.ai_panel.as_mut().unwrap().input.push('!');
        app.open_ai_panel();
        assert_eq!(app.ai_panel.as_ref().unwrap().input, "explain!");
    }

    #[test]
    fn last_reply_moves_to_a_tab_or_the_clipboard() {
        let _clip = CLIP
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let dir = temp("ai-panel-last");
        let mut app = app_at(&dir);
        app.open_ai_panel();
        // With no assistant reply, both are no-ops.
        app.ai_panel_key(alt('t'));
        app.ai_panel_key(alt('c'));
        assert!(app.ai_panel.is_some());
        assert_ne!(app.status, t!("status.ai_copied").to_string());

        app.ai_panel
            .as_mut()
            .unwrap()
            .push(Role::Assistant, "the answer");
        app.ai_panel_key(alt('c'));
        assert_eq!(app.status, t!("status.ai_copied").to_string());
        let tabs = app.editor.tabs.len();
        app.ai_panel_key(alt('t'));
        assert!(app.ai_panel.is_none());
        assert_eq!(app.editor.tabs.len(), tabs + 1);
        assert_eq!(content(&mut app), "the answer");
        assert_eq!(app.focus, Focus::Editor);
    }

    #[test]
    fn sending_ignores_blank_input_and_a_busy_assistant() {
        let dir = temp("ai-send-guards");
        let mut app = app_at(&dir);
        app.open_ai_panel();
        app.ai_panel_send(); // blank input
        assert!(app.ai_panel.as_ref().unwrap().turns.is_empty());
        app.ai_panel.as_mut().unwrap().input = "   ".into();
        app.ai_panel_send();
        assert!(app.ai_panel.as_ref().unwrap().turns.is_empty());

        let (_tx, rx) = mpsc::channel::<AiMsg>();
        app.ai_replace = Some(AiReplace {
            rx,
            dest: AiDest::Panel,
            label: "busy".into(),
        });
        app.ai_panel.as_mut().unwrap().input = "hello".into();
        app.ai_panel_send();
        assert_eq!(app.status, t!("status.ai_busy").to_string());
        assert_eq!(app.ai_panel.as_ref().unwrap().input, "hello", "input kept");
        // No panel at all: nothing to send.
        app.ai_replace = None;
        app.ai_panel = None;
        app.ai_panel_send();
        assert!(app.ai_panel.is_none());
    }

    #[test]
    fn sending_with_ai_off_records_the_user_turn_and_a_spawn_error() {
        let dir = temp("ai-send-off");
        let mut app = ai_off_app(&dir);
        app.open_ai_panel();
        app.ai_panel.as_mut().unwrap().input = "  hi there ".into();
        app.ai_panel_key(code(KeyCode::Enter));
        let panel = app.ai_panel.as_ref().unwrap();
        assert!(!panel.busy);
        assert_eq!(panel.input, "");
        assert_eq!(panel.turns.len(), 2);
        assert_eq!(panel.turns[0].role, Role::User);
        assert_eq!(panel.turns[0].text, "hi there");
        assert_eq!(panel.turns[1].role, Role::Error);
        assert_eq!(
            panel.turns[1].text,
            t!("msg.command_failed", error = "spawn").to_string()
        );
        assert_eq!(last_message(&app), Some(t!("msg.ai_off").to_string()));
        assert!(app.ai_replace.is_none());
    }

    #[test]
    fn the_http_provider_without_an_api_key_reports_and_starts_nothing() {
        let dir = temp("ai-nokey");
        let mut s = Settings::default();
        s.ai_provider = "openai".to_string();
        s.ai_api_key_command = String::new();
        let mut app = app_with(&dir, s);
        buffer_with(&mut app, "text", 0);
        app.run_action("ai.summarize");
        assert!(app.ai_replace.is_none());
        assert_eq!(
            last_message(&app),
            Some(t!("msg.ai_no_api_key", provider = "openai").to_string())
        );
    }

    #[test]
    fn transforms_report_busy_and_missing_input_and_ai_off_refuses_them() {
        let dir = temp("ai-guards");
        let mut app = app_at(&dir);
        buffer_with(&mut app, "   \n", 0);
        app.run_action("ai.summarize");
        assert_eq!(app.status, t!("status.ai_no_input").to_string());
        app.run_action("ai.explain");
        assert_eq!(app.status, t!("status.ai_no_input").to_string());
        app.status.clear();
        app.run_action("ai.define");
        assert_eq!(app.status, t!("status.ai_no_input").to_string());

        let (_tx, rx) = mpsc::channel::<AiMsg>();
        app.ai_replace = Some(AiReplace {
            rx,
            dest: AiDest::NewTab,
            label: "busy".into(),
        });
        for action in [
            "ai.summarize",
            "ai.annotate",
            "ai.improve",
            "ai.edit_with_instruction",
            "ai.generate_doc_comment",
        ] {
            app.status.clear();
            assert!(app.run_ai_action(action));
            assert_eq!(app.status, t!("status.ai_busy").to_string(), "{action}");
        }
        assert!(!app.run_ai_action("ai.no_such_action"));

        let mut off = ai_off_app(&dir);
        buffer_with(&mut off, "some text", 0);
        for action in ["ai.summarize", "ai.annotate", "ai.generate_doc_comment"] {
            off.run_action(action);
            assert!(off.ai_replace.is_none(), "{action}");
        }
        assert!(off.spawn_ghost_ai("a", "b", "a").is_none());
        assert_eq!(last_message(&off), Some(t!("msg.ai_off").to_string()));
    }

    #[test]
    fn edit_with_instruction_needs_a_selection_then_prompts_and_rejects_blank_text() {
        let dir = temp("ai-instr");
        let mut app = app_at(&dir);
        buffer_with(&mut app, "make this loud", 0);
        app.ai_begin_edit_with_instruction();
        assert_eq!(app.status, t!("status.ai_selection_required").to_string());
        assert!(app.prompt.is_none());

        app.editor
            .active_tab_mut()
            .unwrap()
            .editor
            .set_selection_range(5, 9);
        app.ai_begin_edit_with_instruction();
        assert!(matches!(
            app.prompt.as_ref().unwrap().kind,
            PromptKind::AiInstruction
        ));
        assert!(matches!(
            app.ai_instruction_target,
            Some((0, AiTarget::Range(5, 9)))
        ));
        app.submit_ai_instruction("   ");
        assert_eq!(app.status, t!("status.ai_no_input").to_string());
        assert!(app.ai_instruction_target.is_none(), "target consumed");
        // With no captured target a second submit does nothing.
        app.status.clear();
        app.submit_ai_instruction("shout");
        assert_eq!(app.status, "");
    }

    #[test]
    fn an_instruction_with_ai_off_starts_no_request() {
        let dir = temp("ai-instr-off");
        let mut app = ai_off_app(&dir);
        buffer_with(&mut app, "make this loud", 0);
        app.editor
            .active_tab_mut()
            .unwrap()
            .editor
            .set_selection_range(5, 9);
        app.ai_begin_edit_with_instruction();
        app.submit_ai_instruction("shout");
        assert!(app.ai_replace.is_none());
        assert_eq!(last_message(&app), Some(t!("msg.ai_off").to_string()));
        // Stale tab index: nothing happens either.
        app.ai_instruction_target = Some((9, AiTarget::Whole));
        app.submit_ai_instruction("shout");
        assert!(app.ai_instruction_target.is_none());
    }

    #[test]
    fn doc_comment_on_blank_context_says_no_input() {
        let dir = temp("ai-doc-blank");
        let mut app = app_at(&dir);
        buffer_with(&mut app, "\n\n\n", 0);
        app.ai_generate_doc_comment();
        assert_eq!(app.status, t!("status.ai_no_input").to_string());
        assert!(app.ai_replace.is_none());
    }

    #[test]
    fn define_uses_the_selection_else_the_word_at_or_after_the_cursor() {
        let dir = temp("ai-word");
        let mut app = app_at(&dir);
        buffer_with(&mut app, "alpha beta  gamma", 7);
        assert_eq!(app.selected_or_word_text(), "beta");
        buffer_with(&mut app, "alpha   beta", 6);
        assert_eq!(
            app.selected_or_word_text(),
            "beta",
            "next word when between"
        );
        buffer_with(&mut app, "alpha beta", 0);
        app.editor
            .active_tab_mut()
            .unwrap()
            .editor
            .set_selection_range(6, 10);
        assert_eq!(app.selected_or_word_text(), "beta");
        buffer_with(&mut app, "   ", 1);
        app.editor
            .active_tab_mut()
            .unwrap()
            .editor
            .set_selection(None);
        assert_eq!(app.selected_or_word_text(), "");
    }
}

#[cfg(test)]
mod cov_r3;
