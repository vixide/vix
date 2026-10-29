//! The IM dock panel's state, over [`vix_im`]: which conversation is
//! selected, the message list's highlight and scroll, the input line, and
//! what Enter does. Drawing lives in the App shell (`tui/src/ui/`); this
//! crate never draws, sends or opens anything. [`Panel::key`] turns a key
//! into an [`Outcome`] for the App to carry out.

use vix_im::{CodeRef, Command, CommandError, Conversation, ConversationId, Inbox, Input};
use vix_input::{KeyCode, KeyEvent, KeyModifiers};

/// Which part of the panel has the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Focus {
    /// The input line at the bottom.
    #[default]
    Input,
    /// The message list, with one message highlighted.
    Messages,
}

/// What submitting the input line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Submit {
    /// Send this text to the selected conversation.
    Send(String),
    /// Run a slash command.
    Command(Command),
    /// The line is not a valid command; it stays in the input to be fixed.
    Error(CommandError),
    /// Nothing to send (an empty or blank line).
    Nothing,
}

/// What the App should do after a key.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Outcome {
    /// Nothing beyond redrawing.
    None,
    /// The input line was submitted.
    Submit(Submit),
    /// Open this code reference (Enter on a message that has one). The App
    /// resolves it with [`vix_im::resolve_in_root`] before opening.
    Open(CodeRef),
    /// Another conversation was selected (and marked read).
    Selected(ConversationId),
    /// Give focus back to the editor (`Esc`).
    Close,
}

/// The panel's state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Panel {
    selected: usize,
    focus: Focus,
    message: usize,
    scroll: usize,
    input: String,
    cursor: usize,
}

impl Panel {
    /// A panel on the first conversation, with an empty input line.
    #[must_use]
    pub fn new() -> Panel {
        Panel::default()
    }

    /// The selected conversation's position in the inbox.
    #[must_use]
    pub fn selected_index(&self) -> usize {
        self.selected
    }

    /// The selected conversation, if the inbox has any.
    #[must_use]
    pub fn conversation<'a>(&self, inbox: &'a Inbox) -> Option<&'a Conversation> {
        inbox.conversations().get(self.selected)
    }

    /// The selected conversation's id.
    #[must_use]
    pub fn selected_id<'a>(&self, inbox: &'a Inbox) -> Option<&'a ConversationId> {
        self.conversation(inbox).map(|c| &c.id)
    }

    /// Select conversation `id`, mark it read and put focus on the input.
    /// `false` (and nothing changes) if there is no such conversation.
    pub fn select(&mut self, inbox: &mut Inbox, id: &ConversationId) -> bool {
        let Some(at) = inbox.index_of(id) else {
            return false;
        };
        self.select_index(inbox, at);
        true
    }

    fn select_index(&mut self, inbox: &mut Inbox, at: usize) {
        self.selected = at;
        self.focus = Focus::Input;
        let id = inbox.conversations()[at].id.clone();
        inbox.mark_read(&id);
        self.message = inbox.conversations()[at].messages.len().saturating_sub(1);
        self.scroll = 0;
    }

    /// Select the next conversation (`Tab`), wrapping; marks it read.
    pub fn next_conversation(&mut self, inbox: &mut Inbox) -> Option<ConversationId> {
        let len = inbox.conversations().len();
        if len == 0 {
            return None;
        }
        self.select_index(inbox, (self.selected.min(len - 1) + 1) % len);
        self.selected_id(inbox).cloned()
    }

    /// Select the previous conversation (`Shift+Tab`), wrapping; marks it
    /// read.
    pub fn prev_conversation(&mut self, inbox: &mut Inbox) -> Option<ConversationId> {
        let len = inbox.conversations().len();
        if len == 0 {
            return None;
        }
        self.select_index(inbox, (self.selected.min(len - 1) + len - 1) % len);
        self.selected_id(inbox).cloned()
    }

    /// Keep the selection in range after conversations were removed.
    pub fn clamp(&mut self, inbox: &Inbox) {
        self.selected = self
            .selected
            .min(inbox.conversations().len().saturating_sub(1));
        let len = self.conversation(inbox).map_or(0, |c| c.messages.len());
        self.message = self.message.min(len.saturating_sub(1));
        if len == 0 {
            self.focus = Focus::Input;
        }
    }

    /// Which part has the keyboard.
    #[must_use]
    pub fn focus(&self) -> Focus {
        self.focus
    }

    /// The highlighted message's index, when the message list has focus.
    #[must_use]
    pub fn selected_message(&self) -> Option<usize> {
        (self.focus == Focus::Messages).then_some(self.message)
    }

    /// The first message row shown, as last set by [`Panel::ensure_visible`].
    #[must_use]
    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Scroll so the highlighted message shows in a list `height` rows
    /// tall. With the input focused, the newest messages show.
    pub fn ensure_visible(&mut self, inbox: &Inbox, height: usize) {
        let len = self.conversation(inbox).map_or(0, |c| c.messages.len());
        let target = match self.focus {
            Focus::Messages => self.message,
            Focus::Input => len.saturating_sub(1),
        };
        self.scroll = vix_list_state::ensure_visible(target, self.scroll, height, len);
    }

    /// Up: from an empty input line, into the message list at the newest
    /// message; in the list, one message up. Up in a non-empty input does
    /// nothing.
    pub fn up(&mut self, inbox: &Inbox) {
        let len = self.conversation(inbox).map_or(0, |c| c.messages.len());
        match self.focus {
            Focus::Input if self.input.is_empty() && len > 0 => {
                self.focus = Focus::Messages;
                self.message = len - 1;
            }
            Focus::Input => {}
            Focus::Messages => self.message = vix_list_state::up(self.message),
        }
    }

    /// Down: one message down; past the newest, back to the input line.
    pub fn down(&mut self, inbox: &Inbox) {
        let len = self.conversation(inbox).map_or(0, |c| c.messages.len());
        if self.focus == Focus::Messages {
            if self.message + 1 >= len {
                self.focus = Focus::Input;
            } else {
                self.message = vix_list_state::down(self.message, len);
            }
        }
    }

    /// The code reference on the highlighted message, for Enter to open.
    #[must_use]
    pub fn selected_code_ref(&self, inbox: &Inbox) -> Option<CodeRef> {
        let index = self.selected_message()?;
        self.conversation(inbox)?.messages.get(index)?.code_ref()
    }

    /// The input line.
    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
    }

    /// The cursor's byte offset in the input line (always a character
    /// boundary).
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Replace the input line, cursor at its end (to edit a recalled line).
    pub fn set_input(&mut self, text: &str) {
        self.input.clear();
        self.cursor = 0;
        self.insert_str(text);
    }

    /// Insert a character at the cursor. Control characters are ignored.
    pub fn insert_char(&mut self, c: char) {
        if c.is_control() {
            return;
        }
        self.focus = Focus::Input;
        self.input.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Insert pasted text at the cursor, sanitized to one line.
    pub fn insert_str(&mut self, text: &str) {
        let clean = vix_im::sanitize(text);
        self.focus = Focus::Input;
        self.input.insert_str(self.cursor, &clean);
        self.cursor += clean.len();
    }

    /// Delete the character before the cursor.
    pub fn backspace(&mut self) {
        if let Some(prev) = self.prev_boundary() {
            self.input.replace_range(prev..self.cursor, "");
            self.cursor = prev;
        }
    }

    /// Delete the character under the cursor.
    pub fn delete(&mut self) {
        if let Some(next) = self.next_boundary() {
            self.input.replace_range(self.cursor..next, "");
        }
    }

    /// Move the cursor one character left.
    pub fn left(&mut self) {
        if let Some(prev) = self.prev_boundary() {
            self.cursor = prev;
        }
    }

    /// Move the cursor one character right.
    pub fn right(&mut self) {
        if let Some(next) = self.next_boundary() {
            self.cursor = next;
        }
    }

    /// Move the cursor to the start of the line.
    pub fn home(&mut self) {
        self.cursor = 0;
    }

    /// Move the cursor to the end of the line.
    pub fn end(&mut self) {
        self.cursor = self.input.len();
    }

    /// Empty the input line.
    pub fn clear_input(&mut self) {
        self.input.clear();
        self.cursor = 0;
    }

    fn prev_boundary(&self) -> Option<usize> {
        self.input[..self.cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
    }

    fn next_boundary(&self) -> Option<usize> {
        self.input[self.cursor..]
            .chars()
            .next()
            .map(|c| self.cursor + c.len_utf8())
    }

    /// Submit the input line. Text and valid commands clear the line; an
    /// invalid command stays so it can be fixed; a blank line does nothing.
    pub fn submit(&mut self) -> Submit {
        if self.input.trim().is_empty() {
            return Submit::Nothing;
        }
        let result = match vix_im::parse_input(&self.input) {
            Ok(Input::Text(text)) => Submit::Send(text),
            Ok(Input::Command(command)) => Submit::Command(command),
            Ok(_) => Submit::Nothing,
            Err(error) => return Submit::Error(error),
        };
        self.clear_input();
        result
    }

    /// Handle a key press.
    ///
    /// `Enter` submits the input, or on a highlighted message with a code
    /// reference asks to open it; `Tab`/`Shift+Tab` cycle conversations;
    /// `Up`/`Down` move between the input and the message list; `Left`,
    /// `Right`, `Home`, `End`, `Backspace`, `Delete` and typed characters
    /// edit the input (typing in the list moves to the input first); `Esc`
    /// gives focus back to the editor.
    pub fn key(&mut self, key: KeyEvent, inbox: &mut Inbox) -> Outcome {
        if key.is_release() {
            return Outcome::None;
        }
        let selected = |id: Option<ConversationId>| id.map_or(Outcome::None, Outcome::Selected);
        match key.code {
            KeyCode::Esc => return Outcome::Close,
            KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
                return selected(self.prev_conversation(inbox));
            }
            KeyCode::BackTab => return selected(self.prev_conversation(inbox)),
            KeyCode::Tab => return selected(self.next_conversation(inbox)),
            KeyCode::Up => self.up(inbox),
            KeyCode::Down => self.down(inbox),
            KeyCode::Enter => {
                return match self.focus {
                    Focus::Input => Outcome::Submit(self.submit()),
                    Focus::Messages => self
                        .selected_code_ref(inbox)
                        .map_or(Outcome::None, Outcome::Open),
                };
            }
            KeyCode::Left => self.left(),
            KeyCode::Right => self.right(),
            KeyCode::Home => self.home(),
            KeyCode::End => self.end(),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.insert_char(c);
            }
            _ => {}
        }
        Outcome::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vix_im::{CaseMapping, ConversationKind, Message, Presence};

    fn inbox() -> Inbox {
        let mut inbox = Inbox::new("ana", CaseMapping::Rfc1459);
        for (id, title) in [("a", "#vix"), ("b", "#rust"), ("c", "Notes")] {
            inbox.add(Conversation::new(
                ConversationId::new(id),
                ConversationKind::Room,
                title,
            ));
        }
        inbox
    }

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn typed(panel: &mut Panel, inbox: &mut Inbox, text: &str) {
        for c in text.chars() {
            panel.key(press(KeyCode::Char(c)), inbox);
        }
    }

    #[test]
    fn input_editing() {
        let mut p = Panel::new();
        for c in "héllo".chars() {
            p.insert_char(c);
        }
        assert_eq!((p.input(), p.cursor()), ("héllo", 6));
        p.left();
        p.left();
        p.backspace();
        assert_eq!(p.input(), "hélo");
        p.home();
        p.delete();
        p.insert_char('J');
        assert_eq!(p.input(), "Jélo");
        p.end();
        p.right();
        p.delete();
        p.insert_str("\u{1b}[31m!\n?");
        assert_eq!((p.input(), p.cursor()), ("Jélo! ?", 8));
        p.insert_char('\u{7}');
        assert_eq!(p.input(), "Jélo! ?");
        p.home();
        p.left();
        p.backspace();
        assert_eq!(p.cursor(), 0);
    }

    #[test]
    fn submit_variants() {
        let mut p = Panel::new();
        assert_eq!(p.submit(), Submit::Nothing);
        p.set_input("   ");
        assert_eq!(p.submit(), Submit::Nothing);
        p.set_input("see a.rs:3");
        assert_eq!(p.submit(), Submit::Send("see a.rs:3".into()));
        assert_eq!(p.input(), "");
        p.set_input("//etc");
        assert_eq!(p.submit(), Submit::Send("/etc".into()));
        p.set_input("/status busy");
        assert_eq!(p.submit(), Submit::Command(Command::Status(Presence::Busy)));
        p.set_input("/bogus");
        assert_eq!(
            p.submit(),
            Submit::Error(CommandError::Unknown("bogus".into()))
        );
        assert_eq!(p.input(), "/bogus");
    }

    #[test]
    fn tab_cycles_and_marks_read() {
        let mut inbox = inbox();
        let b = ConversationId::new("b");
        inbox.push(&b, Message::text("bo", 1, "hi"));
        let mut p = Panel::new();
        assert_eq!(
            p.key(press(KeyCode::Tab), &mut inbox),
            Outcome::Selected(b.clone())
        );
        assert_eq!(inbox.total_unread(), 0);
        p.key(press(KeyCode::Tab), &mut inbox);
        assert_eq!(
            p.key(press(KeyCode::Tab), &mut inbox),
            Outcome::Selected(ConversationId::new("a"))
        );
        assert_eq!(
            p.key(press(KeyCode::BackTab), &mut inbox),
            Outcome::Selected(ConversationId::new("c"))
        );
        let shift_tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT);
        assert_eq!(p.key(shift_tab, &mut inbox), Outcome::Selected(b));
        let mut empty = Inbox::new("ana", CaseMapping::Ascii);
        assert_eq!(p.key(press(KeyCode::Tab), &mut empty), Outcome::None);
    }

    #[test]
    fn up_enters_list_and_enter_opens_reference() {
        let mut inbox = inbox();
        let a = ConversationId::new("a");
        inbox.push(&a, Message::text("bo", 1, "look at src/app.rs:42:7"));
        inbox.push(&a, Message::text("bo", 2, "no reference here"));
        let mut p = Panel::new();
        p.select(&mut inbox, &a);
        // Up in a non-empty input does nothing.
        typed(&mut p, &mut inbox, "x");
        p.key(press(KeyCode::Up), &mut inbox);
        assert_eq!(p.focus(), Focus::Input);
        p.key(press(KeyCode::Backspace), &mut inbox);
        p.key(press(KeyCode::Up), &mut inbox);
        assert_eq!(p.selected_message(), Some(1));
        assert_eq!(p.key(press(KeyCode::Enter), &mut inbox), Outcome::None);
        p.key(press(KeyCode::Up), &mut inbox);
        let mut want = CodeRef::new("src/app.rs", 42);
        want.column = Some(7);
        assert_eq!(
            p.key(press(KeyCode::Enter), &mut inbox),
            Outcome::Open(want)
        );
        p.key(press(KeyCode::Down), &mut inbox);
        p.key(press(KeyCode::Down), &mut inbox);
        assert_eq!(p.focus(), Focus::Input);
        assert_eq!(p.selected_message(), None);
    }

    #[test]
    fn typing_in_list_goes_to_input_and_enter_submits() {
        let mut inbox = inbox();
        let a = ConversationId::new("a");
        inbox.push(&a, Message::text("bo", 1, "hi"));
        let mut p = Panel::new();
        p.select(&mut inbox, &a);
        p.key(press(KeyCode::Up), &mut inbox);
        typed(&mut p, &mut inbox, "/me waves");
        assert_eq!(p.focus(), Focus::Input);
        assert_eq!(
            p.key(press(KeyCode::Enter), &mut inbox),
            Outcome::Submit(Submit::Command(Command::Me("waves".into())))
        );
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        p.key(ctrl_c, &mut inbox);
        assert_eq!(p.input(), "");
        assert_eq!(p.key(press(KeyCode::Esc), &mut inbox), Outcome::Close);
    }

    #[test]
    fn scroll_and_clamp() {
        let mut inbox = inbox();
        let c = ConversationId::new("c");
        for t in 0..10 {
            inbox.push(&c, Message::text("bo", t, "x"));
        }
        let mut p = Panel::new();
        p.select(&mut inbox, &c);
        p.ensure_visible(&inbox, 4);
        assert_eq!(p.scroll(), 6);
        p.up(&inbox);
        for _ in 0..9 {
            p.up(&inbox);
        }
        p.ensure_visible(&inbox, 4);
        assert_eq!((p.selected_message(), p.scroll()), (Some(0), 0));
        inbox.remove(&c);
        p.clamp(&inbox);
        assert_eq!(p.selected_index(), 1);
        assert_eq!(p.focus(), Focus::Input);
        assert!(!p.select(&mut inbox, &c));
    }
}
