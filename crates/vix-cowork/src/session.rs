//! The host and guest session state machines, still free of I/O.
//!
//! [`HostSession`] is the authority: it owns the shared documents, assigns
//! peer ids, enforces permissions on every inbound [`Message`], and decides
//! what to relay to whom. [`GuestSession`] mirrors the host's documents and
//! turns inbound messages into events for the editor. Both return the
//! messages to send as plain values; the transport (`vix-cowork-net`) moves
//! them. That keeps every rule here testable with two structs and no socket.

use crate::doc::{BufferEdit, SharedDoc};
use crate::error::CoworkError;
use crate::path::SharedPath;
use crate::peer::{Action, Peer, PeerId, Permission};
use crate::presence::PresenceBoard;
use crate::wire::{ErrorCode, Message, Presence};
use std::collections::BTreeMap;

/// A message and who it is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outbound {
    /// The recipient.
    pub to: PeerId,
    /// The message.
    pub msg: Message,
}

/// What a host undo or redo produced: the update for every guest, and the
/// edits to replay on the host's own buffer.
pub type HostRevert = (Vec<Outbound>, Vec<BufferEdit>);

/// What the host should do after handling one inbound message.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Handled {
    /// Messages to send.
    pub out: Vec<Outbound>,
    /// Edits to replay onto the host's own buffers, per path.
    pub edits: Vec<(SharedPath, Vec<BufferEdit>)>,
    /// Buffers a permitted guest asked the host to write to disk, with who
    /// asked (answer with [`HostSession::saved`]).
    pub save: Vec<(PeerId, SharedPath)>,
    /// Connections the host must drop (misbehaving or leaving).
    pub disconnect: Vec<PeerId>,
    /// Guests that asked for the file list (T696); answer each with
    /// [`HostSession::file_list`]. Only produced while the workspace is shared.
    pub list_files: Vec<PeerId>,
    /// Files a guest asked to have shared (T696), already checked for
    /// workspace sharing, permission and [`SharedPath::is_private`]. The host
    /// still resolves each against its root on disk, refusing a symlink that
    /// leaves it, then calls [`HostSession::share`] (or
    /// [`HostSession::reopen_for`] when it is already shared).
    pub open: Vec<(PeerId, SharedPath)>,
    /// Keystrokes a guest typed into the shared terminal (T697), already
    /// checked: the terminal is shared, the host allows input, and the guest
    /// has terminal permission. Write them to the terminal in order.
    pub terminal_keys: Vec<(PeerId, Vec<u8>)>,
}

/// The host's shared terminal (T697): shared at all, and whether guests with
/// terminal permission may type in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TerminalShare {
    input: bool,
}

/// The host's side of one session.
#[derive(Debug)]
pub struct HostSession {
    host_name: String,
    cap: usize,
    peers: BTreeMap<PeerId, Peer>,
    docs: BTreeMap<SharedPath, SharedDoc>,
    board: PresenceBoard,
    next_guest: u64,
    /// Whether guests may list the host's files and ask to open them (T696).
    /// Off until the host turns it on.
    workspace: bool,
    /// The shared terminal (T697), if the host shares one.
    terminal: Option<TerminalShare>,
}

impl HostSession {
    /// Start a session with nothing shared and no guests.
    #[must_use]
    pub fn new(host_name: &str, cap: usize) -> Self {
        let mut board = PresenceBoard::new();
        let host_name = crate::peer::sanitize_name(host_name);
        board.join(PeerId::HOST, &host_name);
        Self {
            host_name,
            cap,
            peers: BTreeMap::new(),
            docs: BTreeMap::new(),
            board,
            next_guest: 0,
            workspace: false,
            terminal: None,
        }
    }

    // ----- shared terminal (T697) ---------------------------------------------

    /// Share the host's terminal with every guest, or send its whole screen
    /// again (after a resize). `screen` is the escape-sequence rendering of
    /// a `rows` x `cols` screen; the session does not interpret it. A new
    /// share starts **read-only**: guests may not type until
    /// [`set_terminal_input`](Self::set_terminal_input) allows it, and a
    /// resend keeps whatever the host chose.
    #[must_use]
    pub fn share_terminal(&mut self, rows: u16, cols: u16, screen: &[u8]) -> Vec<Outbound> {
        let share = *self.terminal.get_or_insert(TerminalShare { input: false });
        self.to_all(None, &Self::terminal_screen(share, rows, cols, screen))
    }

    /// Send the shared terminal's whole screen to one guest (it just joined).
    /// Nothing when the terminal is not shared.
    #[must_use]
    pub fn terminal_for(&self, to: PeerId, rows: u16, cols: u16, screen: &[u8]) -> Vec<Outbound> {
        match self.terminal {
            Some(share) if self.peers.contains_key(&to) => vec![Outbound {
                to,
                msg: Self::terminal_screen(share, rows, cols, screen),
            }],
            _ => Vec::new(),
        }
    }

    fn terminal_screen(share: TerminalShare, rows: u16, cols: u16, screen: &[u8]) -> Message {
        Message::TerminalScreen {
            rows: rows.clamp(1, crate::wire::MAX_TERMINAL_ROWS),
            cols: cols.clamp(1, crate::wire::MAX_TERMINAL_COLS),
            input: share.input,
            screen: screen.to_vec(),
        }
    }

    /// What changed on the shared terminal, for every guest. Nothing when
    /// the terminal is not shared or `bytes` is empty.
    #[must_use]
    pub fn terminal_output(&self, bytes: &[u8]) -> Vec<Outbound> {
        if self.terminal.is_none() || bytes.is_empty() {
            return Vec::new();
        }
        self.to_all(
            None,
            &Message::TerminalOutput {
                bytes: bytes.to_vec(),
            },
        )
    }

    /// Allow or stop keystrokes from guests with terminal permission. Off
    /// for every new share. Nothing when the terminal is not shared.
    #[must_use]
    pub fn set_terminal_input(&mut self, allowed: bool) -> Vec<Outbound> {
        let Some(share) = self.terminal.as_mut() else {
            return Vec::new();
        };
        share.input = allowed;
        self.to_all(None, &Message::TerminalInputAllowed { allowed })
    }

    /// Stop sharing the terminal. Input permission is forgotten with it.
    #[must_use]
    pub fn unshare_terminal(&mut self) -> Vec<Outbound> {
        if self.terminal.take().is_some() {
            self.to_all(None, &Message::TerminalClose)
        } else {
            Vec::new()
        }
    }

    /// Whether the terminal is shared.
    #[must_use]
    pub const fn terminal_shared(&self) -> bool {
        self.terminal.is_some()
    }

    /// Whether guests with terminal permission may type in the shared
    /// terminal.
    #[must_use]
    pub const fn terminal_input(&self) -> bool {
        matches!(self.terminal, Some(TerminalShare { input: true }))
    }

    /// Let guests browse the file tree and ask for files (T696), or stop.
    /// Off by default: until then a guest sees only what the host shares.
    pub fn set_workspace_shared(&mut self, on: bool) {
        self.workspace = on;
    }

    /// Whether the file tree is shared.
    #[must_use]
    pub const fn workspace_shared(&self) -> bool {
        self.workspace
    }

    /// Answer a guest's [`Message::ListFiles`] with `paths`, dropping every
    /// [`SharedPath::is_private`] one and capping the list at
    /// [`MAX_LISTED_FILES`](crate::wire::MAX_LISTED_FILES). Sends nothing
    /// while the workspace is not shared.
    #[must_use]
    pub fn file_list(
        &self,
        to: PeerId,
        paths: impl IntoIterator<Item = SharedPath>,
    ) -> Vec<Outbound> {
        if !self.workspace || !self.peers.contains_key(&to) {
            return Vec::new();
        }
        let paths: Vec<SharedPath> = paths
            .into_iter()
            .filter(|p| !p.is_private())
            .take(crate::wire::MAX_LISTED_FILES)
            .collect();
        vec![Outbound {
            to,
            msg: Message::FileList { paths },
        }]
    }

    /// Send an already shared buffer's current state to one guest (it asked
    /// to open a file that is shared already).
    ///
    /// # Errors
    ///
    /// When the document cannot be snapshotted (not expected).
    pub fn reopen_for(&self, to: PeerId, path: &SharedPath) -> Result<Vec<Outbound>, CoworkError> {
        let Some(doc) = self.docs.get(path) else {
            return Ok(Vec::new());
        };
        Ok(vec![Outbound {
            to,
            msg: Message::Open {
                path: path.clone(),
                snapshot: doc.snapshot()?,
            },
        }])
    }

    /// Refuse a guest's request (for example a file the host could not
    /// open).
    #[must_use]
    pub fn refuse_to(to: PeerId, code: ErrorCode) -> Vec<Outbound> {
        vec![Self::refuse(to, code)]
    }

    /// Per-peer undo of the host's own last edit group on `path`. Returns the
    /// update for every guest and the edits to replay on the host's buffer;
    /// `None` when there is nothing to undo.
    ///
    /// # Errors
    ///
    /// When `path` is not shared or the CRDT fails.
    pub fn undo(&mut self, path: &SharedPath) -> Result<Option<HostRevert>, CoworkError> {
        self.revert(path, true)
    }

    /// Redo what [`undo`](Self::undo) reverted.
    ///
    /// # Errors
    ///
    /// When `path` is not shared or the CRDT fails.
    pub fn redo(&mut self, path: &SharedPath) -> Result<Option<HostRevert>, CoworkError> {
        self.revert(path, false)
    }

    fn revert(&mut self, path: &SharedPath, undo: bool) -> Result<Option<HostRevert>, CoworkError> {
        let doc = self
            .docs
            .get_mut(path)
            .ok_or(CoworkError::BadPath(crate::error::PathProblem::Empty))?;
        let reverted = if undo { doc.undo()? } else { doc.redo()? };
        let Some(r) = reverted else {
            return Ok(None);
        };
        let out = if r.update.is_empty() {
            Vec::new()
        } else {
            self.to_all(
                None,
                &Message::Update {
                    path: path.clone(),
                    bytes: r.update,
                },
            )
        };
        Ok(Some((out, r.edits)))
    }

    /// Connected guests.
    pub fn guests(&self) -> impl Iterator<Item = &Peer> {
        self.peers.values()
    }

    /// A guest by id.
    #[must_use]
    pub fn guest(&self, id: PeerId) -> Option<&Peer> {
        self.peers.get(&id)
    }

    /// The presence board (host's view).
    #[must_use]
    pub const fn board(&self) -> &PresenceBoard {
        &self.board
    }

    /// A shared document, for rendering remote cursors.
    #[must_use]
    pub fn doc(&self, path: &SharedPath) -> Option<&SharedDoc> {
        self.docs.get(path)
    }

    /// Paths currently shared.
    pub fn shared(&self) -> impl Iterator<Item = &SharedPath> {
        self.docs.keys()
    }

    fn to_all(&self, except: Option<PeerId>, msg: &Message) -> Vec<Outbound> {
        self.peers
            .keys()
            .filter(|p| Some(**p) != except)
            .map(|p| Outbound {
                to: *p,
                msg: msg.clone(),
            })
            .collect()
    }

    /// Share a buffer (opt-in, per file). Existing guests are sent a snapshot.
    ///
    /// # Errors
    ///
    /// When the text is over the cap or binary, or [`CoworkError::PermissionDenied`]
    /// for a [`SharedPath::is_private`] path (secrets and settings are never
    /// shared).
    pub fn share(&mut self, path: &SharedPath, text: &str) -> Result<Vec<Outbound>, CoworkError> {
        if path.is_private() {
            return Err(CoworkError::PermissionDenied);
        }
        let doc = SharedDoc::host(PeerId::HOST, text, self.cap)?;
        let open = Message::Open {
            path: path.clone(),
            snapshot: doc.snapshot()?,
        };
        self.docs.insert(path.clone(), doc);
        Ok(self.to_all(None, &open))
    }

    /// Stop sharing a buffer.
    pub fn unshare(&mut self, path: &SharedPath) -> Vec<Outbound> {
        if self.docs.remove(path).is_some() {
            self.to_all(None, &Message::Close { path: path.clone() })
        } else {
            Vec::new()
        }
    }

    /// Admit a guest the host has just confirmed by name, at `permission`.
    /// Returns the new peer and the messages that bring it up to date.
    ///
    /// # Errors
    ///
    /// When a shared document cannot be snapshotted (not expected).
    pub fn admit(
        &mut self,
        name: &str,
        permission: Permission,
    ) -> Result<(Peer, Vec<Outbound>), CoworkError> {
        let mut peer = Peer::new(PeerId::guest(self.next_guest), name);
        self.next_guest += 1;
        peer.permission = permission;
        let id = peer.id;
        let mut out = vec![Outbound {
            to: id,
            msg: Message::Welcome {
                peer: id,
                permission,
                host_name: self.host_name.clone(),
            },
        }];
        for (who, name) in self.board.peers() {
            out.push(Outbound {
                to: id,
                msg: Message::PeerJoined {
                    peer: who,
                    name: name.to_string(),
                },
            });
        }
        for (path, doc) in &self.docs {
            out.push(Outbound {
                to: id,
                msg: Message::Open {
                    path: path.clone(),
                    snapshot: doc.snapshot()?,
                },
            });
        }
        // Inserted first so the newcomer also hears its own join (it learns
        // the sanitized name the host will show for it).
        self.board.join(id, &peer.name);
        self.peers.insert(id, peer.clone());
        out.extend(self.to_all(
            None,
            &Message::PeerJoined {
                peer: id,
                name: peer.name.clone(),
            },
        ));
        Ok((peer, out))
    }

    /// Remove a guest (it left, or the host removed it).
    pub fn remove(&mut self, id: PeerId) -> Vec<Outbound> {
        if self.peers.remove(&id).is_none() {
            return Vec::new();
        }
        self.board.leave(id);
        self.to_all(None, &Message::PeerLeft { peer: id })
    }

    /// Change a guest's permission and tell it.
    pub fn set_permission(&mut self, id: PeerId, permission: Permission) -> Vec<Outbound> {
        match self.peers.get_mut(&id) {
            Some(p) => {
                p.permission = permission;
                vec![Outbound {
                    to: id,
                    msg: Message::PermissionChanged { permission },
                }]
            }
            None => Vec::new(),
        }
    }

    /// The host edited a shared buffer locally.
    ///
    /// # Errors
    ///
    /// When `path` is not shared or the edit is invalid.
    pub fn local_edit(
        &mut self,
        path: &SharedPath,
        offset: usize,
        delete: usize,
        insert: &str,
    ) -> Result<Vec<Outbound>, CoworkError> {
        let doc = self
            .docs
            .get_mut(path)
            .ok_or(CoworkError::BadPath(crate::error::PathProblem::Empty))?;
        let Some(bytes) = doc.edit(offset, delete, insert)? else {
            return Ok(Vec::new());
        };
        let msg = Message::Update {
            path: path.clone(),
            bytes,
        };
        Ok(self.to_all(None, &msg))
    }

    /// The host's own cursor moved.
    #[must_use]
    pub fn local_cursor(
        &self,
        path: &SharedPath,
        head: usize,
        anchor: Option<usize>,
    ) -> Vec<Outbound> {
        let Some(doc) = self.docs.get(path) else {
            return Vec::new();
        };
        let Some(head) = doc.anchor_at(head) else {
            return Vec::new();
        };
        let msg = Message::Presence(Presence {
            peer: PeerId::HOST,
            path: path.clone(),
            head,
            anchor: anchor.and_then(|a| doc.anchor_at(a)),
        });
        self.to_all(None, &msg)
    }

    /// Tell a guest its save request was carried out.
    #[must_use]
    pub fn saved(&self, to: PeerId, path: &SharedPath) -> Vec<Outbound> {
        vec![Outbound {
            to,
            msg: Message::Saved { path: path.clone() },
        }]
    }

    fn refuse(to: PeerId, code: ErrorCode) -> Outbound {
        Outbound {
            to,
            msg: Message::Error { code },
        }
    }

    /// Handle one message from guest `from`.
    ///
    /// # Errors
    ///
    /// [`CoworkError::PermissionDenied`] only for a `from` that is not in the
    /// session at all; protocol misbehavior is reported through
    /// [`Handled::disconnect`] instead.
    pub fn handle(&mut self, from: PeerId, msg: Message) -> Result<Handled, CoworkError> {
        let Some(peer) = self.peers.get(&from).cloned() else {
            return Err(CoworkError::PermissionDenied);
        };
        let mut h = Handled::default();
        match msg {
            Message::Update { path, bytes } => self.on_update(&peer, &path, &bytes, &mut h),
            Message::Presence(p) => {
                // The sender's claimed id is ignored: use the connection's.
                let stamped = Presence { peer: from, ..p };
                if self.docs.contains_key(&stamped.path) {
                    self.board.update(stamped.clone());
                    h.out
                        .extend(self.to_all(Some(from), &Message::Presence(stamped)));
                }
            }
            Message::SaveRequest { path } => {
                if peer.permission.allows(Action::Save) && self.docs.contains_key(&path) {
                    h.save.push((from, path));
                } else {
                    h.out.push(Self::refuse(from, ErrorCode::PermissionDenied));
                }
            }
            Message::ListFiles => {
                if self.workspace && peer.permission.allows(Action::Read) {
                    h.list_files.push(from);
                } else {
                    h.out.push(Self::refuse(from, ErrorCode::PermissionDenied));
                }
            }
            Message::OpenRequest { path } => {
                if !self.workspace || !peer.permission.allows(Action::Read) {
                    h.out.push(Self::refuse(from, ErrorCode::PermissionDenied));
                } else if path.is_private() {
                    h.out.push(Self::refuse(from, ErrorCode::BadPath));
                } else {
                    h.open.push((from, path));
                }
            }
            Message::TerminalKeys { bytes } => {
                // Typing needs all three: a shared terminal, the host's
                // explicit go-ahead, and terminal permission for this guest.
                // A refusal is not fatal: the guest stays connected.
                if self.terminal_input() && peer.permission.allows(Action::Terminal) {
                    if !bytes.is_empty() {
                        h.terminal_keys.push((from, bytes));
                    }
                } else {
                    h.out.push(Self::refuse(from, ErrorCode::PermissionDenied));
                }
            }
            // `Leave`, or anything else: a guest sends nothing but updates,
            // presence, save, list, open requests and terminal keys after the
            // handshake.
            _ => {
                h.out.extend(self.remove(from));
                h.disconnect.push(from);
            }
        }
        Ok(h)
    }

    fn on_update(&mut self, peer: &Peer, path: &SharedPath, bytes: &[u8], h: &mut Handled) {
        let from = peer.id;
        let Some(doc) = self.docs.get_mut(path) else {
            h.out.push(Self::refuse(from, ErrorCode::BadPath));
            return;
        };
        match doc.apply_guest_update(peer, bytes) {
            Ok(edits) => {
                h.edits.push((path.clone(), edits));
                let relay = Message::Update {
                    path: path.clone(),
                    bytes: bytes.to_vec(),
                };
                h.out.extend(self.to_all(Some(from), &relay));
            }
            Err(CoworkError::PermissionDenied) => {
                h.out.push(Self::refuse(from, ErrorCode::PermissionDenied));
            }
            Err(e) => {
                let code = if matches!(e, CoworkError::TooLarge { .. }) {
                    ErrorCode::TooLarge
                } else {
                    ErrorCode::Removed
                };
                h.out.push(Self::refuse(from, code));
                h.out.extend(self.remove(from));
                h.disconnect.push(from);
            }
        }
    }
}

/// What a guest session reports to the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum GuestEvent {
    /// The host shared a buffer: open it with this text.
    Opened(SharedPath, String),
    /// The host stopped sharing a buffer.
    Closed(SharedPath),
    /// Replay these edits onto the open buffer.
    Edits(SharedPath, Vec<BufferEdit>),
    /// The host wrote the buffer to disk.
    Saved(SharedPath),
    /// Our permission changed.
    Permission(Permission),
    /// The host refused something, or removed us.
    Refused(ErrorCode),
    /// The host's file list arrived (T696): files this guest may open.
    Files(Vec<SharedPath>),
    /// A participant joined or left, or a cursor moved: redraw presence.
    Presence,
    /// The session is over.
    Ended,
    /// The host shared its terminal, or sent its whole screen again (T697):
    /// start a fresh `rows` x `cols` emulator and feed it `screen`.
    TerminalScreen {
        /// Screen height.
        rows: u16,
        /// Screen width.
        cols: u16,
        /// Escape sequences that draw the whole screen.
        screen: Vec<u8>,
    },
    /// Output on the shared terminal: feed it to the emulator.
    TerminalOutput(Vec<u8>),
    /// The host allowed (`true`) or stopped terminal input.
    TerminalInput(bool),
    /// The host stopped sharing its terminal.
    TerminalClosed,
}

/// A guest's side of one session.
#[derive(Debug)]
pub struct GuestSession {
    me: PeerId,
    permission: Permission,
    host_name: String,
    cap: usize,
    docs: BTreeMap<SharedPath, SharedDoc>,
    board: PresenceBoard,
    /// The host's shared terminal, if any (T697).
    terminal: Option<TerminalShare>,
}

impl GuestSession {
    /// Begin from the host's `Welcome`.
    ///
    /// # Errors
    ///
    /// When `welcome` is not a `Welcome` message.
    pub fn from_welcome(welcome: &Message, cap: usize) -> Result<Self, CoworkError> {
        let Message::Welcome {
            peer,
            permission,
            host_name,
        } = welcome
        else {
            return Err(CoworkError::PermissionDenied);
        };
        Ok(Self {
            me: *peer,
            permission: *permission,
            host_name: host_name.clone(),
            cap,
            docs: BTreeMap::new(),
            board: PresenceBoard::new(),
            terminal: None,
        })
    }

    /// Whether the host shares its terminal with this session (T697).
    #[must_use]
    pub const fn terminal_shared(&self) -> bool {
        self.terminal.is_some()
    }

    /// Whether this guest may type in the shared terminal: the host shares
    /// it, allows input, and gave this guest terminal permission.
    #[must_use]
    pub const fn can_type_in_terminal(&self) -> bool {
        matches!(self.terminal, Some(TerminalShare { input: true }))
            && self.permission.allows(Action::Terminal)
    }

    /// Keystrokes (already encoded as terminal input) for the shared
    /// terminal. The host checks again; this only saves a round trip.
    ///
    /// # Errors
    ///
    /// [`CoworkError::PermissionDenied`] when this guest may not type
    /// ([`can_type_in_terminal`](Self::can_type_in_terminal)), or
    /// [`CoworkError::Wire`] when `bytes` is over
    /// [`MAX_TERMINAL_KEYS`](crate::wire::MAX_TERMINAL_KEYS).
    pub fn terminal_keys(&self, bytes: &[u8]) -> Result<Message, CoworkError> {
        if !self.can_type_in_terminal() {
            return Err(CoworkError::PermissionDenied);
        }
        if bytes.len() > crate::wire::MAX_TERMINAL_KEYS {
            return Err(crate::error::WireProblem::FieldTooLong.into());
        }
        Ok(Message::TerminalKeys {
            bytes: bytes.to_vec(),
        })
    }

    /// This guest's id.
    #[must_use]
    pub const fn me(&self) -> PeerId {
        self.me
    }

    /// This guest's current permission.
    #[must_use]
    pub const fn permission(&self) -> Permission {
        self.permission
    }

    /// The host's display name.
    #[must_use]
    pub fn host_name(&self) -> &str {
        &self.host_name
    }

    /// The presence board.
    #[must_use]
    pub const fn board(&self) -> &PresenceBoard {
        &self.board
    }

    /// A shared document, for rendering remote cursors.
    #[must_use]
    pub fn doc(&self, path: &SharedPath) -> Option<&SharedDoc> {
        self.docs.get(path)
    }

    /// Handle one message from the host.
    ///
    /// # Errors
    ///
    /// When an update or snapshot is invalid; the caller should leave.
    pub fn handle(&mut self, msg: Message) -> Result<Vec<GuestEvent>, CoworkError> {
        Ok(match msg {
            Message::Open { path, snapshot } => {
                let doc = SharedDoc::from_snapshot(self.me, &snapshot, self.cap)?;
                let text = doc.text();
                self.docs.insert(path.clone(), doc);
                vec![GuestEvent::Opened(path, text)]
            }
            Message::Close { path } => {
                self.docs.remove(&path);
                vec![GuestEvent::Closed(path)]
            }
            Message::Update { path, bytes } => {
                let Some(doc) = self.docs.get_mut(&path) else {
                    return Ok(Vec::new());
                };
                let edits = doc.apply_host_update(&bytes)?;
                vec![GuestEvent::Edits(path, edits)]
            }
            Message::Presence(p) => {
                self.board.update(p);
                vec![GuestEvent::Presence]
            }
            Message::PeerJoined { peer, name } => {
                self.board.join(peer, &name);
                vec![GuestEvent::Presence]
            }
            Message::PeerLeft { peer } => {
                self.board.leave(peer);
                vec![GuestEvent::Presence]
            }
            Message::Saved { path } => vec![GuestEvent::Saved(path)],
            Message::PermissionChanged { permission } => {
                self.permission = permission;
                vec![GuestEvent::Permission(permission)]
            }
            Message::Error { code } => vec![GuestEvent::Refused(code)],
            Message::FileList { paths } => vec![GuestEvent::Files(paths)],
            Message::TerminalScreen {
                rows,
                cols,
                input,
                screen,
            } => {
                self.terminal = Some(TerminalShare { input });
                vec![GuestEvent::TerminalScreen { rows, cols, screen }]
            }
            Message::TerminalOutput { bytes } if self.terminal.is_some() => {
                vec![GuestEvent::TerminalOutput(bytes)]
            }
            Message::TerminalInputAllowed { allowed } => match self.terminal.as_mut() {
                Some(share) => {
                    share.input = allowed;
                    vec![GuestEvent::TerminalInput(allowed)]
                }
                None => Vec::new(),
            },
            Message::TerminalClose => {
                if self.terminal.take().is_some() {
                    vec![GuestEvent::TerminalClosed]
                } else {
                    Vec::new()
                }
            }
            Message::Leave => vec![GuestEvent::Ended],
            _ => Vec::new(),
        })
    }

    /// Record a local edit; returns the message to send, or `None`.
    ///
    /// # Errors
    ///
    /// When this guest may not edit, the buffer is not shared, or the edit is
    /// invalid.
    pub fn edit(
        &mut self,
        path: &SharedPath,
        offset: usize,
        delete: usize,
        insert: &str,
    ) -> Result<Option<Message>, CoworkError> {
        self.permission.require(Action::Edit)?;
        let doc = self
            .docs
            .get_mut(path)
            .ok_or(CoworkError::BadPath(crate::error::PathProblem::Empty))?;
        Ok(doc
            .edit(offset, delete, insert)?
            .map(|bytes| Message::Update {
                path: path.clone(),
                bytes,
            }))
    }

    /// The local cursor moved: the presence message to send.
    #[must_use]
    pub fn cursor(&self, path: &SharedPath, head: usize, anchor: Option<usize>) -> Option<Message> {
        let doc = self.docs.get(path)?;
        Some(Message::Presence(Presence {
            peer: self.me,
            path: path.clone(),
            head: doc.anchor_at(head)?,
            anchor: anchor.and_then(|a| doc.anchor_at(a)),
        }))
    }

    /// Ask the host to write `path` to disk.
    ///
    /// # Errors
    ///
    /// When this guest may not save.
    pub fn request_save(&self, path: &SharedPath) -> Result<Message, CoworkError> {
        self.permission.require(Action::Save)?;
        Ok(Message::SaveRequest { path: path.clone() })
    }

    /// Ask the host for the files this guest may open (T696).
    #[must_use]
    pub const fn request_files(&self) -> Message {
        Message::ListFiles
    }

    /// Ask the host to share `path` (T696). The host decides.
    #[must_use]
    pub fn request_open(&self, path: &SharedPath) -> Message {
        Message::OpenRequest { path: path.clone() }
    }

    /// Per-peer undo of this guest's last edit group.
    ///
    /// # Errors
    ///
    /// When the buffer is not shared or the CRDT fails.
    pub fn undo(
        &mut self,
        path: &SharedPath,
    ) -> Result<Option<(Message, Vec<BufferEdit>)>, CoworkError> {
        self.revert(path, true)
    }

    /// Redo what [`undo`](Self::undo) reverted.
    ///
    /// # Errors
    ///
    /// When the buffer is not shared or the CRDT fails.
    pub fn redo(
        &mut self,
        path: &SharedPath,
    ) -> Result<Option<(Message, Vec<BufferEdit>)>, CoworkError> {
        self.revert(path, false)
    }

    fn revert(
        &mut self,
        path: &SharedPath,
        undo: bool,
    ) -> Result<Option<(Message, Vec<BufferEdit>)>, CoworkError> {
        self.permission.require(Action::Edit)?;
        let doc = self
            .docs
            .get_mut(path)
            .ok_or(CoworkError::BadPath(crate::error::PathProblem::Empty))?;
        let reverted = if undo { doc.undo()? } else { doc.redo()? };
        Ok(reverted.map(|r| {
            (
                Message::Update {
                    path: path.clone(),
                    bytes: r.update,
                },
                r.edits,
            )
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::DEFAULT_CAP_BYTES;

    fn path(s: &str) -> SharedPath {
        SharedPath::parse(s).unwrap()
    }

    /// Deliver `out` from the host to the guests it names.
    fn deliver(out: Vec<Outbound>, guests: &mut [&mut GuestSession]) -> Vec<(PeerId, GuestEvent)> {
        let mut events = Vec::new();
        for o in out {
            for g in guests.iter_mut() {
                if g.me() == o.to {
                    for e in g.handle(o.msg.clone()).unwrap() {
                        events.push((o.to, e));
                    }
                }
            }
        }
        events
    }

    fn join(host: &mut HostSession, name: &str, perm: Permission) -> (GuestSession, Vec<Outbound>) {
        let (peer, out) = host.admit(name, perm).unwrap();
        let welcome = out
            .iter()
            .find(|o| o.to == peer.id && matches!(o.msg, Message::Welcome { .. }))
            .unwrap();
        let g = GuestSession::from_welcome(&welcome.msg, DEFAULT_CAP_BYTES).unwrap();
        (g, out)
    }

    #[test]
    fn nothing_is_shared_until_the_host_opts_in() {
        let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
        let (mut g, out) = join(&mut host, "Ada", Permission::View);
        let events = deliver(out, &mut [&mut g]);
        assert!(
            !events
                .iter()
                .any(|(_, e)| matches!(e, GuestEvent::Opened(..)))
        );
        assert!(g.doc(&path("a.txt")).is_none());
        // Sharing afterwards reaches the guest.
        let out = host.share(&path("a.txt"), "hello").unwrap();
        let events = deliver(out, &mut [&mut g]);
        assert_eq!(
            events[0].1,
            GuestEvent::Opened(path("a.txt"), "hello".into())
        );
    }

    #[test]
    fn a_late_joiner_gets_open_buffers_and_the_roster() {
        let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
        host.share(&path("a.txt"), "abc").unwrap();
        let (mut g, out) = join(&mut host, "Ada", Permission::View);
        let events = deliver(out, &mut [&mut g]);
        assert!(
            events
                .iter()
                .any(|(_, e)| *e == GuestEvent::Opened(path("a.txt"), "abc".into()))
        );
        assert_eq!(g.board().name(PeerId::HOST), Some("Host"));
        assert_eq!(g.board().name(g.me()), Some("Ada"));
        assert_eq!(g.permission(), Permission::View);
    }

    #[test]
    fn view_guests_default_cannot_edit_either_side() {
        let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
        host.share(&path("a"), "abc").unwrap();
        let (mut g, out) = join(&mut host, "Ada", Permission::View);
        deliver(out, &mut [&mut g]);
        assert_eq!(
            g.edit(&path("a"), 0, 0, "x"),
            Err(CoworkError::PermissionDenied)
        );
        assert_eq!(
            g.request_save(&path("a")),
            Err(CoworkError::PermissionDenied)
        );
    }

    #[test]
    fn host_rejects_a_view_guests_hand_crafted_update() {
        let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
        host.share(&path("a"), "abc").unwrap();
        let (mut view, out) = join(&mut host, "Ada", Permission::View);
        deliver(out, &mut [&mut view]);
        // The guest raises its own local permission to craft a real update.
        view.permission = Permission::Edit;
        let msg = view.edit(&path("a"), 0, 0, "x").unwrap().unwrap();
        let h = host.handle(view.me(), msg).unwrap();
        assert_eq!(h.edits, Vec::new());
        assert_eq!(
            h.out,
            vec![Outbound {
                to: view.me(),
                msg: Message::Error {
                    code: ErrorCode::PermissionDenied
                }
            }]
        );
        assert_eq!(host.doc(&path("a")).unwrap().text(), "abc");
    }

    #[test]
    fn edits_flow_guest_to_host_to_other_guests() {
        let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
        host.share(&path("a"), "hello").unwrap();
        let (mut g1, o1) = join(&mut host, "One", Permission::Edit);
        let (mut g2, o2) = join(&mut host, "Two", Permission::View);
        deliver(o1, &mut [&mut g1, &mut g2]);
        deliver(o2, &mut [&mut g1, &mut g2]);

        let msg = g1.edit(&path("a"), 5, 0, "!").unwrap().unwrap();
        let h = host.handle(g1.me(), msg).unwrap();
        assert_eq!(h.edits[0].1[0].insert, "!");
        let events = deliver(h.out, &mut [&mut g1, &mut g2]);
        assert!(events.iter().all(|(to, _)| *to == g2.me()));
        assert_eq!(host.doc(&path("a")).unwrap().text(), "hello!");
        assert_eq!(g2.doc(&path("a")).unwrap().text(), "hello!");
    }

    #[test]
    fn a_host_edit_reaches_every_guest() {
        let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
        host.share(&path("a"), "x").unwrap();
        let (mut g, o) = join(&mut host, "One", Permission::View);
        deliver(o, &mut [&mut g]);
        let out = host.local_edit(&path("a"), 1, 0, "y").unwrap();
        deliver(out, &mut [&mut g]);
        assert_eq!(g.doc(&path("a")).unwrap().text(), "xy");
    }

    #[test]
    fn presence_is_stamped_with_the_real_sender() {
        let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
        host.share(&path("a"), "hello").unwrap();
        let (mut g1, o1) = join(&mut host, "One", Permission::View);
        let (mut g2, o2) = join(&mut host, "Two", Permission::View);
        deliver(o1, &mut [&mut g1, &mut g2]);
        deliver(o2, &mut [&mut g1, &mut g2]);
        // g1 claims to be the host.
        let Some(Message::Presence(mut p)) = g1.cursor(&path("a"), 2, None) else {
            panic!("no presence");
        };
        p.peer = PeerId::HOST;
        let h = host.handle(g1.me(), Message::Presence(p)).unwrap();
        assert!(matches!(&h.out[0].msg, Message::Presence(p) if p.peer == g1.me()));
        deliver(h.out, &mut [&mut g1, &mut g2]);
        let cs = g2.board().cursors(
            &path("a"),
            g2.doc(&path("a")).unwrap(),
            g2.me(),
            crate::presence::Rgb(0, 0, 0),
        );
        assert_eq!(cs.len(), 1);
        assert_eq!(cs[0].peer, g1.me());
        assert_eq!(cs[0].head, 2);
    }

    #[test]
    fn save_requests_need_permission_and_a_shared_path() {
        let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
        host.share(&path("a"), "x").unwrap();
        let (mut ed, o) = join(&mut host, "Ed", Permission::Edit);
        deliver(o, &mut [&mut ed]);
        let h = host
            .handle(ed.me(), ed.request_save(&path("a")).unwrap())
            .unwrap();
        assert_eq!(h.save, vec![(ed.me(), path("a"))]);
        let h = host
            .handle(
                ed.me(),
                Message::SaveRequest {
                    path: path("never"),
                },
            )
            .unwrap();
        assert_eq!(h.save, Vec::new());
        let out = host.saved(ed.me(), &path("a"));
        let events = deliver(out, &mut [&mut ed]);
        assert_eq!(events[0].1, GuestEvent::Saved(path("a")));
    }

    #[test]
    fn a_forged_update_drops_the_guest() {
        let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
        host.share(&path("a"), "abc").unwrap();
        let (mut g1, o1) = join(&mut host, "One", Permission::Edit);
        let (mut g2, o2) = join(&mut host, "Two", Permission::Edit);
        deliver(o1, &mut [&mut g1, &mut g2]);
        deliver(o2, &mut [&mut g1, &mut g2]);
        // g2's edit arrives on g1's connection: authored by the wrong peer.
        let Some(Message::Update { bytes, .. }) = g2.edit(&path("a"), 0, 0, "z").unwrap() else {
            panic!("no update");
        };
        let h = host
            .handle(
                g1.me(),
                Message::Update {
                    path: path("a"),
                    bytes,
                },
            )
            .unwrap();
        assert_eq!(h.disconnect, vec![g1.me()]);
        assert!(host.guest(g1.me()).is_none());
        assert_eq!(host.doc(&path("a")).unwrap().text(), "abc");
    }

    #[test]
    fn leaving_and_removal_tell_the_others() {
        let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
        let (mut g1, o1) = join(&mut host, "One", Permission::View);
        let (mut g2, o2) = join(&mut host, "Two", Permission::View);
        deliver(o1, &mut [&mut g1, &mut g2]);
        deliver(o2, &mut [&mut g1, &mut g2]);
        let h = host.handle(g1.me(), Message::Leave).unwrap();
        assert_eq!(h.disconnect, vec![g1.me()]);
        deliver(h.out, &mut [&mut g2]);
        assert_eq!(g2.board().name(g1.me()), None);
        // An unknown sender is refused outright.
        assert_eq!(
            host.handle(PeerId(99), Message::Leave),
            Err(CoworkError::PermissionDenied)
        );
    }

    #[test]
    fn permission_changes_reach_the_guest() {
        let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
        host.share(&path("a"), "x").unwrap();
        let (mut g, o) = join(&mut host, "One", Permission::View);
        deliver(o, &mut [&mut g]);
        let out = host.set_permission(g.me(), Permission::Edit);
        deliver(out, &mut [&mut g]);
        assert_eq!(g.permission(), Permission::Edit);
        assert!(g.edit(&path("a"), 0, 0, "y").unwrap().is_some());
        host.set_permission(g.me(), Permission::View);
        assert_eq!(host.guest(g.me()).unwrap().permission, Permission::View);
    }

    #[test]
    fn post_handshake_hello_is_a_protocol_violation() {
        let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
        let (g, _o) = join(&mut host, "One", Permission::Edit);
        let h = host
            .handle(
                g.me(),
                Message::Hello {
                    version: 1,
                    name: "again".into(),
                },
            )
            .unwrap();
        assert_eq!(h.disconnect, vec![g.me()]);
    }

    #[test]
    fn oversized_shares_are_refused() {
        let mut host = HostSession::new("Host", 4);
        assert!(matches!(
            host.share(&path("a"), "too long"),
            Err(CoworkError::TooLarge { .. })
        ));
    }

    #[test]
    fn private_files_are_never_shared() {
        let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
        for p in [
            ".env",
            "config/.env.local",
            ".vix/fhir.json",
            "keys/server.pem",
        ] {
            assert_eq!(
                host.share(&path(p), "SECRET=1"),
                Err(CoworkError::PermissionDenied),
                "{p}"
            );
        }
        assert_eq!(host.shared().count(), 0);
    }

    /// T696: the file tree is closed until the host opens it; then a guest
    /// may list and ask, private paths are filtered and refused, and only the
    /// host decides what is shared.
    #[test]
    fn workspace_sharing_is_opt_in_and_filters_private_paths() {
        let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
        let (mut guest, joined) = join(&mut host, "One", Permission::View);
        deliver(joined, &mut [&mut guest]);

        // Closed: both requests are refused, and the guest stays connected.
        let handled = host.handle(guest.me(), guest.request_files()).unwrap();
        assert_eq!(handled.list_files, Vec::<PeerId>::new());
        assert_eq!(handled.disconnect, Vec::<PeerId>::new());
        assert_eq!(
            handled.out,
            vec![Outbound {
                to: guest.me(),
                msg: Message::Error {
                    code: ErrorCode::PermissionDenied
                }
            }]
        );
        let handled = host
            .handle(guest.me(), guest.request_open(&path("src/a.rs")))
            .unwrap();
        assert_eq!(handled.open, Vec::new());
        assert_eq!(host.file_list(guest.me(), [path("a")]), Vec::new());

        // Open: a view-only guest may browse and ask (reading is all it does).
        host.set_workspace_shared(true);
        let handled = host.handle(guest.me(), guest.request_files()).unwrap();
        assert_eq!(handled.list_files, vec![guest.me()]);
        let out = host.file_list(
            guest.me(),
            [
                path("src/a.rs"),
                path(".env"),
                path(".git/config"),
                path("README.md"),
            ],
        );
        let events = deliver(out, &mut [&mut guest]);
        assert_eq!(
            events,
            vec![(
                guest.me(),
                GuestEvent::Files(vec![path("src/a.rs"), path("README.md")])
            )]
        );
        let handled = host
            .handle(guest.me(), guest.request_open(&path("src/a.rs")))
            .unwrap();
        assert_eq!(handled.open, vec![(guest.me(), path("src/a.rs"))]);
        let handled = host
            .handle(guest.me(), guest.request_open(&path(".env")))
            .unwrap();
        assert_eq!(handled.open, Vec::new());
        assert_eq!(
            handled.out,
            HostSession::refuse_to(guest.me(), ErrorCode::BadPath)
        );

        // An already shared file is resent to the asker alone.
        host.share(&path("src/a.rs"), "fn main() {}").unwrap();
        let out = host.reopen_for(guest.me(), &path("src/a.rs")).unwrap();
        assert_eq!(out.len(), 1);
        let events = deliver(out, &mut [&mut guest]);
        assert_eq!(
            events[0].1,
            GuestEvent::Opened(path("src/a.rs"), "fn main() {}".to_string())
        );
    }

    fn refused(to: PeerId) -> Vec<Outbound> {
        HostSession::refuse_to(to, ErrorCode::PermissionDenied)
    }

    /// A hand-made `TerminalKeys` from `from`, as the host handles it.
    fn send_keys(host: &mut HostSession, from: PeerId, bytes: &[u8]) -> Handled {
        host.handle(
            from,
            Message::TerminalKeys {
                bytes: bytes.to_vec(),
            },
        )
        .unwrap()
    }

    /// T697: nothing about the terminal reaches a guest until the host
    /// shares it; a share is read-only until the host allows input; typing
    /// then also needs terminal permission; refusals keep the guest.
    #[test]
    fn the_shared_terminal_is_read_only_until_the_host_allows_input() {
        let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
        let (mut term, o) = join(&mut host, "Term", Permission::EditTerminal);
        deliver(o, &mut [&mut term]);
        let (mut edit, o) = join(&mut host, "Edit", Permission::Edit);
        deliver(o, &mut [&mut term, &mut edit]);

        // Not shared: output, input changes and keys go nowhere.
        assert_eq!(host.terminal_output(b"secret"), Vec::new());
        assert_eq!(host.set_terminal_input(true), Vec::new());
        assert!(!host.terminal_input());
        assert_eq!(host.terminal_for(term.me(), 24, 80, b"x"), Vec::new());
        assert_eq!(
            term.terminal_keys(b"ls\r"),
            Err(CoworkError::PermissionDenied)
        );
        let h = send_keys(&mut host, term.me(), b"ls\r");
        assert_eq!((h.terminal_keys, h.out), (Vec::new(), refused(term.me())));
        assert!(host.guest(term.me()).is_some(), "a refusal is not fatal");

        // Shared: every guest sees the screen, read-only.
        let out = host.share_terminal(24, 80, b"$ ");
        assert_eq!(out.len(), 2);
        let events = deliver(out, &mut [&mut term, &mut edit]);
        assert_eq!(
            events[0].1,
            GuestEvent::TerminalScreen {
                rows: 24,
                cols: 80,
                screen: b"$ ".to_vec()
            }
        );
        assert!(term.terminal_shared() && !term.can_type_in_terminal());
        let events = deliver(host.terminal_output(b"ok"), &mut [&mut term]);
        assert_eq!(events[0].1, GuestEvent::TerminalOutput(b"ok".to_vec()));
        assert_eq!(host.terminal_output(b""), Vec::new());

        // A hand-crafted keystroke while read-only is refused.
        let h = send_keys(&mut host, term.me(), b"rm -rf ~\r");
        assert_eq!(h.terminal_keys, Vec::new());
        assert_eq!(h.out, refused(term.me()));

        // The host allows input: only the guest with terminal permission types.
        let out = host.set_terminal_input(true);
        let events = deliver(out, &mut [&mut term, &mut edit]);
        assert!(
            events
                .iter()
                .all(|(_, e)| *e == GuestEvent::TerminalInput(true))
        );
        assert!(term.can_type_in_terminal());
        assert!(!edit.can_type_in_terminal());
        assert_eq!(edit.terminal_keys(b"x"), Err(CoworkError::PermissionDenied));
        let msg = term.terminal_keys(b"ls\r").unwrap();
        let h = host.handle(term.me(), msg).unwrap();
        assert_eq!(h.terminal_keys, vec![(term.me(), b"ls\r".to_vec())]);
        assert_eq!(h.out, Vec::new());
        let h = send_keys(&mut host, edit.me(), b"x");
        assert_eq!((h.terminal_keys, h.out), (Vec::new(), refused(edit.me())));
        assert!(
            term.terminal_keys(&[b'x'; crate::wire::MAX_TERMINAL_KEYS + 1])
                .is_err()
        );

        // Lowering the guest's permission stops its typing at the host.
        let _ = host.set_permission(term.me(), Permission::Edit);
        let h = send_keys(&mut host, term.me(), b"y");
        assert_eq!(h.terminal_keys, Vec::new());
        let _ = host.set_permission(term.me(), Permission::EditTerminal);

        // A resize keeps the input choice; a late joiner gets the screen.
        let out = host.share_terminal(30, 100, b"$ ");
        assert!(matches!(
            &out[0].msg,
            Message::TerminalScreen {
                rows: 30,
                cols: 100,
                input: true,
                ..
            }
        ));
        let (late, _) = join(&mut host, "Late", Permission::View);
        let out = host.terminal_for(late.me(), 30, 100, b"$ ");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].to, late.me());

        // Unsharing tells everyone and forgets the input choice.
        let events = deliver(host.unshare_terminal(), &mut [&mut term, &mut edit]);
        assert!(events.iter().all(|(_, e)| *e == GuestEvent::TerminalClosed));
        assert!(!term.terminal_shared() && !term.can_type_in_terminal());
        assert!(!host.terminal_shared() && !host.terminal_input());
        assert_eq!(host.unshare_terminal(), Vec::new());
        let out = host.share_terminal(24, 80, b"");
        assert!(matches!(
            &out[0].msg,
            Message::TerminalScreen { input: false, .. }
        ));
    }

    /// T697: output for a terminal the guest never saw shared is dropped.
    #[test]
    fn a_guest_ignores_terminal_output_it_was_not_shared() {
        let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
        let (mut g, o) = join(&mut host, "One", Permission::View);
        deliver(o, &mut [&mut g]);
        assert_eq!(
            g.handle(Message::TerminalOutput {
                bytes: b"x".to_vec()
            })
            .unwrap(),
            Vec::new()
        );
        assert_eq!(
            g.handle(Message::TerminalInputAllowed { allowed: true })
                .unwrap(),
            Vec::new()
        );
        assert_eq!(g.handle(Message::TerminalClose).unwrap(), Vec::new());
        assert!(!g.can_type_in_terminal());
    }

    /// T692: Undo on either side reverses only that side's own text, and the
    /// result reaches the other side.
    #[test]
    fn host_and_guest_undo_only_their_own_edits() {
        let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
        host.share(&path("a"), "").unwrap();
        let (mut g, o) = join(&mut host, "One", Permission::Edit);
        deliver(o, &mut [&mut g]);
        let p = path("a");

        let out = host.local_edit(&p, 0, 0, "H").unwrap();
        deliver(out, &mut [&mut g]);
        let msg = g.edit(&p, 1, 0, "G").unwrap().unwrap();
        host.handle(g.me(), msg).unwrap();
        assert_eq!(host.doc(&p).unwrap().text(), "HG");

        let (out, edits) = host.undo(&p).unwrap().unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(host.doc(&p).unwrap().text(), "G");
        deliver(out, &mut [&mut g]);
        assert_eq!(g.doc(&p).unwrap().text(), "G");

        let (msg, _) = g.undo(&p).unwrap().unwrap();
        host.handle(g.me(), msg).unwrap();
        assert_eq!(host.doc(&p).unwrap().text(), "");

        let (out, _) = host.redo(&p).unwrap().unwrap();
        deliver(out, &mut [&mut g]);
        let (msg, _) = g.redo(&p).unwrap().unwrap();
        host.handle(g.me(), msg).unwrap();
        assert_eq!(host.doc(&p).unwrap().text(), g.doc(&p).unwrap().text());
        assert_eq!(host.doc(&p).unwrap().text().len(), 2);
        assert!(host.undo(&path("nope")).is_err());
    }
}
