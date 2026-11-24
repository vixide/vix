//! Round-3: whole host/guest sessions through the public API, delivering
//! messages by hand: undo/redo, file lists, the shared terminal, chat,
//! presence, refusals and wire round trips of the rarely used messages.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests fail loudly on a broken fixture"
)]

use vix_cowork::wire::decode;
use vix_cowork::{
    Channel, ChatKind, ChatLine, CoworkError, DEFAULT_CAP_BYTES, ErrorCode, GuestEvent,
    GuestSession, HostSession, Message, Outbound, PeerId, Permission, Presence, SharedDoc,
    SharedPath,
};

fn path(s: &str) -> SharedPath {
    SharedPath::parse(s).unwrap()
}

fn line(from: PeerId, body: &str) -> ChatLine {
    ChatLine {
        from,
        time_utc: 1_700_000_000_000,
        body: body.into(),
        kind: ChatKind::Text,
    }
}

/// Admit a guest at `permission`, bring it up to date, return it with its id.
fn admit(host: &mut HostSession, name: &str, permission: Permission) -> (PeerId, GuestSession) {
    let (peer, out) = host.admit(name, permission).unwrap();
    let mut msgs = out.into_iter().filter(|o| o.to == peer.id).map(|o| o.msg);
    let welcome = msgs.next().unwrap();
    let mut guest = GuestSession::from_welcome(&welcome, DEFAULT_CAP_BYTES).unwrap();
    for m in msgs {
        guest.handle(m).unwrap();
    }
    (peer.id, guest)
}

fn to(out: &[Outbound], id: PeerId) -> Vec<Message> {
    out.iter()
        .filter(|o| o.to == id)
        .map(|o| o.msg.clone())
        .collect()
}

fn deliver(out: &[Outbound], id: PeerId, guest: &mut GuestSession) -> Vec<GuestEvent> {
    to(out, id)
        .into_iter()
        .flat_map(|m| guest.handle(m).unwrap())
        .collect()
}

#[test]
fn host_undo_and_redo_reach_every_guest() {
    let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
    let p = path("a.txt");
    host.share(&p, "hello").unwrap();
    let (id, mut guest) = admit(&mut host, "Ada", Permission::View);
    assert_eq!(guest.doc(&p).unwrap().text(), "hello");

    assert!(
        host.undo(&path("never-shared.txt")).is_err(),
        "unshared path is a bad path"
    );
    let out = host.local_edit(&p, 5, 0, " world").unwrap();
    deliver(&out, id, &mut guest);
    assert_eq!(guest.doc(&p).unwrap().text(), "hello world");

    let (out, edits) = host.undo(&p).unwrap().expect("something to undo");
    assert_ne!(edits, Vec::new());
    deliver(&out, id, &mut guest);
    assert_eq!(host.doc(&p).unwrap().text(), guest.doc(&p).unwrap().text());
    let undone = host.doc(&p).unwrap().text();
    assert_ne!(undone, "hello world");

    let (out, _) = host.redo(&p).unwrap().expect("something to redo");
    deliver(&out, id, &mut guest);
    assert_eq!(host.doc(&p).unwrap().text(), "hello world");
    assert_eq!(guest.doc(&p).unwrap().text(), "hello world");
    assert!(host.redo(&p).unwrap().is_none(), "nothing further to redo");
}

#[test]
fn guest_undo_needs_edit_permission_and_mirrors_back() {
    let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
    let p = path("a.txt");
    host.share(&p, "abc").unwrap();
    let (viewer_id, mut viewer) = admit(&mut host, "Viewer", Permission::View);
    assert!(viewer.undo(&p).is_err());
    assert!(viewer.redo(&p).is_err());
    assert!(viewer.request_save(&p).is_err());
    assert_eq!(viewer.request_files(), Message::ListFiles);
    assert_eq!(
        viewer.request_open(&p),
        Message::OpenRequest { path: p.clone() }
    );
    assert!(!viewer.terminal_shared());
    assert!(!viewer.can_type_in_terminal());
    assert_eq!(viewer.permission(), Permission::View);
    assert_eq!(viewer.me(), viewer_id);
    assert_eq!(viewer.host_name(), "Host");

    let (_, mut editor) = admit(&mut host, "Editor", Permission::Edit);
    assert!(matches!(
        editor.request_save(&p).unwrap(),
        Message::SaveRequest { .. }
    ));
    let Some(Message::Update { bytes, .. }) = editor.edit(&p, 3, 0, "d").unwrap() else {
        panic!("an edit produces an update");
    };
    let _ = bytes;
    let (msg, edits) = editor.undo(&p).unwrap().expect("undo");
    assert_ne!(edits, Vec::new());
    assert!(matches!(msg, Message::Update { .. }));
    assert_eq!(editor.doc(&p).unwrap().text(), "abc");
    let (_, redo_edits) = editor.redo(&p).unwrap().expect("redo");
    assert_ne!(redo_edits, Vec::new());
    assert_eq!(editor.doc(&p).unwrap().text(), "abcd");
    assert!(editor.undo(&path("nope.txt")).is_err());
}

#[test]
fn file_lists_and_open_requests_follow_workspace_sharing() {
    let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
    let (id, mut guest) = admit(&mut host, "Ada", Permission::View);
    assert!(!host.workspace_shared());
    // Not shared: the request is refused and nothing is listed.
    let h = host.handle(id, Message::ListFiles).unwrap();
    assert_eq!(h.list_files, Vec::new());
    assert_eq!(
        h.out[0].msg,
        Message::Error {
            code: ErrorCode::PermissionDenied
        }
    );
    assert_eq!(host.file_list(id, [path("x.rs")]), Vec::new());
    let h = host
        .handle(id, Message::OpenRequest { path: path("x.rs") })
        .unwrap();
    assert_eq!(
        h.out[0].msg,
        Message::Error {
            code: ErrorCode::PermissionDenied
        }
    );

    host.set_workspace_shared(true);
    assert!(host.workspace_shared());
    let h = host.handle(id, Message::ListFiles).unwrap();
    assert_eq!(h.list_files, vec![id]);
    let h = host
        .handle(id, Message::OpenRequest { path: path("x.rs") })
        .unwrap();
    assert_eq!(h.open, vec![(id, path("x.rs"))]);
    let h = host
        .handle(id, Message::OpenRequest { path: path(".env") })
        .unwrap();
    assert_eq!(
        h.out[0].msg,
        Message::Error {
            code: ErrorCode::BadPath
        }
    );

    // Private files are dropped from the list; unknown recipients get nothing.
    let out = host.file_list(id, [path("src/a.rs"), path(".env"), path("b.rs")]);
    let events = deliver(&out, id, &mut guest);
    assert_eq!(
        events,
        vec![GuestEvent::Files(vec![path("src/a.rs"), path("b.rs")])]
    );
    assert_eq!(host.file_list(PeerId(999), [path("a")]), Vec::new());

    // Reopening an already shared buffer sends its state; an unshared one nothing.
    host.share(&path("x.rs"), "fn main() {}").unwrap();
    let out = host.reopen_for(id, &path("x.rs")).unwrap();
    let events = deliver(&out, id, &mut guest);
    assert_eq!(
        events,
        vec![GuestEvent::Opened(path("x.rs"), "fn main() {}".into())]
    );
    assert_eq!(host.reopen_for(id, &path("zzz")).unwrap(), Vec::new());
    let refusal = HostSession::refuse_to(id, ErrorCode::Declined);
    assert_eq!(
        refusal[0].msg,
        Message::Error {
            code: ErrorCode::Declined
        }
    );
}

#[test]
fn guests_are_listed_removed_and_told_of_permission_changes() {
    let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
    let (a, mut ga) = admit(&mut host, "Ada", Permission::View);
    let (b, mut gb) = admit(&mut host, "Bob", Permission::View);
    assert_eq!(host.guests().count(), 2);
    assert_eq!(host.guest(a).unwrap().name, "Ada");
    assert!(host.guest(PeerId(777)).is_none());
    assert!(host.board().name(a).is_some());

    let out = host.set_permission(a, Permission::EditTerminal);
    assert_eq!(
        deliver(&out, a, &mut ga),
        vec![GuestEvent::Permission(Permission::EditTerminal)]
    );
    assert_eq!(ga.permission(), Permission::EditTerminal);
    assert_eq!(
        host.set_permission(PeerId(777), Permission::Edit),
        Vec::new()
    );

    let out = host.remove(a);
    let events = deliver(&out, b, &mut gb);
    assert!(
        events.contains(&GuestEvent::PeerLeft(a, "Ada".into())),
        "{events:?}"
    );
    assert!(host.remove(a).is_empty(), "removing twice does nothing");
    assert_eq!(host.guests().count(), 1);
    assert_eq!(
        host.shared().count(),
        0,
        "the session notes are not listed as a shared file"
    );
}

#[test]
fn unsharing_closes_on_guests_but_never_the_session_notes() {
    let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
    let p = path("a.txt");
    host.share(&p, "x").unwrap();
    let (id, mut guest) = admit(&mut host, "Ada", Permission::View);
    assert_eq!(host.unshare(&SharedPath::session_notes()), Vec::new());
    assert_eq!(host.unshare(&path("never.txt")), Vec::new());
    let out = host.unshare(&p);
    assert_eq!(
        deliver(&out, id, &mut guest),
        vec![GuestEvent::Closed(p.clone())]
    );
    assert!(guest.doc(&p).is_none());
    assert!(
        host.share(&path(".env"), "secret").is_err(),
        "private files are never shared"
    );
}

#[test]
fn presence_follows_the_connection_not_the_claim() {
    let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
    let p = path("a.txt");
    host.share(&p, "hello").unwrap();
    let (a, mut ga) = admit(&mut host, "Ada", Permission::View);
    let (b, mut gb) = admit(&mut host, "Bob", Permission::View);

    // The host's cursor reaches both guests.
    let out = host.local_cursor(&p, 2, Some(4));
    assert!(matches!(
        deliver(&out, a, &mut ga)[..],
        [GuestEvent::Presence]
    ));
    assert!(matches!(
        deliver(&out, b, &mut gb)[..],
        [GuestEvent::Presence]
    ));
    assert_eq!(host.local_cursor(&path("zzz"), 0, None), Vec::new());
    assert_eq!(
        host.local_cursor(&p, 999, None).len(),
        2,
        "a cursor past the end still resolves (clamped)"
    );

    // A guest's cursor is relayed to the others, stamped with its real id.
    let Some(Message::Presence(mut claim)) = ga.cursor(&p, 1, None) else {
        panic!("cursor message");
    };
    claim.peer = PeerId::HOST;
    let h = host.handle(a, Message::Presence(claim)).unwrap();
    let relayed = to(&h.out, b);
    assert!(matches!(&relayed[..], [Message::Presence(p)] if p.peer == a));
    assert!(to(&h.out, a).is_empty(), "never echoed back to the sender");
    assert_eq!(host.board().location(a).unwrap().peer, a);
    // A cursor in an unshared buffer is ignored.
    let Some(Message::Presence(stray)) = ga.cursor(&p, 0, None) else {
        panic!()
    };
    let stray = Presence {
        path: path("other.txt"),
        ..stray
    };
    assert_eq!(
        host.handle(a, Message::Presence(stray)).unwrap().out,
        Vec::new()
    );
    assert!(ga.cursor(&path("other.txt"), 0, None).is_none());
}

#[test]
fn the_shared_terminal_gates_keystrokes_three_ways() {
    let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
    let (typist, mut gt) = admit(&mut host, "Typist", Permission::EditTerminal);
    let (watcher, mut gw) = admit(&mut host, "Watcher", Permission::Edit);
    assert!(!host.terminal_shared());

    let keys = |b: &[u8]| Message::TerminalKeys { bytes: b.to_vec() };
    // Nothing shared yet: refused, not fatal.
    let h = host.handle(typist, keys(b"ls\n")).unwrap();
    assert!(h.terminal_keys.is_empty() && h.disconnect.is_empty());
    assert_eq!(
        h.out[0].msg,
        Message::Error {
            code: ErrorCode::PermissionDenied
        }
    );

    let out = host.share_terminal(24, 80, b"$ ");
    assert!(host.terminal_shared() && !host.terminal_input());
    for (id, g) in [(typist, &mut gt), (watcher, &mut gw)] {
        let events = deliver(&out, id, g);
        assert_eq!(
            events,
            vec![GuestEvent::TerminalScreen {
                rows: 24,
                cols: 80,
                screen: b"$ ".to_vec()
            }]
        );
        assert!(g.terminal_shared());
    }
    // Shared but input not allowed: still refused.
    assert_eq!(
        host.handle(typist, keys(b"x")).unwrap().terminal_keys,
        Vec::new()
    );

    let out = host.set_terminal_input(true);
    assert!(host.terminal_input());
    assert_eq!(
        deliver(&out, typist, &mut gt),
        vec![GuestEvent::TerminalInput(true)]
    );
    assert!(gt.can_type_in_terminal());
    assert!(!gw.can_type_in_terminal(), "no terminal permission");
    // Allowed + permitted: delivered; empty input is dropped silently.
    let h = host.handle(typist, keys(b"ls\n")).unwrap();
    assert_eq!(h.terminal_keys, vec![(typist, b"ls\n".to_vec())]);
    let h = host.handle(typist, keys(b"")).unwrap();
    assert!(h.terminal_keys.is_empty() && h.out.is_empty());
    let h = host.handle(watcher, keys(b"rm")).unwrap();
    assert_eq!(h.terminal_keys, Vec::new());
    assert_eq!(
        h.out[0].msg,
        Message::Error {
            code: ErrorCode::PermissionDenied
        }
    );
    assert!(gt.terminal_keys(b"echo").is_ok());
    assert!(gw.terminal_keys(b"echo").is_err());

    let out = host.terminal_output(b"output");
    assert_eq!(
        deliver(&out, typist, &mut gt),
        vec![GuestEvent::TerminalOutput(b"output".to_vec())]
    );
    // A late joiner gets the current screen.
    let out = host.terminal_for(typist, 10, 20, b"redraw");
    assert_eq!(
        deliver(&out, typist, &mut gt),
        vec![GuestEvent::TerminalScreen {
            rows: 10,
            cols: 20,
            screen: b"redraw".to_vec()
        }]
    );
    let out = host.unshare_terminal();
    assert_eq!(
        deliver(&out, typist, &mut gt),
        vec![GuestEvent::TerminalClosed]
    );
    assert!(!host.terminal_shared());
    assert!(!gt.terminal_shared());
    // Output with nothing shared is dropped, as is a stray input toggle.
    assert_eq!(
        gt.handle(Message::TerminalOutput {
            bytes: b"x".to_vec()
        })
        .unwrap(),
        Vec::new()
    );
    assert_eq!(
        gt.handle(Message::TerminalInputAllowed { allowed: true })
            .unwrap(),
        Vec::new()
    );
    assert_eq!(gt.handle(Message::TerminalClose).unwrap(), Vec::new());
}

#[test]
fn chat_is_stamped_relayed_muted_and_kicks_end_the_guest() {
    let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
    let (a, mut ga) = admit(&mut host, "Ada", Permission::View);
    let (b, mut gb) = admit(&mut host, "Bob", Permission::View);

    let h = host
        .handle(a, Message::Chat(line(PeerId::HOST, "hi all")))
        .unwrap();
    assert_eq!(h.chat.len(), 1);
    assert_eq!(
        h.chat[0].from, a,
        "the connection's id wins over the claimed one"
    );
    let events = deliver(&h.out, b, &mut gb);
    assert!(matches!(&events[..], [GuestEvent::Chat(l)] if l.from == a && l.body == "hi all"));

    let out = host.set_muted(a, true);
    assert!(host.is_muted(a));
    assert_eq!(deliver(&out, a, &mut ga), vec![GuestEvent::Muted(true)]);
    assert!(ga.is_muted());
    let h = host.handle(a, Message::Chat(line(a, "muted"))).unwrap();
    assert_eq!(h.chat, Vec::new());
    assert_eq!(
        h.out[0].msg,
        Message::Error {
            code: ErrorCode::PermissionDenied
        }
    );
    let _ = host.set_muted(a, false);
    assert!(!host.is_muted(a));

    // The host speaks to everyone.
    let out = host.chat(line(PeerId::HOST, "welcome")).unwrap();
    assert_eq!(to(&out, a).len(), 1);
    assert!(host.chat(line(PeerId::HOST, &"x".repeat(100_000))).is_err());
    assert!(ga.chat(line(a, &"x".repeat(100_000))).is_err());

    // Kicking sends a removal and drops the guest from the roster.
    let out = host.kick(a);
    assert_eq!(
        deliver(&out, a, &mut ga),
        vec![GuestEvent::Refused(ErrorCode::Removed)],
        "the kicked guest is told why"
    );
    assert!(host.guest(a).is_none());
    // A message from a stranger is refused outright.
    assert_eq!(
        host.handle(a, Message::Leave).unwrap_err(),
        CoworkError::PermissionDenied
    );
}

#[test]
fn a_leaving_guest_is_removed_and_disconnected() {
    let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
    let (a, _ga) = admit(&mut host, "Ada", Permission::Edit);
    let (b, mut gb) = admit(&mut host, "Bob", Permission::Edit);
    let h = host.handle(a, Message::Leave).unwrap();
    assert_eq!(h.disconnect, vec![a]);
    let events = deliver(&h.out, b, &mut gb);
    assert!(events.contains(&GuestEvent::PeerLeft(a, "Ada".into())));
    assert_eq!(gb.handle(Message::Leave).unwrap(), vec![GuestEvent::Ended]);
    assert_eq!(
        gb.handle(Message::Hello {
            version: 1,
            name: "x".into()
        })
        .unwrap(),
        Vec::new()
    );
}

#[test]
fn updates_to_unshared_buffers_and_saves_are_refused_politely() {
    let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
    let p = path("a.txt");
    host.share(&p, "text").unwrap();
    let (viewer, mut gv) = admit(&mut host, "Viewer", Permission::View);
    let (editor, mut ge) = admit(&mut host, "Editor", Permission::Edit);

    // Unshared target.
    let h = host
        .handle(
            editor,
            Message::Update {
                path: path("nope.txt"),
                bytes: vec![1, 2, 3],
            },
        )
        .unwrap();
    assert_eq!(
        h.out[0].msg,
        Message::Error {
            code: ErrorCode::BadPath
        }
    );
    // Garbage CRDT bytes from an editor end that guest's session.
    let h = host
        .handle(
            editor,
            Message::Update {
                path: p.clone(),
                bytes: vec![0xFF; 8],
            },
        )
        .unwrap();
    assert_eq!(h.disconnect, vec![editor]);
    assert!(host.guest(editor).is_none());

    // A viewer's genuine edit is refused (but stays connected).
    let mut forged = SharedDoc::from_snapshot(
        viewer,
        &host.doc(&p).unwrap().snapshot().unwrap(),
        DEFAULT_CAP_BYTES,
    )
    .unwrap();
    let bytes = forged.edit(0, 0, "x").unwrap().unwrap();
    let h = host
        .handle(
            viewer,
            Message::Update {
                path: p.clone(),
                bytes,
            },
        )
        .unwrap();
    assert_eq!(
        h.out[0].msg,
        Message::Error {
            code: ErrorCode::PermissionDenied
        }
    );
    assert_eq!(h.disconnect, Vec::new());
    assert_eq!(host.doc(&p).unwrap().text(), "text");

    // Saving: viewers refused, notes are not files, unshared refused.
    let h = host
        .handle(viewer, Message::SaveRequest { path: p.clone() })
        .unwrap();
    assert_eq!(
        h.out[0].msg,
        Message::Error {
            code: ErrorCode::PermissionDenied
        }
    );
    let h = host
        .handle(
            viewer,
            Message::SaveRequest {
                path: SharedPath::session_notes(),
            },
        )
        .unwrap();
    assert_eq!(
        h.out[0].msg,
        Message::Error {
            code: ErrorCode::BadPath
        }
    );
    let (editor2, mut ge2) = admit(&mut host, "Editor2", Permission::Edit);
    let h = host
        .handle(editor2, Message::SaveRequest { path: p.clone() })
        .unwrap();
    assert_eq!(h.save, vec![(editor2, p.clone())]);
    let out = host.saved(editor2, &p);
    assert_eq!(
        deliver(&out, editor2, &mut ge2),
        vec![GuestEvent::Saved(p.clone())]
    );
    let _ = (&mut gv, &mut ge);
}

#[test]
fn rarely_used_messages_survive_a_wire_round_trip() {
    let doc = SharedDoc::host(PeerId::HOST, "abc", DEFAULT_CAP_BYTES).unwrap();
    let head = doc.anchor_at(1).unwrap();
    let messages = vec![
        Message::TerminalClose,
        Message::TerminalInputAllowed { allowed: true },
        Message::TerminalKeys {
            bytes: b"k".to_vec(),
        },
        Message::ListFiles,
        Message::Error {
            code: ErrorCode::Declined,
        },
        Message::Error {
            code: ErrorCode::Version,
        },
        Message::Error {
            code: ErrorCode::Removed,
        },
        Message::Error {
            code: ErrorCode::BadPath,
        },
        Message::Error {
            code: ErrorCode::TooLarge,
        },
        Message::Presence(Presence {
            peer: PeerId(4),
            path: path("a.txt"),
            head: head.clone(),
            anchor: Some(head.clone()),
        }),
        Message::Presence(Presence {
            peer: PeerId(4),
            path: path("a.txt"),
            head,
            anchor: None,
        }),
    ];
    for m in messages {
        let bytes = m.encode();
        assert_eq!(decode(&bytes).unwrap(), m, "{m:?}");
    }
    assert_eq!(Message::TerminalClose.channel(), Channel::Terminal);
    assert_eq!(Message::ListFiles.channel(), Channel::Workspace);
    assert_eq!(
        Message::Error {
            code: ErrorCode::Declined
        }
        .channel(),
        Message::Error {
            code: ErrorCode::Declined
        }
        .channel()
    );
    // An unknown tag and a truncated message are typed errors, never panics.
    assert!(matches!(decode(&[250]), Err(CoworkError::Wire(_))));
    assert!(decode(&[]).is_err());
    let mut bytes = Message::TerminalKeys {
        bytes: b"abc".to_vec(),
    }
    .encode();
    bytes.truncate(bytes.len() - 1);
    assert!(decode(&bytes).is_err());
    assert!(
        decode(&vec![0u8; 20_000_000]).is_err(),
        "over the message cap"
    );
}
