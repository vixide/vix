//! The **IM** menu (T752, `spec/instant-message`): code-aware chat.
//!
//! The pure pieces live in `vix-im` (conversations, messages, the sanitizer,
//! code references, slash commands, history) and `vix-im-panel` (the dock
//! panel's state). This module is the App side and is in every build: the
//! inbox with its always-present **Notes** conversation, the menu actions,
//! the panel's keys, sharing a selection or location, the status-bar
//! segment and disk history. Everything that needs a socket -- connecting,
//! sending to IRC, the event pump -- lives in `im_net.rs` behind the `im`
//! Cargo feature, with `im_off.rs` no-op stand-ins, so the calls here carry
//! no `cfg`.
//!
//! IM never opens, focuses or pops anything on its own (the spec's "never
//! interrupt" rule): a new message raises the unread count in the status bar
//! and, when the notify policy allows, a status-line note. Only the user's
//! own actions open the panel.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr as _;

use vix_im::{
    CaseMapping, CodeRef, Command, CommandError, Conversation, ConversationId, ConversationKind,
    Delivery, Inbox, Message, MessageKind, NotifyPolicy, Presence,
};
use vix_im_panel::{Outcome, Panel, Submit};
use vix_input::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use super::{App, Focus, Prompt, PromptKind, rect_contains};

#[cfg(feature = "im")]
pub(crate) use super::im_net::Link;
#[cfg(not(feature = "im"))]
pub(crate) use super::im_off::Link;

/// The id of the local Notes conversation, present in every build.
pub(crate) const NOTES: &str = "notes";

/// The most lines a shared snippet may carry; a larger selection is refused
/// rather than flooding a room.
pub(crate) const MAX_SNIPPET_LINES: usize = 50;

/// Where a network conversation lives: its account and, for a room or a
/// direct conversation, the IRC target (`#vix`, `bob`). The account's own
/// server conversation has no target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Route {
    /// The `[[im.accounts]]` name.
    pub(crate) account: String,
    /// The channel or nick, or `None` for the server conversation.
    pub(crate) target: Option<String>,
}

/// The IM state the App owns.
pub(crate) struct ImState {
    /// Every conversation; always holds Notes.
    pub(crate) inbox: Inbox,
    /// The dock panel's selection, scroll and input line.
    pub(crate) panel: Panel,
    /// Whether the dock panel is shown.
    pub(crate) open: bool,
    /// Your presence. Busy silences notifications; it is never sent.
    pub(crate) presence: Presence,
    /// Open IRC connections (an uninhabited type without the `im` feature).
    pub(crate) links: Vec<Link>,
    /// Account and target of each network conversation.
    pub(crate) routes: HashMap<ConversationId, Route>,
    /// `(conversation, folded nick)` pairs the server vouched for with an
    /// `account` tag; every other nick in a room shows as unverified.
    pub(crate) verified: HashSet<(ConversationId, String)>,
    /// Where JSONL history goes with `history = "disk"`, else `None`.
    pub(crate) history_dir: Option<PathBuf>,
    /// The first conversation-list row on screen (set when drawn), so a
    /// click maps to the right conversation.
    pub(crate) list_top: usize,
    /// The workspace's anchored threads (T753), loaded on first use.
    pub(crate) threads: super::im_threads::ThreadStore,
}

/// A small IM dialog over the editor: pick an account, confirm a share or a
/// history wipe, or list the anchored threads. Keys go here before anything
/// else while it is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ImDialog {
    /// **Connect…** with several accounts: which one.
    Accounts {
        /// Account names, in settings order.
        names: Vec<String>,
        /// The highlighted row.
        selected: usize,
    },
    /// **Share Selection**: exactly what will be sent, and to whom.
    Share {
        /// The conversation it goes to.
        to: ConversationId,
        /// That conversation's title, as shown.
        title: String,
        /// The snippet message.
        message: Message,
    },
    /// **Threads…**: the workspace's anchored threads (T753).
    Threads {
        /// Thread ids, sorted by path and line.
        ids: Vec<String>,
        /// The highlighted row.
        selected: usize,
    },
    /// **Clear History**: wipe this conversation's messages.
    Clear {
        /// The conversation.
        id: ConversationId,
        /// Its title, as shown.
        title: String,
    },
}

/// Milliseconds since the Unix epoch, UTC.
pub(crate) fn now_ms() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}

/// `true` for a character IRC allows in a nick.
fn nick_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "-_[]\\`^{}|".contains(c)
}

/// The nick to use: the `nick` setting, else the OS user name, cleaned to
/// IRC's nick characters (a leading digit or `-` is dropped); `vix` when
/// nothing usable is left.
pub(crate) fn default_nick(setting: &str) -> String {
    let raw = if setting.trim().is_empty() {
        std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_default()
    } else {
        setting.trim().to_string()
    };
    let cleaned: String = raw
        .chars()
        .filter(|c| nick_char(*c))
        .skip_while(|c| c.is_ascii_digit() || *c == '-')
        .take(30)
        .collect();
    if cleaned.is_empty() {
        "vix".to_string()
    } else {
        cleaned
    }
}

/// A history file name for a conversation id: ASCII letters, digits, `-`,
/// `_` and `.` kept, every other byte written as `%XX`, so no two ids share
/// a file and none can name a path outside the history directory.
pub(crate) fn history_file_name(id: &ConversationId) -> String {
    let mut out = String::new();
    for b in id.as_str().bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_') || (b == b'.' && !out.is_empty()) {
            out.push(char::from(b));
        } else {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 0xF)]));
        }
    }
    format!("{out}.jsonl")
}

/// A conversation's id for an IRC target on `account`: the target folded
/// under RFC 1459 case mapping, so `#Vix` and `#vix` are one room.
pub(crate) fn irc_id(account: &str, target: Option<&str>) -> ConversationId {
    match target {
        Some(t) => ConversationId::new(format!("irc/{account}/{}", CaseMapping::Rfc1459.fold(t))),
        None => ConversationId::new(format!("irc/{account}")),
    }
}

/// Whether an IRC target names a channel rather than a nick.
pub(crate) fn is_channel(target: &str) -> bool {
    target.starts_with(['#', '&', '+', '!'])
}

impl ImState {
    /// A fresh state from the settings: the inbox with Notes, blocked nicks,
    /// and disk history loaded when `history = "disk"`.
    pub(crate) fn new(settings: &crate::settings::Settings) -> ImState {
        let mut inbox = Inbox::new(default_nick(&settings.im.nick), CaseMapping::Rfc1459);
        inbox.set_blocked(settings.im.blocked.iter().cloned());
        inbox.add(Conversation::new(
            ConversationId::new(NOTES),
            ConversationKind::Notes,
            "Notes",
        ));
        let history_dir = (settings.im.history == "disk")
            .then(|| crate::settings::Settings::data_dir().map(|d| d.join("im")))
            .flatten();
        let mut state = ImState {
            inbox,
            panel: Panel::new(),
            open: false,
            presence: Presence::Available,
            links: Vec::new(),
            routes: HashMap::new(),
            verified: HashSet::new(),
            history_dir,
            list_top: 0,
            threads: super::im_threads::ThreadStore::default(),
        };
        state.load_history(&ConversationId::new(NOTES), settings.im.history_days);
        state
    }

    /// Read a conversation's history file (if disk history is on), drop
    /// messages older than `days`, and put the rest in the conversation as
    /// already read. A file that lost messages to pruning is rewritten.
    pub(crate) fn load_history(&mut self, id: &ConversationId, days: u32) {
        let Some(path) = self.history_path(id) else {
            return;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        let mut messages = vix_im::history::decode(&text);
        let pruned = vix_im::history::prune(&mut messages, now_ms(), days);
        let excess = messages.len().saturating_sub(vix_im::DEFAULT_MESSAGE_LIMIT);
        messages.drain(..excess);
        if pruned > 0 || excess > 0 {
            write_history(&path, &messages);
        }
        if let Some(c) = self.inbox.get_mut(id) {
            c.messages = messages;
            c.mark_read();
        }
    }

    /// The JSONL file for a conversation, when disk history is on.
    pub(crate) fn history_path(&self, id: &ConversationId) -> Option<PathBuf> {
        self.history_dir
            .as_ref()
            .map(|d| d.join(history_file_name(id)))
    }

    /// Append a conversation's newest message to its history file.
    fn append_history(&self, id: &ConversationId) {
        use std::io::Write as _;
        let (Some(path), Some(message)) = (
            self.history_path(id),
            self.inbox.get(id).and_then(|c| c.messages.last()),
        ) else {
            return;
        };
        let Ok(line) = vix_im::history::encode_line(message) else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let fresh = !path.exists();
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(f, "{line}");
            if fresh {
                crate::fileops::restrict_to_owner(&path);
            }
        }
    }

    /// The selected conversation's id.
    pub(crate) fn selected_id(&self) -> Option<ConversationId> {
        self.panel.selected_id(&self.inbox).cloned()
    }

    /// A conversation's title as shown: Notes is translated, the rest are
    /// their (sanitized) names.
    pub(crate) fn title(&self, id: &ConversationId) -> String {
        match self.inbox.get(id) {
            Some(c) if c.kind == ConversationKind::Notes => t!("ui.im_notes").to_string(),
            Some(c) => c.title.clone(),
            None => String::new(),
        }
    }

    /// Whether `author` in conversation `id` should be marked unverified: a
    /// nick in a network conversation the server never vouched for, and not
    /// you.
    pub(crate) fn unverified(&self, id: &ConversationId, author: &str) -> bool {
        self.routes.contains_key(id)
            && !author.is_empty()
            && !self.inbox.case_mapping().nick_eq(author, self.inbox.nick())
            && !self
                .verified
                .contains(&(id.clone(), self.inbox.case_mapping().fold(author)))
    }
}

/// Rewrite a history file with exactly `messages`.
fn write_history(path: &Path, messages: &[Message]) {
    if let Ok(text) = vix_im::history::encode(messages) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if std::fs::write(path, text).is_ok() {
            crate::fileops::restrict_to_owner(path);
        }
    }
}

impl App {
    /// Dispatch an `im.*` action. Returns `true` if `action` was handled.
    pub(super) fn run_im_action(&mut self, action: &str) -> bool {
        match action {
            "im.panel" => self.im_toggle_panel(),
            "im.connect" => self.im_connect(),
            "im.disconnect" => self.im_disconnect(),
            "im.join" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::ImJoin,
                    t!("prompt.im_join").to_string(),
                ));
            }
            "im.leave" => self.im_leave(None, None),
            "im.direct" => {
                self.prompt = Some(Prompt::new(
                    PromptKind::ImDirect,
                    t!("prompt.im_direct").to_string(),
                ));
            }
            "im.next_unread" => self.im_next_unread(),
            "im.share_selection" => self.im_share_selection(),
            "im.share_location" => self.im_share_location(),
            "im.status_available" => self.im_set_presence(Presence::Available),
            "im.status_away" => self.im_set_presence(Presence::Away),
            "im.status_busy" => self.im_set_presence(Presence::Busy),
            "im.notify_all" => self.im_set_notify(NotifyPolicy::All),
            "im.notify_mentions" => self.im_set_notify(NotifyPolicy::Mentions),
            "im.notify_off" => self.im_set_notify(NotifyPolicy::Off),
            "im.clear_history" => self.im_ask_clear(),
            _ => return self.run_im_thread_action(action),
        }
        true
    }

    /// **IM → Chat Panel**: show the panel and give it the keyboard, or hide
    /// it and give the keyboard back to the editor.
    fn im_toggle_panel(&mut self) {
        if self.im.open && self.focus == Focus::Im {
            self.im.open = false;
            self.focus = Focus::Editor;
        } else {
            self.im_show_panel();
        }
    }

    /// Show the panel focused on its input (a user action, never automatic).
    pub(super) fn im_show_panel(&mut self) {
        self.im.open = true;
        self.focus = Focus::Im;
        if let Some(id) = self.im.selected_id() {
            self.im.inbox.mark_read(&id);
        }
    }

    /// The configured notify policy (`mentions` for anything unknown).
    pub(crate) fn im_notify_policy(&self) -> NotifyPolicy {
        NotifyPolicy::from_str(&self.settings.im.notify).unwrap_or_default()
    }

    /// **IM → Status**: presence is local; Busy silences notifications.
    fn im_set_presence(&mut self, presence: Presence) {
        self.im.presence = presence;
        let label = match presence {
            Presence::Away => t!("menu.item.im.status_away"),
            Presence::Busy => t!("menu.item.im.status_busy"),
            _ => t!("menu.item.im.status_available"),
        };
        self.status = t!("status.im_presence", status = label).to_string();
    }

    /// **IM → Notifications**.
    fn im_set_notify(&mut self, policy: NotifyPolicy) {
        self.settings.im.notify = policy.as_str().to_string();
        let label = match policy {
            NotifyPolicy::All => t!("menu.item.im.notify_all"),
            NotifyPolicy::Off => t!("menu.item.im.notify_off"),
            _ => t!("menu.item.im.notify_mentions"),
        };
        self.status = t!("status.im_notify", policy = label).to_string();
    }

    /// **IM → Next Unread**: select the next conversation with unread
    /// messages (mentions first) and show it.
    fn im_next_unread(&mut self) {
        let current = self.im.selected_id();
        let Some(next) = self.im.inbox.next_unread(current.as_ref()).cloned() else {
            self.status = t!("status.im_no_unread").to_string();
            return;
        };
        self.im.panel.select(&mut self.im.inbox, &next);
        self.im_show_panel();
    }

    /// Store a message in a conversation, write it to disk history, and
    /// notify when the policy allows. A message landing in the conversation
    /// on screen is read at once.
    pub(crate) fn im_deliver(&mut self, id: &ConversationId, message: Message) -> Delivery {
        let delivery = self.im.inbox.push(id, message);
        if !matches!(delivery, Delivery::Read | Delivery::Unread { .. }) {
            return delivery;
        }
        self.im.append_history(id);
        let on_screen = self.im.open && self.im.selected_id().as_ref() == Some(id);
        if on_screen {
            self.im.inbox.mark_read(id);
        } else if delivery.notifies(self.im_notify_policy(), self.im.presence) {
            let name = self.im.title(id);
            let key = if delivery == (Delivery::Unread { mention: true }) {
                "status.im_mention"
            } else {
                "status.im_new_message"
            };
            self.status = t!(key, name = name).to_string();
        }
        delivery
    }

    /// Add a conversation if it is new (loading its disk history), and
    /// record its route. Returns its id.
    pub(crate) fn im_ensure_conversation(
        &mut self,
        id: ConversationId,
        kind: ConversationKind,
        title: &str,
        route: Option<Route>,
    ) -> ConversationId {
        if self
            .im
            .inbox
            .add(Conversation::new(id.clone(), kind, title))
        {
            self.im.load_history(&id, self.settings.im.history_days);
        }
        if let Some(route) = route {
            self.im.routes.insert(id.clone(), route);
        }
        id
    }

    /// The account to act on: the selected conversation's, else the only
    /// open connection's.
    pub(crate) fn im_active_account(&self) -> Option<String> {
        if let Some(route) = self.im.selected_id().and_then(|id| self.im.routes.get(&id)) {
            return Some(route.account.clone());
        }
        let names = self.im_link_names();
        (names.len() == 1).then(|| names[0].clone())
    }

    /// The selected conversation's IRC route with a target (a room or a
    /// direct conversation), if it has one.
    fn im_selected_target(&self) -> Option<(ConversationId, Route)> {
        let id = self.im.selected_id()?;
        let route = self.im.routes.get(&id)?;
        route.target.as_ref()?;
        Some((id, route.clone()))
    }

    /// **IM → Leave Room** (or `/part`): leave `room` on the active account,
    /// or the selected room.
    fn im_leave(&mut self, room: Option<String>, reason: Option<String>) {
        let target = match room {
            Some(r) => self.im_active_account().map(|a| (a, r)),
            None => self
                .im_selected_target()
                .and_then(|(_, r)| r.target.filter(|t| is_channel(t)).map(|t| (r.account, t))),
        };
        let Some((account, channel)) = target else {
            self.status = t!("status.im_not_a_room").to_string();
            return;
        };
        if !self.im_send(&account, vix_im_irc::Command::Part { channel, reason }) {
            self.status = t!("status.im_not_connected").to_string();
        }
    }

    /// The Join Room prompt's answer, or `/join`.
    pub(super) fn im_join(&mut self, room: &str) {
        let room = room.trim();
        if room.is_empty() {
            return;
        }
        let room = if is_channel(room) {
            room.to_string()
        } else {
            format!("#{room}")
        };
        let Some(account) = self.im_active_account() else {
            self.status = t!("status.im_not_connected").to_string();
            return;
        };
        if !self.im_send(&account, vix_im_irc::Command::Join(room)) {
            self.status = t!("status.im_not_connected").to_string();
        }
    }

    /// The Direct Message prompt's answer: open (or select) the direct
    /// conversation with `nick` and show the panel. `false` when there is
    /// no connection to message through.
    pub(super) fn im_direct(&mut self, nick: &str) -> bool {
        let nick = nick.trim();
        if nick.is_empty() {
            return false;
        }
        let Some(account) = self.im_active_account() else {
            self.status = t!("status.im_not_connected").to_string();
            return false;
        };
        let id = self.im_ensure_conversation(
            irc_id(&account, Some(nick)),
            ConversationKind::Direct,
            nick,
            Some(Route {
                account,
                target: Some(nick.to_string()),
            }),
        );
        self.im.panel.select(&mut self.im.inbox, &id);
        self.im_show_panel();
        true
    }

    /// Send `text` to the selected conversation: stored locally in Notes,
    /// sent over IRC for a room or direct conversation.
    fn im_send_text(&mut self, text: &str, action: bool) {
        let Some(id) = self.im.selected_id() else {
            return;
        };
        if let Some(route) = self.im.routes.get(&id).cloned() {
            let Some(target) = route.target else {
                self.status = t!("status.im_not_a_room").to_string();
                return;
            };
            let command = if action {
                vix_im_irc::Command::Action {
                    target,
                    text: text.to_string(),
                }
            } else {
                vix_im_irc::Command::Privmsg {
                    target,
                    text: text.to_string(),
                }
            };
            if !self.im_send(&route.account, command) {
                self.status = t!("status.im_not_connected").to_string();
                return;
            }
        }
        let nick = self.im.inbox.nick().to_string();
        let message = if action {
            Message::action(nick, now_ms(), text)
        } else {
            Message::text(nick, now_ms(), text)
        };
        self.im_deliver(&id, message);
    }

    /// Carry out a submitted input line.
    fn im_submit(&mut self, submit: Submit) {
        match submit {
            Submit::Send(text) => self.im_send_text(&text, false),
            Submit::Command(command) => self.im_command(command),
            Submit::Error(error) => {
                self.status = match error {
                    CommandError::Unknown(name) => t!("status.im_unknown_command", name = name),
                    CommandError::MissingArgument(command) => {
                        t!("status.im_missing_argument", command = command)
                    }
                    CommandError::InvalidStatus(value) => {
                        t!("status.im_invalid_status", value = value)
                    }
                    _ => t!("status.im_unknown_command", name = ""),
                }
                .to_string();
            }
            _ => {}
        }
    }

    /// Run a slash command.
    fn im_command(&mut self, command: Command) {
        match command {
            Command::Me(text) => self.im_send_text(&text, true),
            Command::Join { room, .. } => self.im_join(&room),
            Command::Part { room, reason } => self.im_leave(room, reason),
            Command::Msg { target, text } => {
                if self.im_direct(&target) {
                    self.im_send_text(&text, false);
                }
            }
            Command::Nick(nick) => self.im_nick(&nick),
            Command::Topic(topic) => self.im_topic(topic),
            Command::Status(presence) => self.im_set_presence(presence),
            Command::Clear => {
                if let Some(id) = self.im.selected_id()
                    && let Some(c) = self.im.inbox.get_mut(&id)
                {
                    c.clear();
                }
            }
            _ => {}
        }
    }

    /// `/nick NEW`: ask the server (the inbox follows when it agrees), or,
    /// with no connection, just change the local nick.
    fn im_nick(&mut self, nick: &str) {
        match self.im_active_account() {
            Some(account) => {
                if !self.im_send(&account, vix_im_irc::Command::Nick(nick.to_string())) {
                    self.status = t!("status.im_not_connected").to_string();
                }
            }
            None => self.im.inbox.set_nick(default_nick(nick)),
        }
    }

    /// `/topic [TEXT]` in the selected room.
    fn im_topic(&mut self, topic: Option<String>) {
        let Some((_, route)) = self
            .im_selected_target()
            .filter(|(_, r)| r.target.as_deref().is_some_and(is_channel))
        else {
            self.status = t!("status.im_not_a_room").to_string();
            return;
        };
        let channel = route.target.unwrap_or_default();
        if !self.im_send(
            &route.account,
            vix_im_irc::Command::Topic { channel, topic },
        ) {
            self.status = t!("status.im_not_connected").to_string();
        }
    }

    /// Open a message's code reference -- only inside the workspace root, only
    /// an existing file, never running anything.
    fn im_open_ref(&mut self, reference: &CodeRef) {
        let Some(path) = self.im_inside_root(&reference.path) else {
            self.status =
                t!("status.im_ref_outside", reference = reference.to_string()).to_string();
            return;
        };
        let (line, column) = (
            reference.line as usize,
            reference.column.map(|c| c as usize),
        );
        self.with_jump(|s| {
            s.open_path(&path, false);
            let area = s.editor_view();
            s.editor.goto(line, column, area);
            s.focus = Focus::Editor;
        });
    }

    /// A path for sharing: relative to the workspace root when inside it.
    fn im_share_path(&self, path: &Path) -> String {
        let relative = path
            .strip_prefix(&self.root)
            .ok()
            .map(Path::to_path_buf)
            .or_else(|| {
                let (file, root) = (path.canonicalize().ok()?, self.root.canonicalize().ok()?);
                file.strip_prefix(root).ok().map(Path::to_path_buf)
            });
        relative
            .as_deref()
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    /// **IM → Share Selection**: build the snippet and show exactly what will
    /// be sent, and to whom, before anything is sent.
    fn im_share_selection(&mut self) {
        let found = self.editor.active_tab().and_then(|t| {
            let (s, e) = t.editor.selection_span()?;
            if s == e || t.is_image() {
                return None;
            }
            let code = t.editor.code_ref();
            let start = code.char_to_line(s) + 1;
            let end = code.char_to_line(e.saturating_sub(1).max(s)) + 1;
            let path = t.path.clone()?;
            Some((
                path,
                start,
                end,
                t.editor.char_text(s, e),
                t.editor.language().to_string(),
            ))
        });
        let Some((path, start, end, code, language)) = found else {
            self.status = t!("status.im_no_selection").to_string();
            return;
        };
        if end + 1 - start > MAX_SNIPPET_LINES {
            self.status = t!("status.im_snippet_too_long", max = MAX_SNIPPET_LINES).to_string();
            return;
        }
        let Some(to) = self.im.selected_id() else {
            return;
        };
        let path = self.im_share_path(&path);
        let (start, end) = (
            u32::try_from(start).unwrap_or(u32::MAX),
            u32::try_from(end).unwrap_or(u32::MAX),
        );
        let reference = CodeRef {
            path: path.clone(),
            line: start,
            column: None,
            end_line: (end > start).then_some(end),
        };
        let mut message = Message::text(self.im.inbox.nick(), now_ms(), reference.to_string());
        message.kind = MessageKind::Snippet {
            language,
            path,
            start_line: start,
            end_line: end,
            code: code.trim_end_matches('\n').to_string(),
        };
        let title = self.im.title(&to);
        self.im_dialog = Some(ImDialog::Share { to, title, message });
    }

    /// Send a confirmed snippet.
    fn im_send_snippet(&mut self, to: &ConversationId, message: Message) {
        if let Some(route) = self.im.routes.get(to).cloned() {
            let (Some(target), MessageKind::Snippet { code, .. }) = (route.target, &message.kind)
            else {
                self.status = t!("status.im_not_a_room").to_string();
                return;
            };
            let text = format!("{}\n{code}", message.body);
            if !self.im_send(
                &route.account,
                vix_im_irc::Command::Privmsg { target, text },
            ) {
                self.status = t!("status.im_not_connected").to_string();
                return;
            }
        }
        let name = self.im.title(to);
        self.im_deliver(to, message);
        self.status = t!("status.im_sent", name = name).to_string();
    }

    /// **IM → Share Location**: send `path:line:col` of the cursor.
    fn im_share_location(&mut self) {
        let Some(path) = self.active_path() else {
            self.status = t!("status.im_no_file").to_string();
            return;
        };
        let Some(to) = self.im.selected_id() else {
            return;
        };
        let (line, column) = self.editor.cursor_1based();
        let (line, column) = (
            u32::try_from(line).unwrap_or(u32::MAX),
            u32::try_from(column).unwrap_or(u32::MAX),
        );
        let path = self.im_share_path(&path);
        let body = format!("{path}:{line}:{column}");
        if let Some(route) = self.im.routes.get(&to).cloned() {
            let Some(target) = route.target else {
                self.status = t!("status.im_not_a_room").to_string();
                return;
            };
            let command = vix_im_irc::Command::Privmsg {
                target,
                text: body.clone(),
            };
            if !self.im_send(&route.account, command) {
                self.status = t!("status.im_not_connected").to_string();
                return;
            }
        }
        let mut message = Message::text(self.im.inbox.nick(), now_ms(), body);
        message.kind = MessageKind::Location { path, line, column };
        let name = self.im.title(&to);
        self.im_deliver(&to, message);
        self.status = t!("status.im_sent", name = name).to_string();
    }

    /// **IM → Clear History**: ask first.
    fn im_ask_clear(&mut self) {
        let Some(id) = self.im.selected_id() else {
            return;
        };
        let title = self.im.title(&id);
        self.im_dialog = Some(ImDialog::Clear { id, title });
    }

    /// Wipe a conversation's messages and its history file.
    fn im_clear(&mut self, id: &ConversationId) {
        if let Some(c) = self.im.inbox.get_mut(id) {
            c.clear();
        }
        if let Some(path) = self.im.history_path(id) {
            let _ = std::fs::remove_file(path);
        }
        let name = self.im.title(id);
        self.status = t!("status.im_history_cleared", name = name).to_string();
    }

    /// A key while an IM dialog is open.
    pub(super) fn im_dialog_key(&mut self, key: KeyEvent) {
        let Some(dialog) = self.im_dialog.take() else {
            return;
        };
        let yes = matches!(key.code, KeyCode::Enter | KeyCode::Char('y' | 'Y'));
        let no = matches!(key.code, KeyCode::Esc | KeyCode::Char('n' | 'N'));
        match dialog {
            ImDialog::Accounts { names, selected } => match key.code {
                KeyCode::Up => {
                    self.im_dialog = Some(ImDialog::Accounts {
                        selected: selected.saturating_sub(1),
                        names,
                    });
                }
                KeyCode::Down => {
                    let selected = (selected + 1).min(names.len().saturating_sub(1));
                    self.im_dialog = Some(ImDialog::Accounts { names, selected });
                }
                KeyCode::Enter => self.im_connect_account(selected),
                KeyCode::Esc => {}
                _ => self.im_dialog = Some(ImDialog::Accounts { names, selected }),
            },
            ImDialog::Threads { ids, selected } => {
                self.im_dialog = self.im_threads_key(key, ids, selected);
            }
            ImDialog::Share { to, message, .. } if yes => self.im_send_snippet(&to, message),
            ImDialog::Clear { id, .. } if yes => self.im_clear(&id),
            _ if no => self.status = t!("status.im_cancelled").to_string(),
            other => self.im_dialog = Some(other),
        }
    }

    /// **IM → Connect…**: no accounts says how to add one; one connects;
    /// several open the account picker.
    fn im_connect(&mut self) {
        let names: Vec<String> = self
            .settings
            .im
            .accounts
            .iter()
            .map(|a| a.name.clone())
            .collect();
        match names.len() {
            0 => self.status = t!("status.im_no_accounts").to_string(),
            1 => self.im_connect_account(0),
            _ => self.im_dialog = Some(ImDialog::Accounts { names, selected: 0 }),
        }
    }

    /// A key while the panel has focus. Chords with Ctrl or Alt, and the
    /// function keys, are left to the keymap (menus, palette, quit), so they
    /// work from the panel too; returns `false` for those.
    pub(super) fn im_key(&mut self, key: KeyEvent) -> bool {
        if !self.im.open {
            self.focus = Focus::Editor;
            return false;
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
            || matches!(key.code, KeyCode::F(_))
        {
            return false;
        }
        let outcome = self.im.panel.key(key, &mut self.im.inbox);
        match outcome {
            Outcome::Submit(submit) => self.im_submit(submit),
            Outcome::Open(reference) => self.im_open_ref(&reference),
            Outcome::Close => self.focus = Focus::Editor,
            _ => {}
        }
        true
    }

    /// Text pasted while the panel has focus goes into the input line as one
    /// line (never sent on its own).
    pub(super) fn im_paste(&mut self, text: &str) {
        self.im.panel.insert_str(text);
    }

    /// A click in the dock: focus the panel; a click on a conversation row
    /// selects it. The wheel moves through the messages.
    pub(super) fn im_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.focus = Focus::Im;
                let list = self.layout.im_list;
                if rect_contains(list, mouse.column, mouse.row) {
                    let index = self.im.list_top + usize::from(mouse.row - list.y);
                    if let Some(id) = self
                        .im
                        .inbox
                        .conversations()
                        .get(index)
                        .map(|c| c.id.clone())
                    {
                        self.im.panel.select(&mut self.im.inbox, &id);
                    }
                }
            }
            MouseEventKind::ScrollUp => self.im.panel.up(&self.im.inbox),
            MouseEventKind::ScrollDown => self.im.panel.down(&self.im.inbox),
            _ => {}
        }
    }

    /// Drain every connection's events into the inbox. Called from
    /// [`App::tick`].
    pub fn poll_im(&mut self) {
        self.im_poll_links();
    }

    /// The status bar's IM segment: the unread total and `@` for a mention,
    /// as text (not colour alone). Empty when nothing is unread.
    #[must_use]
    pub fn im_status_segment(&self) -> String {
        let total = self.im.inbox.total_unread();
        if total == 0 {
            return String::new();
        }
        let mention = if self.im.inbox.has_mention() {
            " @"
        } else {
            ""
        };
        format!("IM {}{mention}  ", t!("ui.im_unread", count = total))
    }

    /// Turn disk history on at `dir` and load Notes' history from it, so a
    /// test never touches the real data directory.
    #[cfg(test)]
    pub(crate) fn im_use_history_dir(&mut self, dir: PathBuf) {
        self.im.history_dir = Some(dir);
        let notes = ConversationId::new(NOTES);
        self.im.load_history(&notes, self.settings.im.history_days);
    }
}
