//! The pure core of Vix's IM menu (spec: `spec/instant-message`).
//!
//! Conversations and their messages, presence and notification policy,
//! `@nick` mentions under IRC case mapping, unread tracking in an
//! [`Inbox`], slash-command parsing ([`parse_input`]), code references in
//! message text ([`find_refs`], [`resolve_in_root`]), the received-text
//! sanitizer ([`sanitize()`]), threads anchored to lines ([`Thread`], [`reanchor`]) and
//! JSONL history ([`history`]).
//!
//! Nothing here does I/O, starts a thread or reads the clock: the App owns
//! sockets, files and time, and hands this crate plain values. Nothing here
//! returns user-facing text either; results are enums the App translates.

pub mod command;
pub mod history;
pub mod mention;
pub mod reference;
pub mod sanitize;
pub mod thread;

use std::str::FromStr;

use serde::{Deserialize, Serialize};

pub use command::{Command, CommandError, Input, parse_input};
pub use mention::{CaseMapping, find_mentions, mentions};
pub use reference::{CodeRef, CodeRefMatch, find_refs, parse_ref, resolve_in_root};
pub use sanitize::{sanitize, sanitize_multiline};
pub use thread::{Anchor, Thread, reanchor};

/// What a conversation is with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConversationKind {
    /// The people in the current Cowork session.
    Session,
    /// An IRC channel (later, a Matrix room).
    Room,
    /// One other person.
    Direct,
    /// Local notes to yourself; works with no connection at all.
    Notes,
}

/// A conversation's identity, chosen by the App (for example
/// `irc:libera:#vix`). Two conversations in one [`Inbox`] never share one.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ConversationId(pub String);

impl ConversationId {
    /// Wrap an id.
    #[must_use]
    pub fn new(id: impl Into<String>) -> ConversationId {
        ConversationId(id.into())
    }

    /// The id as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What a message carries besides its body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum MessageKind {
    /// Ordinary text.
    Text,
    /// An action, from `/me` (shown as `* author body`).
    Action,
    /// Shared code: a selection from a file.
    Snippet {
        /// The language id for highlighting (`rust`), possibly empty.
        language: String,
        /// The file, relative to the sender's workspace root.
        path: String,
        /// The first line, 1-based.
        start_line: u32,
        /// The last line, 1-based.
        end_line: u32,
        /// The code itself; may span lines.
        code: String,
    },
    /// A shared cursor position.
    Location {
        /// The file, relative to the sender's workspace root.
        path: String,
        /// The line, 1-based.
        line: u32,
        /// The column, 1-based.
        column: u32,
    },
    /// A join, a leave, an error: from the system, not a person.
    System,
}

/// One message in a conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Who sent it (a nick, a verified Cowork identity, or empty for
    /// [`MessageKind::System`]).
    pub author: String,
    /// When it was sent, in milliseconds since the Unix epoch, UTC. The App
    /// shows it in the user's locale and time zone.
    pub time_utc: i64,
    /// The text.
    pub body: String,
    /// What else it carries.
    pub kind: MessageKind,
}

impl Message {
    /// A [`MessageKind::Text`] message.
    #[must_use]
    pub fn text(author: impl Into<String>, time_utc: i64, body: impl Into<String>) -> Message {
        Message {
            author: author.into(),
            time_utc,
            body: body.into(),
            kind: MessageKind::Text,
        }
    }

    /// A [`MessageKind::Action`] message.
    #[must_use]
    pub fn action(author: impl Into<String>, time_utc: i64, body: impl Into<String>) -> Message {
        Message {
            kind: MessageKind::Action,
            ..Message::text(author, time_utc, body)
        }
    }

    /// A [`MessageKind::System`] message.
    #[must_use]
    pub fn system(time_utc: i64, body: impl Into<String>) -> Message {
        Message {
            kind: MessageKind::System,
            ..Message::text(String::new(), time_utc, body)
        }
    }

    /// Sanitize every received string in place: author, body, and the
    /// snippet's or location's fields (a snippet's code keeps its lines).
    pub fn sanitize(&mut self) {
        self.author = sanitize(&self.author);
        self.body = sanitize(&self.body);
        match &mut self.kind {
            MessageKind::Snippet {
                language,
                path,
                code,
                ..
            } => {
                *language = sanitize(language);
                *path = sanitize(path);
                *code = sanitize_multiline(code);
            }
            MessageKind::Location { path, .. } => *path = sanitize(path),
            MessageKind::Text | MessageKind::Action | MessageKind::System => {}
        }
    }

    /// The code reference Enter should open on this message: a location's
    /// or snippet's own place, else the first reference in the body.
    #[must_use]
    pub fn code_ref(&self) -> Option<CodeRef> {
        match &self.kind {
            MessageKind::Location { path, line, column } => Some(CodeRef {
                path: path.clone(),
                line: *line,
                column: Some(*column),
                end_line: None,
            }),
            MessageKind::Snippet {
                path,
                start_line,
                end_line,
                ..
            } => Some(CodeRef {
                path: path.clone(),
                line: *start_line,
                column: None,
                end_line: (end_line > start_line).then_some(*end_line),
            }),
            MessageKind::Text | MessageKind::Action | MessageKind::System => find_refs(&self.body)
                .into_iter()
                .next()
                .map(|m| m.reference),
        }
    }
}

/// Your availability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Presence {
    /// Around and happy to be notified.
    #[default]
    Available,
    /// Away from the keyboard; notifications still count.
    Away,
    /// Do not disturb: no notifications at all.
    Busy,
}

impl Presence {
    /// Parse `available`, `away` or `busy` (any ASCII case).
    #[must_use]
    pub fn parse(text: &str) -> Option<Presence> {
        match text.to_ascii_lowercase().as_str() {
            "available" => Some(Presence::Available),
            "away" => Some(Presence::Away),
            "busy" => Some(Presence::Busy),
            _ => None,
        }
    }

    /// The setting value (`available`, `away`, `busy`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Presence::Available => "available",
            Presence::Away => "away",
            Presence::Busy => "busy",
        }
    }
}

/// Which messages notify (raise the status-bar marker).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum NotifyPolicy {
    /// Every unread message.
    All,
    /// Only mentions of you and direct messages (the default).
    #[default]
    Mentions,
    /// Nothing.
    Off,
}

impl NotifyPolicy {
    /// The setting value (`all`, `mentions`, `off`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            NotifyPolicy::All => "all",
            NotifyPolicy::Mentions => "mentions",
            NotifyPolicy::Off => "off",
        }
    }
}

impl FromStr for NotifyPolicy {
    type Err = ();

    /// Parse `all`, `mentions` or `off` (any ASCII case).
    fn from_str(text: &str) -> Result<NotifyPolicy, ()> {
        match text.to_ascii_lowercase().as_str() {
            "all" => Ok(NotifyPolicy::All),
            "mentions" => Ok(NotifyPolicy::Mentions),
            "off" => Ok(NotifyPolicy::Off),
            _ => Err(()),
        }
    }
}

/// Whether a new unread message should notify. Busy silences everything;
/// otherwise the policy decides. Notifying never opens or focuses anything:
/// it only raises the status-bar marker.
#[must_use]
pub fn should_notify(policy: NotifyPolicy, presence: Presence, is_mention: bool) -> bool {
    match (presence, policy) {
        (Presence::Busy, _) | (_, NotifyPolicy::Off) => false,
        (_, NotifyPolicy::All) => true,
        (_, NotifyPolicy::Mentions) => is_mention,
    }
}

/// A named stream of messages with its unread state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversation {
    /// Its identity.
    pub id: ConversationId,
    /// What it is with.
    pub kind: ConversationKind,
    /// The name to show (`#vix`, a nick, a session name).
    pub title: String,
    /// The messages, oldest first.
    pub messages: Vec<Message>,
    /// How many messages arrived since it was last read.
    pub unread: usize,
    /// The index of the first unread message, if any.
    pub first_unread: Option<usize>,
    /// Whether an unread message mentions you.
    pub mentioned: bool,
    /// The room topic, if it has one.
    pub topic: Option<String>,
}

impl Conversation {
    /// An empty conversation.
    #[must_use]
    pub fn new(
        id: ConversationId,
        kind: ConversationKind,
        title: impl Into<String>,
    ) -> Conversation {
        Conversation {
            id,
            kind,
            title: sanitize(&title.into()),
            messages: Vec::new(),
            unread: 0,
            first_unread: None,
            mentioned: false,
            topic: None,
        }
    }

    /// Set (sanitized) or clear the topic.
    pub fn set_topic(&mut self, topic: Option<&str>) {
        self.topic = topic.map(sanitize);
    }

    /// Clear the unread count, marker and mention.
    pub fn mark_read(&mut self) {
        self.unread = 0;
        self.first_unread = None;
        self.mentioned = false;
    }

    /// Remove every message (the `/clear` command, Clear History).
    pub fn clear(&mut self) {
        self.messages.clear();
        self.mark_read();
    }
}

/// What [`Inbox::push`] did with a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Delivery {
    /// No conversation has that id; nothing was stored.
    NoConversation,
    /// The author is blocked; nothing was stored.
    Blocked,
    /// Stored, and not unread: your own message, or a system message.
    Read,
    /// Stored and counted as unread.
    Unread {
        /// It mentions you (or is a direct message to you).
        mention: bool,
    },
}

impl Delivery {
    /// Whether this delivery should notify, under `policy` and `presence`.
    #[must_use]
    pub fn notifies(self, policy: NotifyPolicy, presence: Presence) -> bool {
        match self {
            Delivery::Unread { mention } => should_notify(policy, presence, mention),
            Delivery::NoConversation | Delivery::Blocked | Delivery::Read => false,
        }
    }
}

/// The default cap on stored messages per conversation.
pub const DEFAULT_MESSAGE_LIMIT: usize = 5_000;

/// Every conversation, with who you are and who you have blocked.
#[derive(Debug, Clone)]
pub struct Inbox {
    conversations: Vec<Conversation>,
    nick: String,
    mapping: CaseMapping,
    blocked: Vec<String>,
    limit: usize,
}

impl Inbox {
    /// An empty inbox for `nick`, comparing nicks under `mapping`.
    #[must_use]
    pub fn new(nick: impl Into<String>, mapping: CaseMapping) -> Inbox {
        Inbox {
            conversations: Vec::new(),
            nick: nick.into(),
            mapping,
            blocked: Vec::new(),
            limit: DEFAULT_MESSAGE_LIMIT,
        }
    }

    /// Your nick.
    #[must_use]
    pub fn nick(&self) -> &str {
        &self.nick
    }

    /// Change your nick (after `/nick` is accepted).
    pub fn set_nick(&mut self, nick: impl Into<String>) {
        self.nick = nick.into();
    }

    /// The case mapping nicks compare under.
    #[must_use]
    pub fn case_mapping(&self) -> CaseMapping {
        self.mapping
    }

    /// Use the case mapping a server declared.
    pub fn set_case_mapping(&mut self, mapping: CaseMapping) {
        self.mapping = mapping;
    }

    /// Replace the blocked-nick list.
    pub fn set_blocked<I, S>(&mut self, nicks: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.blocked = nicks.into_iter().map(Into::into).collect();
    }

    /// Whether `nick` is blocked (under the case mapping).
    #[must_use]
    pub fn is_blocked(&self, nick: &str) -> bool {
        self.blocked.iter().any(|b| self.mapping.nick_eq(b, nick))
    }

    /// Cap stored messages per conversation (at least 1); the oldest go
    /// first.
    pub fn set_message_limit(&mut self, limit: usize) {
        self.limit = limit.max(1);
    }

    /// Add a conversation. `false` (and nothing changes) if one with the
    /// same id is already here.
    pub fn add(&mut self, conversation: Conversation) -> bool {
        if self.get(&conversation.id).is_some() {
            return false;
        }
        self.conversations.push(conversation);
        true
    }

    /// Remove a conversation, returning it.
    pub fn remove(&mut self, id: &ConversationId) -> Option<Conversation> {
        let at = self.index_of(id)?;
        Some(self.conversations.remove(at))
    }

    /// Every conversation, in the order added.
    #[must_use]
    pub fn conversations(&self) -> &[Conversation] {
        &self.conversations
    }

    /// The position of a conversation.
    #[must_use]
    pub fn index_of(&self, id: &ConversationId) -> Option<usize> {
        self.conversations.iter().position(|c| &c.id == id)
    }

    /// A conversation by id.
    #[must_use]
    pub fn get(&self, id: &ConversationId) -> Option<&Conversation> {
        self.conversations.iter().find(|c| &c.id == id)
    }

    /// A conversation by id, to change.
    pub fn get_mut(&mut self, id: &ConversationId) -> Option<&mut Conversation> {
        self.conversations.iter_mut().find(|c| &c.id == id)
    }

    /// Store a message in conversation `id`.
    ///
    /// The message is sanitized first. A blocked author's message is
    /// dropped. Your own messages and system messages are stored as read;
    /// anything else raises the unread count (setting the first-unread
    /// marker if it was clear) and is a mention when it `@`-mentions your
    /// nick or the conversation is [`ConversationKind::Direct`].
    pub fn push(&mut self, id: &ConversationId, mut message: Message) -> Delivery {
        message.sanitize();
        if self.is_blocked(&message.author) {
            return Delivery::Blocked;
        }
        let own = self.mapping.nick_eq(&message.author, &self.nick);
        let mention = !own
            && match message.kind {
                MessageKind::Text | MessageKind::Action => {
                    mentions(&message.body, &self.nick, self.mapping)
                }
                _ => false,
            };
        let system = matches!(message.kind, MessageKind::System);
        let limit = self.limit;
        let Some(conversation) = self.get_mut(id) else {
            return Delivery::NoConversation;
        };
        let mention = mention || (!own && !system && conversation.kind == ConversationKind::Direct);
        conversation.messages.push(message);
        let overflow = conversation.messages.len().saturating_sub(limit);
        if overflow > 0 {
            conversation.messages.drain(..overflow);
            conversation.first_unread = conversation
                .first_unread
                .map(|i| i.saturating_sub(overflow));
        }
        if own || system {
            // Answering in a conversation reads it.
            if own {
                conversation.mark_read();
            }
            return Delivery::Read;
        }
        conversation.unread += 1;
        conversation
            .first_unread
            .get_or_insert(conversation.messages.len() - 1);
        conversation.mentioned |= mention;
        Delivery::Unread { mention }
    }

    /// Mark conversation `id` read. `false` if there is no such
    /// conversation.
    pub fn mark_read(&mut self, id: &ConversationId) -> bool {
        self.get_mut(id).map(Conversation::mark_read).is_some()
    }

    /// The next conversation with unread messages after `after` (wrapping
    /// around, and coming back to `after` itself last), or from the start
    /// when `after` is `None` or unknown. Conversations with a mention come
    /// before those without.
    #[must_use]
    pub fn next_unread(&self, after: Option<&ConversationId>) -> Option<&ConversationId> {
        let len = self.conversations.len();
        let start = after.and_then(|id| self.index_of(id)).map_or(0, |i| i + 1);
        let order = || (0..len).map(|k| &self.conversations[(start + k) % len]);
        order()
            .find(|c| c.unread > 0 && c.mentioned)
            .or_else(|| order().find(|c| c.unread > 0))
            .map(|c| &c.id)
    }

    /// The unread count over every conversation.
    #[must_use]
    pub fn total_unread(&self) -> usize {
        self.conversations.iter().map(|c| c.unread).sum()
    }

    /// Whether any unread message mentions you.
    #[must_use]
    pub fn has_mention(&self) -> bool {
        self.conversations.iter().any(|c| c.mentioned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> ConversationId {
        ConversationId::new(s)
    }

    fn inbox() -> Inbox {
        let mut inbox = Inbox::new("Ana", CaseMapping::Rfc1459);
        inbox.add(Conversation::new(
            id("room"),
            ConversationKind::Room,
            "#vix",
        ));
        inbox.add(Conversation::new(id("dm"), ConversationKind::Direct, "bo"));
        inbox.add(Conversation::new(
            id("notes"),
            ConversationKind::Notes,
            "Notes",
        ));
        inbox
    }

    #[test]
    fn notify_rules() {
        use NotifyPolicy::{All, Mentions, Off};
        use Presence::{Available, Away, Busy};
        assert!(should_notify(All, Available, false));
        assert!(should_notify(All, Away, false));
        assert!(should_notify(Mentions, Available, true));
        assert!(!should_notify(Mentions, Available, false));
        assert!(!should_notify(Off, Available, true));
        for policy in [All, Mentions, Off] {
            assert!(!should_notify(policy, Busy, true));
        }
    }

    #[test]
    fn settings_values() {
        for p in [NotifyPolicy::All, NotifyPolicy::Mentions, NotifyPolicy::Off] {
            assert_eq!(p.as_str().parse(), Ok(p));
        }
        assert!("loud".parse::<NotifyPolicy>().is_err());
        for p in [Presence::Available, Presence::Away, Presence::Busy] {
            assert_eq!(Presence::parse(p.as_str()), Some(p));
        }
        assert_eq!(NotifyPolicy::default(), NotifyPolicy::Mentions);
    }

    #[test]
    fn duplicate_ids_rejected() {
        let mut inbox = inbox();
        assert!(!inbox.add(Conversation::new(id("room"), ConversationKind::Room, "x")));
        assert_eq!(inbox.conversations().len(), 3);
        assert!(inbox.remove(&id("notes")).is_some());
        assert!(inbox.get(&id("notes")).is_none());
    }

    #[test]
    fn unread_and_mentions() {
        let mut inbox = inbox();
        let room = id("room");
        assert_eq!(
            inbox.push(&room, Message::text("bo", 1, "hi all")),
            Delivery::Unread { mention: false }
        );
        assert_eq!(
            inbox.push(&room, Message::text("cy", 2, "@ana look at a.rs:3")),
            Delivery::Unread { mention: true }
        );
        let c = inbox.get(&room).unwrap();
        assert_eq!((c.unread, c.first_unread, c.mentioned), (2, Some(0), true));
        assert_eq!(inbox.total_unread(), 2);
        assert!(inbox.has_mention());
        assert!(inbox.mark_read(&room));
        assert_eq!(inbox.total_unread(), 0);
        assert!(!inbox.has_mention());
        assert!(!inbox.mark_read(&id("nope")));
    }

    #[test]
    fn own_and_system_messages_are_read() {
        let mut inbox = inbox();
        let room = id("room");
        inbox.push(&room, Message::text("bo", 1, "hi"));
        assert_eq!(
            inbox.push(&room, Message::system(2, "cy joined")),
            Delivery::Read
        );
        assert_eq!(inbox.get(&room).unwrap().unread, 1);
        // Your own reply (nick compared under the case mapping) reads it.
        assert_eq!(
            inbox.push(&room, Message::text("ANA", 3, "@ana me")),
            Delivery::Read
        );
        assert_eq!(inbox.get(&room).unwrap().unread, 0);
    }

    #[test]
    fn direct_messages_are_mentions() {
        let mut inbox = inbox();
        assert_eq!(
            inbox.push(&id("dm"), Message::text("bo", 1, "psst")),
            Delivery::Unread { mention: true }
        );
        assert!(
            Delivery::Unread { mention: true }.notifies(NotifyPolicy::Mentions, Presence::Away)
        );
        assert!(!Delivery::Read.notifies(NotifyPolicy::All, Presence::Available));
    }

    #[test]
    fn blocked_and_missing() {
        let mut inbox = inbox();
        inbox.set_blocked(["Spam[mer]"]);
        assert!(inbox.is_blocked("spam{mer}"));
        assert_eq!(
            inbox.push(&id("room"), Message::text("SPAM{MER}", 1, "@ana buy")),
            Delivery::Blocked
        );
        assert_eq!(inbox.get(&id("room")).unwrap().messages.len(), 0);
        assert_eq!(
            inbox.push(&id("gone"), Message::text("bo", 1, "x")),
            Delivery::NoConversation
        );
    }

    #[test]
    fn pushed_text_is_sanitized() {
        let mut inbox = inbox();
        inbox.push(
            &id("room"),
            Message::text("b\u{1b}]0;x\u{7}o", 1, "a\u{202e}b\tc"),
        );
        let m = &inbox.get(&id("room")).unwrap().messages[0];
        assert_eq!((m.author.as_str(), m.body.as_str()), ("bo", "ab    c"));
    }

    #[test]
    fn limit_keeps_first_unread_in_step() {
        let mut inbox = inbox();
        inbox.set_message_limit(3);
        let room = id("room");
        inbox.push(&room, Message::text("ana", 0, "mine"));
        for t in 1..=4 {
            inbox.push(&room, Message::text("bo", t, "x"));
        }
        let c = inbox.get(&room).unwrap();
        assert_eq!(c.messages.len(), 3);
        assert_eq!(c.first_unread, Some(0));
        assert_eq!(c.messages[0].time_utc, 2);
    }

    #[test]
    fn next_unread_prefers_mentions_and_wraps() {
        let mut inbox = inbox();
        assert_eq!(inbox.next_unread(None), None);
        inbox.push(&id("room"), Message::text("bo", 1, "hi"));
        inbox.push(&id("notes"), Message::text("bo", 1, "hi"));
        assert_eq!(inbox.next_unread(None), Some(&id("room")));
        assert_eq!(inbox.next_unread(Some(&id("room"))), Some(&id("notes")));
        assert_eq!(inbox.next_unread(Some(&id("notes"))), Some(&id("room")));
        inbox.push(&id("dm"), Message::text("bo", 1, "psst"));
        assert_eq!(inbox.next_unread(Some(&id("notes"))), Some(&id("dm")));
    }

    #[test]
    fn message_code_refs() {
        assert_eq!(
            Message::text("a", 0, "see x/a.rs:3 and b.rs:4").code_ref(),
            Some(CodeRef::new("x/a.rs", 3))
        );
        let loc = Message {
            kind: MessageKind::Location {
                path: "a.rs".into(),
                line: 2,
                column: 5,
            },
            ..Message::text("a", 0, "")
        };
        assert_eq!(loc.code_ref().unwrap().to_string(), "a.rs:2:5");
        assert_eq!(Message::system(0, "hi").code_ref(), None);
    }

    #[test]
    fn conversation_topic_and_clear() {
        let mut c = Conversation::new(id("r"), ConversationKind::Room, "#v\u{1b}[1mix");
        assert_eq!(c.title, "#vix");
        c.set_topic(Some("rel\u{2066}ease"));
        assert_eq!(c.topic.as_deref(), Some("release"));
        c.messages.push(Message::text("a", 0, "x"));
        c.unread = 1;
        c.clear();
        assert!(c.messages.is_empty() && c.unread == 0);
    }
}
