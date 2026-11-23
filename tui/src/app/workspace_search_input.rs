//! Keys for the project-search panel, split into what a key *means* and what
//! the app does about it.
//!
//! [`command_for`] is a pure table from a key to a [`Command`], testable
//! without an [`App`]; [`App::ps_key`] applies the command to the open panel.

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

use vix_input::{KeyCode, KeyEvent, KeyModifiers};

use super::{App, Field, Prompt, PromptKind, WorkspaceFlags};

/// What a key does in the project-search panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Command {
    /// Close the panel, remembering the search.
    Close,
    /// Highlight the hit above.
    Up,
    /// Highlight the hit below.
    Down,
    /// Switch between the query, replacement and path-filter fields.
    ToggleField,
    /// Open the highlighted hit; with `alt`, or on the replace field of a
    /// replace, apply the replacement everywhere instead.
    Enter {
        /// Whether Alt was held.
        alt: bool,
    },
    /// Hand the search on to the workspace listing in the dock (`Alt+I`).
    ContinueInDock,
    /// Toggle case sensitivity (`Alt+C`).
    ToggleCase,
    /// Toggle regular-expression matching (`Alt+R`).
    ToggleRegex,
    /// Turn replace mode on or off (`Alt+H`).
    ToggleReplace,
    /// Open the results as an editable buffer (`Alt+E`).
    OpenAsBuffer,
    /// Any other `Alt+<letter>`: change nothing but search again.
    AltOther,
    /// Delete the last character of the active field.
    Backspace,
    /// Append a character to the active field.
    Type(char),
}

/// The command `key` stands for, or `None` for a key the panel ignores.
pub(super) fn command_for(key: KeyEvent) -> Option<Command> {
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    Some(match key.code {
        KeyCode::Esc => Command::Close,
        KeyCode::Up => Command::Up,
        KeyCode::Down => Command::Down,
        KeyCode::Tab => Command::ToggleField,
        KeyCode::Enter => Command::Enter { alt },
        KeyCode::Char('i' | 'I') if alt => Command::ContinueInDock,
        KeyCode::Char(c) if alt => match c.to_ascii_lowercase() {
            'c' => Command::ToggleCase,
            'r' => Command::ToggleRegex,
            'h' => Command::ToggleReplace,
            'e' => Command::OpenAsBuffer,
            _ => Command::AltOther,
        },
        KeyCode::Backspace => Command::Backspace,
        KeyCode::Char(c) => Command::Type(c),
        _ => return None,
    })
}

impl App {
    /// Handle a key in the project-search panel.
    pub(super) fn ps_key(&mut self, key: KeyEvent) {
        let Some(command) = command_for(key) else {
            return;
        };
        match command {
            Command::Close => {
                self.remember_workspace_search();
                self.workspace_search = None;
            }
            Command::Up => {
                if let Some(p) = self.workspace_search.as_mut() {
                    p.up();
                }
            }
            Command::Down => {
                if let Some(p) = self.workspace_search.as_mut() {
                    p.down();
                }
            }
            Command::ToggleField => {
                if let Some(p) = self.workspace_search.as_mut() {
                    p.toggle_field();
                }
            }
            Command::Enter { alt } => {
                let replacing = self
                    .workspace_search
                    .as_ref()
                    .is_some_and(|p| p.flags.contains(WorkspaceFlags::REPLACING));
                let on_replace =
                    self.workspace_search.as_ref().map(|p| p.field) == Some(Field::Replace);
                if replacing && (alt || on_replace) {
                    self.workspace_replace_all();
                } else {
                    self.open_selected_hit();
                }
            }
            // The scope option continues here: the panel is the "Files" stage,
            // so `Alt+I` moves on to the workspace listing in the dock.
            Command::ContinueInDock => {
                let Some(p) = self.workspace_search.as_ref() else {
                    return;
                };
                let (query, case, regex) = (
                    p.query.clone(),
                    p.flags.contains(WorkspaceFlags::CASE_SENSITIVE),
                    p.flags.contains(WorkspaceFlags::REGEX),
                );
                self.workspace_search = None;
                if query.is_empty() {
                    self.prompt = Some(Prompt::new(
                        PromptKind::SearchToDock,
                        t!("prompt.search_dock").to_string(),
                    ));
                } else {
                    self.search_workspace_to_dock(&query, case, regex);
                }
            }
            Command::ToggleCase
            | Command::ToggleRegex
            | Command::ToggleReplace
            | Command::OpenAsBuffer
            | Command::AltOther => {
                if let Some(p) = self.workspace_search.as_mut() {
                    match command {
                        Command::ToggleCase => p.flags.toggle(WorkspaceFlags::CASE_SENSITIVE),
                        Command::ToggleRegex => p.flags.toggle(WorkspaceFlags::REGEX),
                        // Replace is a mode of this panel, so a search started
                        // as a find can become a replace without reopening.
                        Command::ToggleReplace => {
                            p.flags.toggle(WorkspaceFlags::REPLACING);
                            p.field = if p.flags.contains(WorkspaceFlags::REPLACING) {
                                Field::Replace
                            } else {
                                Field::Query
                            };
                        }
                        _ => {}
                    }
                }
                // T211: Alt+E opens the results as a real editable buffer, closing
                // this panel -- `open_wgrep_results` handles that itself, so the
                // `run_workspace_search` below (harmless either way) becomes a
                // no-op once it does.
                if command == Command::OpenAsBuffer {
                    self.open_wgrep_results();
                }
                self.run_workspace_search();
            }
            Command::Backspace | Command::Type(_) => {
                // Editing any field except Replace (query or the path filters)
                // changes the result set, so re-run the search.
                let affects =
                    self.workspace_search.as_ref().map(|p| p.field) != Some(Field::Replace);
                if let Some(p) = self.workspace_search.as_mut() {
                    match command {
                        Command::Type(c) => p.active_field_mut().push(c),
                        _ => {
                            p.active_field_mut().pop();
                        }
                    }
                }
                if affects {
                    self.run_workspace_search();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn alt(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT)
    }

    #[test]
    fn plain_keys_navigate_edit_and_close() {
        let table = [
            (KeyCode::Esc, Command::Close),
            (KeyCode::Up, Command::Up),
            (KeyCode::Down, Command::Down),
            (KeyCode::Tab, Command::ToggleField),
            (KeyCode::Enter, Command::Enter { alt: false }),
            (KeyCode::Backspace, Command::Backspace),
            (KeyCode::Char('q'), Command::Type('q')),
            (KeyCode::Char('I'), Command::Type('I')),
        ];
        for (code, command) in table {
            assert_eq!(command_for(plain(code)), Some(command), "{code:?}");
        }
    }

    #[test]
    fn alt_letters_pick_options_whatever_their_case() {
        assert_eq!(command_for(alt('i')), Some(Command::ContinueInDock));
        assert_eq!(command_for(alt('I')), Some(Command::ContinueInDock));
        for (c, command) in [
            ('c', Command::ToggleCase),
            ('C', Command::ToggleCase),
            ('r', Command::ToggleRegex),
            ('h', Command::ToggleReplace),
            ('e', Command::OpenAsBuffer),
            ('E', Command::OpenAsBuffer),
        ] {
            assert_eq!(command_for(alt(c)), Some(command), "alt+{c}");
        }
    }

    #[test]
    fn an_unbound_alt_letter_still_searches_again_but_alt_enter_is_a_replace_request() {
        assert_eq!(command_for(alt('z')), Some(Command::AltOther));
        let alt_enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT);
        assert_eq!(command_for(alt_enter), Some(Command::Enter { alt: true }));
    }

    #[test]
    fn keys_the_panel_does_not_use_are_ignored() {
        for code in [
            KeyCode::Left,
            KeyCode::Home,
            KeyCode::PageDown,
            KeyCode::F(2),
        ] {
            assert_eq!(command_for(plain(code)), None, "{code:?}");
        }
    }
}
