//! The IM menu, panel and App wiring (T752), the status-bar indicator and
//! notify policy (T754), and disk history (T755). The IRC round trip runs
//! against an in-process fake server on 127.0.0.1; no test uses the network.
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
