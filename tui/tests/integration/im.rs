//! The IM menu, panel and App wiring (T752), anchored threads (T753), the
//! status-bar indicator and notify policy (T754), and disk history (T755).
//! The IRC round trip runs against an in-process fake server on 127.0.0.1;
//! no test uses the network.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use crate::integration::common::*;
use vix_im::{ConversationId, Message, MessageKind, Presence};

/// Every action id the spec's IM menu table lists.
const IM_ACTIONS: &[&str] = &[
    "im.panel",
    "im.connect",
    "im.disconnect",
    "im.join",
    "im.leave",
    "im.direct",
    "im.next_unread",
    "im.share_selection",
    "im.share_location",
    "im.comment_line",
    "im.threads",
    "im.next_thread",
    "im.prev_thread",
    "im.resolve_thread",
    "im.status_available",
    "im.status_away",
    "im.status_busy",
    "im.notify_all",
    "im.notify_mentions",
    "im.notify_off",
    "im.clear_history",
];

fn notes() -> ConversationId {
    ConversationId::new("notes")
}

fn type_line(app: &mut App, text: &str) {
    for c in text.chars() {
        app.on_key(key(c));
    }
    app.on_key(keycode(KeyCode::Enter));
}

fn last_note(app: &App) -> Option<Message> {
    app.im
        .inbox
        .get(&notes())
        .and_then(|c| c.messages.last().cloned())
}

/// A workspace with `src/a.rs`, and a file just outside it.
fn workspace(tag: &str) -> PathBuf {
    let dir = unique_dir(tag);
    let root = dir.join("root");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/a.rs"), "fn a() {}\nfn b() {}\nfn c() {}\n").unwrap();
    fs::write(dir.join("outside.rs"), "secret\n").unwrap();
    root
}

/// Every action id under `items`, submenus included.
fn walk(items: &[vix::menu::Item], out: &mut Vec<&'static str>) {
    for it in items {
        match it.submenu {
            Some(sub) => walk(sub, out),
            None if !it.action.is_empty() => out.push(it.action),
            None => {}
        }
    }
}

#[test]
fn im_menu_sits_between_git_and_org_with_every_action() {
    let _app = app_at(Path::new("."));
    let menus = vix::menu::menus();
    let im = menus
        .iter()
        .position(|m| m.name == "menu.im")
        .expect("IM menu");
    let mut ids = Vec::new();
    walk(menus[im].items, &mut ids);
    for id in IM_ACTIONS {
        assert_eq!(
            ids.contains(id),
            vix::capability::is_available(id),
            "{id} is in the IM menu exactly when this build can run it"
        );
        assert!(
            vix::palette::COMMANDS.iter().any(|(_, a)| a == id),
            "{id} has a palette entry"
        );
    }
    assert_eq!(menus[im + 1].name, "menu.org");
    if cfg!(feature = "git") {
        assert_eq!(menus[im - 1].name, "menu.git");
    }
}

#[test]
fn the_panel_never_opens_on_its_own_and_toggles_focus() {
    let mut app = app_at(Path::new("."));
    assert!(!app.im.open, "IM starts closed");
    app.im_deliver(&notes(), Message::text("bob", 0, "hi @someone"));
    assert!(!app.im.open, "a new message does not open the panel");
    assert_eq!(app.focus, Focus::Editor, "nor take focus");
    app.run_action("im.panel");
    assert!(app.im.open);
    assert_eq!(app.focus, Focus::Im);
    app.on_key(esc());
    assert_eq!(app.focus, Focus::Editor, "Esc gives the editor back");
    assert!(app.im.open, "the panel stays shown");
}

#[test]
fn typing_in_notes_stores_a_message_without_a_connection() {
    let mut app = app_at(Path::new("."));
    app.run_action("im.panel");
    type_line(&mut app, "remember src/a.rs:2");
    let m = last_note(&app).expect("a note");
    assert_eq!(m.body, "remember src/a.rs:2");
    assert_eq!(m.author, app.im.inbox.nick());
    assert_eq!(m.kind, MessageKind::Text);
    assert_eq!(app.im.panel.input(), "", "the input clears");
    // `/me` stores an action.
    type_line(&mut app, "/me waves");
    assert_eq!(last_note(&app).unwrap().kind, MessageKind::Action);
}

#[test]
fn slash_commands_report_errors_and_never_send_them_as_text() {
    let mut app = app_at(Path::new("."));
    app.run_action("im.panel");
    let before = app.im.inbox.get(&notes()).unwrap().messages.len();
    type_line(&mut app, "/bogus hello");
    assert_eq!(app.im.inbox.get(&notes()).unwrap().messages.len(), before);
    assert_eq!(
        app.im.panel.input(),
        "/bogus hello",
        "an unknown command stays to be fixed"
    );
    // Clear the line, then a missing argument is an error too.
    for _ in 0.."/bogus hello".len() {
        app.on_key(keycode(KeyCode::Backspace));
    }
    type_line(&mut app, "/join");
    assert_eq!(app.im.panel.input(), "/join");
    for _ in 0.."/join".len() {
        app.on_key(keycode(KeyCode::Backspace));
    }
    // `/status busy` sets presence; `//x` sends the text `/x`.
    type_line(&mut app, "/status busy");
    assert_eq!(app.im.presence, Presence::Busy);
    type_line(&mut app, "//etc/hosts");
    assert_eq!(last_note(&app).unwrap().body, "/etc/hosts");
}

#[test]
fn share_location_sends_the_cursor_position() {
    let root = workspace("im-location");
    let mut app = app_at(&root);
    app.open_initial(&root.join("src/a.rs"));
    let area = app.layout.editor;
    app.editor.goto(2, Some(4), area);
    app.run_action("im.share_location");
    let m = last_note(&app).expect("a location message");
    assert_eq!(
        m.kind,
        MessageKind::Location {
            path: "src/a.rs".into(),
            line: 2,
            column: 4,
        }
    );
    assert_eq!(m.body, "src/a.rs:2:4");
    assert!(!app.im.open, "sharing does not open the panel");
}

#[test]
fn share_selection_asks_first_and_shows_what_will_be_sent() {
    let root = workspace("im-share");
    let mut app = app_at(&root);
    app.open_initial(&root.join("src/a.rs"));
    app.run_action("im.share_selection");
    assert!(
        app.im_dialog.is_none(),
        "nothing selected, nothing to share"
    );
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_selection_range(10, 19); // "fn b() {}"
    let before = app.im.inbox.get(&notes()).unwrap().messages.len();
    app.run_action("im.share_selection");
    assert!(app.im_dialog.is_some(), "a confirm opens first");
    assert_eq!(
        app.im.inbox.get(&notes()).unwrap().messages.len(),
        before,
        "nothing is sent before the answer"
    );
    app.on_key(key('n'));
    assert!(app.im_dialog.is_none());
    assert_eq!(app.im.inbox.get(&notes()).unwrap().messages.len(), before);
    app.run_action("im.share_selection");
    app.on_key(key('y'));
    let m = last_note(&app).expect("a snippet");
    let MessageKind::Snippet {
        path,
        start_line,
        end_line,
        code,
        ..
    } = m.kind
    else {
        panic!("expected a snippet, got {:?}", m.kind);
    };
    assert_eq!(
        (path.as_str(), start_line, end_line, code.as_str()),
        ("src/a.rs", 2, 2, "fn b() {}")
    );
    assert_eq!(m.body, "src/a.rs:2");
}

#[test]
fn enter_on_a_code_reference_opens_only_inside_the_workspace() {
    let root = workspace("im-open");
    let mut app = app_at(&root);
    for body in ["see ../outside.rs:1", "see src/a.rs:3"] {
        app.im_deliver(&notes(), Message::text("bob", 0, body));
    }
    app.run_action("im.panel");
    let opened = |app: &App| app.editor.active_tab().and_then(|t| t.path.clone());
    let before = opened(&app);
    // Up from the empty input highlights the newest message; Up again the
    // one before it.
    app.on_key(keycode(KeyCode::Up));
    app.on_key(keycode(KeyCode::Up));
    app.on_key(keycode(KeyCode::Enter));
    assert_eq!(
        opened(&app),
        before,
        "a reference outside the root opens nothing"
    );
    assert_eq!(app.focus, Focus::Im);
    app.on_key(keycode(KeyCode::Down));
    app.on_key(keycode(KeyCode::Enter));
    let path = opened(&app).expect("src/a.rs opened");
    assert!(path.ends_with("src/a.rs"));
    assert_eq!(app.editor.cursor_1based().0, 3);
    assert_eq!(app.focus, Focus::Editor);
}

#[test]
fn unread_count_and_mention_show_in_the_status_segment_and_busy_is_quiet() {
    let mut app = app_at(Path::new("."));
    assert_eq!(app.im_status_segment(), "");
    let nick = app.im.inbox.nick().to_string();
    let m = Message::text("bob", 0, format!("hey @{nick} look"));
    let status_before = app.status.clone();
    app.im.presence = Presence::Busy;
    app.im_deliver(&notes(), m.clone());
    assert_eq!(app.status, status_before, "Busy silences the notification");
    let segment = app.im_status_segment();
    assert!(segment.starts_with("IM "), "{segment}");
    assert!(segment.contains('@'), "a mention is marked as text");
    app.im.presence = Presence::Available;
    app.im_deliver(&notes(), m.clone());
    assert_ne!(app.status, status_before, "a mention notifies by default");
    // `notify = off` keeps the count but never notifies.
    app.run_action("im.notify_off");
    let quiet = app.status.clone();
    app.im_deliver(&notes(), m);
    assert_eq!(app.status, quiet);
    // Opening the panel on the conversation reads it.
    app.run_action("im.panel");
    assert_eq!(app.im_status_segment(), "");
    assert!(!app.im.inbox.has_mention());
}

#[test]
fn disk_history_round_trips_prunes_and_clears() {
    let dir = unique_dir("im-history");
    let _ = fs::remove_dir_all(&dir);
    // An old message beyond `history_days` is pruned on load.
    fs::create_dir_all(&dir).unwrap();
    let old = vix_im::history::encode_line(&Message::text("ana", 0, "ancient")).unwrap();
    fs::write(dir.join("notes.jsonl"), format!("{old}\n")).unwrap();

    let mut app = app_at(Path::new("."));
    app.im_use_history_dir(dir.clone());
    assert!(
        app.im.inbox.get(&notes()).unwrap().messages.is_empty(),
        "the 1970 message is older than 30 days"
    );
    app.run_action("im.panel");
    type_line(&mut app, "kept on disk");
    let text = fs::read_to_string(dir.join("notes.jsonl")).unwrap();
    assert!(text.contains("kept on disk"));
    assert!(!text.contains("ancient"));

    let mut again = app_at(Path::new("."));
    again.im_use_history_dir(dir.clone());
    assert_eq!(last_note(&again).unwrap().body, "kept on disk");
    assert_eq!(again.im.inbox.total_unread(), 0, "history loads as read");

    again.run_action("im.clear_history");
    assert!(again.im_dialog.is_some(), "Clear History asks first");
    again.on_key(key('y'));
    assert_eq!(again.im.inbox.get(&notes()).unwrap().messages.len(), 0);
    assert!(!dir.join("notes.jsonl").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn network_actions_need_a_connection_or_the_capability() {
    let mut app = app_at(Path::new("."));
    app.run_action("im.connect");
    if cfg!(feature = "im") {
        // No `[[im.accounts]]` configured: a status says how to add one.
        assert!(app.im_dialog.is_none());
        assert!(app.im.links.is_empty());
    } else {
        assert!(
            app.messages.items.iter().any(|m| m.text.contains("im")),
            "a build without `im` says the capability is missing"
        );
    }
    // Notes still works either way.
    app.run_action("im.panel");
    type_line(&mut app, "offline");
    assert_eq!(last_note(&app).unwrap().body, "offline");
}

#[cfg(feature = "im")]
mod irc {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    struct Server {
        reader: BufReader<std::net::TcpStream>,
        writer: std::net::TcpStream,
    }

    impl Server {
        fn expect(&mut self, want: &str) {
            let mut line = String::new();
            self.reader.read_line(&mut line).unwrap();
            assert_eq!(line.trim_end(), want);
        }

        fn say(&mut self, line: &str) {
            self.writer.write_all(line.as_bytes()).unwrap();
            self.writer.write_all(b"\r\n").unwrap();
        }
    }

    fn room() -> ConversationId {
        ConversationId::new("irc/Test/#vix")
    }

    #[test]
    fn connect_join_receive_and_send_against_a_fake_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut s = Server {
                reader: BufReader::new(stream.try_clone().unwrap()),
                writer: stream,
            };
            s.expect("CAP LS 302");
            s.expect("NICK ana");
            s.expect("USER ana 0 * ana");
            s.say(":irc.test CAP * LS :account-tag");
            s.expect("CAP REQ account-tag");
            s.say(":irc.test CAP ana ACK :account-tag");
            s.expect("CAP END");
            s.say(":irc.test 001 ana :Welcome");
            s.expect("JOIN #vix");
            s.say(":ana!a@h JOIN #vix");
            s.say("@account=bob :bob!b@h PRIVMSG #vix :hi @ana see src/a.rs:2");
            s.say(":eve!e@h PRIVMSG #vix :\u{1b}[31mred\u{202e}txt");
            s.expect("PRIVMSG #vix :hello from vix");
            s.expect("QUIT");
        });

        let mut settings = Settings::default();
        settings.im.nick = "ana".into();
        let mut account = vix::settings::ImAccount::default();
        account.name = "Test".into();
        account.server = "127.0.0.1".into();
        account.port = port;
        account.tls = false;
        account.channels = vec!["#vix".into()];
        settings.im.accounts = vec![account];
        let mut app = App::new(Path::new(".").to_path_buf(), settings)
            .with_session_path(isolated_session_path());

        app.run_action("im.connect");
        assert_eq!(app.im.links.len(), 1);
        assert!(!app.im.open, "connecting does not open the panel");
        let plaintext = app
            .im
            .inbox
            .get(&ConversationId::new("irc/Test"))
            .unwrap()
            .messages
            .iter()
            .any(|m| m.kind == MessageKind::System);
        assert!(plaintext, "tls = false is flagged in the conversation");

        wait_until(
            &mut app,
            App::poll_im,
            |a| {
                a.im.inbox.get(&room()).is_some_and(|c| {
                    c.messages.iter().filter(|m| !m.author.is_empty()).count() >= 2
                })
            },
            Duration::from_secs(10),
        );
        let conv = app.im.inbox.get(&room()).unwrap();
        let people: Vec<&Message> = conv
            .messages
            .iter()
            .filter(|m| !m.author.is_empty())
            .collect();
        assert_eq!(people[0].author, "bob");
        assert_eq!(people[0].body, "hi @ana see src/a.rs:2");
        assert!(conv.mentioned, "@ana is a mention");
        assert!(
            !app.im.unverified(&room(), "bob"),
            "bob's account tag verifies him"
        );
        assert!(app.im.unverified(&room(), "eve"));
        // Received escape sequences and bidi overrides are stripped.
        assert_eq!(people[1].body, "redtxt");
        assert!(app.im_status_segment().contains('@'));
        assert!(!app.im.open, "receiving does not open the panel");

        // The joined room is selected; type into it.
        app.run_action("im.panel");
        assert_eq!(app.im.selected_id(), Some(room()));
        type_line(&mut app, "hello from vix");
        let mine = app
            .im
            .inbox
            .get(&room())
            .unwrap()
            .messages
            .last()
            .unwrap()
            .clone();
        assert_eq!(mine.body, "hello from vix");

        app.run_action("im.disconnect");
        assert!(app.im.links.is_empty());
        server.join().unwrap();
    }
}

/// A Matrix account (T793) with no helper behind it yet.
#[cfg(feature = "im")]
fn matrix_settings(helper: &str) -> Settings {
    let mut settings = Settings::default();
    settings.im.nick = "ana".into();
    settings.im.matrix_helper = helper.into();
    let mut account = vix::settings::ImAccount::default();
    account.name = "Home".into();
    account.protocol = "matrix".into();
    account.homeserver = "https://matrix.invalid".into();
    account.user = "@ana:matrix.invalid".into();
    settings.im.accounts = vec![account];
    settings
}

#[cfg(feature = "im")]
#[test]
fn a_matrix_account_needs_its_fields_and_the_feature() {
    let mut settings = matrix_settings("");
    settings.im.accounts[0].homeserver = String::new();
    let mut app =
        App::new(Path::new(".").to_path_buf(), settings).with_session_path(isolated_session_path());
    app.run_action("im.connect");
    assert!(app.im.links.is_empty(), "no homeserver: nothing starts");

    let mut settings = matrix_settings("");
    settings.im.accounts[0].protocol = "xmpp".into();
    let mut app =
        App::new(Path::new(".").to_path_buf(), settings).with_session_path(isolated_session_path());
    app.run_action("im.connect");
    assert!(app.im.links.is_empty(), "an unknown protocol is refused");

    #[cfg(not(feature = "matrix"))]
    {
        let mut app = App::new(Path::new(".").to_path_buf(), matrix_settings(""))
            .with_session_path(isolated_session_path());
        app.run_action("im.connect");
        assert!(app.im.links.is_empty(), "no `matrix` feature: no helper");
    }
}

#[cfg(feature = "matrix")]
mod matrix {
    use super::*;
    use vix_matrix_protocol::net::Handle;
    use vix_matrix_protocol::{ErrorKey, MessageKind as Kind, Reply, Request};

    const ROOM: &str = "!room:matrix.invalid";

    fn room() -> ConversationId {
        ConversationId::new(format!("matrix/Home/{ROOM}"))
    }

    fn home() -> ConversationId {
        ConversationId::new("matrix/Home")
    }

    fn message(sender: &str, body: &str, kind: Kind, verified: bool) -> Reply {
        Reply::Message {
            room: ROOM.into(),
            event_id: "$e".into(),
            sender: sender.into(),
            sender_name: Some("Ana".into()),
            time: 1_762_732_800_000,
            body: body.into(),
            kind,
            encrypted: true,
            verified,
        }
    }

    fn system_lines(app: &App, id: &ConversationId) -> usize {
        app.im.inbox.get(id).map_or(0, |c| {
            c.messages
                .iter()
                .filter(|m| m.kind == MessageKind::System)
                .count()
        })
    }

    /// What the stand-in helper says after login.
    fn incoming() -> Vec<Reply> {
        vec![
            Reply::Ready {
                helper: "test".into(),
                protocol: 1,
            },
            Reply::LoggedIn {
                user_id: "@ana:matrix.invalid".into(),
                device_id: "DEV".into(),
                restored: false,
            },
            Reply::Room {
                id: ROOM.into(),
                name: "Vix \u{1b}[31mdev\u{202e}".into(),
                encrypted: true,
                direct: false,
            },
            Reply::Room {
                id: "!dm:matrix.invalid".into(),
                name: "Bob".into(),
                encrypted: true,
                direct: true,
            },
            message(
                "@bob:matrix.invalid",
                "hi ana see src/a.rs:2",
                Kind::Text,
                true,
            ),
            message(
                "@eve:matrix.invalid",
                "\u{1b}[31mred\u{202e}txt",
                Kind::Emote,
                false,
            ),
            Reply::Undecryptable {
                room: ROOM.into(),
                event_id: "$u".into(),
                sender: "@carol:matrix.invalid".into(),
                time: 0,
            },
            Reply::error(ErrorKey::NoSession),
            Reply::error_with(ErrorKey::Network, "M_LIMIT_EXCEEDED"),
        ]
    }

    #[test]
    fn rooms_messages_and_sends_go_through_the_helper() {
        let settings = matrix_settings("");
        let account = settings.im.accounts[0].clone();
        let mut app = App::new(Path::new(".").to_path_buf(), settings)
            .with_session_path(isolated_session_path());
        let (handle, peer) = Handle::pair();
        app.im_matrix_start(&account, handle);
        let sent = peer.drain();
        assert!(
            matches!(sent.as_slice(), [Request::Login { account, homeserver, user, .. }]
                if account == "Home" && homeserver == "https://matrix.invalid"
                    && user == "@ana:matrix.invalid"),
            "{sent:?}"
        );
        assert!(!app.im.open, "connecting does not open the panel");

        for reply in incoming() {
            assert!(peer.reply(reply));
        }
        app.poll_im();

        let conv = app.im.inbox.get(&room()).unwrap();
        assert_eq!(conv.title, "Vix dev", "the room name is sanitized");
        assert_eq!(
            app.im
                .inbox
                .get(&ConversationId::new("matrix/Home/!dm:matrix.invalid"))
                .unwrap()
                .kind,
            vix_im::ConversationKind::Direct
        );
        let people: Vec<&Message> = conv
            .messages
            .iter()
            .filter(|m| !m.author.is_empty())
            .collect();
        assert_eq!(people.len(), 2);
        assert_eq!(people[0].author, "@bob:matrix.invalid", "shown by user id");
        assert_eq!(people[0].body, "hi ana see src/a.rs:2");
        assert_eq!(people[1].kind, MessageKind::Action);
        assert_eq!(people[1].body, "redtxt", "escapes and bidi are stripped");
        assert!(!app.im.unverified(&room(), "@bob:matrix.invalid"));
        assert!(app.im.unverified(&room(), "@eve:matrix.invalid"));
        assert_eq!(system_lines(&app, &room()), 1, "the undecryptable event");
        assert!(system_lines(&app, &home()) >= 3, "signed in, two errors");
        assert!(!app.im.open, "receiving does not open the panel");

        // Typing into the room sends `m.text`, `/me` sends `m.emote`.
        app.run_action("im.panel");
        app.im.panel.select(&mut app.im.inbox, &room());
        type_line(&mut app, "hello from vix");
        type_line(&mut app, "/me waves");
        assert_eq!(
            peer.drain(),
            vec![
                Request::Send {
                    room: ROOM.into(),
                    body: "hello from vix".into(),
                    kind: Kind::Text
                },
                Request::Send {
                    room: ROOM.into(),
                    body: "waves".into(),
                    kind: Kind::Emote
                },
            ]
        );
        let last = app.im.inbox.get(&room()).unwrap().messages.last().cloned();
        assert_eq!(last.map(|m| m.body), Some("waves".to_string()));

        // `/nick` and `/topic` have no Matrix counterpart yet; nothing is sent.
        type_line(&mut app, "/nick other");
        type_line(&mut app, "/topic new");
        assert_eq!(peer.drain(), Vec::<Request>::new());

        // A later message from a device that is not cross-signed clears
        // the sender's verified mark.
        assert!(peer.reply(message("@bob:matrix.invalid", "again", Kind::Text, false)));
        app.poll_im();
        assert!(app.im.unverified(&room(), "@bob:matrix.invalid"));

        // `/part` leaves; the helper's `left` removes the conversation.
        type_line(&mut app, "/part");
        assert_eq!(peer.drain(), vec![Request::Leave { room: ROOM.into() }]);
        assert!(peer.reply(Reply::Left { id: ROOM.into() }));
        app.poll_im();
        assert!(app.im.inbox.get(&room()).is_none());

        app.run_action("im.disconnect");
        assert!(app.im.links.is_empty());
        assert_eq!(peer.drain(), vec![Request::Quit]);
    }

    #[test]
    fn a_missing_helper_says_so_and_the_link_ends() {
        let missing = unique_dir("matrix-missing").join("no-vix-matrix");
        let settings = matrix_settings(&missing.to_string_lossy());
        let mut app = App::new(Path::new(".").to_path_buf(), settings)
            .with_session_path(isolated_session_path());
        app.run_action("im.connect");
        app.poll_im();
        assert!(app.im.links.is_empty(), "the ended link is dropped");
        assert_eq!(
            system_lines(&app, &home()),
            2,
            "helper missing, then closed"
        );
    }
}

// ----- anchored threads (T753) ---------------------------------------------

/// A fresh workspace (no thread files left from an earlier run) with
/// `src/a.rs` open.
fn thread_workspace(tag: &str) -> (PathBuf, App) {
    let root = workspace(tag);
    let _ = fs::remove_dir_all(root.join(".vix"));
    fs::write(
        root.join("src/a.rs"),
        "fn a() {}\nfn b() {}\nfn c() {}\nfn d() {}\n",
    )
    .unwrap();
    let mut app = app_at(&root);
    app.open_initial(&root.join("src/a.rs"));
    (root, app)
}

fn goto_line(app: &mut App, line: usize) {
    let area = app.layout.editor;
    app.editor.goto(line, Some(1), area);
}

fn thread_files(root: &Path) -> Vec<vix_im::Thread> {
    let Ok(entries) = fs::read_dir(root.join(".vix/im/threads")) else {
        return Vec::new();
    };
    let mut threads: Vec<vix_im::Thread> = entries
        .map(|e| {
            vix_im::Thread::from_json(&fs::read_to_string(e.unwrap().path()).unwrap()).unwrap()
        })
        .collect();
    threads.sort_by_key(|t| t.start_line);
    threads
}

fn marks(app: &mut App) -> Vec<(usize, vix_editor_core::editor::CommentMark)> {
    app.refresh_thread_marks_if_stale();
    app.editor
        .active_tab()
        .unwrap()
        .editor
        .comment_marks()
        .to_vec()
}

#[test]
fn comment_on_line_starts_a_thread_then_replies_to_it() {
    use vix_editor_core::editor::CommentMark;
    let (root, mut app) = thread_workspace("im-thread-start");
    goto_line(&mut app, 2);
    app.run_action("im.comment_line");
    assert!(app.prompt.is_some(), "a prompt asks for the comment");
    type_line(&mut app, "why b?");
    assert!(app.prompt.is_none());
    let files = thread_files(&root);
    assert_eq!(files.len(), 1, "the thread is written at once");
    let t = &files[0];
    assert_eq!(
        (t.path.as_str(), t.start_line, t.end_line),
        ("src/a.rs", 2, 2)
    );
    assert_eq!(t.anchor_text, "fn b() {}");
    assert_eq!(t.messages[0].body, "why b?");
    assert!(!t.resolved);
    assert_eq!(marks(&mut app), [(1, CommentMark::Open)]);
    assert!(!app.im.open, "commenting never opens the panel");
    assert_eq!(app.focus, Focus::Editor);

    // On the same line it is a reply, not a second thread.
    app.run_action("im.comment_line");
    type_line(&mut app, "because");
    let files = thread_files(&root);
    assert_eq!(files.len(), 1);
    let bodies: Vec<&str> = files[0].messages.iter().map(|m| m.body.as_str()).collect();
    assert_eq!(bodies, ["why b?", "because"]);

    // An empty answer adds nothing.
    app.run_action("im.comment_line");
    type_line(&mut app, "");
    assert_eq!(thread_files(&root)[0].messages.len(), 2);
}

#[test]
fn a_selection_anchors_its_whole_line_range() {
    let (root, mut app) = thread_workspace("im-thread-range");
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_selection_range(10, 29); // "fn b() {}\nfn c() {}"
    app.run_action("im.comment_line");
    type_line(&mut app, "these two");
    let t = &thread_files(&root)[0];
    assert_eq!((t.start_line, t.end_line), (2, 3));
    assert_eq!(t.anchor_text, "fn b() {}\nfn c() {}");
}

#[test]
fn threads_follow_their_lines_as_the_buffer_changes_and_save_moves_them() {
    use vix_editor_core::editor::CommentMark;
    let (root, mut app) = thread_workspace("im-thread-move");
    goto_line(&mut app, 3);
    app.run_action("im.comment_line");
    type_line(&mut app, "about c");
    assert_eq!(marks(&mut app), [(2, CommentMark::Open)]);
    // Two new lines at the top: the mark moves down with `fn c`.
    goto_line(&mut app, 1);
    for c in "// one\n// two\n".chars() {
        if c == '\n' {
            app.on_key(keycode(KeyCode::Enter));
        } else {
            app.on_key(key(c));
        }
    }
    assert_eq!(marks(&mut app), [(4, CommentMark::Open)]);
    assert_eq!(
        thread_files(&root)[0].start_line,
        3,
        "an unsaved buffer does not move the stored line"
    );
    app.run_action("file.save");
    assert_eq!(thread_files(&root)[0].start_line, 5, "saving moves it");
    // Replace the line and its context: the thread is outdated, unmarked.
    let tab = app.editor.active_tab_mut().unwrap();
    tab.editor
        .set_selection_range(0, tab.editor.code_ref().len_chars());
    app.on_key(key('x'));
    assert_eq!(marks(&mut app), []);
}

#[test]
fn threads_list_jumps_resolves_and_reopens() {
    use vix_editor_core::editor::CommentMark;
    let (root, mut app) = thread_workspace("im-thread-list");
    for (line, text) in [(2, "on b"), (4, "on d")] {
        goto_line(&mut app, line);
        app.run_action("im.comment_line");
        type_line(&mut app, text);
    }
    goto_line(&mut app, 4);
    app.run_action("im.threads");
    let Some(vix::app::im::ImDialog::Threads { ids, selected }) = app.im_dialog.clone() else {
        panic!("the Threads list opens");
    };
    assert_eq!(ids.len(), 2);
    assert_eq!(selected, 1, "the thread at the cursor is highlighted");
    app.on_key(key('r'));
    assert!(
        thread_files(&root)[1].resolved,
        "r resolves, and it is saved"
    );
    assert!(app.im_dialog.is_some(), "the list stays open");
    assert_eq!(
        marks(&mut app),
        [(1, CommentMark::Open), (3, CommentMark::Resolved)]
    );
    app.on_key(keycode(KeyCode::Up));
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.im_dialog.is_none(), "Enter jumps and closes");
    assert_eq!(app.editor.cursor_1based().0, 2);
    // Resolve or Reopen on the cursor's line.
    goto_line(&mut app, 4);
    app.run_action("im.resolve_thread");
    assert!(!thread_files(&root)[1].resolved, "reopened");
    goto_line(&mut app, 3);
    app.run_action("im.resolve_thread");
    assert!(
        thread_files(&root).iter().all(|t| !t.resolved),
        "no thread on line 3"
    );
    // Esc closes the list.
    app.run_action("im.threads");
    app.on_key(esc());
    assert!(app.im_dialog.is_none());
}

#[test]
fn next_and_previous_thread_wrap_around_the_file() {
    let (_root, mut app) = thread_workspace("im-thread-step");
    app.run_action("im.next_thread");
    assert_eq!(app.editor.cursor_1based().0, 1, "no threads: nothing moves");
    for line in [2, 4] {
        goto_line(&mut app, line);
        app.run_action("im.comment_line");
        type_line(&mut app, "x");
    }
    goto_line(&mut app, 1);
    app.run_action("im.next_thread");
    assert_eq!(app.editor.cursor_1based().0, 2);
    app.run_action("im.next_thread");
    assert_eq!(app.editor.cursor_1based().0, 4);
    app.run_action("im.next_thread");
    assert_eq!(app.editor.cursor_1based().0, 2, "wraps to the first");
    app.run_action("im.prev_thread");
    assert_eq!(app.editor.cursor_1based().0, 4, "wraps to the last");
    assert!(!app.im.open);
}

#[test]
fn thread_files_from_elsewhere_load_and_unsafe_ones_are_skipped() {
    use vix_editor_core::editor::CommentMark;
    let root = workspace("im-thread-load");
    let _ = fs::remove_dir_all(root.join(".vix"));
    let dir = root.join(".vix/im/threads");
    fs::create_dir_all(&dir).unwrap();
    let lines = ["fn a() {}", "fn b() {}", "fn c() {}"];
    let mut good = vix_im::Thread::new("from-git", "src/a.rs", &lines, 3, 3);
    good.reply(Message::text("bob", 0, "see \u{1b}[31mthis"));
    fs::write(dir.join("from-git.json"), good.to_json().unwrap()).unwrap();
    let escape = vix_im::Thread::new("escape", "../outside.rs", &lines, 1, 1);
    fs::write(dir.join("escape.json"), escape.to_json().unwrap()).unwrap();
    let renamed = vix_im::Thread::new("other", "src/a.rs", &lines, 1, 1);
    fs::write(dir.join("renamed.json"), renamed.to_json().unwrap()).unwrap();
    fs::write(dir.join("junk.json"), "{").unwrap();

    let mut app = app_at(&root);
    app.open_initial(&root.join("src/a.rs"));
    assert_eq!(marks(&mut app), [(2, CommentMark::Open)]);
    let ids: Vec<&str> = app
        .im
        .threads
        .threads
        .iter()
        .map(|t| t.id.as_str())
        .collect();
    assert_eq!(ids, ["from-git"], "only the safe, well-formed file loads");
    assert_eq!(
        app.im.threads.threads[0].messages[0].body, "see this",
        "message text is sanitized"
    );
    assert!(!app.im.open && app.im_dialog.is_none(), "nothing pops up");
}

#[test]
fn outdated_threads_have_no_mark_and_say_so_on_jump() {
    let root = workspace("im-thread-outdated");
    let _ = fs::remove_dir_all(root.join(".vix"));
    let dir = root.join(".vix/im/threads");
    fs::create_dir_all(&dir).unwrap();
    let lines = ["unrelated", "text that", "is gone"];
    let t = vix_im::Thread::new("gone", "src/a.rs", &lines, 2, 2);
    fs::write(dir.join("gone.json"), t.to_json().unwrap()).unwrap();
    let mut app = app_at(&root);
    app.open_initial(&root.join("src/a.rs"));
    assert_eq!(marks(&mut app), []);
    app.run_action("im.threads");
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.im.threads.outdated.contains("gone"));
    assert!(app.im_dialog.is_none(), "Enter still opens the file");
    assert!(
        app.editor
            .active_tab()
            .and_then(|t| t.path.as_ref())
            .is_some_and(|p| p.ends_with("src/a.rs"))
    );
}

#[test]
fn comment_needs_a_file_inside_the_workspace() {
    let root = workspace("im-thread-nofile");
    let _ = fs::remove_dir_all(root.join(".vix"));
    let mut app = app_at(&root);
    app.run_action("im.comment_line");
    assert!(app.prompt.is_none(), "an untitled buffer has no thread");
    app.open_initial(&root.join("../outside.rs"));
    app.run_action("im.comment_line");
    assert!(
        app.prompt.is_none(),
        "a file outside the root has none either"
    );
    app.run_action("im.threads");
    assert!(app.im_dialog.is_none(), "no threads, no list");
    assert!(!root.join(".vix").exists(), "nothing was written");
}
