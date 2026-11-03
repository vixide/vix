//! Cowork (T695, spec/cowork): host or join a real-time shared session.
//!
//! Everything here is glue. The rules live in `vix-cowork` (documents,
//! permissions, session state machines), the encryption and invite checks in
//! `vix-cowork-net`, and the buffer plumbing in `vix_editor_core::remote`.
//! This file only (a) turns user actions into those calls, (b) once per
//! event-loop tick pumps local edits into the session, then applies remote
//! edits to the buffers, and (c) draws nothing itself: presence reaches the
//! renderer through each tab's remote carets.
//!
//! Order matters in [`App::poll_cowork`]: local edits are fed to the CRDT
//! *before* remote edits are applied, because the editor's edit log holds
//! offsets in the buffer as it was, and the buffer and the document are
//! identical between ticks.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::Duration;

use vix_cowork::presence::{Follow, PresenceBoard, Rgb};
use vix_cowork::wire::Message;
use vix_cowork::{
    BufferEdit, ChatKind, ChatLine, DEFAULT_CAP_BYTES, ErrorCode, GuestEvent, GuestSession,
    HostSession, Outbound, PeerId, Permission, SharedPath,
};
use vix_cowork_net::{Invite, Joined, Knock, Link, LinkEvent, Listener, NetError};
use vix_editor_core::remote::{RemoteCaret, Splice};
use vix_im::{ConversationId, ConversationKind, MessageKind};

use super::im::SESSION;
use super::{App, Confirm, ConfirmKind, KeyCode, KeyEvent, Prompt, PromptKind};
use crate::editor::REMOTE_PATH_PREFIX;

/// How long an invite stays valid.
const INVITE_TTL: Duration = Duration::from_mins(10);
/// How long a guest waits to connect.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a guest waits for the host to approve it.
const CONSENT_TIMEOUT: Duration = Duration::from_mins(2);
/// How long the host reuses its file list before walking the project again,
/// so a guest repeating List cannot keep the host's UI thread busy.
const FILE_LIST_REUSE: Duration = Duration::from_secs(5);
/// Most buffers a host shares at once; further guest open requests are
/// refused, so a guest cannot make the host open every file in the project.
const MAX_SHARED_BUFFERS: usize = 32;

/// The host half of a session: the listener that accepts guests, the `vix-cowork` session
/// state, one network link per admitted guest, and the address advertised in invites.
struct Host {
    listener: Listener,
    session: HostSession,
    links: BTreeMap<PeerId, Link>,
    advertise: String,
}

/// The guest half of a session: the `vix-cowork` guest state and the single link to the host.
struct Guest {
    session: GuestSession,
    link: Link,
}

/// Cowork state held by [`App`] (all of it absent unless a session is open).
pub(crate) struct State {
    host: Option<Host>,
    guest: Option<Guest>,
    /// An authenticated guest awaiting the host's answer to the consent prompt.
    knock: Option<Knock>,
    /// A join in progress on a background thread.
    joining: Option<Receiver<Result<Joined, NetError>>>,
    tx: Sender<(PeerId, LinkEvent)>,
    rx: Receiver<(PeerId, LinkEvent)>,
    /// The last cursor sent per shared buffer, to send only changes.
    last_cursor: BTreeMap<SharedPath, (usize, Option<usize>)>,
    /// The participant whose caret the view follows, if any (T693).
    follow: Follow,
    /// When the host last walked the project for a guest's file list (T696).
    listed_at: Option<std::time::Instant>,
    /// Files this guest asked the host for: they take the focus when they
    /// open; files shared for someone else open in the background.
    asked: std::collections::BTreeSet<SharedPath>,
    /// Host (T697): the shared terminal's screen as guests last saw it, so
    /// each tick sends only what changed since.
    term_sent: Option<vt100::Screen>,
    /// Host (T631): which terminal tab is shared — the one active when the
    /// host confirmed sharing. Switching tabs never changes it; closing it
    /// ends the share.
    term_id: Option<u64>,
    /// Guest (T697): this guest's copy of the host's shared terminal.
    term_mirror: Option<vt100::Parser>,
    /// Guest (T697): whether the shared-terminal overlay is open (it then
    /// takes every key, like the integrated terminal).
    term_view: bool,
}

impl Default for State {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            host: None,
            guest: None,
            knock: None,
            joining: None,
            tx,
            rx,
            last_cursor: BTreeMap::new(),
            follow: Follow::default(),
            listed_at: None,
            asked: std::collections::BTreeSet::new(),
            term_sent: None,
            term_id: None,
            term_mirror: None,
            term_view: false,
        }
    }
}

/// Resolve a guest-named path on disk (T696): it must exist as a regular
/// file whose *canonical* path (symlinks followed) is still inside the root
/// and still not private. Returns the path as spelled under the root.
fn confine(root: &std::path::Path, rel: &SharedPath) -> Option<PathBuf> {
    let auth = crate::authority::current();
    let real_root = auth.canonicalize(root).ok()?;
    let spelled = rel.under(root);
    let resolved = auth.canonicalize(&spelled).ok()?;
    let inside = resolved.strip_prefix(&real_root).ok()?;
    let real_rel = SharedPath::parse(&inside.to_string_lossy()).ok()?;
    (auth.is_file(&resolved) && !real_rel.is_private()).then_some(spelled)
}

/// The shared path behind a shared-in (`cowork:`) tab.
fn remote_rel(tab: &crate::editor::Tab) -> Option<SharedPath> {
    let p = tab
        .path
        .as_ref()?
        .to_str()?
        .strip_prefix(REMOTE_PATH_PREFIX)?;
    SharedPath::parse(p).ok()
}

/// Convert the CRDT's `BufferEdit`s into the editor core's `Splice` form (same
/// offset/delete/insert triple) so they can be applied to a buffer.
fn splices(edits: &[BufferEdit]) -> Vec<Splice> {
    edits
        .iter()
        .map(|e| Splice {
            offset: e.offset,
            delete: e.delete,
            insert: e.insert.clone(),
        })
        .collect()
}

/// An IM message as a session-chat line (T756). A location or snippet whose
/// path is not a clean root-relative path goes as plain text (its body
/// already names the place); system lines are never sent.
fn to_chat_line(message: vix_im::Message) -> Option<ChatLine> {
    let place = |p: &str| SharedPath::parse(p).ok();
    let kind = match message.kind {
        MessageKind::Text => ChatKind::Text,
        MessageKind::Action => ChatKind::Action,
        MessageKind::Location { path, line, column } => match place(&path) {
            Some(path) => ChatKind::Location { path, line, column },
            None => ChatKind::Text,
        },
        MessageKind::Snippet {
            language,
            path,
            start_line,
            end_line,
            code,
        } => match place(&path) {
            Some(path) => ChatKind::Snippet {
                language,
                path,
                start_line,
                end_line,
                code,
            },
            None => ChatKind::Text,
        },
        _ => return None,
    };
    Some(ChatLine {
        from: PeerId::HOST,
        time_utc: message.time_utc,
        body: message.body,
        kind,
    })
}

/// A session-chat line as an IM message by `author`. The inbox sanitizes it
/// when it is stored.
fn from_chat_line(line: ChatLine, author: String) -> vix_im::Message {
    let kind = match line.kind {
        ChatKind::Action => MessageKind::Action,
        ChatKind::Location { path, line, column } => MessageKind::Location {
            path: path.as_str().to_string(),
            line,
            column,
        },
        ChatKind::Snippet {
            language,
            path,
            start_line,
            end_line,
            code,
        } => MessageKind::Snippet {
            language,
            path: path.as_str().to_string(),
            start_line,
            end_line,
            code,
        },
        _ => MessageKind::Text,
    };
    vix_im::Message {
        author,
        time_utc: line.time_utc,
        body: line.body,
        kind,
    }
}

/// One participant of the Cowork session, as the IM panel lists them above
/// the session conversation (T759).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RosterEntry {
    /// The verified identity: the name the host confirmed and the number it
    /// gave (`ana #2`).
    pub(crate) who: String,
    /// What else is true of them, in this order.
    pub(crate) tags: Vec<RosterTag>,
    /// Where their cursor is.
    pub(crate) place: RosterPlace,
}

/// A fact about a roster participant, written out as a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RosterTag {
    /// This is you.
    You,
    /// This is the host.
    Host,
    /// The host muted this guest in the chat.
    Muted,
    /// Your view follows this participant.
    Following,
}

/// Where a roster participant's cursor is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RosterPlace {
    /// Not in any shared buffer that we know of.
    Nowhere,
    /// In the session notes (T759).
    Notes,
    /// In this shared file, relative to the host's root.
    File(String),
}

/// One roster row in reading order, every state written out as text:
/// `ana #2 (you) (muted) -- src/a.rs`.
fn roster_row(entry: &RosterEntry) -> String {
    let mut row = entry.who.clone();
    for tag in &entry.tags {
        let word = match tag {
            RosterTag::You => t!("ui.cowork_roster_you"),
            RosterTag::Host => t!("ui.cowork_roster_host"),
            RosterTag::Muted => t!("ui.cowork_roster_muted"),
            RosterTag::Following => t!("ui.cowork_roster_following"),
        };
        row.push_str(" (");
        row.push_str(&word);
        row.push(')');
    }
    row.push_str(" -- ");
    match &entry.place {
        RosterPlace::Notes => row.push_str(&t!("ui.cowork_roster_notes")),
        RosterPlace::File(file) => row.push_str(file),
        RosterPlace::Nowhere => row.push_str(&t!("ui.cowork_roster_nowhere")),
    }
    row
}

/// The IM conversation id of the session chat (`SESSION`), which doubles as the Cowork chat
/// room.
fn session_id() -> ConversationId {
    ConversationId::new(SESSION)
}

/// The word shown to users for a permission level; any level beyond view/edit is shown as
/// `terminal`.
fn level_name(p: Permission) -> &'static str {
    match p {
        Permission::View => "view",
        Permission::Edit => "edit",
        _ => "terminal",
    }
}

/// Parse a permission word typed at a prompt (`view`, `edit`, `terminal`, case-insensitive)
/// into a [`Permission`]; anything else is `None`.
fn parse_level(word: &str) -> Option<Permission> {
    match word.trim().to_ascii_lowercase().as_str() {
        "view" => Some(Permission::View),
        "edit" => Some(Permission::Edit),
        "terminal" => Some(Permission::EditTerminal),
        _ => None,
    }
}

/// The name shown to other participants: `$USER`, else `$USERNAME`, else `guest`.
fn display_name() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "guest".to_string())
}

impl App {
    /// Dispatch the Cowork actions. Returns `true` if handled.
    pub(super) fn run_cowork_action(&mut self, action: &str) -> bool {
        match action {
            "cowork.host" => self.cowork_begin_host(),
            "cowork.invite" => self.cowork_invite(),
            "cowork.share" => self.cowork_share(),
            "cowork.unshare" => self.cowork_unshare(),
            "cowork.join" => self.cowork_begin_join(),
            "cowork.peers" => self.cowork_peers(),
            "cowork.permissions" => self.cowork_begin_permissions(),
            "cowork.leave" => self.cowork_leave(),
            "cowork.follow" => self.cowork_begin_follow(),
            "cowork.share_workspace" => self.cowork_toggle_workspace(),
            "cowork.files" => self.cowork_files(),
            "cowork.share_terminal" => self.cowork_toggle_terminal_share(),
            "cowork.terminal_input" => self.cowork_toggle_terminal_input(),
            "cowork.terminal" => self.cowork_toggle_terminal_view(),
            "cowork.notes" => self.cowork_open_notes(),
            "cowork.mute" => self.cowork_begin_host_prompt(PromptKind::CoworkMute),
            "cowork.remove" => self.cowork_begin_host_prompt(PromptKind::CoworkRemove),
            _ => return false,
        }
        true
    }

    /// Report a Cowork failure as an error message.
    fn cowork_error(&mut self, error: impl std::fmt::Display) {
        self.messages
            .error(t!("msg.cowork_error", error = error.to_string()).to_string());
    }

    /// Whether a session is hosted, joined, or a join is still connecting; a second host/join
    /// request is then answered with the peer list instead.
    fn cowork_busy(&self) -> bool {
        self.cowork.host.is_some() || self.cowork.guest.is_some() || self.cowork.joining.is_some()
    }

    // ----- user actions -----------------------------------------------------

    /// Start hosting: ask for the address to bind (default `127.0.0.1:0`, an OS-chosen port).
    /// If a session is already open, show its peers instead.
    fn cowork_begin_host(&mut self) {
        if self.cowork_busy() {
            self.cowork_peers();
            return;
        }
        self.prompt = Some(
            Prompt::new(PromptKind::CoworkHost, t!("prompt.cowork_host").to_string())
                .with_input("127.0.0.1:0".to_string()),
        );
    }

    /// Bind the listener and open the host session. `input` is `bind` or `bind=advertise`,
    /// where the optional `advertise` address is what invites carry (for NAT or port-forwarded
    /// setups). Opens the session chat and shows a first invite.
    fn cowork_start_host(&mut self, input: &str) {
        let (bind, advertise) = match input.trim().split_once('=') {
            Some((b, a)) => (b.trim(), Some(a.trim().to_string())),
            None => (input.trim(), None),
        };
        let Ok(addr) = bind.parse::<SocketAddr>() else {
            self.cowork_error(NetError::BadInvite);
            return;
        };
        let listener = match Listener::bind(addr) {
            Ok(l) => l,
            Err(e) => {
                self.cowork_error(e);
                return;
            }
        };
        let advertise = advertise.unwrap_or_else(|| listener.local_addr().to_string());
        self.status = t!("status.cowork_hosting", addr = advertise).to_string();
        self.cowork.host = Some(Host {
            listener,
            session: HostSession::new(&display_name(), DEFAULT_CAP_BYTES),
            links: BTreeMap::new(),
            advertise,
        });
        self.cowork_chat_open();
        self.cowork_invite();
    }

    /// Create a fresh invite (valid for `INVITE_TTL`) and show it in a text dialog; reports
    /// when this instance is not hosting.
    fn cowork_invite(&mut self) {
        let Some(host) = self.cowork.host.as_ref() else {
            self.status = t!("status.cowork_not_hosting").to_string();
            return;
        };
        match host.listener.new_invite(&host.advertise, INVITE_TTL) {
            Ok(invite) => self.open_text_dialog(
                t!("menu.item.cowork.invite").to_string(),
                &invite.to_string(),
            ),
            Err(e) => self.cowork_error(e),
        }
    }

    /// `path` relative to the project root, as a validated [`SharedPath`].
    /// Tries the paths as given, then their canonical forms (a temp or
    /// symlinked root may be spelled two ways).
    fn cowork_rel(&self, path: &std::path::Path) -> Option<SharedPath> {
        let rel = path
            .strip_prefix(&self.root)
            .map(PathBuf::from)
            .or_else(|_| {
                let auth = crate::authority::current();
                let (p, r) = (auth.canonicalize(path)?, auth.canonicalize(&self.root)?);
                p.strip_prefix(&r)
                    .map(PathBuf::from)
                    .map_err(|_| std::io::Error::other("outside the root"))
            });
        SharedPath::parse(&rel.ok()?.to_string_lossy()).ok()
    }

    /// The path of the active tab relative to the project root, if it can be
    /// shared (a saved, non-remote file inside the root).
    fn cowork_active_rel(&self) -> Option<SharedPath> {
        let tab = self.editor.active_tab()?;
        if tab.is_remote() || tab.is_image() {
            return None;
        }
        self.cowork_rel(tab.path.as_ref()?)
    }

    /// Share the active file with the guests: seed a CRDT document from its text and start
    /// logging the buffer's edits so they can be pumped to guests. Refuses unsaved, remote,
    /// image or private files.
    fn cowork_share(&mut self) {
        if self.cowork.host.is_none() {
            self.status = t!("status.cowork_not_hosting").to_string();
            return;
        }
        let Some(rel) = self.cowork_active_rel() else {
            self.status = t!("status.cowork_unsaved").to_string();
            return;
        };
        if rel.is_private() {
            self.status = t!("status.cowork_private", path = rel.as_str()).to_string();
            return;
        }
        let Some(text) = self.editor.active_tab().map(crate::editor::Tab::text) else {
            return;
        };
        let result = self
            .cowork
            .host
            .as_mut()
            .map(|h| h.session.share(&rel, &text));
        match result {
            Some(Ok(out)) => {
                if let Some(tab) = self.editor.tabs.get_mut(self.editor.active) {
                    tab.editor.start_edit_log();
                }
                self.cowork_send(&out);
                self.status = t!("status.cowork_shared", path = rel.as_str()).to_string();
            }
            Some(Err(e)) => self.cowork_error(e),
            None => {}
        }
    }

    /// Stop sharing the active file.
    fn cowork_unshare(&mut self) {
        let Some(rel) = self.cowork_active_rel() else {
            return;
        };
        self.cowork_unshare_path(&rel);
    }

    /// Stop sharing `rel`: tell the guests, stop its edit log and forget its last-sent cursor.
    fn cowork_unshare_path(&mut self, rel: &SharedPath) {
        let out = self.cowork.host.as_mut().map(|h| h.session.unshare(rel));
        let Some(out) = out else {
            return;
        };
        if let Some(i) = self.cowork_host_tab(rel)
            && let Some(tab) = self.editor.tabs.get_mut(i)
        {
            tab.editor.stop_edit_log();
        }
        self.cowork.last_cursor.remove(rel);
        self.cowork_send(&out);
        self.status = t!("status.cowork_unshared", path = rel.as_str()).to_string();
    }

    /// Start joining: ask for an invite string. If a session is already open, show its peers
    /// instead.
    fn cowork_begin_join(&mut self) {
        if self.cowork_busy() {
            self.cowork_peers();
            return;
        }
        self.prompt = Some(Prompt::new(
            PromptKind::CoworkJoin,
            t!("prompt.cowork_join").to_string(),
        ));
    }

    /// Parse the invite and connect on a background thread (the handshake and the host's
    /// consent wait can take minutes); the result arrives on `State::joining` and is picked up
    /// by `cowork_poll_join`.
    fn cowork_start_join(&mut self, input: &str) {
        let invite = match input.parse::<Invite>() {
            Ok(i) => i,
            Err(e) => {
                self.cowork_error(e);
                return;
            }
        };
        let name = display_name();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(vix_cowork_net::join(
                &invite,
                &name,
                CONNECT_TIMEOUT,
                CONSENT_TIMEOUT,
            ));
        });
        self.cowork.joining = Some(rx);
    }

    /// Show the session's participants (with the host's view of each guest's permission) in a
    /// text dialog.
    fn cowork_peers(&mut self) {
        let mut lines = Vec::new();
        if let Some(h) = self.cowork.host.as_ref() {
            for (id, name) in h.session.board().peers() {
                let level = h
                    .session
                    .guest(id)
                    .map_or("host", |g| level_name(g.permission));
                lines.push(format!("{} {name} ({level})", id.0));
            }
        } else if let Some(g) = self.cowork.guest.as_ref() {
            for (id, name) in g.session.board().peers() {
                lines.push(format!("{} {name}", id.0));
            }
        } else {
            self.status = t!("status.cowork_inactive").to_string();
            return;
        }
        self.open_text_dialog(t!("menu.item.cowork.peers").to_string(), &lines.join("\n"));
    }

    /// Host only: ask which guest to change and to what level (`<id> view|edit|terminal`).
    fn cowork_begin_permissions(&mut self) {
        if self.cowork.host.is_none() {
            self.status = t!("status.cowork_not_hosting").to_string();
            return;
        }
        self.prompt = Some(Prompt::new(
            PromptKind::CoworkPermission,
            t!("prompt.cowork_permission").to_string(),
        ));
    }

    /// Apply a permission change typed as `<peer id> <level>` and tell the affected guest; an
    /// unknown guest or level only sets a status message.
    fn cowork_set_permission(&mut self, input: &str) {
        let mut words = input.split_whitespace();
        let id = words.next().and_then(|w| w.parse::<u64>().ok()).map(PeerId);
        let level = words.next().and_then(parse_level);
        let (Some(id), Some(level)) = (id, level) else {
            self.status = t!("status.cowork_no_guest").to_string();
            return;
        };
        let Some(host) = self.cowork.host.as_mut() else {
            return;
        };
        let Some(name) = host.session.guest(id).map(|g| g.name.clone()) else {
            self.status = t!("status.cowork_no_guest").to_string();
            return;
        };
        let out = host.session.set_permission(id, level);
        self.cowork_send(&out);
        self.status = t!(
            "status.cowork_permission_set",
            name = name,
            level = level_name(level)
        )
        .to_string();
    }

    /// Leave or end the session. A host says goodbye to every guest and closes the listener; a
    /// guest says goodbye to the host. Either way shared buffers stop logging edits and shared-
    /// in ones become ordinary local copies; presence, follow, terminal and chat state are
    /// cleared.
    fn cowork_leave(&mut self) {
        if let Some(host) = self.cowork.host.take() {
            for link in host.links.values() {
                let _ = link.send(&Message::Leave);
                link.close();
            }
            host.listener.close();
            for tab in &mut self.editor.tabs {
                tab.editor.stop_edit_log();
                tab.editor.set_remote_carets(Vec::new());
            }
            // The session notes become an ordinary unsaved buffer.
            self.cowork_release_guest_tabs();
        } else if let Some(guest) = self.cowork.guest.take() {
            let _ = guest.link.send(&Message::Leave);
            guest.link.close();
            self.cowork_release_guest_tabs();
        } else {
            self.status = t!("status.cowork_inactive").to_string();
            return;
        }
        self.cowork.knock = None;
        self.cowork.last_cursor.clear();
        self.cowork.follow.stop();
        self.cowork_forget_terminal();
        self.cowork_chat_close();
        self.status = t!("status.cowork_left").to_string();
    }

    /// Turn every shared-in buffer into an ordinary local, untitled copy.
    fn cowork_release_guest_tabs(&mut self) {
        for tab in &mut self.editor.tabs {
            if tab.is_remote() {
                tab.path = None;
                tab.read_only = false;
                tab.dirty = true;
                tab.editor.stop_edit_log();
                tab.editor.set_remote_carets(Vec::new());
            }
        }
    }

    // ----- prompts ------------------------------------------------------------

    /// Answer one of the Cowork prompts.
    pub(super) fn accept_cowork_prompt(&mut self, kind: PromptKind, input: &str) {
        match kind {
            PromptKind::CoworkHost => self.cowork_start_host(input),
            PromptKind::CoworkJoin => self.cowork_start_join(input),
            PromptKind::CoworkPermission => self.cowork_set_permission(input),
            PromptKind::CoworkConsent => self.cowork_answer_consent(input),
            PromptKind::CoworkFollow => self.cowork_set_follow(input),
            PromptKind::CoworkMute => self.cowork_toggle_mute(input),
            PromptKind::CoworkRemove => self.cowork_remove(input),
            _ => {}
        }
    }

    /// Host: answer the consent prompt for a knocking guest. `input` is the permission level to
    /// grant; anything else declines. On success the guest is admitted, its link gets a reader
    /// thread feeding `State::rx`, and everyone is told.
    fn cowork_answer_consent(&mut self, input: &str) {
        let Some(knock) = self.cowork.knock.take() else {
            return;
        };
        let Some(level) = parse_level(input) else {
            knock.decline(vix_cowork::ErrorCode::Declined);
            return;
        };
        let admitted = self
            .cowork
            .host
            .as_mut()
            .map(|h| h.session.admit(&knock.name, level));
        let Some(admitted) = admitted else {
            knock.decline(vix_cowork::ErrorCode::Declined);
            return;
        };
        match admitted {
            Ok((peer, out)) => {
                let (link, reader) = knock.approve();
                let _reader = reader.spawn(peer.id, self.cowork.tx.clone());
                if let Some(h) = self.cowork.host.as_mut() {
                    h.links.insert(peer.id, link);
                }
                self.cowork_send(&out);
                self.cowork_terminal_for(peer.id);
                let who = self.cowork_author(peer.id);
                self.cowork_chat_system(&t!("msg.im_joined", nick = who));
                self.status = t!("status.cowork_peer_joined", name = peer.name).to_string();
            }
            Err(e) => {
                knock.decline(vix_cowork::ErrorCode::Declined);
                self.cowork_error(e);
            }
        }
    }

    // ----- transport ------------------------------------------------------------

    /// Send host-session output to the guests it names.
    fn cowork_send(&mut self, out: &[Outbound]) {
        let Some(host) = self.cowork.host.as_ref() else {
            return;
        };
        for o in out {
            if let Some(link) = host.links.get(&o.to)
                && link.send(&o.msg).is_err()
            {
                link.close();
            }
        }
    }

    /// The host-side tab index for a shared path: the real file's tab (also matched through a
    /// differently spelled but equivalent path), or the `cowork:` tab for the session notes.
    fn cowork_host_tab(&self, rel: &SharedPath) -> Option<usize> {
        if rel.is_session_notes() {
            // The notes are not a file: the host's tab is a `cowork:` one.
            return self.cowork_guest_tab(rel);
        }
        let abs = rel.under(&self.root);
        self.editor
            .tabs
            .iter()
            .position(|t| t.path.as_deref() == Some(abs.as_path()))
            .or_else(|| {
                // Same file spelled differently (symlinked or temp roots).
                self.editor.tabs.iter().position(|t| {
                    !t.is_remote()
                        && t.path
                            .as_deref()
                            .is_some_and(|p| self.cowork_rel(p).as_ref() == Some(rel))
                })
            })
    }

    /// The tab index of the shared-in (`cowork:`) buffer for `rel`, if open.
    fn cowork_guest_tab(&self, rel: &SharedPath) -> Option<usize> {
        let want = PathBuf::from(format!("{REMOTE_PATH_PREFIX}{}", rel.as_str()));
        self.editor
            .tabs
            .iter()
            .position(|t| t.path.as_deref() == Some(want.as_path()))
    }

    /// Whether a session is open or opening (keeps the event loop polling fast).
    #[must_use]
    pub fn cowork_active(&self) -> bool {
        self.cowork_busy()
    }

    /// Names of the other participants whose cursor is in the active buffer,
    /// formatted for the status bar; empty when there are none.
    #[must_use]
    pub fn cowork_segment(&self) -> String {
        let Some(tab) = self.editor.active_tab() else {
            return String::new();
        };
        let (rel, board, me) = if let Some(h) = self.cowork.host.as_ref() {
            (
                self.cowork_active_host_rel(),
                h.session.board(),
                PeerId::HOST,
            )
        } else if let Some(g) = self.cowork.guest.as_ref() {
            let rel = tab
                .path
                .as_ref()
                .and_then(|p| p.to_str())
                .and_then(|p| p.strip_prefix(REMOTE_PATH_PREFIX))
                .and_then(|p| SharedPath::parse(p).ok());
            (rel, g.session.board(), g.session.me())
        } else {
            return String::new();
        };
        let Some(rel) = rel else {
            return String::new();
        };
        let names = board.names_on(&rel, me);
        if names.is_empty() {
            String::new()
        } else {
            format!("[{}] ", names.join(", "))
        }
    }

    /// Guest-side Save: ask the host to write the buffer instead of writing a
    /// local file. Returns `true` when the active buffer is a shared-in one.
    pub(super) fn cowork_intercept_save(&mut self) -> bool {
        let Some(tab) = self.editor.active_tab().filter(|t| t.is_remote()) else {
            return false;
        };
        let rel = tab
            .path
            .as_ref()
            .and_then(|p| p.to_str())
            .and_then(|p| p.strip_prefix(REMOTE_PATH_PREFIX))
            .and_then(|p| SharedPath::parse(p).ok());
        if rel.as_ref().is_some_and(SharedPath::is_session_notes) {
            self.status = t!("status.cowork_notes_not_saved").to_string();
            return true;
        }
        let Some(guest) = self.cowork.guest.as_ref() else {
            return true;
        };
        let Some(rel) = rel else {
            return true;
        };
        match guest.session.request_save(&rel) {
            Ok(msg) => {
                self.status = if guest.link.send(&msg).is_ok() {
                    t!("status.cowork_save_asked").to_string()
                } else {
                    t!("status.cowork_left").to_string()
                };
            }
            Err(_) => self.status = t!("status.cowork_view_only").to_string(),
        }
        true
    }

    // ----- undo (T692) ----------------------------------------------------------

    /// Undo (or redo) in a shared buffer through the session's per-participant
    /// stack, so it reverses only this participant's own text. Returns `false`
    /// when the active buffer is not shared (the editor's own undo applies).
    pub(super) fn cowork_undo(&mut self, undo: bool) -> bool {
        if !self
            .editor
            .active_tab()
            .is_some_and(|t| t.editor.is_edit_logging())
        {
            return false;
        }
        // The document must hold every keystroke before it can revert one.
        self.cowork_pump_local();
        let i = self.editor.active;
        let reverted = if let Some(rel) = self.cowork_active_host_rel()
            && let Some(host) = self.cowork.host.as_mut()
        {
            let r = if undo {
                host.session.undo(&rel)
            } else {
                host.session.redo(&rel)
            };
            r.map(|o| {
                o.map(|(out, edits)| {
                    self.cowork_send(&out);
                    edits
                })
            })
        } else if let Some(rel) = self.cowork_active_remote_rel()
            && let Some(guest) = self.cowork.guest.as_mut()
        {
            let r = if undo {
                guest.session.undo(&rel)
            } else {
                guest.session.redo(&rel)
            };
            match r {
                Ok(Some((msg, edits))) => {
                    if guest.link.send(&msg).is_err() {
                        self.cowork_guest_lost();
                        return true;
                    }
                    Ok(Some(edits))
                }
                Ok(None) => Ok(None),
                Err(e) => Err(e),
            }
        } else {
            return true;
        };
        match reverted {
            Ok(Some(edits)) => {
                let area = self.editor_view();
                if let Some(tab) = self.editor.tabs.get_mut(i) {
                    tab.editor.apply_remote_edits(&splices(&edits));
                    if let Some(last) = edits.last() {
                        tab.editor
                            .set_cursor(last.offset + last.insert.chars().count());
                        tab.editor.focus(&area);
                    }
                    tab.dirty = true;
                }
            }
            Ok(None) => {}
            Err(e) => self.cowork_error(e),
        }
        true
    }

    /// The shared path of the active tab when it is a shared-in (guest) buffer.
    fn cowork_active_remote_rel(&self) -> Option<SharedPath> {
        remote_rel(self.editor.active_tab()?)
    }

    /// The host's view of the active tab: a shared project file, or the
    /// session notes (T759), whose tab is a `cowork:` one on the host too.
    fn cowork_active_host_rel(&self) -> Option<SharedPath> {
        self.cowork_active_rel().or_else(|| {
            self.cowork_active_remote_rel()
                .filter(SharedPath::is_session_notes)
        })
    }

    /// What the host pumps: every shared file, then the session notes.
    fn cowork_host_paths(&self) -> Vec<SharedPath> {
        let Some(h) = self.cowork.host.as_ref() else {
            return Vec::new();
        };
        let mut paths: Vec<SharedPath> = h.session.shared().cloned().collect();
        paths.push(SharedPath::session_notes());
        paths
    }

    // ----- follow mode (T693) ---------------------------------------------------

    /// The presence board and this instance's own peer id (`PeerId::HOST` when hosting); `None`
    /// when no session is open.
    fn cowork_board_and_me(&self) -> Option<(&PresenceBoard, PeerId)> {
        if let Some(h) = self.cowork.host.as_ref() {
            Some((h.session.board(), PeerId::HOST))
        } else {
            self.cowork
                .guest
                .as_ref()
                .map(|g| (g.session.board(), g.session.me()))
        }
    }

    /// Ask which participant to follow. The input is prefilled with the first other
    /// participant's id, or left empty when already following (submitting empty stops
    /// following).
    fn cowork_begin_follow(&mut self) {
        let Some((board, me)) = self.cowork_board_and_me() else {
            self.status = t!("status.cowork_inactive").to_string();
            return;
        };
        let first = board
            .peers()
            .find(|(id, _)| *id != me)
            .map(|(id, _)| id.0.to_string())
            .unwrap_or_default();
        let input = if self.cowork.follow.target().is_some() {
            String::new()
        } else {
            first
        };
        self.prompt = Some(
            Prompt::new(
                PromptKind::CoworkFollow,
                t!("prompt.cowork_follow").to_string(),
            )
            .with_input(input),
        );
    }

    /// Start following the participant whose id was typed, or stop following when the input is
    /// empty. Your own id and unknown ids are rejected.
    fn cowork_set_follow(&mut self, input: &str) {
        if input.trim().is_empty() {
            self.cowork.follow.stop();
            self.status = t!("status.cowork_follow_stopped").to_string();
            return;
        }
        let id = input.trim().parse::<u64>().ok().map(PeerId);
        let name = id.and_then(|id| {
            let (board, me) = self.cowork_board_and_me()?;
            (id != me).then(|| board.name(id).map(str::to_string))?
        });
        match (id, name) {
            (Some(id), Some(name)) => {
                self.cowork.follow.start(id);
                self.status = t!("status.cowork_following", name = name).to_string();
            }
            _ => self.status = t!("status.cowork_no_peer").to_string(),
        }
    }

    /// Any local key or mouse input ends follow mode.
    pub(super) fn cowork_local_input(&mut self) {
        if self.cowork.follow.target().is_some() {
            self.cowork.follow.on_local_scroll();
            self.status = t!("status.cowork_follow_stopped").to_string();
        }
    }

    /// Move to the followed participant's buffer and caret.
    fn cowork_follow_tick(&mut self) {
        let Some(target) = self.cowork.follow.target() else {
            return;
        };
        let Some((board, _)) = self.cowork_board_and_me() else {
            self.cowork.follow.stop();
            return;
        };
        if board.name(target).is_none() {
            self.cowork.follow.on_peer_left(target);
            self.status = t!("status.cowork_follow_stopped").to_string();
            return;
        }
        let Some(loc) = board.location(target).cloned() else {
            return;
        };
        let (tab, doc) = if let Some(h) = self.cowork.host.as_ref() {
            (self.cowork_host_tab(&loc.path), h.session.doc(&loc.path))
        } else if let Some(g) = self.cowork.guest.as_ref() {
            (self.cowork_guest_tab(&loc.path), g.session.doc(&loc.path))
        } else {
            return;
        };
        let (Some(i), Some(head)) = (tab, doc.and_then(|d| d.resolve(&loc.head))) else {
            return;
        };
        let area = self.editor_view();
        self.editor.active = i;
        if let Some(t) = self.editor.tabs.get_mut(i)
            && t.editor.get_cursor() != head
        {
            t.editor.set_cursor(head);
            t.editor.focus(&area);
        }
    }

    // ----- workspace sharing (T696) -----------------------------------------------

    /// Host only: toggle whether guests may list and request any (non-private) file under the
    /// project root, not just buffers the host shared explicitly (T696).
    fn cowork_toggle_workspace(&mut self) {
        let Some(host) = self.cowork.host.as_mut() else {
            self.status = t!("status.cowork_not_hosting").to_string();
            return;
        };
        let on = !host.session.workspace_shared();
        host.session.set_workspace_shared(on);
        self.status = if on {
            t!("status.cowork_workspace_on")
        } else {
            t!("status.cowork_workspace_off")
        }
        .to_string();
    }

    /// Guest: ask the host for the list of files it offers; the answer arrives as
    /// `GuestEvent::Files` and opens the palette over them.
    fn cowork_files(&mut self) {
        let Some(guest) = self.cowork.guest.as_ref() else {
            self.status = t!("status.cowork_not_guest").to_string();
            return;
        };
        if guest.link.send(&guest.session.request_files()).is_err() {
            self.cowork_guest_lost();
            return;
        }
        self.status = t!("status.cowork_files_asked").to_string();
    }

    /// The guest picked one of the host's files in the palette: ask for it.
    pub(super) fn cowork_request_open(&mut self, path: &std::path::Path) {
        let rel = path
            .to_str()
            .and_then(|p| p.strip_prefix(REMOTE_PATH_PREFIX))
            .and_then(|p| SharedPath::parse(p).ok());
        let (Some(rel), Some(guest)) = (rel, self.cowork.guest.as_ref()) else {
            return;
        };
        if let Some(i) = self.cowork_guest_tab(&rel) {
            self.editor.active = i;
            return;
        }
        if guest.link.send(&guest.session.request_open(&rel)).is_err() {
            self.cowork_guest_lost();
            return;
        }
        self.status = t!("status.cowork_open_asked", path = rel.as_str()).to_string();
        self.cowork.asked.insert(rel);
    }

    /// The host's files a guest may see: the project index (it honors
    /// `.gitignore` and skips dotfiles), inside the root, as shared paths.
    /// Private paths are dropped again by `HostSession::file_list`.
    fn cowork_listable(&mut self) -> Vec<SharedPath> {
        if self
            .cowork
            .listed_at
            .is_none_or(|t| t.elapsed() > FILE_LIST_REUSE)
        {
            self.build_file_index();
            self.cowork.listed_at = Some(std::time::Instant::now());
        }
        self.file_index
            .iter()
            .filter_map(|p| {
                let rel = p.strip_prefix(&self.root).ok()?;
                SharedPath::parse(&rel.to_string_lossy()).ok()
            })
            .collect()
    }

    /// A guest asked for `rel`: share it if the host may, else refuse.
    fn cowork_open_for(&mut self, peer: PeerId, rel: &SharedPath) {
        let refuse = |app: &mut Self, code| {
            app.cowork_send(&HostSession::refuse_to(peer, code));
        };
        if let Some(host) = self.cowork.host.as_ref()
            && host.session.doc(rel).is_some()
        {
            match host.session.reopen_for(peer, rel) {
                Ok(out) => self.cowork_send(&out),
                Err(_) => refuse(self, ErrorCode::BadPath),
            }
            return;
        }
        if self
            .cowork
            .host
            .as_ref()
            .is_some_and(|h| h.session.shared().count() >= MAX_SHARED_BUFFERS)
        {
            refuse(self, ErrorCode::PermissionDenied);
            return;
        }
        let Some(abs) = confine(&self.root, rel) else {
            refuse(self, ErrorCode::BadPath);
            return;
        };
        if crate::editor::is_image_path(&abs) {
            refuse(self, ErrorCode::BadPath);
            return;
        }
        let i = if let Some(i) = self.cowork_host_tab(rel) {
            i
        } else {
            // Open it beside the host's work without taking the focus.
            let previous = self.editor.active;
            if self.editor.open(&abs, false).is_err() {
                refuse(self, ErrorCode::BadPath);
                return;
            }
            let opened = self.editor.active;
            self.editor.active = previous.min(self.editor.tabs.len().saturating_sub(1));
            opened
        };
        let Some(text) = self
            .editor
            .tabs
            .get(i)
            .filter(|t| !t.read_only && !t.is_image())
            .map(crate::editor::Tab::text)
        else {
            refuse(self, ErrorCode::BadPath);
            return;
        };
        let shared = self
            .cowork
            .host
            .as_mut()
            .map(|h| h.session.share(rel, &text));
        match shared {
            Some(Ok(out)) => {
                if let Some(tab) = self.editor.tabs.get_mut(i) {
                    tab.editor.start_edit_log();
                }
                self.cowork_send(&out);
                self.status = t!("status.cowork_shared", path = rel.as_str()).to_string();
            }
            Some(Err(vix_cowork::CoworkError::TooLarge { .. })) => {
                refuse(self, ErrorCode::TooLarge);
            }
            _ => refuse(self, ErrorCode::BadPath),
        }
    }

    // ----- shared terminal (T697) ---------------------------------------------
    //
    // The host shares the integrated terminal's *rendered screen*, never the
    // PTY: guests get escape sequences that redraw it (a full screen, then
    // per-tick differences), and the shell keeps running on the host only.
    // Sharing and allowing input are each the host's explicit, confirmed act.

    /// Host: stop sharing the terminal, or (after an explicit confirmation naming the number of
    /// guests) start sharing the active terminal tab. Refuses when no terminal is open.
    fn cowork_toggle_terminal_share(&mut self) {
        let Some(host) = self.cowork.host.as_mut() else {
            self.status = t!("status.cowork_not_hosting").to_string();
            return;
        };
        if host.session.terminal_shared() {
            let out = host.session.unshare_terminal();
            self.cowork_send(&out);
            self.cowork.term_sent = None;
            self.cowork.term_id = None;
            self.status = t!("status.cowork_terminal_unshared").to_string();
            return;
        }
        if self.terminals.is_empty() {
            self.status = t!("status.cowork_terminal_none").to_string();
            return;
        }
        let guests = host.session.guests().count();
        self.confirm = Some(Confirm {
            message: t!("confirm.cowork_share_terminal", n = guests).to_string(),
            paths: Vec::new(),
            kind: ConfirmKind::CoworkShareTerminal,
        });
    }

    /// Host: revoke guests' terminal typing, or (after an explicit confirmation) allow it. The
    /// terminal must be shared first.
    fn cowork_toggle_terminal_input(&mut self) {
        let Some(host) = self.cowork.host.as_mut() else {
            self.status = t!("status.cowork_not_hosting").to_string();
            return;
        };
        if !host.session.terminal_shared() {
            self.status = t!("status.cowork_terminal_not_shared").to_string();
            return;
        }
        if host.session.terminal_input() {
            let out = host.session.set_terminal_input(false);
            self.cowork_send(&out);
            self.status = t!("status.cowork_terminal_input_off").to_string();
            return;
        }
        self.confirm = Some(Confirm {
            message: t!("confirm.cowork_terminal_input").to_string(),
            paths: Vec::new(),
            kind: ConfirmKind::CoworkTerminalInput,
        });
    }

    /// Answer the share-terminal or allow-input confirmation: `y` does it,
    /// `n` or Esc cancels, anything else waits.
    pub(super) fn cowork_confirm_key(&mut self, kind: ConfirmKind, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y' | 'Y') => {
                self.confirm = None;
                if kind == ConfirmKind::CoworkShareTerminal {
                    self.cowork_share_terminal_now();
                } else {
                    let out = self
                        .cowork
                        .host
                        .as_mut()
                        .filter(|h| h.session.terminal_shared())
                        .map(|h| h.session.set_terminal_input(true));
                    if let Some(out) = out {
                        self.cowork_send(&out);
                        self.status = t!("status.cowork_terminal_input_on").to_string();
                    }
                }
            }
            KeyCode::Char('n' | 'N') | KeyCode::Esc => {
                self.confirm = None;
                self.status = t!("status.cowork_terminal_cancelled").to_string();
            }
            _ => {}
        }
    }

    /// Share the active terminal tab: send guests its full rendered screen, remember it as the
    /// baseline for later diffs and pin the shared tab by id (T631).
    fn cowork_share_terminal_now(&mut self) {
        // The active tab is the one shared (T631), pinned by id.
        let Some((id, screen)) = self.terminals.active_id().zip(
            self.terminals
                .active()
                .map(crate::terminal::Terminal::live_screen),
        ) else {
            self.status = t!("status.cowork_terminal_none").to_string();
            return;
        };
        let (rows, cols) = screen.size();
        let bytes = screen.state_formatted();
        let Some(host) = self.cowork.host.as_mut() else {
            return;
        };
        let out = host.session.share_terminal(rows, cols, &bytes);
        self.cowork_send(&out);
        self.cowork.term_sent = Some(screen);
        self.cowork.term_id = Some(id);
        self.status = t!("status.cowork_terminal_shared").to_string();
    }

    /// A guest was just admitted: send it the shared terminal as the others
    /// last saw it (later differences are relative to that screen).
    fn cowork_terminal_for(&mut self, peer: PeerId) {
        let (Some(host), Some(screen)) =
            (self.cowork.host.as_ref(), self.cowork.term_sent.as_ref())
        else {
            return;
        };
        let (rows, cols) = screen.size();
        let out = host
            .session
            .terminal_for(peer, rows, cols, &screen.state_formatted());
        self.cowork_send(&out);
    }

    /// Host, once per tick: send what changed on the shared terminal (the
    /// whole screen after a resize). If the terminal was closed, or a screen
    /// is too large to send, stop sharing it.
    fn cowork_pump_terminal(&mut self) {
        if !self
            .cowork
            .host
            .as_ref()
            .is_some_and(|h| h.session.terminal_shared())
        {
            return;
        }
        // The shared tab's live screen: the host scrolling back is not sent.
        let Some(screen) = self
            .cowork
            .term_id
            .and_then(|id| self.terminals.get(id))
            .map(crate::terminal::Terminal::live_screen)
        else {
            self.cowork_stop_terminal_share();
            return;
        };
        let (rows, cols) = screen.size();
        let same_size = self
            .cowork
            .term_sent
            .as_ref()
            .is_some_and(|prev| prev.size() == (rows, cols));
        let bytes = match self.cowork.term_sent.as_ref() {
            Some(prev) if same_size => screen.state_diff(prev),
            _ => screen.state_formatted(),
        };
        if bytes.is_empty() {
            return;
        }
        if bytes.len() > vix_cowork::wire::MAX_TERMINAL_BYTES {
            self.cowork_stop_terminal_share();
            return;
        }
        let Some(host) = self.cowork.host.as_mut() else {
            return;
        };
        let out = if same_size {
            host.session.terminal_output(&bytes)
        } else {
            host.session.share_terminal(rows, cols, &bytes)
        };
        self.cowork_send(&out);
        self.cowork.term_sent = Some(screen);
    }

    /// Host: the shared terminal is gone (closed or unshared); tell guests and forget the
    /// baseline screen and tab id.
    fn cowork_stop_terminal_share(&mut self) {
        let out = self
            .cowork
            .host
            .as_mut()
            .map(|h| h.session.unshare_terminal())
            .unwrap_or_default();
        self.cowork_send(&out);
        self.cowork.term_sent = None;
        self.cowork.term_id = None;
        self.status = t!("status.cowork_terminal_unshared").to_string();
    }

    /// Guest: the host sent its whole terminal screen.
    fn cowork_terminal_screen(&mut self, rows: u16, cols: u16, screen: &[u8]) {
        let first = self.cowork.term_mirror.is_none();
        let mut mirror = vt100::Parser::new(rows, cols, 0);
        mirror.process(screen);
        self.cowork.term_mirror = Some(mirror);
        if first {
            // Announced, never opened over the guest's work: the overlay
            // would take every key.
            self.status = t!("status.cowork_terminal_offered").to_string();
        }
    }

    /// Guest: report whether this guest may type into the shared terminal or only watch.
    fn cowork_terminal_input_status(&mut self) {
        let can = self
            .cowork
            .guest
            .as_ref()
            .is_some_and(|g| g.session.can_type_in_terminal());
        self.status = if can {
            t!("status.cowork_terminal_can_type")
        } else {
            t!("status.cowork_terminal_read_only")
        }
        .to_string();
    }

    /// Guest: open or close the shared-terminal overlay (which takes every key while open).
    /// Needs a session and a terminal the host has shared.
    fn cowork_toggle_terminal_view(&mut self) {
        if self.cowork.guest.is_none() {
            self.status = t!("status.cowork_not_guest").to_string();
            return;
        }
        if self.cowork.term_mirror.is_none() {
            self.cowork.term_view = false;
            self.status = t!("status.cowork_terminal_not_shared").to_string();
            return;
        }
        self.cowork.term_view = !self.cowork.term_view;
        if self.cowork.term_view {
            self.cowork_terminal_input_status();
        }
    }

    /// Guest: the shared terminal went away (closed, or the session ended).
    fn cowork_forget_terminal(&mut self) {
        self.cowork.term_mirror = None;
        self.cowork.term_view = false;
        self.cowork.term_sent = None;
        self.cowork.term_id = None;
    }

    /// Host (T631): the id of the terminal tab shared with the session, if
    /// one is (the renderer marks its tab).
    #[must_use]
    pub(crate) fn cowork_shared_terminal(&self) -> Option<u64> {
        self.cowork
            .host
            .as_ref()
            .filter(|h| h.session.terminal_shared())
            .and(self.cowork.term_id)
    }

    /// Whether the guest's shared-terminal overlay is open (it takes keys).
    #[must_use]
    pub(super) fn cowork_terminal_viewing(&self) -> bool {
        self.cowork.term_view && self.cowork.term_mirror.is_some()
    }

    /// A key while the shared-terminal overlay is open: `Ctrl+]` closes the
    /// view (the host's shell keeps running); with input allowed, every other
    /// key goes to the host's terminal; otherwise the status says it is
    /// read-only.
    pub(super) fn cowork_terminal_key(&mut self, key: KeyEvent) {
        if Self::ctrl(&key) && matches!(key.code, KeyCode::Char(']')) {
            self.cowork.term_view = false;
            return;
        }
        let Some(guest) = self.cowork.guest.as_ref() else {
            self.cowork_forget_terminal();
            return;
        };
        let bytes = crate::terminal::encode_key(key);
        if bytes.is_empty() {
            return;
        }
        match guest.session.terminal_keys(&bytes) {
            Ok(msg) => {
                if guest.link.send(&msg).is_err() {
                    self.cowork_guest_lost();
                }
            }
            Err(_) => self.status = t!("status.cowork_terminal_read_only").to_string(),
        }
    }

    /// The guest's view of the shared terminal for the renderer, with whether
    /// this guest may type in it; `None` unless the overlay is open.
    #[must_use]
    pub(crate) fn cowork_terminal_view(&self) -> Option<(&vt100::Screen, bool)> {
        if !self.cowork.term_view {
            return None;
        }
        let mirror = self.cowork.term_mirror.as_ref()?;
        let can = self
            .cowork
            .guest
            .as_ref()
            .is_some_and(|g| g.session.can_type_in_terminal());
        Some((mirror.screen(), can))
    }

    /// Whether this guest may type in the host's shared terminal right now:
    /// it is shared, the host allows input, and this guest has terminal
    /// permission (T697).
    #[must_use]
    pub fn cowork_terminal_can_type(&self) -> bool {
        self.cowork
            .guest
            .as_ref()
            .is_some_and(|g| g.session.can_type_in_terminal())
    }

    /// The guest's copy of the host's shared terminal as plain text, or
    /// `None` when no terminal is shared with it (T697). For tests and
    /// diagnostics.
    #[must_use]
    pub fn cowork_terminal_text(&self) -> Option<String> {
        self.cowork
            .term_mirror
            .as_ref()
            .map(|m| m.screen().contents())
    }

    // ----- session chat (T756) ------------------------------------------------
    //
    // The session's conversation lives in the IM inbox from the moment a
    // session starts until it ends, and is never written to disk. Lines ride
    // the session's encrypted link; the host stamps who said each one, so an
    // author is always the verified identity: the name the host confirmed at
    // consent and the number it gave (`ana #2`). Like all of IM, nothing here
    // opens or focuses anything: a line raises the unread count.

    /// A session started: its conversation appears (the panel stays as is).
    fn cowork_chat_open(&mut self) {
        let title = t!("ui.im_session").to_string();
        self.im_ensure_conversation(session_id(), ConversationKind::Session, &title, None);
    }

    /// The session ended: its conversation is dropped.
    fn cowork_chat_close(&mut self) {
        self.im.inbox.remove(&session_id());
        self.im.panel.clamp(&self.im.inbox);
    }

    /// The verified identity of a participant: confirmed name and number.
    fn cowork_author(&self, id: PeerId) -> String {
        let name = self
            .cowork_board_and_me()
            .and_then(|(board, _)| board.name(id))
            .unwrap_or("?");
        format!("{name} #{}", id.0)
    }

    /// A system line (joins, leaves, mutes) in the session conversation.
    fn cowork_chat_system(&mut self, text: &str) {
        let line = vix_im::Message::system(super::im::now_ms(), text);
        self.im_deliver(&session_id(), line);
    }

    /// A line someone else said, from the host's relay (or a guest, on the
    /// host): into the session conversation, counted unread.
    fn cowork_chat_receive(&mut self, line: ChatLine) {
        let author = self.cowork_author(line.from);
        self.im_deliver(&session_id(), from_chat_line(line, author));
    }

    /// Say `message` (its author is filled in here) in the session chat, and
    /// show it in the conversation as read. A muted guest is told so.
    pub(super) fn cowork_chat_send(&mut self, message: vix_im::Message) {
        let Some(line) = to_chat_line(message.clone()) else {
            return;
        };
        let sent = if let Some(h) = self.cowork.host.as_ref() {
            h.session
                .chat(line)
                .map(|out| (PeerId::HOST, Some(out), None))
        } else if let Some(g) = self.cowork.guest.as_ref() {
            g.session
                .chat(line)
                .map(|msg| (g.session.me(), None, Some(msg)))
        } else {
            self.status = t!("status.cowork_inactive").to_string();
            return;
        };
        let me = match sent {
            Ok((me, out, msg)) => {
                if let Some(out) = out {
                    self.cowork_send(&out);
                }
                if let Some(msg) = msg
                    && let Some(g) = self.cowork.guest.as_ref()
                    && g.link.send(&msg).is_err()
                {
                    self.cowork_guest_lost();
                    return;
                }
                me
            }
            Err(vix_cowork::CoworkError::PermissionDenied) => {
                self.status = t!("status.cowork_chat_muted").to_string();
                return;
            }
            Err(e) => {
                self.cowork_error(e);
                return;
            }
        };
        let own = vix_im::Message {
            author: self.cowork_author(me),
            ..message
        };
        let id = session_id();
        let _ = self.im.inbox.push(&id, own);
        self.im.inbox.mark_read(&id);
    }

    // ----- presence, follow and notes in the IM panel (T759) -------------------

    /// Everyone in the session, in id order, with where their cursor is:
    /// the IM panel's session roster, one row each (see `roster_row`).
    /// Empty without a session.
    #[must_use]
    pub fn cowork_roster(&self) -> Vec<String> {
        self.cowork_roster_entries()
            .iter()
            // Names are free of control characters already; this also drops
            // the bidi overrides a guest could put in its name.
            .map(|e| vix_im::sanitize(&roster_row(e)))
            .collect()
    }

    /// Build the participant roster from the presence board: each participant's current file,
    /// You/host tags and mute state.
    fn cowork_roster_entries(&self) -> Vec<RosterEntry> {
        let Some((board, me)) = self.cowork_board_and_me() else {
            return Vec::new();
        };
        let mine = if self.cowork.host.is_some() {
            self.cowork_active_host_rel()
        } else {
            self.cowork_active_remote_rel()
        };
        let host = self.cowork.host.as_ref();
        let self_muted = self
            .cowork
            .guest
            .as_ref()
            .is_some_and(|g| g.session.is_muted());
        board
            .peers()
            .map(|(id, name)| {
                let at = if id == me {
                    mine.clone()
                } else {
                    board.location(id).map(|p| p.path.clone())
                };
                let muted =
                    host.is_some_and(|h| h.session.is_muted(id)) || (id == me && self_muted);
                let tags = [
                    (id == me, RosterTag::You),
                    (id == PeerId::HOST, RosterTag::Host),
                    (muted, RosterTag::Muted),
                    (
                        self.cowork.follow.target() == Some(id),
                        RosterTag::Following,
                    ),
                ]
                .into_iter()
                .filter_map(|(on, tag)| on.then_some(tag))
                .collect();
                let place = match at {
                    Some(p) if p.is_session_notes() => RosterPlace::Notes,
                    Some(p) => RosterPlace::File(p.as_str().to_string()),
                    None => RosterPlace::Nowhere,
                };
                RosterEntry {
                    who: format!("{name} #{}", id.0),
                    tags,
                    place,
                }
            })
            .collect()
    }

    /// `/follow N` in the IM panel: follow participant `N`, or with no
    /// number stop following (the same as **Follow…**).
    pub(super) fn cowork_follow_from_chat(&mut self, who: Option<&str>) {
        if self.cowork_board_and_me().is_none() {
            self.status = t!("status.cowork_inactive").to_string();
            return;
        }
        self.cowork_set_follow(who.unwrap_or_default());
    }

    /// A guest pressed Enter on a code reference in the session chat: the
    /// path names a file under the *host's* root, so open the shared-in
    /// buffer there, or ask the host to share it (T696 rules apply). `false`
    /// leaves the reference to the ordinary, local-root resolver.
    pub(super) fn cowork_open_ref(&mut self, reference: &vix_im::CodeRef) -> bool {
        if self.cowork.guest.is_none() || self.im.selected_id() != Some(session_id()) {
            return false;
        }
        let raw = reference
            .path
            .strip_prefix(REMOTE_PATH_PREFIX)
            .unwrap_or(&reference.path);
        let Ok(rel) = SharedPath::parse(raw) else {
            return false;
        };
        if let Some(i) = self.cowork_guest_tab(&rel) {
            let (line, column) = (
                reference.line as usize,
                reference.column.map(|c| c as usize),
            );
            self.with_jump(|s| {
                s.editor.active = i;
                let area = s.editor_view();
                s.editor.goto(line, column, area);
                s.focus = super::Focus::Editor;
            });
        } else {
            let remote = PathBuf::from(format!("{REMOTE_PATH_PREFIX}{}", rel.as_str()));
            self.cowork_request_open(&remote);
        }
        true
    }

    /// **Session Notes** (T759): open the session's shared Markdown notes,
    /// or switch to them. Never opened on its own; a view-only guest reads
    /// them, an edit guest and the host write them. They are not a file:
    /// Save does nothing, and when the session ends they stay behind as an
    /// unsaved buffer.
    fn cowork_open_notes(&mut self) {
        let notes = SharedPath::session_notes();
        if let Some(i) = self.cowork_guest_tab(&notes) {
            self.editor.active = i;
            self.focus = super::Focus::Editor;
            return;
        }
        let found = if let Some(h) = self.cowork.host.as_ref() {
            h.session.doc(&notes).map(|d| (d.text(), false))
        } else if let Some(g) = self.cowork.guest.as_ref() {
            let read_only = !g.session.permission().allows(vix_cowork::Action::Edit);
            g.session.doc(&notes).map(|d| (d.text(), read_only))
        } else {
            None
        };
        let Some((text, read_only)) = found else {
            self.status = t!("status.cowork_inactive").to_string();
            return;
        };
        self.editor.new_tab_with_content_as(&text, Some("notes.md"));
        let i = self.editor.active;
        if let Some(tab) = self.editor.tabs.get_mut(i) {
            tab.path = Some(PathBuf::from(format!(
                "{REMOTE_PATH_PREFIX}{}",
                notes.as_str()
            )));
            tab.dirty = false;
            tab.read_only = read_only;
            tab.editor.start_edit_log();
        }
        self.cowork.last_cursor.remove(&notes);
        self.focus = super::Focus::Editor;
    }

    // ----- host moderation (T756) ---------------------------------------------

    /// **Mute in Chat…** and **Remove Guest…** ask which guest (by number).
    fn cowork_begin_host_prompt(&mut self, kind: PromptKind) {
        if self.cowork.host.is_none() {
            self.status = t!("status.cowork_not_hosting").to_string();
            return;
        }
        let label = if kind == PromptKind::CoworkMute {
            t!("prompt.cowork_mute")
        } else {
            t!("prompt.cowork_remove")
        };
        self.prompt = Some(Prompt::new(kind, label.to_string()));
    }

    /// The guest a prompt answer names, with its confirmed name.
    fn cowork_named_guest(&self, input: &str) -> Option<(PeerId, String)> {
        let id = PeerId(input.trim().parse::<u64>().ok()?);
        let name = self.cowork.host.as_ref()?.session.guest(id)?.name.clone();
        Some((id, name))
    }

    /// Mute the named guest in the chat, or unmute it if it is muted.
    fn cowork_toggle_mute(&mut self, input: &str) {
        let Some((id, _)) = self.cowork_named_guest(input) else {
            self.status = t!("status.cowork_no_guest").to_string();
            return;
        };
        let Some(host) = self.cowork.host.as_mut() else {
            return;
        };
        let muted = !host.session.is_muted(id);
        let out = host.session.set_muted(id, muted);
        self.cowork_send(&out);
        let who = self.cowork_author(id);
        let text = if muted {
            t!("status.cowork_muted", name = who)
        } else {
            t!("status.cowork_unmuted", name = who)
        };
        self.cowork_chat_system(&text);
        self.status = text.to_string();
    }

    /// Remove the named guest from the session and close its connection.
    fn cowork_remove(&mut self, input: &str) {
        let Some((id, name)) = self.cowork_named_guest(input) else {
            self.status = t!("status.cowork_no_guest").to_string();
            return;
        };
        let who = self.cowork_author(id);
        let Some(host) = self.cowork.host.as_mut() else {
            return;
        };
        let out = host.session.kick(id);
        self.cowork_send(&out);
        if let Some(h) = self.cowork.host.as_mut()
            && let Some(link) = h.links.remove(&id)
        {
            link.close();
        }
        if self.cowork.follow.target() == Some(id) {
            self.cowork.follow.on_peer_left(id);
        }
        self.cowork_chat_system(&t!("msg.im_left", nick = who));
        self.status = t!("status.cowork_removed_peer", name = name).to_string();
    }

    // ----- the per-tick pump --------------------------------------------------

    /// Drive the session: finish a join, offer new guests for consent, feed
    /// local edits to the document, apply remote edits to the buffers, and
    /// refresh presence. Cheap when no session is open.
    pub fn poll_cowork(&mut self) {
        if !self.cowork_busy() && self.cowork.knock.is_none() {
            return;
        }
        self.cowork_poll_join();
        self.cowork_poll_knock();
        // Local edits first: the edit log is relative to the buffer as it was.
        self.cowork_pump_local();
        self.cowork_pump_remote();
        self.cowork_pump_presence();
        self.cowork_pump_terminal();
        self.cowork_follow_tick();
    }

    /// Pick up the result of a background join: on success become a guest (start its reader
    /// thread and the session chat), otherwise report the error.
    fn cowork_poll_join(&mut self) {
        let Some(rx) = self.cowork.joining.as_ref() else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(NetError::Closed),
        };
        self.cowork.joining = None;
        match result {
            Ok(joined) => match GuestSession::from_welcome(&joined.welcome, DEFAULT_CAP_BYTES) {
                Ok(session) => {
                    let _reader = joined.reader.spawn(PeerId::HOST, self.cowork.tx.clone());
                    self.status =
                        t!("status.cowork_joined", host = session.host_name()).to_string();
                    self.cowork.guest = Some(Guest {
                        session,
                        link: joined.link,
                    });
                    self.cowork_chat_open();
                }
                Err(e) => self.cowork_error(e),
            },
            Err(e) => self.cowork_error(e),
        }
    }

    /// Host: surface the next authenticated guest that is waiting for approval as a consent
    /// prompt (default answer `view`). A pending guest whose consent prompt was dismissed is
    /// declined; nothing is shown while another prompt is open.
    fn cowork_poll_knock(&mut self) {
        // A consent prompt dismissed with Esc is a refusal.
        let prompting = self
            .prompt
            .as_ref()
            .is_some_and(|p| p.kind == PromptKind::CoworkConsent);
        if !prompting && let Some(k) = self.cowork.knock.take() {
            k.decline(vix_cowork::ErrorCode::Declined);
        }
        if self.cowork.knock.is_some() || self.prompt.is_some() {
            return;
        }
        let Some(knock) = self
            .cowork
            .host
            .as_ref()
            .and_then(|h| h.listener.try_knock())
        else {
            return;
        };
        let label = t!(
            "prompt.cowork_consent",
            name = knock.name,
            addr = knock.peer_addr.map_or_else(String::new, |a| a.to_string())
        )
        .to_string();
        self.cowork.knock = Some(knock);
        self.prompt =
            Some(Prompt::new(PromptKind::CoworkConsent, label).with_input("view".to_string()));
    }

    /// Feed locally made edits into the session. The host drains the edit log of every shared
    /// buffer (and the session notes) into its document and broadcasts; a closed shared tab
    /// stops being shared, and a failed edit stops sharing that buffer because buffer and
    /// document could no longer agree. A guest does the same for shared-in buffers, leaving the
    /// session if its document rejects an edit.
    fn cowork_pump_local(&mut self) {
        // Host: every shared buffer, and the session notes when open.
        for rel in self.cowork_host_paths() {
            let Some(i) = self.cowork_host_tab(&rel) else {
                // The tab was closed: stop sharing it. The notes stay; their
                // text lives in the session until the tab is opened again.
                if !rel.is_session_notes() {
                    self.cowork_unshare_path(&rel);
                }
                continue;
            };
            let log = self
                .editor
                .tabs
                .get_mut(i)
                .map(|t| t.editor.take_edit_log())
                .unwrap_or_default();
            for s in log {
                let result = self
                    .cowork
                    .host
                    .as_mut()
                    .map(|h| h.session.local_edit(&rel, s.offset, s.delete, &s.insert));
                match result {
                    Some(Ok(out)) => self.cowork_send(&out),
                    Some(Err(e)) => {
                        // The document and the buffer can no longer agree:
                        // fail closed by ceasing to share this buffer.
                        self.cowork_error(e);
                        self.cowork_unshare_path(&rel);
                        break;
                    }
                    None => {}
                }
            }
        }
        // Guest: every shared-in buffer.
        if self.cowork.guest.is_none() {
            return;
        }
        let tabs: Vec<(usize, SharedPath)> = self
            .editor
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                let p = t
                    .path
                    .as_ref()?
                    .to_str()?
                    .strip_prefix(REMOTE_PATH_PREFIX)?;
                Some((i, SharedPath::parse(p).ok()?))
            })
            .collect();
        for (i, rel) in tabs {
            let log = self
                .editor
                .tabs
                .get_mut(i)
                .map(|t| t.editor.take_edit_log())
                .unwrap_or_default();
            for s in log {
                let Some(guest) = self.cowork.guest.as_mut() else {
                    break;
                };
                match guest.session.edit(&rel, s.offset, s.delete, &s.insert) {
                    Ok(Some(msg)) => {
                        if guest.link.send(&msg).is_err() {
                            self.cowork_guest_lost();
                            return;
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        self.cowork_error(e);
                        self.cowork_leave();
                        return;
                    }
                }
            }
        }
    }

    /// Guest: the connection to the host is gone. Close it, turn shared-in tabs into local
    /// copies and clear terminal and chat state.
    fn cowork_guest_lost(&mut self) {
        if let Some(g) = self.cowork.guest.take() {
            g.link.close();
        }
        self.cowork_release_guest_tabs();
        self.cowork_forget_terminal();
        self.cowork_chat_close();
        self.status = t!("status.cowork_left").to_string();
    }

    /// Drain the network events queued by the reader threads and route each message (or closed
    /// link) to the host or guest handler.
    fn cowork_pump_remote(&mut self) {
        while let Ok((peer, event)) = self.cowork.rx.try_recv() {
            match event {
                LinkEvent::Message(msg) => {
                    if self.cowork.host.is_some() {
                        self.cowork_host_message(peer, msg);
                    } else {
                        self.cowork_guest_message(msg);
                    }
                }
                LinkEvent::Closed(_) => {
                    if self.cowork.host.is_some() {
                        self.cowork_drop_guest(peer);
                    } else {
                        self.cowork_guest_lost();
                    }
                }
            }
        }
    }

    /// Host: a guest's link closed. Drop its session state and link, tell the rest and announce
    /// it in the chat.
    fn cowork_drop_guest(&mut self, peer: PeerId) {
        let Some(host) = self.cowork.host.as_mut() else {
            return;
        };
        let name = host.session.guest(peer).map(|g| g.name.clone());
        let who = host
            .session
            .board()
            .name(peer)
            .map(|n| format!("{n} #{}", peer.0));
        let out = host.session.remove(peer);
        if let Some(link) = host.links.remove(&peer) {
            link.close();
        }
        self.cowork_send(&out);
        if let Some(who) = who {
            self.cowork_chat_system(&t!("msg.im_left", nick = who));
        }
        if let Some(name) = name {
            self.status = t!("status.cowork_peer_left", name = name).to_string();
        }
    }

    /// Host: handle one guest message through the session, then carry out its results: forward
    /// replies, apply the guest's edits to the host buffers, save files the guest asked to save
    /// (after which the saver is notified), answer file-list and open requests, forward
    /// terminal keystrokes to the shared tab, show chat, and close links of peers the session
    /// dropped.
    fn cowork_host_message(&mut self, peer: PeerId, msg: Message) {
        // Its name, while the session still knows it: a guest that leaves
        // (or is dropped for misbehaving) is gone after `handle`.
        let who = self.cowork_author(peer);
        let handled = self
            .cowork
            .host
            .as_mut()
            .map(|h| h.session.handle(peer, msg));
        let Some(Ok(handled)) = handled else {
            return;
        };
        self.cowork_send(&handled.out);
        for (rel, edits) in handled.edits {
            if let Some(i) = self.cowork_host_tab(&rel)
                && let Some(tab) = self.editor.tabs.get_mut(i)
            {
                tab.editor.apply_remote_edits(&splices(&edits));
                tab.dirty = true;
            }
        }
        for (asker, rel) in handled.save {
            if let Some(i) = self.cowork_host_tab(&rel) {
                let previous = std::mem::replace(&mut self.editor.active, i);
                let saved = self.write_active_to_disk();
                self.editor.active = previous;
                if saved && let Some(h) = self.cowork.host.as_ref() {
                    let out = h.session.saved(asker, &rel);
                    self.cowork_send(&out);
                }
            }
        }
        for asker in handled.list_files {
            let paths = self.cowork_listable();
            if let Some(h) = self.cowork.host.as_ref() {
                let out = h.session.file_list(asker, paths);
                self.cowork_send(&out);
            }
        }
        for (asker, rel) in handled.open {
            self.cowork_open_for(asker, &rel);
        }
        // Checked by the session: shared, input allowed, terminal permission.
        for (_, keys) in handled.terminal_keys {
            // Only ever the shared tab, whichever one the host is looking at.
            if let Some(term) = self
                .cowork
                .term_id
                .and_then(|id| self.terminals.get_mut(id))
            {
                term.send_bytes(&keys);
            }
        }
        for line in handled.chat {
            self.cowork_chat_receive(line);
        }
        for gone in handled.disconnect {
            if let Some(h) = self.cowork.host.as_mut()
                && let Some(link) = h.links.remove(&gone)
            {
                link.close();
            }
            if gone == peer {
                self.cowork_chat_system(&t!("msg.im_left", nick = who.as_str()));
            }
        }
    }

    /// Guest: handle one host message through the session and apply each resulting event
    /// (open/close/edit shared buffers, permission changes, file lists, terminal mirror
    /// updates, chat, presence). A protocol error ends the session.
    fn cowork_guest_message(&mut self, msg: Message) {
        let events = match self.cowork.guest.as_mut().map(|g| g.session.handle(msg)) {
            Some(Ok(events)) => events,
            Some(Err(e)) => {
                self.cowork_error(e);
                self.cowork_leave();
                return;
            }
            None => return,
        };
        for event in events {
            match event {
                GuestEvent::Opened(rel, text) => self.cowork_open_remote(&rel, &text),
                GuestEvent::Closed(rel) => {
                    if let Some(i) = self.cowork_guest_tab(&rel)
                        && let Some(tab) = self.editor.tabs.get_mut(i)
                    {
                        tab.path = None;
                        tab.read_only = false;
                        tab.editor.stop_edit_log();
                        tab.editor.set_remote_carets(Vec::new());
                    }
                }
                GuestEvent::Edits(rel, edits) => {
                    if let Some(i) = self.cowork_guest_tab(&rel)
                        && let Some(tab) = self.editor.tabs.get_mut(i)
                    {
                        tab.editor.apply_remote_edits(&splices(&edits));
                    }
                }
                GuestEvent::Saved(_) => self.status = t!("status.cowork_saved_remote").to_string(),
                GuestEvent::Permission(p) => {
                    for tab in &mut self.editor.tabs {
                        if tab.is_remote() {
                            tab.read_only = !p.allows(vix_cowork::Action::Edit);
                        }
                    }
                    if p == Permission::View {
                        self.status = t!("status.cowork_view_only").to_string();
                    }
                    if self.cowork.term_mirror.is_some() {
                        self.cowork_terminal_input_status();
                    }
                }
                GuestEvent::Refused(ErrorCode::TooLarge) => {
                    self.status = t!("status.cowork_too_large").to_string();
                }
                GuestEvent::Refused(ErrorCode::Removed) => {
                    self.status = t!("status.cowork_you_were_removed").to_string();
                }
                GuestEvent::Refused(_) => self.status = t!("status.cowork_refused").to_string(),
                GuestEvent::Files(paths) => {
                    if paths.is_empty() {
                        self.status = t!("status.cowork_no_files").to_string();
                    } else {
                        let files = paths
                            .iter()
                            .map(|p| PathBuf::from(format!("{REMOTE_PATH_PREFIX}{}", p.as_str())))
                            .collect();
                        self.open_palette_over(files);
                    }
                }
                GuestEvent::Ended => {
                    self.cowork_guest_lost();
                    return;
                }
                GuestEvent::TerminalScreen { rows, cols, screen } => {
                    self.cowork_terminal_screen(rows, cols, &screen);
                }
                GuestEvent::TerminalOutput(bytes) => {
                    if let Some(mirror) = self.cowork.term_mirror.as_mut() {
                        mirror.process(&bytes);
                    }
                }
                GuestEvent::TerminalInput(_) => self.cowork_terminal_input_status(),
                GuestEvent::TerminalClosed => {
                    self.cowork_forget_terminal();
                    self.status = t!("status.cowork_terminal_closed").to_string();
                }
                GuestEvent::Chat(line) => self.cowork_chat_receive(line),
                GuestEvent::Muted(muted) => {
                    let text = if muted {
                        t!("status.cowork_chat_muted")
                    } else {
                        t!("status.cowork_chat_unmuted")
                    };
                    self.cowork_chat_system(&text);
                    self.status = text.to_string();
                }
                GuestEvent::PeerJoined(peer, name) => {
                    let who = format!("{name} #{}", peer.0);
                    self.cowork_chat_system(&t!("msg.im_joined", nick = who));
                }
                GuestEvent::PeerLeft(peer, name) => {
                    let who = format!("{name} #{}", peer.0);
                    self.cowork_chat_system(&t!("msg.im_left", nick = who));
                }
                _ => {}
            }
        }
    }

    /// Guest: open (or refresh) the shared-in buffer for `rel` from the host's snapshot `text`.
    /// It is read-only unless the guest may edit; the first shared buffer and ones this guest
    /// asked for take the focus. The session notes only open when their tab already exists.
    fn cowork_open_remote(&mut self, rel: &SharedPath, text: &str) {
        if rel.is_session_notes() && self.cowork_guest_tab(rel).is_none() {
            // The notes wait in the session until Session Notes opens them:
            // nothing opens over the guest's work on its own.
            return;
        }
        let read_only = self
            .cowork
            .guest
            .as_ref()
            .is_none_or(|g| !g.session.permission().allows(vix_cowork::Action::Edit));
        // The first shared buffer and the ones this guest asked for take the
        // focus; the rest (shared for another guest) open behind it.
        let focus = self.cowork.asked.remove(rel)
            || !self.editor.tabs.iter().any(crate::editor::Tab::is_remote);
        let previous = self.editor.active;
        if let Some(i) = self.cowork_guest_tab(rel) {
            self.editor.active = i;
            // A fresh snapshot replaced the document: the buffer must match it
            // exactly, and that reset is not a local edit to publish.
            if let Some(tab) = self.editor.tabs.get_mut(i)
                && tab.text() != text
            {
                tab.editor.stop_edit_log();
                tab.editor.set_content(text);
            }
        } else {
            // Named after the file so it gets that language's highlighting.
            let name = rel.as_str().rsplit('/').next();
            self.editor.new_tab_with_content_as(text, name);
        }
        let opened = self.editor.active;
        if !focus {
            self.editor.active = previous.min(self.editor.tabs.len().saturating_sub(1));
        }
        if let Some(tab) = self.editor.tabs.get_mut(opened) {
            tab.path = Some(PathBuf::from(format!(
                "{REMOTE_PATH_PREFIX}{}",
                rel.as_str()
            )));
            tab.dirty = false;
            tab.read_only = read_only;
            tab.editor.start_edit_log();
        }
    }

    /// Publish this participant's caret/selection in each shared buffer when it changed, and
    /// refresh every shared buffer's remote carets from the presence board, with colors checked
    /// for contrast against the editor background.
    fn cowork_pump_presence(&mut self) {
        // Background color for contrast-checked peer colors.
        let bg = match crate::theme::region_base(crate::theme::Region::Editor).bg {
            Some(ratatui::style::Color::Rgb(r, g, b)) => Rgb(r, g, b),
            _ => Rgb(0x1e, 0x1e, 0x1e),
        };
        let managed: Vec<(usize, SharedPath)> = if self.cowork.host.is_some() {
            self.cowork_host_paths()
                .into_iter()
                .filter_map(|r| Some((self.cowork_host_tab(&r)?, r)))
                .collect()
        } else {
            self.editor
                .tabs
                .iter()
                .enumerate()
                .filter_map(|(i, t)| {
                    let p = t
                        .path
                        .as_ref()?
                        .to_str()?
                        .strip_prefix(REMOTE_PATH_PREFIX)?;
                    Some((i, SharedPath::parse(p).ok()?))
                })
                .collect()
        };
        for (i, rel) in managed {
            let Some(tab) = self.editor.tabs.get(i) else {
                continue;
            };
            let head = tab.editor.get_cursor();
            let anchor = tab
                .editor
                .selection_span()
                .map(|(s, e)| if head == s { e } else { s });
            if self.cowork.last_cursor.get(&rel) != Some(&(head, anchor)) {
                self.cowork.last_cursor.insert(rel.clone(), (head, anchor));
                self.cowork_send_cursor(&rel, head, anchor);
            }
            let carets = self.cowork_carets(&rel, bg);
            if let Some(tab) = self.editor.tabs.get_mut(i) {
                tab.editor.set_remote_carets(carets);
            }
        }
    }

    /// Send this participant's cursor for `rel` to the others: the host broadcasts it, a guest
    /// sends it to the host (a failed send ends the session).
    fn cowork_send_cursor(&mut self, rel: &SharedPath, head: usize, anchor: Option<usize>) {
        if let Some(host) = self.cowork.host.as_ref() {
            let out = host.session.local_cursor(rel, head, anchor);
            self.cowork_send(&out);
        } else if let Some(guest) = self.cowork.guest.as_ref()
            && let Some(msg) = guest.session.cursor(rel, head, anchor)
            && guest.link.send(&msg).is_err()
        {
            self.cowork_guest_lost();
        }
    }

    /// The other participants' carets in `rel`, resolved against the current document and
    /// colored for contrast against `bg`.
    fn cowork_carets(&self, rel: &SharedPath, bg: Rgb) -> Vec<RemoteCaret> {
        let (board, doc, me) = if let Some(h) = self.cowork.host.as_ref() {
            (h.session.board(), h.session.doc(rel), PeerId::HOST)
        } else if let Some(g) = self.cowork.guest.as_ref() {
            (g.session.board(), g.session.doc(rel), g.session.me())
        } else {
            return Vec::new();
        };
        let Some(doc) = doc else {
            return Vec::new();
        };
        board
            .cursors(rel, doc, me, bg)
            .into_iter()
            .map(|c| RemoteCaret {
                head: c.head,
                selection: c.selection(),
                color: ratatui::style::Color::Rgb(c.color.0, c.color.1, c.color.2),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> SharedPath {
        SharedPath::parse(s).unwrap()
    }

    /// T697: the host's shared-terminal stream rests on two `vt100` facts:
    /// an unchanged screen diffs to nothing (so an idle terminal sends
    /// nothing), and a guest that replays the full screen and then every
    /// diff ends up with exactly the host's screen, cursor included.
    #[test]
    fn a_terminal_mirror_replays_the_hosts_screen_exactly() {
        let mut host = vt100::Parser::new(6, 20, 0);
        host.process(b"$ ls\r\n\x1b[1;31msrc\x1b[0m  tests\r\n$ ");
        let mut sent = host.screen().clone();
        assert!(host.screen().state_diff(&sent).is_empty(), "idle is silent");

        let mut guest = vt100::Parser::new(6, 20, 0);
        guest.process(&sent.state_formatted());
        for chunk in [
            &b"echo hi\r\nhi\r\n$ "[..],
            b"\x1b[2J\x1b[H$ clear done",
            b"\r\n1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7",
        ] {
            host.process(chunk);
            guest.process(&host.screen().state_diff(&sent));
            sent = host.screen().clone();
            assert_eq!(guest.screen().contents(), host.screen().contents());
            assert_eq!(
                guest.screen().cursor_position(),
                host.screen().cursor_position()
            );
        }
        assert_eq!(
            guest.screen().contents_formatted(),
            host.screen().contents_formatted(),
            "colors and attributes too"
        );
    }

    /// T696: a guest-named path resolves only to a regular, non-private file
    /// inside the root, even through symlinks.
    #[test]
    fn confine_keeps_guests_inside_the_root() {
        let base = std::env::temp_dir().join(format!("vix-cowork-confine-{}", std::process::id()));
        let root = base.join("root");
        let outside = base.join("outside");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(root.join("src/a.rs"), "a").unwrap();
        std::fs::write(root.join("notes.txt"), "TOKEN=1").unwrap();
        std::fs::write(outside.join("secret.txt"), "s").unwrap();

        assert_eq!(confine(&root, &p("src/a.rs")), Some(root.join("src/a.rs")));
        assert_eq!(confine(&root, &p("src")), None, "a directory");
        assert_eq!(confine(&root, &p("missing.txt")), None);
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(outside.join("secret.txt"), root.join("escape.txt")).unwrap();
            symlink(&outside, root.join("out")).unwrap();
            assert_eq!(
                confine(&root, &p("escape.txt")),
                None,
                "symlink out of the root"
            );
            assert_eq!(
                confine(&root, &p("out/secret.txt")),
                None,
                "via a linked directory"
            );
            // A harmless name that points at a private file inside the root.
            std::fs::write(root.join(".env"), "KEY=1").unwrap();
            symlink(root.join(".env"), root.join("config.txt")).unwrap();
            assert_eq!(
                confine(&root, &p("config.txt")),
                None,
                "symlink to a private file"
            );
        }
        std::fs::remove_dir_all(&base).ok();
    }
}
