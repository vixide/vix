//! Keys for the file browser overlay, split into what a key *means* and what
//! the app does about it.
//!
//! [`command_for`] is a pure table from a key to a [`Command`]; a test can
//! check the whole keymap without an [`App`]. [`App::file_browser_key`] then
//! applies the command to the open browser.

use vix_input::{KeyCode, KeyEvent, KeyModifiers};

use super::App;

/// What a key does in the file browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Command {
    /// Highlight the row above.
    Up,
    /// Highlight the row below.
    Down,
    /// Move a page up.
    PageUp,
    /// Move a page down.
    PageDown,
    /// Jump to the first row.
    First,
    /// Jump to the last row.
    Last,
    /// Walk up to the parent directory.
    Parent,
    /// Enter the highlighted row when it is a directory.
    EnterDirectory,
    /// Delete the last character of the query.
    Backspace,
    /// Cycle the sort key.
    CycleSort,
    /// Flip ascending and descending.
    ToggleOrder,
    /// Show or hide dotfiles.
    ToggleHidden,
    /// Leave for the Open Path prompt.
    OpenPathPrompt,
    /// Leave for the command palette's file finder.
    FileFinder,
    /// Append a character to the query.
    Type(char),
    /// Open the highlighted row.
    Open,
    /// Close the browser.
    Close,
}

/// The command `key` stands for, or `None` for a key the browser ignores.
pub(super) fn command_for(key: KeyEvent) -> Option<Command> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    Some(match key.code {
        KeyCode::Up => Command::Up,
        KeyCode::Down => Command::Down,
        KeyCode::PageUp => Command::PageUp,
        KeyCode::PageDown => Command::PageDown,
        KeyCode::Home => Command::First,
        KeyCode::End => Command::Last,
        KeyCode::Left => Command::Parent,
        KeyCode::Right => Command::EnterDirectory,
        KeyCode::Backspace => Command::Backspace,
        KeyCode::Char('s') if ctrl => Command::CycleSort,
        KeyCode::Char('r') if ctrl => Command::ToggleOrder,
        KeyCode::Char('h') if alt => Command::ToggleHidden,
        KeyCode::Char('o') if ctrl => Command::OpenPathPrompt,
        KeyCode::Char('p') if ctrl => Command::FileFinder,
        KeyCode::Char(c) if !ctrl && !alt => Command::Type(c),
        KeyCode::Enter => Command::Open,
        KeyCode::Esc => Command::Close,
        _ => return None,
    })
}

impl App {
    /// Handle a key in the file browser: arrows and paging move, Left and
    /// Right walk the directory tree, typing filters, Enter opens, Esc closes.
    pub(super) fn file_browser_key(&mut self, key: KeyEvent) {
        let page = usize::from(self.layout.file_browser.height).max(1);
        let Some(command) = command_for(key) else {
            return;
        };
        match command {
            Command::Open => self.file_browser_open_selected(),
            Command::Close => self.file_browser = None,
            Command::OpenPathPrompt => {
                self.file_browser = None;
                self.run_action("file.open_path");
            }
            Command::FileFinder => {
                self.file_browser = None;
                self.run_action("tools.palette");
            }
            other => {
                let Some(fb) = self.file_browser.as_mut() else {
                    return;
                };
                match other {
                    Command::Up => fb.up(),
                    Command::Down => fb.down(),
                    Command::PageUp => fb.page_up(page),
                    Command::PageDown => fb.page_down(page),
                    Command::First => {
                        fb.selected = 0;
                        fb.scroll = 0;
                    }
                    Command::Last => fb.selected = fb.len().saturating_sub(1),
                    Command::Parent => fb.parent(),
                    Command::EnterDirectory => {
                        if fb.selected_entry().is_some_and(|e| e.is_dir) {
                            fb.activate();
                        }
                    }
                    Command::Backspace => fb.backspace(),
                    Command::CycleSort => fb.cycle_sort(),
                    Command::ToggleOrder => fb.toggle_order(),
                    Command::ToggleHidden => fb.toggle_hidden(),
                    Command::Type(c) => fb.push(c),
                    Command::Open
                    | Command::Close
                    | Command::OpenPathPrompt
                    | Command::FileFinder => {}
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

    fn with(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn movement_and_navigation_keys_map_to_their_commands() {
        let table = [
            (KeyCode::Up, Command::Up),
            (KeyCode::Down, Command::Down),
            (KeyCode::PageUp, Command::PageUp),
            (KeyCode::PageDown, Command::PageDown),
            (KeyCode::Home, Command::First),
            (KeyCode::End, Command::Last),
            (KeyCode::Left, Command::Parent),
            (KeyCode::Right, Command::EnterDirectory),
            (KeyCode::Backspace, Command::Backspace),
            (KeyCode::Enter, Command::Open),
            (KeyCode::Esc, Command::Close),
        ];
        for (code, command) in table {
            assert_eq!(command_for(plain(code)), Some(command), "{code:?}");
        }
    }

    #[test]
    fn modified_letters_are_commands_and_plain_letters_are_typed() {
        let ctrl = KeyModifiers::CONTROL;
        assert_eq!(
            command_for(with(KeyCode::Char('s'), ctrl)),
            Some(Command::CycleSort)
        );
        assert_eq!(
            command_for(with(KeyCode::Char('r'), ctrl)),
            Some(Command::ToggleOrder)
        );
        assert_eq!(
            command_for(with(KeyCode::Char('o'), ctrl)),
            Some(Command::OpenPathPrompt)
        );
        assert_eq!(
            command_for(with(KeyCode::Char('p'), ctrl)),
            Some(Command::FileFinder)
        );
        assert_eq!(
            command_for(with(KeyCode::Char('h'), KeyModifiers::ALT)),
            Some(Command::ToggleHidden)
        );
        assert_eq!(
            command_for(plain(KeyCode::Char('s'))),
            Some(Command::Type('s'))
        );
        assert_eq!(
            command_for(plain(KeyCode::Char('é'))),
            Some(Command::Type('é'))
        );
    }

    #[test]
    fn other_modified_letters_and_unbound_keys_are_ignored() {
        let ctrl = KeyModifiers::CONTROL;
        assert_eq!(command_for(with(KeyCode::Char('x'), ctrl)), None);
        assert_eq!(
            command_for(with(KeyCode::Char('x'), KeyModifiers::ALT)),
            None
        );
        assert_eq!(command_for(plain(KeyCode::F(5))), None);
        assert_eq!(command_for(plain(KeyCode::Tab)), None);
    }
}
