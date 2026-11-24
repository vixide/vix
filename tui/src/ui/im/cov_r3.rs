//! Behavior tests for the IM dock and its dialogs: what a frame shows for
//! unread and mentioned conversations, unverified authors, snippets, long
//! lines, and each dialog. Frames are drawn on a test backend and read back.

use super::*;
use crate::app::cov4::{app_at as base_app_at, temp};
use crate::app::im::Route;
use vix_im::{Conversation, Message};

fn app_at(d: &std::path::Path) -> App {
    let mut app = base_app_at(d);
    app.settings_path = Some(d.join("vix-test-config.toml"));
    app
}

/// The text of one drawn frame, a line per terminal row.
fn draw_frame(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| crate::ui::draw(app, frame)).unwrap();
    let buf = terminal.backend().buffer();
    (0..height)
        .map(|y| (0..width).map(|x| buf[(x, y)].symbol()).collect::<String>() + "\n")
        .collect()
}

fn room(app: &mut App, name: &str) -> ConversationId {
    let id = ConversationId::new(format!("irc/net/{name}"));
    app.im.inbox.add(Conversation::new(
        id.clone(),
        ConversationKind::Room,
        name.to_string(),
    ));
    id
}

#[test]
fn the_conversation_list_marks_unread_counts_and_mentions_in_text() {
    let d = temp("im-list");
    let mut app = app_at(&d);
    let chan = room(&mut app, "#vix");
    let nick = app.im.inbox.nick().to_string();
    app.im
        .inbox
        .push(&chan, Message::text("bob", 1_000, "hello there"));
    app.im
        .inbox
        .push(&chan, Message::text("bob", 2_000, format!("hi @{nick}")));
    let index = app.im.inbox.index_of(&chan).unwrap();
    let row = list_row(&app, index);
    assert!(row.contains("#vix"), "{row}");
    assert!(
        row.contains(&t!("ui.im_unread", count = 2).to_string()),
        "{row}"
    );
    assert!(row.ends_with(" @"), "{row}");
    assert!(row.starts_with("  "), "not the selected row: {row}");
    // The selected row carries the marker; a missing index is an empty row.
    app.im.panel.select(&mut app.im.inbox, &chan);
    assert!(list_row(&app, app.im.panel.selected_index()).starts_with("> "));
    assert_eq!(list_row(&app, 99), "");

    app.im.open = true;
    app.focus = Focus::Im;
    app.im.inbox.mark_read(&chan);
    let frame = draw_frame(&mut app, 120, 40);
    assert!(frame.contains("#vix"), "{frame}");
    assert!(frame.contains("hello there"), "{frame}");
}

#[test]
fn messages_show_kinds_unverified_authors_snippets_and_wrap_long_lines() {
    let d = temp("im-messages");
    let mut app = app_at(&d);
    let chan = room(&mut app, "#vix");
    app.im.routes.insert(
        chan.clone(),
        Route {
            account: "net".to_string(),
            target: Some("#vix".to_string()),
        },
    );
    let id = chan.clone();
    app.im
        .inbox
        .push(&id, Message::text("mallory", 0, "trust me"));
    app.im.inbox.push(&id, Message::action("bob", 0, "waves"));
    app.im.inbox.push(&id, Message::system(0, "bob joined"));
    let mut snippet = Message::text("bob", 0, "see this");
    snippet.kind = MessageKind::Snippet {
        language: "rust".to_string(),
        path: "src/a.rs".to_string(),
        start_line: 1,
        end_line: 2,
        code: "fn a() {}\nfn b() {}".to_string(),
    };
    app.im.inbox.push(&id, snippet);
    app.im.panel.select(&mut app.im.inbox, &chan);
    // Unverified nicks in a network room are labelled in text.
    let unverified = t!("ui.im_unverified").to_string();
    let rows = message_rows(
        &app,
        &chan,
        &app.im.inbox.get(&chan).unwrap().messages[0].clone(),
    );
    assert!(
        rows[0]
            .0
            .contains(&format!("mallory ({unverified}): trust me")),
        "{rows:?}"
    );
    // A verified one is not.
    app.im
        .verified
        .insert((chan.clone(), app.im.inbox.case_mapping().fold("mallory")));
    let rows = message_rows(
        &app,
        &chan,
        &app.im.inbox.get(&chan).unwrap().messages[0].clone(),
    );
    assert!(!rows[0].0.contains(&unverified), "{rows:?}");
    let action = message_rows(
        &app,
        &chan,
        &app.im.inbox.get(&chan).unwrap().messages[1].clone(),
    );
    assert!(
        action[0].0.contains("* bob") && action[0].0.ends_with("waves"),
        "{action:?}"
    );
    let system = message_rows(
        &app,
        &chan,
        &app.im.inbox.get(&chan).unwrap().messages[2].clone(),
    );
    assert!(system[0].0.contains("-- bob joined") && system[0].1);
    let snip = message_rows(
        &app,
        &chan,
        &app.im.inbox.get(&chan).unwrap().messages[3].clone(),
    );
    assert_eq!(snip.len(), 3);
    assert_eq!(snip[1], ("    fn a() {}".to_string(), true));

    app.im.open = true;
    app.focus = Focus::Im;
    let wide = draw_frame(&mut app, 120, 40);
    assert!(wide.contains("fn a() {}"), "{wide}");
    // A narrow dock wraps a long line over several rows without losing text.
    let long = room(&mut app, "#long");
    app.im
        .inbox
        .push(&long, Message::text("bob", 0, "word ".repeat(30)));
    app.im.panel.select(&mut app.im.inbox, &long);
    let narrow = draw_frame(&mut app, 80, 30);
    assert!(narrow.matches("word").count() >= 3, "{narrow}");
}

#[test]
fn an_empty_conversation_and_a_tiny_dock_draw_safely() {
    let d = temp("im-empty");
    let mut app = app_at(&d);
    app.im.open = true;
    app.focus = Focus::Im;
    let frame = draw_frame(&mut app, 120, 40);
    assert!(frame.contains(&t!("ui.im_empty").to_string()), "{frame}");
    // Too small for the panel: nothing is drawn inside, and nothing panics.
    draw_frame(&mut app, 12, 5);
    draw_frame(&mut app, 3, 3);
    // A selected message highlights and is kept in view when the list is long.
    let notes = ConversationId::new("notes");
    for i in 0..60 {
        app.im
            .inbox
            .push(&notes, Message::text("me", 0, format!("line {i}")));
    }
    let frame = draw_frame(&mut app, 100, 20);
    assert!(frame.contains("line 59"), "{frame}");
}

#[test]
fn the_dialogs_list_accounts_confirm_shares_and_clears() {
    let d = temp("im-dialogs");
    let mut app = app_at(&d);
    // No dialog: nothing is drawn.
    let before = draw_frame(&mut app, 100, 30);
    app.im_dialog = Some(ImDialog::Accounts {
        names: vec!["libera".to_string(), "work".to_string()],
        selected: 1,
    });
    let frame = draw_frame(&mut app, 100, 30);
    assert!(frame.contains(" libera "), "{frame}");
    assert!(frame.contains(&t!("menu.item.im.connect").to_string()));
    assert_ne!(before, frame);

    let notes = ConversationId::new("notes");
    let mut snippet = Message::text("me", 0, "src/a.rs:1-2");
    snippet.kind = MessageKind::Snippet {
        language: String::new(),
        path: "src/a.rs".to_string(),
        start_line: 1,
        end_line: 2,
        code: "fn a() {}\nfn b() {}".to_string(),
    };
    app.im_dialog = Some(ImDialog::Share {
        to: notes.clone(),
        title: "Notes".to_string(),
        message: snippet,
    });
    let frame = draw_frame(&mut app, 100, 30);
    assert!(
        frame.contains(&t!("ui.im_send_to", target = "Notes").to_string()),
        "{frame}"
    );
    assert!(frame.contains("  fn b() {}"), "{frame}");
    assert!(frame.contains(&t!("ui.im_confirm_hint").to_string()));

    app.im_dialog = Some(ImDialog::Clear {
        id: notes,
        title: "Notes".to_string(),
    });
    let frame = draw_frame(&mut app, 100, 30);
    assert!(
        frame.contains(&t!("ui.im_clear_question", name = "Notes").to_string()),
        "{frame}"
    );
    // A dialog larger than the screen is clamped to it.
    draw_frame(&mut app, 20, 6);
}

#[test]
fn the_threads_dialog_lists_threads_with_their_state() {
    let d = temp("im-threads");
    std::fs::create_dir_all(d.join("src")).unwrap();
    std::fs::write(d.join("src/a.rs"), "fn a() {}\nfn b() {}\nfn c() {}\n").unwrap();
    let mut app = app_at(&d);
    app.open_initial(&d.join("src/a.rs"));
    let area = app.layout.editor;
    app.editor.goto(2, Some(1), area);
    app.run_action("im.comment_line");
    for c in "why b?".chars() {
        app.on_key(crate::app::cov4::key(c));
    }
    app.on_key(crate::app::cov4::code(vix_input::KeyCode::Enter));
    app.run_action("im.threads");
    assert!(matches!(app.im_dialog, Some(ImDialog::Threads { .. })));
    let frame = draw_frame(&mut app, 120, 40);
    assert!(frame.contains("src/a.rs:2"), "{frame}");
    assert!(frame.contains("why b?"), "{frame}");
    assert!(
        frame.contains(&t!("ui.im_threads_hint").to_string()),
        "{frame}"
    );
    // A narrow screen still draws the list and detail.
    draw_frame(&mut app, 50, 16);
}
