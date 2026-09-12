//! Keyboard dispatch: the modal-overlay routing chain (`on_key`,
//! `try_overlay_key`/`try_panel_key`/`try_tool_dialog_key`), the ten
//! keymap-specific dispatchers (Apple/VSCode/Emacs/Vi/Spacemacs/IntelliJ/
//! Eclipse/Sublime and their `*_token` helpers), and the small pieces that
//! read the active keymap for display (`mode_indicator`, `which_key`).
//!
//! Moved out of `app.rs` verbatim (T141, first slice); every other feature's
//! own `_key` handler (`git_panel_key`, `db_key`, ...) still lives in
//! `app.rs` today and is called from here exactly as before -- an inherent
//! `impl App` block can live in any module of the crate, so splitting where
//! a method is *defined* never has to happen in lockstep with everything it
//! calls.

#![warn(clippy::pedantic)]

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::{App, Focus, Keymap, display_key, menu_index_for_alt};
use crate::editor_core::actions::Copy as CopyAction;

impl App {
    // ----- top-level event entry -----------------------------------------

    /// Route `key` to the highest-priority open modal layer, if any. Returns
    /// `true` when a layer consumed the key (so [`App::on_key`] should stop).
    /// Extracted from `on_key` to keep that function within the line limit.
    /// Route a key to an open tool dialog (color/unit converter, calculator,
    /// regex tester, code-action chooser). Returns `true` if one consumed it.
    fn try_tool_dialog_key(&mut self, key: KeyEvent) -> bool {
        if self.color_converter.is_some() {
            self.color_converter_key(key);
        } else if self.unit_converter.is_some() {
            self.unit_converter_key(key);
        } else if self.calculator.is_some() {
            self.calculator_key(key);
        } else if self.regex_tester.is_some() {
            self.regex_tester_key(key);
        } else if self.code_actions.is_some() {
            self.code_action_key(key);
        } else if self.code_lens.is_some() {
            self.code_lens_key(key);
        } else {
            return false;
        }
        true
    }

    /// Whether a modal layer is taking keystrokes, so a keypress does not reach
    /// the editor. Mirrors the conditions of the dispatch chain below —
    /// [`App::try_overlay_key`], [`App::try_tool_dialog_key`], and
    /// [`App::try_panel_key`] — plus the jump-label mode `on_key` checks first;
    /// keep this list in step with them.
    ///
    /// Used to route a bracketed paste ([`App::on_paste`]): text pasted while a
    /// prompt, the palette, the search bar, or a panel is up belongs to that
    /// input, not to the buffer behind it.
    fn overlay_capturing_keys(&self) -> bool {
        macro_rules! any_open {
            ($($field:ident),* $(,)?) => { $(self.$field.is_some() ||)* false };
        }
        any_open!(
            // `on_key`'s jump-label mode, then `try_overlay_key`.
            jump,
            welcome,
            help,
            dialog,
            // `try_tool_dialog_key`.
            color_converter,
            unit_converter,
            calculator,
            regex_tester,
            code_actions,
            code_lens,
            // `try_panel_key`.
            terminal,
            db,
            edit_table,
            column_view,
            edit_outline,
            edit_value,
            edit_bytes,
            edit_sql,
            file_browser,
            recent_chooser,
            location_chooser,
            capture_chooser,
            refile_chooser,
            nerd_palette,
            ascii_panel,
            x11_panel,
            theme_editor,
            media_type_panel,
            html_panel,
            system_info,
            file_info,
            text_info,
            markdown_preview,
            markdown_toc,
            snippets,
            vcard,
            contacts,
            dashboard,
            qrcode,
            ai_diff,
            ai_panel,
            outline,
            query_replace,
            structural_replace,
            replace_confirm,
            wgrep_confirm,
            workspace_search,
            confirm,
            script_trust,
            unsaved,
            spell_suggest,
            context_menu,
            git_panel,
            branch_chooser,
            git_log,
            task_chooser,
            macro_chooser,
            script_chooser,
            clipboard_chooser,
            workspace_chooser,
            diff_view,
            prompt,
            keybinding_editor,
            palette,
            search,
        ) || self.pomodoro_open
            || self.show_calendar
            || self.show_clock
            || self.menu.is_open()
            || self.paste.as_ref().is_some_and(|p| p.conflict.is_some())
    }

    fn try_overlay_key(&mut self, key: KeyEvent) -> bool {
        if self.welcome.is_some() {
            self.welcome_key(key);
            return true;
        }
        if self.help.is_some() {
            let page = (self.layout.help_body.height as usize).max(1);
            match key.code {
                KeyCode::Esc | KeyCode::F(1) => self.help = None,
                KeyCode::Backspace => {
                    if let Some(h) = self.help.as_mut() {
                        h.backspace();
                    }
                }
                KeyCode::Up => {
                    if let Some(h) = self.help.as_mut() {
                        h.scroll_up(1);
                    }
                }
                KeyCode::Down => {
                    if let Some(h) = self.help.as_mut() {
                        h.scroll_down(1);
                    }
                }
                KeyCode::PageUp => {
                    if let Some(h) = self.help.as_mut() {
                        h.scroll_up(page);
                    }
                }
                KeyCode::PageDown => {
                    if let Some(h) = self.help.as_mut() {
                        h.scroll_down(page);
                    }
                }
                KeyCode::Char(c) if !Self::ctrl(&key) && !Self::alt(&key) => {
                    if let Some(h) = self.help.as_mut() {
                        h.push(c);
                    }
                }
                _ => {}
            }
            return true;
        }
        // The info dialog. A plain dialog (About) closes on Enter/Esc/Space/O. A
        // text-field dialog (Website/Email) closes on Esc; Ctrl+C copies the
        // selection, and other keys drive selection/navigation in the field.
        if self.dialog.is_some() {
            let has_field = self.dialog.as_ref().is_some_and(|d| d.editor.is_some());
            if !has_field {
                if matches!(
                    key.code,
                    KeyCode::Enter | KeyCode::Esc | KeyCode::Char(' ' | 'o' | 'O')
                ) {
                    self.dialog = None;
                }
                return true;
            }
            if key.code == KeyCode::Esc {
                self.dialog = None;
                return true;
            }
            let area = self.dialog_field_area();
            if let Some(ed) = self.dialog.as_mut().and_then(|d| d.editor.as_mut()) {
                if Self::ctrl(&key) && matches!(key.code, KeyCode::Char('c')) {
                    ed.apply(CopyAction {});
                } else {
                    let _ = ed.input(key, &area);
                }
            }
            return true;
        }
        if self.try_tool_dialog_key(key) {
            return true;
        }
        if self.pomodoro_open {
            self.pomodoro_key(key);
            return true;
        }
        // While the calendar box is open it captures left/right to page months.
        if self.show_calendar {
            self.calendar_key(key);
            return true;
        }
        // While the clock box is open it captures up/down to pick a time row and
        // Enter to insert it.
        if self.show_clock {
            match key.code {
                KeyCode::Up => self.clock.up(),
                KeyCode::Down => self.clock.down(),
                KeyCode::Enter => {
                    let now = crate::clock::now_local();
                    if let Some(text) = self.clock.selected_value(&now) {
                        let area = self.editor_view();
                        self.editor.insert_str(&text, area);
                    }
                    self.show_clock = false;
                }
                KeyCode::Esc | KeyCode::Char('q') => self.show_clock = false,
                _ => {}
            }
            return true;
        }
        self.try_panel_key(key)
    }

    /// Handle a key while the calendar overlay is open: arrows page the day/
    /// month/year (with Ctrl / Ctrl+Shift), Enter accepts, Esc/`q` closes. Split
    /// out of [`App::try_overlay_key`] to keep it within the line limit.
    fn calendar_key(&mut self, key: KeyEvent) {
        let ctrl = Self::ctrl(&key);
        let shift = Self::shift(&key);
        match key.code {
            KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down => {
                let sign: i64 = if matches!(key.code, KeyCode::Right | KeyCode::Down) {
                    1
                } else {
                    -1
                };
                if ctrl && shift {
                    self.calendar.move_years(sign); // Ctrl+Shift+arrows: year
                } else if ctrl {
                    self.calendar.move_months(sign); // Ctrl+arrows: month
                } else {
                    // Plain arrows move the selected day: Left/Right by a day,
                    // Up/Down by a week.
                    let step = if matches!(key.code, KeyCode::Up | KeyCode::Down) {
                        7
                    } else {
                        1
                    };
                    self.calendar.move_days(sign * step);
                }
            }
            KeyCode::Enter => self.calendar_accept(),
            KeyCode::Esc | KeyCode::Char('q') => {
                self.show_calendar = false;
                self.calendar_dailies = false;
            }
            _ => {}
        }
    }

    /// Route `key` to the lower-priority list/panel overlays (choosers, tool
    /// panels, prompt, palette, search, menu). Returns `true` when one consumed
    /// the key. Split out of [`App::try_overlay_key`] to keep both within the
    /// line limit; the priority order is preserved by calling this last.
    fn try_panel_key(&mut self, key: KeyEvent) -> bool {
        // Each panel that is open captures the key by delegating to its handler.
        macro_rules! panel {
            ($field:ident, $handler:ident) => {
                if self.$field.is_some() {
                    self.$handler(key);
                    return true;
                }
            };
        }
        // The integrated terminal captures all keys (forwarded to the shell)
        // except its close chord, handled inside the key handler.
        panel!(terminal, terminal_key);
        panel!(db, db_key);
        panel!(edit_table, edit_table_key);
        panel!(column_view, column_view_key);
        panel!(edit_outline, edit_outline_key);
        panel!(edit_value, edit_value_key);
        panel!(edit_bytes, edit_bytes_key);
        panel!(edit_sql, edit_sql_key);
        panel!(file_browser, file_browser_key);
        panel!(recent_chooser, recent_key);
        panel!(location_chooser, location_key);
        panel!(capture_chooser, capture_chooser_key);
        panel!(refile_chooser, refile_chooser_key);
        panel!(nerd_palette, nerd_key);
        panel!(ascii_panel, ascii_key);
        panel!(x11_panel, x11_key);
        panel!(theme_editor, theme_editor_key);
        panel!(media_type_panel, media_type_key);
        panel!(html_panel, html_key);
        panel!(system_info, system_info_key);
        panel!(file_info, file_info_key);
        panel!(text_info, text_info_key);
        panel!(markdown_toc, markdown_toc_key);
        panel!(markdown_preview, markdown_preview_key);
        panel!(snippets, snippets_key);
        panel!(vcard, vcard_key);
        panel!(contacts, contacts_key);
        if self.dashboard.is_some() {
            if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
                self.close_dashboard();
            }
            return true;
        }
        if self.qrcode.is_some() {
            if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q')) {
                self.qrcode = None;
            }
            return true;
        }
        panel!(ai_diff, ai_diff_key);
        panel!(ai_panel, ai_panel_key);
        panel!(outline, outline_key);
        panel!(query_replace, qr_key);
        panel!(structural_replace, sr_key);
        panel!(replace_confirm, replace_confirm_key);
        panel!(wgrep_confirm, wgrep_confirm_key);
        panel!(workspace_search, ps_key);
        panel!(confirm, confirm_key);
        panel!(script_trust, script_trust_key);
        panel!(unsaved, unsaved_key);
        panel!(spell_suggest, spell_suggest_key);
        panel!(context_menu, context_menu_key);
        panel!(git_panel, git_panel_key);
        panel!(branch_chooser, branch_key);
        panel!(git_log, git_log_key);
        panel!(task_chooser, tasks_key);
        panel!(macro_chooser, macro_key);
        panel!(script_chooser, script_chooser_key);
        panel!(clipboard_chooser, clipboard_key);
        panel!(workspace_chooser, workspace_chooser_key);
        panel!(diff_view, diff_view_key);
        if self.paste.as_ref().is_some_and(|p| p.conflict.is_some()) {
            self.paste_key(key);
            return true;
        }
        panel!(prompt, prompt_key);
        // Must come after `prompt`: rebinding opens a `Prompt` while
        // `keybinding_editor` stays `Some` underneath it, and `Prompt` has
        // to win the key or the rebind prompt would be unreachable.
        panel!(keybinding_editor, keybinding_editor_key);
        panel!(palette, palette_key);
        panel!(search, search_key);
        if self.menu.is_open() {
            self.menu_key(key);
            return true;
        }
        false
    }

    /// Fold macOS's `Command` modifier into `Control`, so `Cmd+C` drives the
    /// same binding as `Ctrl+C` and every other keymap chord works from the key
    /// Mac users reach for. Other modifiers on the chord (`Shift`, `Alt`) are
    /// kept, so `Cmd+Shift+Z` becomes `Ctrl+Shift+Z`.
    ///
    /// Off macOS the key is returned untouched: `Super` there is the window
    /// manager's key, not an editor modifier.
    ///
    /// A terminal only reports `Command` at all when the kitty keyboard
    /// protocol is on (see `src/main.rs`, which asks for it) *and* the terminal
    /// does not keep that particular `Cmd` shortcut for its own menus — which
    /// most do, `Cmd+C` and `Cmd+V` included. Whatever does arrive is folded
    /// here.
    #[must_use]
    pub(super) fn command_as_control(mut key: KeyEvent) -> KeyEvent {
        if cfg!(target_os = "macos") && key.modifiers.contains(KeyModifiers::SUPER) {
            key.modifiers.remove(KeyModifiers::SUPER);
            key.modifiers.insert(KeyModifiers::CONTROL);
        }
        key
    }

    /// Handle a key event, routing it to the active modal layer or focused pane.
    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        // macOS: `Command` stands in for `Control` throughout.
        let key = Self::command_as_control(key);
        // Jump-to-line mode captures all keys until a label matches or Esc.
        if self.jump.is_some() && self.jump_key(key) {
            return;
        }
        // LSP completion popup captures navigation/accept/cancel keys; any other
        // key dismisses it and falls through to normal handling.
        if self.completion.is_some() && self.completion_key(key) {
            return;
        }
        // A hover tooltip is dismissed by the next keypress (Esc just dismisses).
        if self.hover.is_some() {
            self.hover = None;
            if key.code == KeyCode::Esc {
                return;
            }
        }
        // Modal layers, in priority order.
        if self.try_overlay_key(key) {
            return;
        }
        // Org table context-sensitive keys (Tab/S-Tab/RET field & row nav,
        // Meta-/Shift-arrow structural edits) take priority over every
        // keymap's own bindings for the same keys — including `Alt`+arrow and
        // `Ctrl+Shift`+arrow, which `global_shared_key` below would otherwise
        // claim first — but only while the cursor is actually inside a pipe
        // table; see `org_table_key`.
        if self.org_table_key(key) {
            return;
        }
        // Persisted key binding overrides (T104i) take priority over every
        // keymap's own dispatch below — that's what "override" means. See
        // `override_key`.
        if self.override_key(key) {
            return;
        }
        // Keymap-specific dispatch. Each keymap first gets a chance to consume the
        // key; `Emacs`/`Vim` then fall back to the shared keys (menu mnemonics and
        // function keys) before the focused pane handles it.
        match self.active_keymap() {
            Keymap::Apple => {
                if self.global_key(key) {
                    return;
                }
            }
            Keymap::Vscode => {
                if self.vscode_key(key) {
                    return;
                }
            }
            Keymap::Emacs => {
                if self.emacs_key(key) || self.global_shared_key(key) {
                    return;
                }
            }
            Keymap::Vi => {
                if self.vim_key(key) || self.global_shared_key(key) {
                    return;
                }
            }
            Keymap::Spacemacs => {
                if self.spacemacs_key(key) || self.global_shared_key(key) {
                    return;
                }
            }
            Keymap::IntelliJMacOS => {
                if self.intellij_key(key, false) || self.global_shared_key(key) {
                    return;
                }
            }
            Keymap::IntelliJWindows => {
                if self.intellij_key(key, true) || self.global_shared_key(key) {
                    return;
                }
            }
            Keymap::Eclipse => {
                if self.eclipse_key(key) || self.global_shared_key(key) {
                    return;
                }
            }
            Keymap::Sublime => {
                if self.sublime_key(key) || self.global_shared_key(key) {
                    return;
                }
            }
        }
        match self.focus {
            Focus::Editor => {
                self.editor_key(key);
                // Auto-complete Org-roam/Node `[[` wiki-links with node titles.
                if matches!(key.code, KeyCode::Char('[')) {
                    self.maybe_complete_wiki_link();
                }
            }
            Focus::Explorer => self.explorer_key(key),
            Focus::Messages => self.messages_key(key),
            Focus::BottomDock => self.bottomdock_key(key),
        }
    }

    /// Handle a bracketed paste: text the terminal hands over in one piece
    /// (`Event::Paste`) rather than as individual key events.
    ///
    /// With the editor focused the whole chunk is inserted as **one** edit, so
    /// undo removes the paste in a single step, and verbatim, so auto-indent and
    /// auto-pairing cannot mangle it on the way in. That is the point of asking
    /// the terminal to bracket pastes: delivered as keystrokes, a paste left one
    /// undo entry per character, and every pasted newline re-indented the line
    /// after it.
    ///
    /// Anything layered over the editor — a prompt, the palette, the search bar,
    /// a panel — still receives the paste as keystrokes, which is what those
    /// inputs expect; the same goes for the explorer and the docks.
    pub fn on_paste(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.focus == Focus::Editor && !self.overlay_capturing_keys() {
            if self.active_read_only() {
                self.status = t!("status.read_only_blocked").to_string();
                return;
            }
            let area = self.editor_view();
            self.editor.paste_str(text, area);
            return;
        }
        // Replay for every other target. `\r\n` is one line break, not two.
        for ch in text.replace("\r\n", "\n").chars() {
            let code = match ch {
                '\n' | '\r' => KeyCode::Enter,
                '\t' => KeyCode::Tab,
                c => KeyCode::Char(c),
            };
            self.on_key(KeyEvent::new(code, KeyModifiers::NONE));
        }
    }

    /// The keyboard navigation style currently in effect. Falls back to
    /// [`Keymap::Apple`] on an unrecognized `settings.keymap` purely as a
    /// last-resort safety net — `App::new` already validates and corrects
    /// that field once at startup (T146), so every real call here sees a
    /// known id.
    fn active_keymap(&self) -> Keymap {
        Keymap::from_id(&self.settings.keymap).unwrap_or(Keymap::Apple)
    }

    /// A short keymap-mode indicator for the status bar (Vim's mode / command
    /// line, or Emacs's pending chord prefix), or `None` when there is nothing to
    /// show (e.g. the Apple keymap).
    #[must_use]
    pub fn mode_indicator(&self) -> Option<String> {
        match self.active_keymap() {
            Keymap::Vi => Some(if let Some(cmd) = &self.vim_cmd {
                format!(":{cmd}")
            } else if self.modal_insert {
                t!("status.vim_insert").to_string()
            } else {
                t!("status.vim_normal").to_string()
            }),
            Keymap::Emacs if self.emacs_prefix => Some("C-x-".to_string()),
            Keymap::Emacs if self.emacs_c_x_prefix => Some("C-c C-x-".to_string()),
            Keymap::Emacs if self.emacs_c_p_c_m_prefix => Some("C-c p c m-".to_string()),
            Keymap::Emacs if self.emacs_c_p_c_prefix => Some("C-c p c-".to_string()),
            Keymap::Emacs if self.emacs_c_p_prefix => Some("C-c p-".to_string()),
            Keymap::Emacs if self.emacs_c_prefix => Some("C-c-".to_string()),
            Keymap::Spacemacs => Some(if let Some(seq) = &self.spacemacs_leader {
                format!("SPC {seq}")
            } else if self.modal_insert {
                t!("status.vim_insert").to_string()
            } else {
                t!("status.vim_normal").to_string()
            }),
            _ => None,
        }
    }

    /// The which-key hint for a pending key prefix: `(title, [(keys, action)])`.
    /// `None` when no prefix is pending. Drives the which-key popup so chorded
    /// keymaps are discoverable.
    #[must_use]
    pub fn which_key(&self) -> Option<(String, Vec<(String, String)>)> {
        match self.active_keymap() {
            Keymap::Emacs if self.emacs_prefix => {
                Some(("C-x".to_string(), Self::emacs_context_rows("C-x")))
            }
            Keymap::Emacs if self.emacs_c_x_prefix => {
                Some(("C-c C-x".to_string(), Self::emacs_context_rows("C-c C-x")))
            }
            Keymap::Emacs if self.emacs_c_p_c_m_prefix => Some((
                "C-c p c m".to_string(),
                Self::emacs_context_rows("C-c p c m"),
            )),
            Keymap::Emacs if self.emacs_c_p_c_prefix => {
                // `m` continues into the subproject family; shown here as a
                // hint row even though it is not itself a dispatchable
                // action (so not itself in the "C-c p c" table).
                let mut rows = Self::emacs_context_rows("C-c p c");
                rows.push(("m".to_string(), "project.subproject.*".to_string()));
                Some(("C-c p c".to_string(), rows))
            }
            Keymap::Emacs if self.emacs_c_p_prefix => Some((
                "C-c p".to_string(),
                vec![("c".to_string(), "project.*".to_string())],
            )),
            Keymap::Emacs if self.emacs_c_prefix => {
                Some(("C-c".to_string(), Self::emacs_context_rows("C-c")))
            }
            Keymap::Spacemacs => {
                let seq = self.spacemacs_leader.as_ref()?;
                // Candidates whose sequence extends what's typed; show the next key.
                let rows = Self::spacemacs_leader_bindings()
                    .iter()
                    .filter(|b| {
                        b.key_token.starts_with(seq.as_str()) && b.key_token.len() > seq.len()
                    })
                    .map(|b| {
                        (
                            display_key(&b.key_token[seq.len()..]),
                            b.action_id.to_string(),
                        )
                    })
                    .collect();
                Some((format!("SPC {seq}"), rows))
            }
            _ => None,
        }
    }

    /// Every `(key_token, action_id)` binding in Emacs's `context`
    /// (`vix-keybindings`, T104a), as owned strings for display — feeds
    /// [`App::which_key`] and (via [`App::shortcut_rows`]) the F1 help
    /// overlay from the one registry instead of each walking its own copy
    /// of the table.
    fn emacs_context_rows(context: &str) -> Vec<(String, String)> {
        vix_keybindings::TABLES
            .iter()
            .find(|t| t.keymap_id == "emacs")
            .into_iter()
            .flat_map(|t| t.contexts)
            .find(|c| c.name == context)
            .map(|c| {
                c.bindings
                    .iter()
                    .map(|b| (b.key_token.to_string(), b.action_id.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Every binding in Spacemacs's leader context (`vix-keybindings`,
    /// T104b) — feeds [`App::which_key`] and [`App::shortcut_rows`].
    pub(super) fn spacemacs_leader_bindings() -> &'static [vix_keybindings::Binding] {
        vix_keybindings::TABLES
            .iter()
            .find(|t| t.keymap_id == "spacemacs")
            .into_iter()
            .flat_map(|t| t.contexts)
            .find(|c| c.name.is_empty())
            .map_or(&[], |c| c.bindings)
    }

    pub(super) fn ctrl(key: &KeyEvent) -> bool {
        key.modifiers.contains(KeyModifiers::CONTROL)
    }

    pub(super) fn alt(key: &KeyEvent) -> bool {
        key.modifiers.contains(KeyModifiers::ALT)
    }

    pub(super) fn shift(key: &KeyEvent) -> bool {
        key.modifiers.contains(KeyModifiers::SHIFT)
    }

    /// Global shortcuts available when no modal is active (Apple keymap). Returns
    /// true if the key was consumed.
    fn global_key(&mut self, key: KeyEvent) -> bool {
        self.apple_ctrl_key(key) || self.global_shared_key(key)
    }

    /// The `vix-macros` token for a `Ctrl`-chord lookup shared by every
    /// keymap whose dispatch is "just" `Ctrl` (+ optionally `Alt`) held
    /// `Char` keys with `Shift` read from the modifier bit — Apple, VS
    /// Code, `IntelliJ`, Sublime, and Eclipse's own `Ctrl` branch (T104c–g
    /// each hand-wrote a near-identical copy of this; T145 consolidated
    /// them once the epic settled, one copy per slice having been the
    /// right call while each conversion was still being reviewed on its
    /// own). Returns `None` if `key` isn't a `Ctrl`-held `Char`.
    ///
    /// `encode_alt` is the one axis that actually varies across callers —
    /// `true` for `IntelliJ` (`Ctrl+Alt+L`/`Ctrl+Alt+O` are a single
    /// keystroke's modifier combination, not a chord), `false` for every
    /// other caller (Alt is either irrelevant to them, or — Apple's
    /// `Ctrl+Alt+R`, Eclipse's `Alt+/` — genuinely handled host-side
    /// instead, see each one's own dispatch function). There's no
    /// equivalent `encode_shift` parameter: every caller here needs Shift
    /// encoded from the modifier bit — a terminal can report
    /// `Ctrl+Shift+p` as a lowercase `p` with the bit set, not an
    /// uppercase `P` — so unlike `Alt`, that choice never actually
    /// varies; adding a knob nothing exercises would just be
    /// unexercised complexity.
    fn ctrl_token(key: &KeyEvent, encode_alt: bool) -> Option<String> {
        if !Self::ctrl(key) {
            return None;
        }
        let KeyCode::Char(c) = key.code else {
            return None;
        };
        let mut token = String::from("C-");
        if encode_alt && Self::alt(key) {
            token.push_str("A-");
        }
        if Self::shift(key) {
            token.push_str("S-");
        }
        token.push(c.to_ascii_lowercase());
        Some(token)
    }

    /// The Apple keymap's `Ctrl`-letter shortcuts, looked up in
    /// `vix-keybindings`' `"apple"` table (T104g — see
    /// `crates/vix-keybindings/src/apple.rs`). Two bindings are handled
    /// before the table lookup, since neither fits a static
    /// `(token, action_id)` row: `Ctrl+Alt+R` (query replace) is the only
    /// binding here that keys off `Alt` (the table's token function is
    /// otherwise Alt-agnostic, matching the original dispatch); `Ctrl+D`
    /// (forward delete) only claims the key while the editor pane is
    /// focused — macOS convention, removing the character to the right of
    /// the cursor like the `Delete` key — so it never reaches
    /// `editor_core`'s own `Ctrl+D` (add next occurrence, kept in the
    /// keymaps modeled on VS Code and Sublime) while claiming nothing
    /// outside the editor, letting other panes keep their own `Ctrl+D`.
    /// Returns true if consumed.
    fn apple_ctrl_key(&mut self, key: KeyEvent) -> bool {
        if Self::ctrl(&key) && Self::alt(&key) && matches!(key.code, KeyCode::Char('r' | 'R')) {
            self.run_action("edit.query_replace");
            return true;
        }
        if Self::ctrl(&key)
            && !Self::shift(&key)
            && matches!(key.code, KeyCode::Char('d'))
            && self.focus == Focus::Editor
        {
            self.run_action("motion.delete_forward");
            return true;
        }
        let Some(token) = Self::ctrl_token(&key, false) else {
            return false;
        };
        if let Some(action) = vix_keybindings::lookup("apple", "", &token) {
            self.run_action(action);
            return true;
        }
        false
    }

    // ----- keymap: VS Code (macOS) ----------------------------------------

    /// VS Code (macOS) keymap dispatch: VS Code's signature shortcuts — Quick
    /// Open (`Ctrl+P`), Command Palette (`Ctrl+Shift+P`), Go to Symbol
    /// (`Ctrl+Shift+O`), Go to Line (`Ctrl+G`) — plus the familiar editing
    /// chords, then the shared menu mnemonics and function keys. Returns true if
    /// the key was consumed.
    fn vscode_key(&mut self, key: KeyEvent) -> bool {
        self.vscode_ctrl_key(key) || self.global_shared_key(key)
    }

    /// The VS Code keymap's `Ctrl`-key shortcuts (VS Code's `Cmd` bindings,
    /// with `Ctrl` standing in for `Cmd`), looked up in `vix-keybindings`'
    /// `"vscode-macos"`/`"vscode-windows"` table (T104c — one shared table
    /// underneath, see `crates/vix-keybindings/src/vscode.rs`). Returns
    /// true if consumed.
    fn vscode_ctrl_key(&mut self, key: KeyEvent) -> bool {
        let Some(token) = Self::ctrl_token(&key, false) else {
            return false;
        };
        if let Some(action) = vix_keybindings::lookup(&self.settings.keymap, "", &token) {
            self.run_action(action);
            return true;
        }
        false
    }

    // ----- keymap: IntelliJ (macOS / Windows) -----------------------

    /// `IntelliJ` IDEA keymap dispatch (`Ctrl` stands in for `Cmd` on
    /// macOS), looked up in `vix-keybindings`' `"intellij-macos"`/
    /// `"intellij-windows"` tables (T104d — two genuinely different
    /// tables this time, unlike VS Code's shared one; see
    /// `crates/vix-keybindings/src/intellij.rs`). Editing chords (undo/
    /// cut/copy/paste/select-all) fall through to the editor widget.
    /// Returns true if consumed.
    fn intellij_key(&mut self, key: KeyEvent, win: bool) -> bool {
        // `Ctrl+Alt+L`/`Ctrl+Alt+O` are a single keystroke's modifier
        // combination, not a chord prefix, so `encode_alt: true` folds
        // them into this same lookup rather than a separate context.
        let Some(token) = Self::ctrl_token(&key, true) else {
            return false;
        };
        let keymap_id = if win {
            "intellij-windows"
        } else {
            "intellij-macos"
        };
        if let Some(action) = vix_keybindings::lookup(keymap_id, "", &token) {
            self.run_action(action);
            return true;
        }
        false
    }

    // ----- keymap: Eclipse ------------------------------------------------

    /// Eclipse (Windows) keymap dispatch, looked up in `vix-keybindings`'
    /// `"eclipse"` table (T104e — see `crates/vix-keybindings/src/
    /// eclipse.rs`). Editing chords fall through to the editor widget.
    /// Returns true if consumed.
    fn eclipse_key(&mut self, key: KeyEvent) -> bool {
        let Some(token) = Self::eclipse_token(&key) else {
            return false;
        };
        if let Some(action) = vix_keybindings::lookup("eclipse", "", &token) {
            self.run_action(action);
            return true;
        }
        false
    }

    /// The `vix-macros` token for an Eclipse lookup, or `None` if `key` is
    /// neither a `Ctrl`-held nor an `Alt`-held `Char` (every Eclipse
    /// binding is one or the other). `Ctrl` takes priority over `Alt` —
    /// a `Ctrl`-held key builds its token via [`Self::ctrl_token`]
    /// (`encode_alt: false`, so `Alt` is ignored there too) regardless of
    /// whether `Alt` is also held, exactly mirroring the original
    /// dispatch's `Self::alt(&key) && !Self::ctrl(&key)` guard on its one
    /// `Alt`-only binding (word completion) — that one binding is the
    /// reason this keymap needs its own small wrapper instead of calling
    /// `ctrl_token` directly like every other simple `Ctrl`-only keymap.
    fn eclipse_token(key: &KeyEvent) -> Option<String> {
        if let Some(token) = Self::ctrl_token(key, false) {
            return Some(token);
        }
        let KeyCode::Char(c) = key.code else {
            return None;
        };
        if Self::alt(key) {
            Some(format!("A-{c}"))
        } else {
            None
        }
    }

    // ----- keymap: Sublime Text -------------------------------------------

    /// Sublime Text keymap dispatch (`Ctrl` stands in for `Cmd` on macOS),
    /// looked up in `vix-keybindings`' `"sublime"` table (T104f — see
    /// `crates/vix-keybindings/src/sublime.rs`). Editing chords (undo/cut/
    /// copy/paste/select-all) fall through to the editor widget. Returns
    /// true if consumed.
    fn sublime_key(&mut self, key: KeyEvent) -> bool {
        let Some(token) = Self::ctrl_token(&key, false) else {
            return false;
        };
        if let Some(action) = vix_keybindings::lookup("sublime", "", &token) {
            self.run_action(action);
            return true;
        }
        false
    }

    /// Keys shared by every keymap: menu-bar mnemonics and function keys,
    /// looked up in `vix-keybindings`' keymap-agnostic `SHARED` table
    /// (T104g — see `crates/vix-keybindings/src/shared.rs`) for the
    /// bindings that don't need any extra runtime state. Three things stay
    /// host-side, ahead of or around that lookup:
    /// - The menu-bar `Alt+letter` mnemonics (live in
    ///   [`menu_index_for_alt`]) — a dynamic lookup into the current menu
    ///   structure, not static table data.
    /// - `Ctrl+Shift+Right`/`Left` (extend/shrink selection) — focus-gated,
    ///   checked before the table so the rare case of `Ctrl+Alt+Shift+
    ///   Left`/`Right` (also matching the table's Alt-only `nav.back`/
    ///   `nav.forward` rows) resolves in the same order the original
    ///   dispatch did.
    /// - `Alt+Up`/`Down` and `Alt+n`/`p` (line move/column-select,
    ///   next/prev search selection) — also focus-gated, checked after the
    ///   table since they never share a key with anything in it.
    ///
    /// Returns true if consumed.
    fn global_shared_key(&mut self, key: KeyEvent) -> bool {
        if Self::alt(&key)
            && let KeyCode::Char(c) = key.code
            && let Some(i) = menu_index_for_alt(c)
        {
            self.toggle_menu(i);
            return true;
        }
        if Self::ctrl(&key) && Self::shift(&key) && self.focus == Focus::Editor {
            match key.code {
                KeyCode::Right => {
                    self.run_action("edit.select_more");
                    return true;
                }
                KeyCode::Left => {
                    self.run_action("edit.select_less");
                    return true;
                }
                _ => {}
            }
        }
        if let Some(token) = Self::shared_token(&key)
            && let Some(action) = vix_keybindings::lookup_shared(&token)
        {
            self.run_action(action);
            return true;
        }
        if self.focus == Focus::Editor {
            match key.code {
                KeyCode::Up if Self::alt(&key) => {
                    let a = if Self::shift(&key) {
                        "edit.column_select_up"
                    } else {
                        "edit.move_line_up"
                    };
                    self.run_action(a);
                    return true;
                }
                KeyCode::Down if Self::alt(&key) => {
                    let a = if Self::shift(&key) {
                        "edit.column_select_down"
                    } else {
                        "edit.move_line_down"
                    };
                    self.run_action(a);
                    return true;
                }
                KeyCode::Char('n' | 'N') if Self::alt(&key) => {
                    self.run_action("search.next_selection");
                    return true;
                }
                KeyCode::Char('p' | 'P') if Self::alt(&key) => {
                    self.run_action("search.prev_selection");
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    /// The `vix-macros`-flavored token for a [`vix_keybindings::SHARED`]
    /// lookup, or `None` if `key` doesn't map to one of that table's key
    /// shapes (`Ctrl+Space`, `Ctrl+Tab`/`BackTab`, `Alt+Left`/`Right`/`j`,
    /// or an `F`-key). Only `F`-keys ever encode `Shift` (`F3` vs
    /// `Shift+F3`) — every other shape ignores the Shift bit entirely,
    /// matching the original dispatch's own guards (or lack of one).
    ///
    /// Deliberately **not** folded into [`Self::ctrl_token`] (T145): that
    /// helper only ever builds `Ctrl`-held `Char` tokens; this one also
    /// covers `Alt`-only bindings (no `Ctrl` required at all) and several
    /// non-`Char` key codes (`Tab`, `BackTab`, `Left`, `Right`, `F`-keys),
    /// with Shift gated on the key's *type* rather than a per-keymap
    /// policy. A genuinely different shape, not a `ShiftRule`/`AltRule`
    /// variation of the same one.
    fn shared_token(key: &KeyEvent) -> Option<String> {
        let mut token = String::new();
        if Self::ctrl(key) {
            token.push_str("C-");
        }
        if Self::alt(key) {
            token.push_str("A-");
        }
        if Self::shift(key) && matches!(key.code, KeyCode::F(_)) {
            token.push_str("S-");
        }
        match key.code {
            KeyCode::Char(' ') => token.push_str("Space"),
            KeyCode::Char(c) => token.push(c.to_ascii_lowercase()),
            KeyCode::Tab => token.push_str("Tab"),
            KeyCode::BackTab => token.push_str("BackTab"),
            KeyCode::Left => token.push_str("Left"),
            KeyCode::Right => token.push_str("Right"),
            KeyCode::F(n) => {
                use std::fmt::Write as _;
                let _ = write!(token, "F{n}");
            }
            _ => return None,
        }
        Some(token)
    }

    pub(super) fn toggle_focus_explorer_editor(&mut self) {
        self.focus = if self.focus == Focus::Explorer {
            Focus::Editor
        } else {
            if !self.show_explorer {
                self.show_explorer = true;
            }
            Focus::Explorer
        };
    }

    // ----- keymap: Emacs --------------------------------------------------

    /// Feed a key to the editor as if it were typed with no modifiers, but only
    /// when the editor pane is focused. Used to translate keymap motions
    /// (`Ctrl+F`, `l`, …) into the editor's existing handling.
    pub(super) fn editor_motion(&mut self, code: KeyCode) {
        if self.focus == Focus::Editor {
            self.editor_key(KeyEvent::new(code, KeyModifiers::NONE));
        }
    }

    /// Emacs keymap dispatch: the `Ctrl+X` prefix, `Ctrl`-key chords, and the
    /// `Meta` (Alt) bindings. Returns true if the key was consumed (so it should
    /// not fall through).
    fn emacs_key(&mut self, key: KeyEvent) -> bool {
        // Second key of a `Ctrl+X …` chord.
        if self.emacs_prefix {
            self.emacs_prefix = false;
            return self.emacs_chord_key(key);
        }
        // Third key of a `Ctrl+C Ctrl+X …` chord (the extended Org family).
        if self.emacs_c_x_prefix {
            self.emacs_c_x_prefix = false;
            return self.emacs_c_x_chord_key(key);
        }
        // Fifth key of a `Ctrl+C p c m …` chord (the `project.subproject.*`
        // family).
        if self.emacs_c_p_c_m_prefix {
            self.emacs_c_p_c_m_prefix = false;
            return self.emacs_c_p_c_m_chord_key(key);
        }
        // Fourth key of a `Ctrl+C p c …` chord (the `project.*` family).
        if self.emacs_c_p_c_prefix {
            self.emacs_c_p_c_prefix = false;
            return self.emacs_c_p_c_chord_key(key);
        }
        // Third key of a `Ctrl+C p …` chord.
        if self.emacs_c_p_prefix {
            self.emacs_c_p_prefix = false;
            return self.emacs_c_p_chord_key(key);
        }
        // Second key of a `Ctrl+C …` chord (the Org command family).
        if self.emacs_c_prefix {
            self.emacs_c_prefix = false;
            return self.emacs_c_chord_key(key);
        }
        // `Ctrl+U` starts a universal argument, applied to the next command
        // (here: `C-u C-c C-t` closes a task with a note instead of just cycling).
        if Self::ctrl(&key) && matches!(key.code, KeyCode::Char('u')) {
            self.emacs_universal = true;
            self.status = t!("status.emacs_universal").to_string();
            return true;
        }
        // Any key other than the `Ctrl+C` prefix cancels a pending universal arg.
        if self.emacs_universal && !(Self::ctrl(&key) && matches!(key.code, KeyCode::Char('c'))) {
            self.emacs_universal = false;
        }
        // `Ctrl+X`/`Ctrl+C` start a chord — a mode transition, not a
        // dispatchable action, so these two stay special-cased rather than
        // living in the table. Everything else at the top level (Ctrl
        // chords and Meta/Alt bindings alike) is one registry lookup —
        // see `crates/vix-keybindings/spec/index.md`.
        if Self::ctrl(&key) && matches!(key.code, KeyCode::Char('x')) {
            self.emacs_prefix = true;
            return true;
        }
        if Self::ctrl(&key) && matches!(key.code, KeyCode::Char('c')) {
            self.emacs_c_prefix = true;
            return true;
        }
        if Self::ctrl(&key) || Self::alt(&key) {
            let token = Self::emacs_top_level_token(&key);
            if let Some(action) = vix_keybindings::lookup("emacs", "", &token) {
                self.run_action(action);
                return true;
            }
        }
        false
    }

    /// The `vix-macros` token for a top-level Emacs (Ctrl or Meta) binding
    /// lookup. Ctrl-chord letters are matched case-insensitively (terminals
    /// normally already send `Ctrl+<letter>` lowercase regardless of Shift,
    /// and the dispatch this replaced defensively lowercased too); Meta
    /// (Alt) bindings are case-sensitive, matching that same original
    /// dispatch (`emacs_meta_key`, since folded into this one table).
    ///
    /// Deliberately **not** folded into [`Self::ctrl_token`] (T145): Emacs
    /// needs `Alt`-only bindings too (no `Ctrl` required), delegates to
    /// `crate::macros::encode_key`'s general grammar rather than
    /// hand-building a `"C-"`-prefixed string, and — unlike every
    /// `ctrl_token` caller — never encodes `Shift` explicitly at all
    /// (Emacs bindings don't need the Shift-bit-vs-char-case
    /// disambiguation those keymaps do). A different problem, not a
    /// parameter of the same one.
    fn emacs_top_level_token(key: &KeyEvent) -> String {
        if Self::ctrl(key)
            && let KeyCode::Char(c) = key.code
        {
            crate::macros::encode_key(KeyEvent::new(
                KeyCode::Char(c.to_ascii_lowercase()),
                key.modifiers,
            ))
        } else {
            crate::macros::encode_key(*key)
        }
    }

    /// The second key of an Emacs `Ctrl+X …` chord. Always consumes the key.
    fn emacs_chord_key(&mut self, key: KeyEvent) -> bool {
        let KeyCode::Char(c) = key.code else {
            self.status = t!("status.emacs_no_chord").to_string();
            return true;
        };
        let pressed = Self::chord_key_name(&key, c);
        if let Some(action) = vix_keybindings::lookup("emacs", "C-x", &pressed) {
            self.run_action(action);
        } else {
            self.status = t!("status.emacs_no_chord").to_string();
        }
        true
    }

    /// The second key of an Emacs `Ctrl+C …` chord — the Org command family.
    /// Always consumes the key. `C-c C-t` cycles a headline's TODO state (or,
    /// after `C-u`, closes it with a note), and `C-c C-c` runs the context action
    /// (toggle a checkbox / refresh statistics).
    pub(super) fn emacs_c_chord_key(&mut self, key: KeyEvent) -> bool {
        let universal = std::mem::take(&mut self.emacs_universal);
        // `C-c RET` / `org-table-hline-and-move`: RET arrives as `KeyCode::Enter`,
        // not a `Char`, so it cannot go through the "C-c" context's table lookup.
        if key.code == KeyCode::Enter {
            self.run_action("org.table.hline_and_move");
            return true;
        }
        let KeyCode::Char(c) = key.code else {
            self.status = t!("status.emacs_no_chord").to_string();
            return true;
        };
        // `C-c C-x` opens the third-key chord family.
        if Self::ctrl(&key) && c.eq_ignore_ascii_case(&'x') {
            self.emacs_c_x_prefix = true;
            return true;
        }
        let pressed = Self::chord_key_name(&key, c);
        // `C-c p` opens the `project.*` chord family (see the "C-c p c"
        // context's doc comment, `crates/vix-keybindings/src/emacs.rs`, for
        // why `p` is plain, not `C-p`).
        if pressed == "p" {
            self.emacs_c_p_prefix = true;
            return true;
        }
        if pressed == "C-t" {
            // The universal-argument variant closes with a note.
            self.run_action(if universal {
                "org.close_note"
            } else {
                "org.cycle_todo"
            });
            return true;
        }
        if pressed == "C-c" && universal {
            // `C-u C-c C-c`: force-apply `#+TBLFM:` formulas (org-table-recalc's
            // universal-argument variant), instead of the plain context action.
            self.run_action("org.table.recalc");
            return true;
        }
        if let Some(action) = vix_keybindings::lookup("emacs", "C-c", &pressed) {
            self.run_action(action);
        } else {
            self.status = t!("status.emacs_no_chord").to_string();
        }
        true
    }

    /// Third key of an Emacs `Ctrl+C Ctrl+X …` chord (the extended Org family).
    /// Inside a pipe table, `C-w`/`C-y`/`M-w` mean the table's own rectangle
    /// clipboard (`org-table-cut/paste/copy-rectangle`) instead of their usual
    /// subtree cut/paste/copy meanings, matching how Org itself
    /// context-sensitively shadows these same chords inside a table.
    pub(super) fn emacs_c_x_chord_key(&mut self, key: KeyEvent) -> bool {
        if self.org_table_active_at_cursor() {
            let (alt, ctrl) = (Self::alt(&key), Self::ctrl(&key));
            match key.code {
                KeyCode::Char('w') if alt && !ctrl => {
                    self.run_action("org.table.copy_rectangle");
                    return true;
                }
                KeyCode::Char('w') if ctrl && !alt => {
                    self.run_action("org.table.cut_rectangle");
                    return true;
                }
                KeyCode::Char('y') if ctrl && !alt => {
                    self.run_action("org.table.paste_rectangle");
                    return true;
                }
                _ => {}
            }
        }
        let KeyCode::Char(c) = key.code else {
            self.status = t!("status.emacs_no_chord").to_string();
            return true;
        };
        let pressed = Self::chord_key_name(&key, c);
        if let Some(action) = vix_keybindings::lookup("emacs", "C-c C-x", &pressed) {
            self.run_action(action);
        } else {
            self.status = t!("status.emacs_no_chord").to_string();
        }
        true
    }

    /// Third key of an Emacs `Ctrl+C p …` chord. The only recognized
    /// continuation is a plain `c`, opening the fourth-key `project.*`
    /// family; any other key cleanly cancels back to normal editing, like an
    /// unrecognized key at any other chord depth.
    pub(super) fn emacs_c_p_chord_key(&mut self, key: KeyEvent) -> bool {
        if key.code == KeyCode::Char('c') && !Self::ctrl(&key) {
            self.emacs_c_p_c_prefix = true;
        } else {
            self.status = t!("status.emacs_no_chord").to_string();
        }
        true
    }

    /// Fourth key of an Emacs `Ctrl+C p c …` chord — the `project.*` family
    /// (`vix_keybindings`'s `"C-c p c"` context). A plain `m` opens the
    /// fifth-key `project.subproject.*` family (the `"C-c p c m"` context);
    /// any other key is looked up in this one.
    pub(super) fn emacs_c_p_c_chord_key(&mut self, key: KeyEvent) -> bool {
        if key.code == KeyCode::Char('m') && !Self::ctrl(&key) {
            self.emacs_c_p_c_m_prefix = true;
            return true;
        }
        let KeyCode::Char(c) = key.code else {
            self.status = t!("status.emacs_no_chord").to_string();
            return true;
        };
        let pressed = Self::chord_key_name(&key, c);
        if let Some(action) = vix_keybindings::lookup("emacs", "C-c p c", &pressed) {
            self.run_action(action);
        } else {
            self.status = t!("status.emacs_no_chord").to_string();
        }
        true
    }

    /// Fifth key of an Emacs `Ctrl+C p c m …` chord — the
    /// `project.subproject.*` family (`vix_keybindings`'s `"C-c p c m"`
    /// context).
    pub(super) fn emacs_c_p_c_m_chord_key(&mut self, key: KeyEvent) -> bool {
        let KeyCode::Char(c) = key.code else {
            self.status = t!("status.emacs_no_chord").to_string();
            return true;
        };
        let pressed = Self::chord_key_name(&key, c);
        if let Some(action) = vix_keybindings::lookup("emacs", "C-c p c m", &pressed) {
            self.run_action(action);
        } else {
            self.status = t!("status.emacs_no_chord").to_string();
        }
        true
    }

    /// The chord-table name of a pressed key: `C-<char>` with Ctrl held, the
    /// bare char otherwise.
    fn chord_key_name(key: &KeyEvent, c: char) -> String {
        if Self::ctrl(key) {
            format!("C-{}", c.to_ascii_lowercase())
        } else {
            c.to_string()
        }
    }

    // ----- keymap: Vim ----------------------------------------------------

    /// Vim keymap dispatch: Normal-mode motions/commands, Insert mode, and the
    /// `:` command line. Returns true if the key was consumed.
    fn vim_key(&mut self, key: KeyEvent) -> bool {
        if self.vim_cmd.is_some() {
            self.vim_cmd_key(key);
            return true;
        }
        if self.modal_insert {
            if key.code == KeyCode::Esc {
                self.modal_insert = false;
                return true;
            }
            // Let typing and shared keys flow through to the editor.
            return false;
        }
        // Normal mode. Defer modifier combos and function keys to the shared
        // handler (menu mnemonics, F10, …).
        if Self::ctrl(&key) || Self::alt(&key) || matches!(key.code, KeyCode::F(_)) {
            return false;
        }
        // `:` opens the command line from any pane (shown in the mode indicator).
        if key.code == KeyCode::Char(':') {
            self.vim_cmd = Some(String::new());
            return true;
        }
        // Other Normal-mode keys only make sense over the editor; elsewhere let the
        // focused pane keep its own navigation.
        if self.focus != Focus::Editor {
            return false;
        }
        self.vim_normal_key(key);
        // Swallow every other Normal-mode key so it never types into the buffer.
        true
    }

    /// One Normal-mode key for the Vi keymap (the editor is focused).
    /// Handles the pending two-key operators (`gg`, `dd`, `yy`) first, then
    /// looks everything else up in `vix-keybindings`' `"vi"` table
    /// (T104b) — shared with Spacemacs, which delegates its own
    /// Normal-mode vocabulary to this same function.
    fn vim_normal_key(&mut self, key: KeyEvent) {
        // Second key of a pending `g` / `d` / `y` operator: a one-entry
        // context per operator (only `gg`/`dd`/`yy` continue it; anything
        // else silently cancels, matching the original dispatch, which had
        // no fallback for a miss here either).
        if let Some(op) = self.vim_pending.take() {
            let token = crate::macros::encode_key(key);
            if let Some(action) = vix_keybindings::lookup("vi", &op.to_string(), &token) {
                self.run_action(action);
            }
            return;
        }
        // `g`/`d`/`y` start a pending operator — a mode transition, not a
        // dispatchable action, so (like Emacs's `C-x`/`C-c` prefix-entry,
        // T104a) stays special-cased rather than living in the table.
        if let KeyCode::Char(c @ ('g' | 'd' | 'y')) = key.code {
            self.vim_pending = Some(c);
            return;
        }
        let token = crate::macros::encode_key(key);
        if let Some(action) = vix_keybindings::lookup("vi", "", &token) {
            self.run_action(action);
        }
    }

    fn vim_enter_insert(&mut self) {
        self.modal_insert = true;
    }

    /// Dispatch a `vim.*` action — the Vim/Spacemacs insert-mode-entry
    /// commands (`i`/`a`/`A`/`I`/`o`/`O`), each a compound of an
    /// `editor_motion` (or two) plus entering Insert mode. Added (T104b)
    /// so `vim_normal_key`'s table lookup has a real action id for what
    /// used to be a bespoke method call per key. Returns `true` if
    /// `action` was handled.
    pub(super) fn run_vim_action(&mut self, action: &str) -> bool {
        match action {
            "vim.insert" => self.vim_enter_insert(),
            "vim.append" => {
                self.editor_motion(KeyCode::Right);
                self.vim_enter_insert();
            }
            "vim.append_end" => {
                self.editor_motion(KeyCode::End);
                self.vim_enter_insert();
            }
            "vim.insert_line_start" => {
                self.editor_motion(KeyCode::Home);
                self.vim_enter_insert();
            }
            "vim.open_below" => {
                self.editor_motion(KeyCode::End);
                self.editor_motion(KeyCode::Enter);
                self.vim_enter_insert();
            }
            "vim.open_above" => {
                self.editor_motion(KeyCode::Home);
                self.editor_motion(KeyCode::Enter);
                self.editor_motion(KeyCode::Up);
                self.vim_enter_insert();
            }
            _ => return false,
        }
        true
    }

    /// Handle a key while the Vim `:` command line is open. The in-progress text
    /// is reflected live by the mode indicator, so this only mutates state.
    fn vim_cmd_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.vim_cmd = None,
            KeyCode::Enter => {
                let cmd = self.vim_cmd.take().unwrap_or_default();
                self.run_vim_command(cmd.trim());
            }
            KeyCode::Backspace => {
                // Backspacing past the empty `:` closes the command line.
                if let Some(s) = self.vim_cmd.as_mut()
                    && s.pop().is_none()
                {
                    self.vim_cmd = None;
                }
            }
            KeyCode::Char(c) => {
                if let Some(s) = self.vim_cmd.as_mut() {
                    s.push(c);
                }
            }
            _ => {}
        }
    }

    /// Run a Vim ex command (the text after `:`).
    fn run_vim_command(&mut self, cmd: &str) {
        match cmd {
            "w" => self.run_action("file.save"),
            "q" => self.run_action("file.close"),
            // Vim force-quit discards unsaved changes without prompting.
            "q!" => self.should_quit = true,
            "wq" | "x" => {
                self.run_action("file.save");
                self.run_action("file.close");
            }
            "Ex" => {
                self.show_explorer = true;
                self.focus = Focus::Explorer;
            }
            "e" => self.run_action("file.open"),
            "" => {}
            other => {
                // `:N` — go to line N. `:e path` — open path.
                if let Ok(line) = other.parse::<usize>() {
                    let area = self.editor_view();
                    self.editor.goto(line, None, area);
                } else if let Some(path) = other.strip_prefix("e ") {
                    let path = self.resolve(path.trim());
                    self.open_path(&path, false);
                } else {
                    self.status = t!("status.vim_no_command", cmd = other).to_string();
                }
            }
        }
    }

    // ----- Spacemacs keymap ----------------------------------------------

    /// Spacemacs dispatch: Vi-style modal editing in Normal/Insert, plus a
    /// `Space` leader that opens menu-like command sequences. Returns true if the
    /// key was consumed.
    fn spacemacs_key(&mut self, key: KeyEvent) -> bool {
        // A leader sequence is in progress (after `SPC` in Normal mode).
        if self.spacemacs_leader.is_some() {
            self.spacemacs_leader_key(key);
            return true;
        }
        // Shared `:` command line (reuses the Vim command machinery).
        if self.vim_cmd.is_some() {
            self.vim_cmd_key(key);
            return true;
        }
        if self.modal_insert {
            if key.code == KeyCode::Esc {
                self.modal_insert = false;
                return true;
            }
            return false;
        }
        if Self::ctrl(&key) || Self::alt(&key) || matches!(key.code, KeyCode::F(_)) {
            return false;
        }
        // `SPC` opens the leader from the editor; elsewhere it types normally.
        if key.code == KeyCode::Char(' ') && self.focus == Focus::Editor {
            self.spacemacs_leader = Some(String::new());
            return true;
        }
        if key.code == KeyCode::Char(':') {
            self.vim_cmd = Some(String::new());
            return true;
        }
        if self.focus != Focus::Editor {
            return false;
        }
        // Shared Vi Normal-mode vocabulary (motions, operators, insert entry).
        self.vim_normal_key(key);
        true
    }

    /// Handle a key while a Spacemacs `SPC` leader sequence is accumulating. Runs
    /// the action when the sequence matches, keeps waiting while it is a prefix,
    /// and aborts (with a status note) otherwise.
    fn spacemacs_leader_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            self.spacemacs_leader = None;
            return;
        }
        let KeyCode::Char(c) = key.code else { return };
        let mut seq = self.spacemacs_leader.take().unwrap_or_default();
        seq.push(c);
        match vix_keybindings::lookup_sequence("spacemacs", "", &seq) {
            vix_keybindings::SequenceMatch::Action(action) => self.run_action(action),
            vix_keybindings::SequenceMatch::Prefix => self.spacemacs_leader = Some(seq),
            vix_keybindings::SequenceMatch::None => {
                self.status = t!("status.spacemacs_no_leader", seq = seq).to_string();
            }
        }
    }
}
