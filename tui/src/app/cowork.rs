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
    BufferEdit, DEFAULT_CAP_BYTES, ErrorCode, GuestEvent, GuestSession, HostSession, Outbound,
    PeerId, Permission, SharedPath,
};
use vix_cowork_net::{Invite, Joined, Knock, Link, LinkEvent, Listener, NetError};
use vix_editor_core::remote::{RemoteCaret, Splice};

use super::{App, Prompt, PromptKind};
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

struct Host {
    listener: Listener,
    session: HostSession,
    links: BTreeMap<PeerId, Link>,
    advertise: String,
}

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

fn level_name(p: Permission) -> &'static str {
    match p {
        Permission::View => "view",
        Permission::Edit => "edit",
        _ => "terminal",
    }
}

fn parse_level(word: &str) -> Option<Permission> {
    match word.trim().to_ascii_lowercase().as_str() {
        "view" => Some(Permission::View),
        "edit" => Some(Permission::Edit),
        "terminal" => Some(Permission::EditTerminal),
        _ => None,
    }
}

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
            _ => return false,
        }
        true
    }

    fn cowork_error(&mut self, error: impl std::fmt::Display) {
        self.messages
            .error(t!("msg.cowork_error", error = error.to_string()).to_string());
    }

    fn cowork_busy(&self) -> bool {
        self.cowork.host.is_some() || self.cowork.guest.is_some() || self.cowork.joining.is_some()
    }

    // ----- user actions -----------------------------------------------------

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
        self.cowork_invite();
    }

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

    fn cowork_unshare(&mut self) {
        let Some(rel) = self.cowork_active_rel() else {
            return;
        };
        self.cowork_unshare_path(&rel);
    }

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
            _ => {}
        }
    }

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

    fn cowork_host_tab(&self, rel: &SharedPath) -> Option<usize> {
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
            (self.cowork_active_rel(), h.session.board(), PeerId::HOST)
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
        let reverted = if let Some(rel) = self.cowork_active_rel()
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

    // ----- follow mode (T693) ---------------------------------------------------

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
        self.cowork_follow_tick();
    }

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
                }
                Err(e) => self.cowork_error(e),
            },
            Err(e) => self.cowork_error(e),
        }
    }

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

    fn cowork_pump_local(&mut self) {
        // Host: every shared buffer.
        let shared: Vec<SharedPath> = self
            .cowork
            .host
            .as_ref()
            .map(|h| h.session.shared().cloned().collect())
            .unwrap_or_default();
        for rel in shared {
            let Some(i) = self.cowork_host_tab(&rel) else {
                // The tab was closed: stop sharing it.
                self.cowork_unshare_path(&rel);
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

    fn cowork_guest_lost(&mut self) {
        if let Some(g) = self.cowork.guest.take() {
            g.link.close();
        }
        self.cowork_release_guest_tabs();
        self.status = t!("status.cowork_left").to_string();
    }

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

    fn cowork_drop_guest(&mut self, peer: PeerId) {
        let Some(host) = self.cowork.host.as_mut() else {
            return;
        };
        let name = host.session.guest(peer).map(|g| g.name.clone());
        let out = host.session.remove(peer);
        if let Some(link) = host.links.remove(&peer) {
            link.close();
        }
        self.cowork_send(&out);
        if let Some(name) = name {
            self.status = t!("status.cowork_peer_left", name = name).to_string();
        }
    }

    fn cowork_host_message(&mut self, peer: PeerId, msg: Message) {
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
        for gone in handled.disconnect {
            if let Some(h) = self.cowork.host.as_mut()
                && let Some(link) = h.links.remove(&gone)
            {
                link.close();
            }
        }
    }

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
                }
                GuestEvent::Refused(ErrorCode::TooLarge) => {
                    self.status = t!("status.cowork_too_large").to_string();
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
                _ => {}
            }
        }
    }

    fn cowork_open_remote(&mut self, rel: &SharedPath, text: &str) {
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

    fn cowork_pump_presence(&mut self) {
        // Background color for contrast-checked peer colors.
        let bg = match crate::theme::region_base(crate::theme::Region::Editor).bg {
            Some(ratatui::style::Color::Rgb(r, g, b)) => Rgb(r, g, b),
            _ => Rgb(0x1e, 0x1e, 0x1e),
        };
        let managed: Vec<(usize, SharedPath)> = if self.cowork.host.is_some() {
            let shared: Vec<SharedPath> = self
                .cowork
                .host
                .as_ref()
                .map(|h| h.session.shared().cloned().collect())
                .unwrap_or_default();
            shared
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
