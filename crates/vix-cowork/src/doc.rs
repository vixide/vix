//! The shared text document: a CRDT replica of one buffer.
//!
//! A [`SharedDoc`] mirrors one editor buffer. The editor's rope stays the
//! source of truth for rendering; the CRDT records every edit so replicas
//! converge. Positions everywhere are **characters** (Unicode scalar values),
//! the unit `ropey` and the editor core already use, so there is no
//! conversion at the bridge.
//!
//! Flow:
//!
//! - A **local edit** is applied to the buffer by the editor, then reported
//!   with [`SharedDoc::edit`], which returns the update bytes to broadcast.
//! - A **remote update** is passed to [`SharedDoc::apply_host_update`] (a
//!   guest receiving from the host) or [`SharedDoc::apply_guest_update`] (the
//!   host receiving from a guest, with permission and forgery checks). Both
//!   return [`BufferEdit`]s to replay, in order, onto the buffer.
//! - The local cursor survives remote edits through [`transform_offset`]; a
//!   remote cursor is carried as a CRDT-anchored [`Anchor`], which keeps
//!   pointing at the same character however the text around it changes.

use crate::error::CoworkError;
use crate::peer::{Action, Peer, PeerId};
use loro::cursor::{Cursor, Side};
use loro::event::Diff;
use loro::{
    ContainerTrait, EncodedBlobMode, ExportMode, LoroDoc, LoroText, Subscription, TextDelta,
    UndoManager, UpdateOptions, VersionVector,
};
use std::sync::{Arc, Mutex, PoisonError};

/// The default cap on one shared buffer, in bytes (5 MB).
pub const DEFAULT_CAP_BYTES: usize = 5 * 1024 * 1024;

/// The name of the one text container inside a [`SharedDoc`].
const TEXT: &str = "text";

/// One splice to replay onto a buffer: delete `delete` characters at `offset`,
/// then insert `insert` there. Offsets are characters in the buffer as it
/// stands when this edit is applied (edits are applied in order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferEdit {
    /// Start, in characters.
    pub offset: usize,
    /// Characters to delete first.
    pub delete: usize,
    /// Text to insert after the deletion.
    pub insert: String,
}

/// A position that follows its character through edits (a CRDT cursor).
///
/// Used for remote cursors and selection ends. It is an opaque, encodable
/// value: [`Anchor::as_bytes`] and [`Anchor::from_bytes`] move it across the
/// wire; [`SharedDoc::resolve`] turns it back into a character offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor(Vec<u8>);

impl Anchor {
    /// The encoded form, for the wire.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Adopt bytes received from a peer. Bytes that do not decode are refused
    /// here so a malformed anchor can never reach the document.
    ///
    /// # Errors
    ///
    /// When the bytes are not a valid encoded cursor.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CoworkError> {
        Cursor::decode(bytes)
            .map(|_| Self(bytes.to_vec()))
            .map_err(|e| CoworkError::Crdt(e.to_string()))
    }
}

/// What an undo or redo produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reverted {
    /// Update bytes to broadcast to the other peers.
    pub update: Vec<u8>,
    /// Edits to replay onto the local buffer.
    pub edits: Vec<BufferEdit>,
}

/// One replica of a shared buffer.
pub struct SharedDoc {
    doc: LoroDoc,
    text: LoroText,
    cap: usize,
    /// Everything up to here has already been returned for broadcast.
    sent: VersionVector,
    /// Buffer edits produced by import/undo events, drained by the caller.
    pending: Arc<Mutex<Vec<BufferEdit>>>,
    undo: UndoManager,
    _subscription: Subscription,
}

impl std::fmt::Debug for SharedDoc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedDoc")
            .field("chars", &self.text.len_unicode())
            .field("cap", &self.cap)
            .finish_non_exhaustive()
    }
}

/// Refuse text that must not be shared.
fn check_shareable(text: &str, cap: usize) -> Result<(), CoworkError> {
    if text.len() > cap {
        return Err(CoworkError::TooLarge {
            size: text.len(),
            cap,
        });
    }
    if text.contains('\0') {
        return Err(CoworkError::Binary);
    }
    Ok(())
}

/// Wrap any CRDT-library error as [`CoworkError::Crdt`]. The library's error
/// type is not part of this crate's API, so only its message is kept.
fn crdt<E: std::fmt::Display>(e: E) -> CoworkError {
    CoworkError::Crdt(e.to_string())
}

/// Lock `m`, recovering from poisoning: the guarded edit queue is only ever
/// appended to or drained whole, so a panic elsewhere cannot leave it half
/// updated and propagating the poison would just wedge the collaboration.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Turn a text delta (retain / insert / delete runs over the whole text) into
/// sequentially applicable splices.
fn push_edits(deltas: &[TextDelta], out: &mut Vec<BufferEdit>) {
    let mut pos = 0usize;
    for d in deltas {
        match d {
            TextDelta::Retain { retain, .. } => pos += retain,
            TextDelta::Insert { insert, .. } => {
                out.push(BufferEdit {
                    offset: pos,
                    delete: 0,
                    insert: insert.clone(),
                });
                pos += insert.chars().count();
            }
            TextDelta::Delete { delete } => out.push(BufferEdit {
                offset: pos,
                delete: *delete,
                insert: String::new(),
            }),
        }
    }
}

impl SharedDoc {
    /// Finish construction around an already-seeded `doc`: set the peer id, then
    /// subscribe to text events so every change (local or imported) lands as
    /// [`BufferEdit`]s in the shared `pending` queue for the caller to drain.
    fn build(doc: LoroDoc, me: PeerId, cap: usize) -> Result<Self, CoworkError> {
        doc.set_peer_id(me.0).map_err(crdt)?;
        let text = doc.get_text(TEXT);
        let pending = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&pending);
        let subscription = doc.subscribe(
            &text.id(),
            Arc::new(move |event| {
                let mut out = lock(&sink);
                for container in &event.events {
                    if let Diff::Text(deltas) = &container.diff {
                        push_edits(deltas, &mut out);
                    }
                }
            }),
        );
        // Built after any initial content so the file load itself is never
        // an undo step.
        let undo = UndoManager::new(&doc);
        let sent = doc.oplog_vv();
        Ok(Self {
            doc,
            text,
            cap,
            sent,
            pending,
            undo,
            _subscription: subscription,
        })
    }

    /// Start a document as the host, holding `initial` (the file's text).
    ///
    /// # Errors
    ///
    /// When `initial` is larger than `cap` bytes or contains a NUL (binary).
    pub fn host(me: PeerId, initial: &str, cap: usize) -> Result<Self, CoworkError> {
        check_shareable(initial, cap)?;
        let doc = LoroDoc::new();
        doc.set_peer_id(me.0).map_err(crdt)?;
        doc.get_text(TEXT).insert(0, initial).map_err(crdt)?;
        doc.commit();
        Self::build(doc, me, cap)
    }

    /// Join as a guest from the host's [`snapshot`](Self::snapshot).
    ///
    /// # Errors
    ///
    /// When the snapshot does not decode or its text exceeds `cap`.
    pub fn from_snapshot(me: PeerId, snapshot: &[u8], cap: usize) -> Result<Self, CoworkError> {
        let doc = LoroDoc::new();
        doc.set_peer_id(me.0).map_err(crdt)?;
        doc.import(snapshot).map_err(crdt)?;
        let this = Self::build(doc, me, cap)?;
        check_shareable(&this.text(), cap)?;
        Ok(this)
    }

    /// The whole document as bytes, to bring a joining guest up to date.
    ///
    /// # Errors
    ///
    /// When the CRDT cannot encode (not expected).
    pub fn snapshot(&self) -> Result<Vec<u8>, CoworkError> {
        self.doc.export(ExportMode::Snapshot).map_err(crdt)
    }

    /// The current text.
    #[must_use]
    pub fn text(&self) -> String {
        self.text.to_string()
    }

    /// The current length in characters.
    #[must_use]
    pub fn len_chars(&self) -> usize {
        self.text.len_unicode()
    }

    /// The configured cap, in bytes.
    #[must_use]
    pub const fn cap(&self) -> usize {
        self.cap
    }

    /// Set the undo grouping window in milliseconds (0 = one step per edit).
    pub fn set_undo_merge_ms(&mut self, ms: i64) {
        self.undo.set_merge_interval(ms);
    }

    /// Update bytes for everything authored since the last call, or `None`.
    fn take_update(&mut self) -> Result<Option<Vec<u8>>, CoworkError> {
        let now = self.doc.oplog_vv();
        if now == self.sent {
            return Ok(None);
        }
        let bytes = self
            .doc
            .export(ExportMode::updates(&self.sent))
            .map_err(crdt)?;
        self.sent = now;
        Ok(Some(bytes))
    }

    /// Record a local edit that the editor has applied (or is about to apply)
    /// to its buffer: delete `delete` characters at `offset`, then insert
    /// `insert`. Returns the update bytes to broadcast, or `None` for a no-op.
    ///
    /// # Errors
    ///
    /// When the range is outside the document, the result would exceed the
    /// size cap, or `insert` contains a NUL.
    pub fn edit(
        &mut self,
        offset: usize,
        delete: usize,
        insert: &str,
    ) -> Result<Option<Vec<u8>>, CoworkError> {
        let len = self.text.len_unicode();
        if offset > len || delete > len - offset {
            return Err(CoworkError::OutOfRange { offset, len });
        }
        if insert.contains('\0') {
            return Err(CoworkError::Binary);
        }
        let projected = self.text.len_utf8() + insert.len();
        if projected > self.cap {
            return Err(CoworkError::TooLarge {
                size: projected,
                cap: self.cap,
            });
        }
        if delete == 0 && insert.is_empty() {
            return Ok(None);
        }
        if delete > 0 {
            self.text.delete(offset, delete).map_err(crdt)?;
        }
        if !insert.is_empty() {
            self.text.insert(offset, insert).map_err(crdt)?;
        }
        self.doc.commit();
        // The caller's buffer already reflects this edit; the event it raised
        // must not be replayed back onto it.
        lock(&self.pending).clear();
        self.take_update()
    }

    /// Replace the whole text (the host reloading the file from disk),
    /// recording only the difference. Returns the update to broadcast.
    ///
    /// # Errors
    ///
    /// When `new_text` is over the cap or binary.
    pub fn replace_all(&mut self, new_text: &str) -> Result<Option<Vec<u8>>, CoworkError> {
        check_shareable(new_text, self.cap)?;
        self.text
            .update(new_text, UpdateOptions::default())
            .map_err(crdt)?;
        self.doc.commit();
        lock(&self.pending).clear();
        self.take_update()
    }

    /// Apply an update that arrived from the host (guest side). The host is
    /// trusted to relay many authors, so no authorship check is made.
    ///
    /// # Errors
    ///
    /// When the bytes do not decode, or the text would exceed the cap.
    pub fn apply_host_update(&mut self, bytes: &[u8]) -> Result<Vec<BufferEdit>, CoworkError> {
        self.import(bytes)
    }

    /// Apply an update that arrived from a guest (host side).
    ///
    /// Refused unless `from` may edit; and the update must be a plain
    /// incremental update whose operations are all authored by `from.id`, so a
    /// guest can neither forge another participant's edits nor replace the
    /// document with a crafted snapshot.
    ///
    /// # Errors
    ///
    /// [`CoworkError::PermissionDenied`], [`CoworkError::NotAnUpdate`],
    /// [`CoworkError::ForgedPeer`], [`CoworkError::TooLarge`] if the result
    /// breaks the cap (the document is then over the cap and the caller should
    /// drop that peer), or [`CoworkError::Crdt`] for undecodable bytes.
    pub fn apply_guest_update(
        &mut self,
        from: &Peer,
        bytes: &[u8],
    ) -> Result<Vec<BufferEdit>, CoworkError> {
        from.permission.require(Action::Edit)?;
        let meta = LoroDoc::decode_import_blob_meta(bytes, true).map_err(crdt)?;
        if meta.mode != EncodedBlobMode::Updates {
            return Err(CoworkError::NotAnUpdate);
        }
        if meta
            .partial_end_vv
            .iter()
            .any(|(peer, _)| *peer != from.id.0)
        {
            return Err(CoworkError::ForgedPeer);
        }
        self.import(bytes)
    }

    /// Import update `bytes` and return the buffer edits they imply. The queue is
    /// cleared first so stale edits from an earlier failed call cannot leak in;
    /// the size cap is checked *after* applying (the CRDT cannot be previewed),
    /// so an over-cap result is an error the caller must treat as a rejection.
    fn import(&mut self, bytes: &[u8]) -> Result<Vec<BufferEdit>, CoworkError> {
        lock(&self.pending).clear();
        self.doc.import(bytes).map_err(crdt)?;
        // Remote operations are never "ours to send": keep `sent` current.
        self.sent = self.doc.oplog_vv();
        let edits = std::mem::take(&mut *lock(&self.pending));
        let size = self.text.len_utf8();
        if size > self.cap {
            return Err(CoworkError::TooLarge {
                size,
                cap: self.cap,
            });
        }
        Ok(edits)
    }

    /// Undo this peer's most recent edit group. Other peers' text is never
    /// touched. `None` when there is nothing to undo.
    ///
    /// # Errors
    ///
    /// When the CRDT fails to apply the reversal.
    pub fn undo(&mut self) -> Result<Option<Reverted>, CoworkError> {
        self.revert(true)
    }

    /// Redo what [`undo`](Self::undo) reverted. `None` when there is nothing.
    ///
    /// # Errors
    ///
    /// When the CRDT fails to apply the replay.
    pub fn redo(&mut self) -> Result<Option<Reverted>, CoworkError> {
        self.revert(false)
    }

    /// Shared body of [`undo`](Self::undo) / [`redo`](Self::redo): run the
    /// per-peer undo manager, commit, and package the resulting edits with the
    /// outgoing update so peers converge on the reversal.
    fn revert(&mut self, undo: bool) -> Result<Option<Reverted>, CoworkError> {
        lock(&self.pending).clear();
        let did = if undo {
            self.undo.undo()
        } else {
            self.undo.redo()
        }
        .map_err(crdt)?;
        if !did {
            return Ok(None);
        }
        self.doc.commit();
        let edits = std::mem::take(&mut *lock(&self.pending));
        let update = self.take_update()?.unwrap_or_default();
        Ok(Some(Reverted { update, edits }))
    }

    /// An anchor at character `offset` that follows its character.
    #[must_use]
    pub fn anchor_at(&self, offset: usize) -> Option<Anchor> {
        self.text
            .get_cursor(offset, Side::Middle)
            .map(|c| Anchor(c.encode()))
    }

    /// Where `anchor` points now, in characters. `None` if the anchored
    /// character's history is unknown to this replica.
    #[must_use]
    pub fn resolve(&self, anchor: &Anchor) -> Option<usize> {
        let cursor = Cursor::decode(anchor.as_bytes()).ok()?;
        self.doc
            .get_cursor_pos(&cursor)
            .ok()
            .map(|r| r.current.pos.min(self.text.len_unicode()))
    }
}

/// Where a local character offset ends up after `edits` are applied to the
/// buffer in order. An insertion exactly at the offset leaves it before the
/// new text, so a collaborator typing at your cursor never drags it along.
#[must_use]
pub fn transform_offset(mut pos: usize, edits: &[BufferEdit]) -> usize {
    for e in edits {
        if e.delete > 0 {
            let end = e.offset + e.delete;
            if pos >= end {
                pos -= e.delete;
            } else if pos > e.offset {
                pos = e.offset;
            }
        }
        let n = e.insert.chars().count();
        if n > 0 && e.offset < pos {
            pos += n;
        }
    }
    pos
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peer::Permission;

    const A: PeerId = PeerId(1);
    const B: PeerId = PeerId(2);

    fn pair(text: &str) -> (SharedDoc, SharedDoc) {
        let host = SharedDoc::host(A, text, DEFAULT_CAP_BYTES).unwrap();
        let guest =
            SharedDoc::from_snapshot(B, &host.snapshot().unwrap(), DEFAULT_CAP_BYTES).unwrap();
        (host, guest)
    }

    fn editor(p: PeerId) -> Peer {
        let mut peer = Peer::new(p, "g");
        peer.permission = Permission::Edit;
        peer
    }

    fn replay(buf: &mut String, edits: &[BufferEdit]) {
        for e in edits {
            let mut chars: Vec<char> = buf.chars().collect();
            chars.splice(e.offset..e.offset + e.delete, e.insert.chars());
            *buf = chars.into_iter().collect();
        }
    }

    #[test]
    fn guest_joins_with_the_host_text() {
        let (h, g) = pair("hello world");
        assert_eq!(h.text(), "hello world");
        assert_eq!(g.text(), "hello world");
    }

    #[test]
    fn a_local_edit_reaches_the_other_replica_as_buffer_edits() {
        let (mut h, mut g) = pair("hello world");
        let update = g.edit(5, 0, ",").unwrap().unwrap();
        let edits = h.apply_guest_update(&editor(B), &update).unwrap();
        assert_eq!(h.text(), "hello, world");
        let mut buf = "hello world".to_string();
        replay(&mut buf, &edits);
        assert_eq!(buf, "hello, world");
    }

    #[test]
    fn deltas_use_characters_not_bytes() {
        let (mut h, mut g) = pair("héllo wörld");
        let update = g.edit(6, 0, "ñ😀").unwrap().unwrap();
        let edits = h.apply_guest_update(&editor(B), &update).unwrap();
        let mut buf = "héllo wörld".to_string();
        replay(&mut buf, &edits);
        assert_eq!(buf, h.text());
        assert_eq!(buf, "héllo ñ😀wörld");
    }

    #[test]
    fn the_local_edit_is_not_echoed_back() {
        let (_h, mut g) = pair("abc");
        g.edit(1, 1, "XY").unwrap();
        assert!(lock(&g.pending).is_empty());
    }

    #[test]
    fn concurrent_edits_converge() {
        let (mut h, mut g) = pair("one two");
        let uh = h.edit(0, 0, "A ").unwrap().unwrap();
        let ug = g.edit(7, 0, " B").unwrap().unwrap();
        h.apply_guest_update(&editor(B), &ug).unwrap();
        g.apply_host_update(&uh).unwrap();
        assert_eq!(h.text(), g.text());
        assert_eq!(h.text(), "A one two B");
    }

    #[test]
    fn view_only_guests_cannot_edit_the_host() {
        let (mut h, mut g) = pair("abc");
        let update = g.edit(0, 0, "x").unwrap().unwrap();
        let view = Peer::new(B, "v");
        assert_eq!(
            h.apply_guest_update(&view, &update),
            Err(CoworkError::PermissionDenied)
        );
        assert_eq!(h.text(), "abc");
    }

    #[test]
    fn a_guest_cannot_forge_the_hosts_edits() {
        let (mut h, mut g) = pair("abc");
        let update = g.edit(0, 0, "x").unwrap().unwrap();
        // The update is authored by peer B, but arrives claiming to be peer 3.
        let other = editor(PeerId(3));
        assert_eq!(
            h.apply_guest_update(&other, &update),
            Err(CoworkError::ForgedPeer)
        );
        assert_eq!(h.text(), "abc");
    }

    #[test]
    fn a_guest_cannot_push_a_snapshot() {
        let (mut h, g) = pair("abc");
        let snap = g.snapshot().unwrap();
        assert_eq!(
            h.apply_guest_update(&editor(B), &snap),
            Err(CoworkError::NotAnUpdate)
        );
    }

    #[test]
    fn garbage_bytes_are_an_error_not_a_panic() {
        let (mut h, _g) = pair("abc");
        assert!(
            h.apply_guest_update(&editor(B), b"not a loro update")
                .is_err()
        );
        assert!(h.apply_host_update(&[0xff; 64]).is_err());
        assert_eq!(h.text(), "abc");
    }

    #[test]
    fn out_of_range_edits_are_refused() {
        let (mut h, _g) = pair("abc");
        assert_eq!(
            h.edit(4, 0, "x"),
            Err(CoworkError::OutOfRange { offset: 4, len: 3 })
        );
        assert_eq!(
            h.edit(2, 5, ""),
            Err(CoworkError::OutOfRange { offset: 2, len: 3 })
        );
        assert_eq!(
            h.edit(1, usize::MAX, ""),
            Err(CoworkError::OutOfRange { offset: 1, len: 3 })
        );
    }

    #[test]
    fn the_size_cap_and_binary_files_are_refused() {
        assert!(matches!(
            SharedDoc::host(A, "abcdef", 5),
            Err(CoworkError::TooLarge { size: 6, cap: 5 })
        ));
        assert_eq!(
            SharedDoc::host(A, "a\0b", 100).unwrap_err(),
            CoworkError::Binary
        );
        let mut d = SharedDoc::host(A, "abc", 5).unwrap();
        assert!(matches!(
            d.edit(0, 0, "xyz"),
            Err(CoworkError::TooLarge { .. })
        ));
        assert_eq!(d.edit(0, 0, "a\0"), Err(CoworkError::Binary));
    }

    #[test]
    fn an_update_that_breaks_the_cap_is_reported() {
        let host = SharedDoc::host(A, "abc", 8).unwrap();
        let mut big = SharedDoc::from_snapshot(B, &host.snapshot().unwrap(), 1000).unwrap();
        let update = big.edit(0, 0, "0123456789").unwrap().unwrap();
        let mut h = host;
        assert!(matches!(
            h.apply_guest_update(&editor(B), &update),
            Err(CoworkError::TooLarge { .. })
        ));
    }

    #[test]
    fn undo_reverts_only_my_text() {
        let (mut h, mut g) = pair("base");
        h.set_undo_merge_ms(0);
        g.set_undo_merge_ms(0);
        let uh = h.edit(4, 0, " host").unwrap().unwrap();
        g.apply_host_update(&uh).unwrap();
        let ug = g.edit(0, 0, "guest ").unwrap().unwrap();
        h.apply_guest_update(&editor(B), &ug).unwrap();
        assert_eq!(h.text(), "guest base host");

        let rev = h.undo().unwrap().unwrap();
        assert_eq!(h.text(), "guest base");
        g.apply_host_update(&rev.update).unwrap();
        assert_eq!(g.text(), "guest base");
        let mut buf = "guest base host".to_string();
        replay(&mut buf, &rev.edits);
        assert_eq!(buf, "guest base");

        let again = h.redo().unwrap().unwrap();
        assert_eq!(h.text(), "guest base host");
        g.apply_host_update(&again.update).unwrap();
        assert_eq!(g.text(), h.text());
    }

    #[test]
    fn the_initial_file_load_is_not_undoable() {
        let mut h = SharedDoc::host(A, "loaded", DEFAULT_CAP_BYTES).unwrap();
        assert_eq!(h.undo().unwrap(), None);
        assert_eq!(h.text(), "loaded");
    }

    #[test]
    fn an_anchor_follows_its_character_through_remote_inserts() {
        let (h, mut g) = pair("hello world");
        let anchor = h.anchor_at(6).unwrap();
        let mut h = h;
        let u1 = g.edit(0, 0, ">>> ").unwrap().unwrap();
        let u2 = g.edit(g.len_chars(), 0, " <<<").unwrap().unwrap();
        h.apply_guest_update(&editor(B), &u1).unwrap();
        h.apply_guest_update(&editor(B), &u2).unwrap();
        assert_eq!(h.resolve(&anchor), Some(10));
        // And it survives the wire.
        let again = Anchor::from_bytes(anchor.as_bytes()).unwrap();
        assert_eq!(h.resolve(&again), Some(10));
    }

    #[test]
    fn malformed_anchor_bytes_are_refused() {
        assert!(Anchor::from_bytes(&[0xff; 3]).is_err());
    }

    #[test]
    fn transform_keeps_the_local_cursor_on_its_character() {
        let ins = |o: usize, s: &str| BufferEdit {
            offset: o,
            delete: 0,
            insert: s.to_string(),
        };
        let del = |o: usize, n: usize| BufferEdit {
            offset: o,
            delete: n,
            insert: String::new(),
        };
        assert_eq!(transform_offset(5, &[ins(0, "abc")]), 8);
        assert_eq!(transform_offset(5, &[ins(5, "abc")]), 5);
        assert_eq!(transform_offset(5, &[ins(9, "abc")]), 5);
        assert_eq!(transform_offset(5, &[del(0, 2)]), 3);
        assert_eq!(transform_offset(5, &[del(3, 10)]), 3);
        assert_eq!(transform_offset(5, &[del(6, 2)]), 5);
        assert_eq!(transform_offset(5, &[ins(0, "ab"), del(0, 1)]), 6);
    }

    #[test]
    fn replace_all_records_only_the_difference() {
        let (mut h, mut g) = pair("one two three");
        let update = h.replace_all("one 2 three").unwrap().unwrap();
        let edits = g.apply_host_update(&update).unwrap();
        assert_eq!(g.text(), "one 2 three");
        let mut buf = "one two three".to_string();
        replay(&mut buf, &edits);
        assert_eq!(buf, "one 2 three");
        assert_eq!(h.replace_all("one 2 three").unwrap(), None);
    }
}
