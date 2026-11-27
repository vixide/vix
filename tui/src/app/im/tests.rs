//! Behavior tests for the IM App side that need no network: routes without a
//! live connection, the Cowork session conversation (capability off), Matrix
//! account detection, dialogs, history and the status segment.

use super::*;
use crate::settings::Settings;
use ratatui::layout::Rect;
use vix_input::KeyModifiers;

fn temp(tag: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("vix-imt-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn app() -> App {
    let mut app = App::new(temp("app"), Settings::default());
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn notes() -> ConversationId {
    ConversationId::new(NOTES)
}

/// Add a room `#vix` on account `acct` (no live connection) and select it.
fn room(app: &mut App) -> ConversationId {
    let id = app.im_ensure_conversation(
        irc_id("acct", Some("#vix")),
        ConversationKind::Room,
        "#vix",
        Some(Route {
            account: "acct".into(),
            target: Some("#vix".into()),
        }),
    );
    app.im.panel.select(&mut app.im.inbox, &id);
    id
}

fn server(app: &mut App) -> ConversationId {
    let id = app.im_ensure_conversation(
        irc_id("acct", None),
        ConversationKind::Room,
        "acct",
        Some(Route {
            account: "acct".into(),
            target: None,
        }),
    );
    app.im.panel.select(&mut app.im.inbox, &id);
    id
}

fn session(app: &mut App) -> ConversationId {
    let id = app.im_ensure_conversation(
        ConversationId::new(SESSION),
        ConversationKind::Session,
        "session",
        None,
    );
    app.im.panel.select(&mut app.im.inbox, &id);
    id
}

fn type_line(app: &mut App, text: &str) {
    for c in text.chars() {
        app.im_key(key(KeyCode::Char(c)));
    }
    app.im_key(key(KeyCode::Enter));
}

fn messages(app: &App, id: &ConversationId) -> Vec<String> {
    app.im
        .inbox
        .get(id)
        .unwrap()
        .messages
        .iter()
        .map(|m| m.body.clone())
        .collect()
}

// ----- pure helpers --------------------------------------------------------------------------

#[test]
fn nicks_are_cleaned_to_irc_characters() {
    assert_eq!(default_nick("  ana "), "ana");
    assert_eq!(
        default_nick("123abc!x"),
        "abcx",
        "digits lead no nick; '!' is dropped"
    );
    assert_eq!(default_nick("-9-x"), "x");
    assert_eq!(default_nick("999"), "vix");
    assert_eq!(default_nick("!!!"), "vix");
    assert_eq!(default_nick(&"a".repeat(50)).len(), 30);
    // With no setting the OS user name is used, cleaned the same way.
    let derived = default_nick("");
    assert!(!derived.is_empty() && derived.len() <= 30);
    assert!(derived.chars().all(nick_char));
}

#[test]
fn history_file_names_escape_everything_unsafe() {
    let name = |s: &str| history_file_name(&ConversationId::new(s));
    assert_eq!(name("notes"), "notes.jsonl");
    assert_eq!(name("irc/net/#vix"), "irc%2Fnet%2F%23vix.jsonl");
    assert_eq!(
        name(".hidden"),
        "%2Ehidden.jsonl",
        "a leading dot never survives"
    );
    assert_eq!(name("a.b"), "a.b.jsonl");
    assert_eq!(name("../x"), "%2E.%2Fx.jsonl");
    assert_eq!(name("é"), "%C3%A9.jsonl");
}

#[test]
fn irc_ids_fold_case_and_channels_are_recognised() {
    assert_eq!(irc_id("net", Some("#Vix")), irc_id("net", Some("#vix")));
    assert_eq!(irc_id("net", None).as_str(), "irc/net");
    assert_eq!(irc_id("net", Some("#vix")).as_str(), "irc/net/#vix");
    for c in ["#a", "&a", "+a", "!a"] {
        assert!(is_channel(c), "{c}");
    }
    assert!(!is_channel("bob"));
}

#[test]
fn titles_and_verification() {
    let mut app = app();
    assert_eq!(app.im.title(&notes()), t!("ui.im_notes").to_string());
    let s = session(&mut app);
    assert_eq!(app.im.title(&s), t!("ui.im_session").to_string());
    let r = room(&mut app);
    assert_eq!(app.im.title(&r), "#vix");
    assert_eq!(app.im.title(&ConversationId::new("nope")), "");

    // Only network conversations flag unverified nicks.
    assert!(!app.im.unverified(&notes(), "bob"));
    assert!(app.im.unverified(&r, "bob"));
    assert!(!app.im.unverified(&r, ""));
    let me = app.im.inbox.nick().to_string();
    assert!(!app.im.unverified(&r, &me));
    app.im
        .verified
        .insert((r.clone(), app.im.inbox.case_mapping().fold("Bob")));
    assert!(!app.im.unverified(&r, "BOB"));
}

// ----- history -----------------------------------------------------------------------------------

#[test]
fn history_paths_skip_the_session_and_follow_the_directory() {
    let mut app = app();
    assert!(
        app.im.history_path(&notes()).is_none(),
        "session-only by default"
    );
    let dir = temp("hist-path");
    app.im_use_history_dir(dir.clone());
    assert_eq!(
        app.im.history_path(&notes()).unwrap(),
        dir.join("notes.jsonl")
    );
    assert!(app.im.history_path(&ConversationId::new(SESSION)).is_none());
    // A conversation with no file yet loads as empty.
    let id = room(&mut app);
    assert_eq!(messages(&app, &id).len(), 0);
}

/// An app as `App::new` builds it with `history = "disk"` at `dir`: the
/// Notes history is known but not read yet (T830).
fn app_with_deferred_history(dir: &Path) -> App {
    let mut app = app();
    app.im.history_dir = Some(dir.to_path_buf());
    app.im.notes_pending = Some(30);
    app
}

#[test]
fn notes_history_is_read_on_the_first_im_use_not_at_startup() {
    let dir = temp("hist-deferred");
    let line = vix_im::history::encode_line(&Message::text("ana", now_ms(), "kept")).unwrap();
    std::fs::write(dir.join("notes.jsonl"), format!("{line}\n")).unwrap();
    let mut app = app_with_deferred_history(&dir);
    assert!(
        messages(&app, &notes()).is_empty(),
        "nothing read at startup"
    );
    // A non-IM action leaves it alone.
    app.run_action("view.toggle_explorer");
    assert!(app.im.notes_pending.is_some());
    app.run_action("im.panel");
    assert_eq!(messages(&app, &notes()), ["kept"]);
    assert!(app.im.notes_pending.is_none());
    assert_eq!(app.im.inbox.total_unread(), 0);
}

#[test]
fn a_note_delivered_before_the_deferred_load_keeps_the_history() {
    let dir = temp("hist-deferred-deliver");
    let line = vix_im::history::encode_line(&Message::text("ana", now_ms(), "old")).unwrap();
    std::fs::write(dir.join("notes.jsonl"), format!("{line}\n")).unwrap();
    let mut app = app_with_deferred_history(&dir);
    app.im_deliver(&notes(), Message::text("ana", now_ms(), "new"));
    assert_eq!(messages(&app, &notes()), ["old", "new"]);
    let raw = std::fs::read_to_string(dir.join("notes.jsonl")).unwrap();
    assert_eq!(raw.lines().count(), 2);
}

#[test]
fn delivered_messages_are_appended_and_reloaded() {
    let dir = temp("hist-roundtrip");
    let mut app = app();
    app.im_use_history_dir(dir.clone());
    app.im_deliver(&notes(), Message::text("ana", now_ms(), "first"));
    app.im_deliver(&notes(), Message::text("ana", now_ms(), "second"));
    let raw = std::fs::read_to_string(dir.join("notes.jsonl")).unwrap();
    assert_eq!(raw.lines().count(), 2);

    let mut again = App::new(temp("hist-again"), Settings::default());
    again.im_use_history_dir(dir);
    assert_eq!(messages(&again, &notes()), ["first", "second"]);
    assert_eq!(again.im.inbox.total_unread(), 0);
}

#[test]
fn old_messages_are_pruned_and_the_file_rewritten() {
    let dir = temp("hist-prune");
    let old = vix_im::history::encode_line(&Message::text("ana", 0, "ancient")).unwrap();
    let fresh = vix_im::history::encode_line(&Message::text("ana", now_ms(), "recent")).unwrap();
    std::fs::write(dir.join("notes.jsonl"), format!("{old}\n{fresh}\n")).unwrap();
    let mut app = app();
    app.im_use_history_dir(dir.clone());
    assert_eq!(messages(&app, &notes()), ["recent"]);
    let rewritten = std::fs::read_to_string(dir.join("notes.jsonl")).unwrap();
    assert!(!rewritten.contains("ancient"));
    assert!(rewritten.contains("recent"));
}

#[test]
fn history_beyond_the_message_limit_keeps_the_newest() {
    let dir = temp("hist-limit");
    let mut lines = String::new();
    for i in 0..(vix_im::DEFAULT_MESSAGE_LIMIT + 3) {
        let l =
            vix_im::history::encode_line(&Message::text("ana", now_ms(), format!("m{i}"))).unwrap();
        lines.push_str(&l);
        lines.push('\n');
    }
    std::fs::write(dir.join("notes.jsonl"), lines).unwrap();
    let mut app = app();
    app.im_use_history_dir(dir);
    let got = messages(&app, &notes());
    assert_eq!(got.len(), vix_im::DEFAULT_MESSAGE_LIMIT);
    assert_eq!(got.first().unwrap(), "m3");
}

#[test]
fn clearing_history_asks_then_wipes_messages_and_file() {
    let dir = temp("hist-clear");
    let mut app = app();
    app.im_use_history_dir(dir.clone());
    app.im_deliver(&notes(), Message::text("ana", now_ms(), "keep?"));
    assert!(dir.join("notes.jsonl").exists());
    app.run_action("im.clear_history");
    assert!(matches!(app.im_dialog, Some(ImDialog::Clear { .. })));
    app.im_dialog_key(key(KeyCode::Char('x'))); // other keys keep the dialog
    assert!(app.im_dialog.is_some());
    app.im_dialog_key(key(KeyCode::Char('n')));
    assert_eq!(app.status, t!("status.im_cancelled").to_string());
    assert_eq!(messages(&app, &notes()).len(), 1);
    app.run_action("im.clear_history");
    app.im_dialog_key(key(KeyCode::Enter));
    assert_eq!(messages(&app, &notes()).len(), 0);
    assert!(!dir.join("notes.jsonl").exists());
    assert_eq!(
        app.status,
        t!("status.im_history_cleared", name = t!("ui.im_notes")).to_string()
    );
}

// ----- panel, presence, notifications -------------------------------------------------------------------

#[test]
fn toggling_the_panel_shows_then_hides_it() {
    let mut app = app();
    app.run_action("im.panel");
    assert!(app.im.open && app.focus == Focus::Im);
    app.run_action("im.panel");
    assert!(!app.im.open && app.focus == Focus::Editor);
    // Shown but unfocused: the action focuses instead of hiding.
    app.im.open = true;
    app.focus = Focus::Editor;
    app.run_action("im.panel");
    assert!(app.im.open && app.focus == Focus::Im);
}

#[test]
fn presence_and_notify_policy_report_their_labels() {
    let mut app = app();
    for (action, p, key) in [
        ("im.status_away", Presence::Away, "menu.item.im.status_away"),
        ("im.status_busy", Presence::Busy, "menu.item.im.status_busy"),
        (
            "im.status_available",
            Presence::Available,
            "menu.item.im.status_available",
        ),
    ] {
        app.run_action(action);
        assert_eq!(app.im.presence, p);
        assert_eq!(
            app.status,
            t!("status.im_presence", status = t!(key)).to_string()
        );
    }
    for (action, policy, key) in [
        (
            "im.notify_all",
            NotifyPolicy::All,
            "menu.item.im.notify_all",
        ),
        (
            "im.notify_off",
            NotifyPolicy::Off,
            "menu.item.im.notify_off",
        ),
        (
            "im.notify_mentions",
            NotifyPolicy::Mentions,
            "menu.item.im.notify_mentions",
        ),
    ] {
        app.run_action(action);
        assert_eq!(app.im_notify_policy(), policy);
        assert_eq!(
            app.status,
            t!("status.im_notify", policy = t!(key)).to_string()
        );
    }
    app.settings.im.notify = "bogus".into();
    assert_eq!(app.im_notify_policy(), NotifyPolicy::Mentions);
}

#[test]
fn deliveries_notify_according_to_policy_and_screen() {
    let mut app = app();
    let r = room(&mut app);
    app.im.panel.select(&mut app.im.inbox, &notes());
    app.im.open = false;
    app.settings.im.notify = "all".into();
    let me = app.im.inbox.nick().to_string();

    assert_eq!(
        app.im_deliver(&r, Message::text("bob", now_ms(), "hello")),
        Delivery::Unread { mention: false }
    );
    assert_eq!(
        app.status,
        t!("status.im_new_message", name = "#vix").to_string()
    );
    app.im_deliver(
        &r,
        Message::text("bob", now_ms(), format!("hey @{me} look")),
    );
    assert_eq!(
        app.status,
        t!("status.im_mention", name = "#vix").to_string()
    );
    assert!(app.im.inbox.has_mention());
    assert!(app.im_status_segment().contains('@'));

    // Busy silences the status note; blocked authors and unknown rooms store nothing.
    app.status.clear();
    app.im.presence = Presence::Busy;
    app.im_deliver(&r, Message::text("bob", now_ms(), "quiet"));
    assert_eq!(app.status, "");
    app.im.inbox.set_blocked(["spam".to_string()]);
    assert_eq!(
        app.im_deliver(&r, Message::text("spam", now_ms(), "x")),
        Delivery::Blocked
    );
    assert_eq!(
        app.im_deliver(
            &ConversationId::new("nowhere"),
            Message::text("bob", now_ms(), "x")
        ),
        Delivery::NoConversation
    );

    // A message into the conversation on screen is read at once.
    app.im.panel.select(&mut app.im.inbox, &r);
    app.im.open = true;
    app.im_deliver(&r, Message::text("bob", now_ms(), "on screen"));
    assert_eq!(app.im.inbox.get(&r).unwrap().unread, 0);
}

#[test]
fn the_status_segment_counts_unread() {
    let mut app = app();
    assert_eq!(app.im_status_segment(), "");
    let r = room(&mut app);
    app.im.panel.select(&mut app.im.inbox, &notes());
    app.im_deliver(&r, Message::text("bob", now_ms(), "one"));
    app.im_deliver(&r, Message::text("bob", now_ms(), "two"));
    assert_eq!(
        app.im_status_segment(),
        format!("IM {}  ", t!("ui.im_unread", count = 2))
    );
}

#[test]
fn next_unread_selects_and_shows_the_conversation() {
    let mut app = app();
    app.run_action("im.next_unread");
    assert_eq!(app.status, t!("status.im_no_unread").to_string());
    let r = room(&mut app);
    app.im.panel.select(&mut app.im.inbox, &notes());
    app.im_deliver(&r, Message::text("bob", now_ms(), "ping"));
    app.run_action("im.next_unread");
    assert_eq!(app.im.selected_id(), Some(r.clone()));
    assert!(app.im.open && app.focus == Focus::Im);
    assert_eq!(app.im.inbox.get(&r).unwrap().unread, 0);
}

// ----- connections and rooms without a link ---------------------------------------------------------------------

#[cfg(feature = "im")]
#[test]
fn connect_without_accounts_or_with_several() {
    let mut app = app();
    app.run_action("im.connect");
    assert_eq!(app.status, t!("status.im_no_accounts").to_string());
    for name in ["one", "two", "three"] {
        let mut a = crate::settings::ImAccount::default();
        a.name = name.into();
        app.settings.im.accounts.push(a);
    }
    app.run_action("im.connect");
    assert!(matches!(
        app.im_dialog,
        Some(ImDialog::Accounts { selected: 0, .. })
    ));
    app.im_dialog_key(key(KeyCode::Down));
    app.im_dialog_key(key(KeyCode::Down));
    app.im_dialog_key(key(KeyCode::Down)); // clamped
    assert!(matches!(
        app.im_dialog,
        Some(ImDialog::Accounts { selected: 2, .. })
    ));
    app.im_dialog_key(key(KeyCode::Up));
    app.im_dialog_key(key(KeyCode::Char('z'))); // other keys keep it open
    assert!(matches!(
        app.im_dialog,
        Some(ImDialog::Accounts { selected: 1, .. })
    ));
    app.im_dialog_key(key(KeyCode::Esc));
    assert!(app.im_dialog.is_none());
    // No dialog: a key does nothing.
    app.im_dialog_key(key(KeyCode::Esc));
}

#[cfg(feature = "im")]
#[test]
fn joining_leaving_and_messaging_need_a_connection() {
    let mut app = app();
    app.status.clear();
    app.im_join("  ");
    assert_eq!(app.status, "");
    app.im_join("vix");
    assert_eq!(app.status, t!("status.im_not_connected").to_string());
    // With a selected network room the account is known, but nothing is linked.
    room(&mut app);
    app.status.clear();
    app.im_join("#other");
    assert_eq!(app.status, t!("status.im_not_connected").to_string());
    app.status.clear();
    app.run_action("im.leave");
    assert_eq!(app.status, t!("status.im_not_connected").to_string());
    // Leaving from Notes, or from a nick conversation, is not a room.
    app.im.panel.select(&mut app.im.inbox, &notes());
    app.status.clear();
    app.run_action("im.leave");
    assert_eq!(app.status, t!("status.im_not_a_room").to_string());
    // An explicit room name goes through the active account (none here).
    app.status.clear();
    app.im_leave(Some("#x".into()), None);
    assert_eq!(app.status, t!("status.im_not_a_room").to_string());
}

#[test]
fn direct_conversations_open_through_the_active_account() {
    let mut app = app();
    assert!(!app.im_direct("  "));
    assert!(!app.im_direct("bob"));
    assert_eq!(app.status, t!("status.im_not_connected").to_string());
    room(&mut app);
    assert!(app.im_direct("bob"));
    let id = irc_id("acct", Some("bob"));
    assert_eq!(app.im.selected_id(), Some(id.clone()));
    assert!(app.im.open);
    assert_eq!(
        app.im.inbox.get(&id).unwrap().kind,
        ConversationKind::Direct
    );
    assert_eq!(
        app.im.routes.get(&id).unwrap().target.as_deref(),
        Some("bob")
    );
}

#[test]
fn a_matrix_account_cannot_open_direct_rooms_yet() {
    let mut app = app();
    let mut a = crate::settings::ImAccount::default();
    a.name = "acct".into();
    a.protocol = "matrix".into();
    app.settings.im.accounts.push(a);
    assert!(app.im_is_matrix("acct"));
    assert!(!app.im_is_matrix("other"));
    room(&mut app);
    assert!(!app.im_direct("bob"));
    assert_eq!(app.status, t!("status.im_matrix_unsupported").to_string());
}

#[test]
fn typed_lines_in_notes_are_stored_and_in_a_room_need_a_link() {
    let mut app = app();
    app.run_action("im.panel");
    type_line(&mut app, "a note to self");
    assert_eq!(messages(&app, &notes()), ["a note to self"]);

    let r = room(&mut app);
    type_line(&mut app, "hello room");
    assert_eq!(app.status, t!("status.im_not_connected").to_string());
    assert!(
        messages(&app, &r).is_empty(),
        "not stored when it could not be sent"
    );
    // An action line the same.
    app.status.clear();
    type_line(&mut app, "/me waves");
    assert_eq!(app.status, t!("status.im_not_connected").to_string());

    // The server conversation has no target to talk to.
    server(&mut app);
    app.status.clear();
    type_line(&mut app, "anything");
    assert_eq!(app.status, t!("status.im_not_a_room").to_string());
    app.status.clear();
    app.im_send_text("x", true);
    assert_eq!(app.status, t!("status.im_not_a_room").to_string());
}

#[test]
fn slash_commands_dispatch() {
    let mut app = app();
    app.run_action("im.panel");
    type_line(&mut app, "/status away");
    assert_eq!(app.im.presence, Presence::Away);
    type_line(&mut app, "/status bogus");
    assert_eq!(
        app.status,
        t!("status.im_invalid_status", value = "bogus").to_string()
    );
    app.im.panel.clear_input();
    type_line(&mut app, "/bogus");
    assert_eq!(
        app.status,
        t!("status.im_unknown_command", name = "bogus").to_string()
    );
    app.im.panel.clear_input();
    type_line(&mut app, "/join");
    assert_eq!(
        app.status,
        t!("status.im_missing_argument", command = "join").to_string()
    );
    app.im.panel.clear_input();

    // /nick without a connection only renames locally.
    type_line(&mut app, "/nick zed");
    assert_eq!(app.im.inbox.nick(), "zed");
    // /clear empties the selected conversation.
    type_line(&mut app, "a line");
    assert_eq!(messages(&app, &notes()).len(), 1);
    type_line(&mut app, "/clear");
    assert_eq!(messages(&app, &notes()).len(), 0);
    // /follow needs a Cowork session this build lacks.
    type_line(&mut app, "/follow bob");
    assert_eq!(
        app.status,
        t!("msg.capability_missing", name = "cowork").to_string()
    );

    // Network commands with no connection.
    room(&mut app);
    type_line(&mut app, "/join #x");
    assert_eq!(app.status, t!("status.im_not_connected").to_string());
    type_line(&mut app, "/part");
    assert_eq!(app.status, t!("status.im_not_connected").to_string());
    type_line(&mut app, "/topic New topic");
    assert_eq!(app.status, t!("status.im_not_connected").to_string());
    type_line(&mut app, "/nick other");
    assert_eq!(app.status, t!("status.im_not_connected").to_string());
    type_line(&mut app, "/msg bob hi");
    assert_eq!(app.status, t!("status.im_not_connected").to_string());
    // /topic outside a channel.
    app.im.panel.select(&mut app.im.inbox, &notes());
    app.status.clear();
    type_line(&mut app, "/topic x");
    assert_eq!(app.status, t!("status.im_not_a_room").to_string());
}

#[cfg(not(feature = "cowork"))]
#[test]
fn the_cowork_session_conversation_routes_to_cowork() {
    let mut app = app();
    session(&mut app);
    app.run_action("im.panel");
    type_line(&mut app, "hi team");
    assert_eq!(
        app.status,
        t!("msg.capability_missing", name = "cowork").to_string()
    );
    app.status.clear();
    type_line(&mut app, "/me nods");
    assert_eq!(
        app.status,
        t!("msg.capability_missing", name = "cowork").to_string()
    );
}

// ----- sharing -----------------------------------------------------------------------------------------------------

fn app_with_file(tag: &str, body: &str) -> App {
    let d = temp(tag);
    std::fs::write(d.join("a.rs"), body).unwrap();
    let mut app = App::new(d.clone(), Settings::default());
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app.open_initial(&d.join("a.rs"));
    app
}

#[test]
fn sharing_needs_a_selection_and_a_file() {
    let mut app = app_with_file("share-none", "fn a() {}\n");
    app.run_action("im.share_selection");
    assert_eq!(app.status, t!("status.im_no_selection").to_string());
    let mut scratch = App::new(temp("share-scratch"), Settings::default());
    scratch.editor.new_tab_with_content("x\n");
    scratch.run_action("im.share_location");
    assert_eq!(scratch.status, t!("status.im_no_file").to_string());
}

#[test]
fn a_selection_that_is_too_long_is_refused() {
    let body = "line\n".repeat(MAX_SNIPPET_LINES + 5);
    let mut app = app_with_file("share-long", &body);
    let len = body.chars().count();
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_selection_range(0, len);
    app.run_action("im.share_selection");
    assert_eq!(
        app.status,
        t!("status.im_snippet_too_long", max = MAX_SNIPPET_LINES).to_string()
    );
    assert!(app.im_dialog.is_none());
}

#[test]
fn sharing_a_selection_to_notes_confirms_then_stores_it() {
    let mut app = app_with_file("share-notes", "fn a() {}\nfn b() {}\n");
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_selection_range(0, 9);
    app.run_action("im.share_selection");
    let Some(ImDialog::Share { title, message, .. }) = app.im_dialog.clone() else {
        panic!("expected the share confirmation");
    };
    assert_eq!(title, t!("ui.im_notes").to_string());
    assert!(matches!(message.kind, MessageKind::Snippet { .. }));
    app.im_dialog_key(key(KeyCode::Enter));
    assert_eq!(
        app.status,
        t!("status.im_sent", name = t!("ui.im_notes")).to_string()
    );
    assert_eq!(messages(&app, &notes()).len(), 1);
}

#[test]
fn sharing_into_a_room_needs_a_link_and_into_a_server_a_room() {
    let mut app = app_with_file("share-room", "fn a() {}\n");
    let r = room(&mut app);
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_selection_range(0, 9);
    app.run_action("im.share_selection");
    app.im_dialog_key(key(KeyCode::Char('y')));
    assert_eq!(app.status, t!("status.im_not_connected").to_string());
    assert_eq!(messages(&app, &r).len(), 0);

    app.run_action("im.share_location");
    assert_eq!(app.status, t!("status.im_not_connected").to_string());

    server(&mut app);
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_selection_range(0, 9);
    app.run_action("im.share_selection");
    app.im_dialog_key(key(KeyCode::Enter));
    assert_eq!(app.status, t!("status.im_not_a_room").to_string());
    app.status.clear();
    app.run_action("im.share_location");
    assert_eq!(app.status, t!("status.im_not_a_room").to_string());
}

#[cfg(not(feature = "cowork"))]
#[test]
fn sharing_to_the_cowork_session_goes_to_cowork() {
    let mut app = app_with_file("share-session", "fn a() {}\n");
    session(&mut app);
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_selection_range(0, 9);
    app.run_action("im.share_selection");
    app.im_dialog_key(key(KeyCode::Enter));
    assert_eq!(
        app.status,
        t!("msg.capability_missing", name = "cowork").to_string()
    );
    app.status.clear();
    app.run_action("im.share_location");
    assert_eq!(
        app.status,
        t!("msg.capability_missing", name = "cowork").to_string()
    );
}

#[test]
fn share_location_to_notes_stores_the_position() {
    let mut app = app_with_file("share-loc", "fn a() {}\n");
    app.run_action("im.share_location");
    assert_eq!(messages(&app, &notes()), ["a.rs:1:1"]);
    assert_eq!(
        app.status,
        t!("status.im_sent", name = t!("ui.im_notes")).to_string()
    );
}

#[test]
fn share_paths_prefer_workspace_relative_and_shared_names() {
    let app = app_with_file("share-path", "x\n");
    assert_eq!(app.im_share_path(&app.root.join("sub/b.rs")), "sub/b.rs");
    assert_eq!(
        app.im_share_path(Path::new("/elsewhere/c.rs")),
        "/elsewhere/c.rs"
    );
    let shared = PathBuf::from(format!("{}src/a.rs", crate::editor::REMOTE_PATH_PREFIX));
    assert_eq!(app.im_share_path(&shared), "src/a.rs");
}

// ----- opening references, keys, mouse -----------------------------------------------------------------------------

#[test]
fn a_reference_outside_the_workspace_is_refused_and_inside_opens() {
    let mut app = app_with_file("ref", "one\ntwo\nthree\n");
    app.im_open_ref(&CodeRef {
        path: "/etc/passwd".into(),
        line: 1,
        column: None,
        end_line: None,
    });
    assert!(app.status.contains("/etc/passwd"), "{}", app.status);
    app.focus = Focus::Im;
    app.im_open_ref(&CodeRef {
        path: "a.rs".into(),
        line: 2,
        column: Some(2),
        end_line: None,
    });
    assert_eq!(app.focus, Focus::Editor);
    assert_eq!(app.editor.active_tab().unwrap().editor.cursor_line(), 1);
}

#[test]
fn the_panel_ignores_chords_and_a_closed_panel_returns_focus() {
    let mut app = app();
    app.focus = Focus::Im;
    assert!(!app.im_key(key(KeyCode::Char('x'))), "closed panel");
    assert_eq!(app.focus, Focus::Editor);
    app.run_action("im.panel");
    assert!(!app.im_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL)));
    assert!(!app.im_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::ALT)));
    assert!(!app.im_key(key(KeyCode::F(5))));
    assert!(app.im_key(key(KeyCode::Char('p'))));
    app.im_paste("pasted text");
    assert_eq!(app.im.panel.input(), "ppasted text");
    assert!(app.im_key(key(KeyCode::Esc)));
    assert_eq!(app.focus, Focus::Editor);
}

#[test]
fn clicks_select_a_conversation_and_the_wheel_scrolls() {
    let mut app = app();
    let r = room(&mut app);
    app.im.panel.select(&mut app.im.inbox, &notes());
    app.im.open = true;
    app.layout.im_list = Rect::new(0, 5, 20, 4);
    let at = |kind, col, row| MouseEvent {
        kind,
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    };
    // Outside the list: focuses only.
    app.focus = Focus::Editor;
    app.im_mouse(at(MouseEventKind::Down(MouseButton::Left), 40, 0));
    assert_eq!(app.focus, Focus::Im);
    assert_eq!(app.im.selected_id(), Some(notes()));
    // Second row of the list is the room.
    app.im_mouse(at(MouseEventKind::Down(MouseButton::Left), 1, 6));
    assert_eq!(app.im.selected_id(), Some(r));
    // Beyond the conversations: nothing changes.
    let sel = app.im.selected_id();
    app.im_mouse(at(MouseEventKind::Down(MouseButton::Left), 1, 8));
    assert_eq!(app.im.selected_id(), sel);
    app.im_mouse(at(MouseEventKind::ScrollUp, 1, 6));
    app.im_mouse(at(MouseEventKind::ScrollDown, 1, 6));
    app.im_mouse(at(MouseEventKind::Moved, 1, 6));
}

#[test]
fn poll_im_without_links_is_quiet() {
    let mut app = app();
    app.poll_im();
    assert_eq!(app.im_status_segment(), "");
    assert!(app.run_im_action("im.panel"));
    assert!(!app.run_im_action("im.not_an_action"));
}
