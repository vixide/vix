//! T692/T693/T695: two real `App`s in one process collaborate over an
//! encrypted loopback socket (the `cowork` feature). No external network.
#![allow(
    clippy::wildcard_imports,
    reason = "every test needs a handful of the shared helpers in common.rs"
)]

use crate::integration::common::*;
use vix::app::PromptKind;

/// Run both apps' Cowork pumps until `pred` holds, failing after 10 s.
fn pump(host: &mut App, guest: &mut App, pred: impl Fn(&App, &App) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        host.poll_cowork();
        guest.poll_cowork();
        if pred(host, guest) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "cowork pump timed out\nhost status: {:?}\nguest status: {:?}\nguest messages: {:?}\nhost messages: {:?}",
            host.status,
            guest.status,
            guest
                .messages
                .items
                .iter()
                .map(|m| m.text.clone())
                .collect::<Vec<_>>(),
            host.messages
                .items
                .iter()
                .map(|m| m.text.clone())
                .collect::<Vec<_>>()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn enter(app: &mut App) {
    app.on_key(keycode(KeyCode::Enter));
}

fn tab_text(app: &App) -> String {
    app.editor.active_tab().unwrap().text()
}

/// Host a session on `dir`'s `a.txt`, join a second app, and approve it at
/// `level`. Returns both apps with `a.txt` shared and open on the guest.
fn connected(tag: &str, level: &str) -> (App, App, PathBuf) {
    let hdir = unique_dir(&format!("cw-host-{tag}"));
    let gdir = unique_dir(&format!("cw-guest-{tag}"));
    let file = hdir.join("a.txt");
    fs::write(&file, "hello\n").unwrap();
    let mut host = app_at(&hdir);
    let mut guest = app_at(&gdir);
    host.editor.open(&file, false).unwrap();

    host.run_action("cowork.host");
    assert!(host.prompt.as_ref().unwrap().kind == PromptKind::CoworkHost);
    enter(&mut host);
    let invite = host.dialog.take().expect("an invite dialog").body;
    assert!(invite.starts_with("vix-cowork:1:"), "{invite}");

    guest.run_action("cowork.join");
    guest.prompt.as_mut().unwrap().input = invite;
    enter(&mut guest);

    // The host is asked for consent; nothing is shared before it answers.
    pump(&mut host, &mut guest, |h, _| {
        h.prompt
            .as_ref()
            .is_some_and(|p| p.kind == PromptKind::CoworkConsent)
    });
    assert!(guest.editor.tabs.iter().all(|t| t.path.is_none()));
    host.prompt.as_mut().unwrap().input = level.to_string();
    enter(&mut host);
    host.run_action("cowork.share");
    pump(&mut host, &mut guest, |_, g| {
        g.editor.tabs.iter().any(vix::editor::Tab::is_remote)
    });
    (host, guest, file)
}

#[test]
fn an_edit_on_either_side_reaches_the_other_and_cursors_show() {
    let (mut host, mut guest, file) = connected("edit", "edit");
    assert_eq!(tab_text(&guest), "hello\n");
    assert!(
        guest
            .editor
            .active_tab()
            .unwrap()
            .path
            .as_ref()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("cowork:"),
        "a shared-in buffer has a synthetic path, never a local file"
    );

    guest.on_key(key('X'));
    pump(&mut host, &mut guest, |h, _| tab_text(h).contains('X'));
    host.on_key(key('Y'));
    pump(&mut host, &mut guest, |h, g| {
        tab_text(h).contains('Y') && tab_text(h) == tab_text(g)
    });
    assert_eq!(tab_text(&host).len(), "hello\n".len() + 2);

    // Presence: each side sees the other's caret.
    pump(&mut host, &mut guest, |h, g| {
        !h.editor
            .active_tab()
            .unwrap()
            .editor
            .remote_carets()
            .is_empty()
            && !g
                .editor
                .tabs
                .iter()
                .find(|t| t.is_remote())
                .unwrap()
                .editor
                .remote_carets()
                .is_empty()
    });

    // The guest's Save asks the host; the guest's disk is untouched.
    guest.run_action("file.save");
    pump(&mut host, &mut guest, |_, _| {
        fs::read_to_string(&file).is_ok_and(|s| s.contains('X') && s.contains('Y'))
    });
}

#[test]
fn a_view_only_guest_cannot_edit() {
    let (mut host, mut guest, file) = connected("view", "view");
    assert!(
        guest
            .editor
            .tabs
            .iter()
            .any(|t| t.is_remote() && t.read_only)
    );
    let before = tab_text(&host);
    guest.on_key(key('Z'));
    for _ in 0..20 {
        host.poll_cowork();
        guest.poll_cowork();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        tab_text(&host),
        before,
        "the host never saw a view-only edit"
    );
    // Granting edit rights lifts the lock on the guest's side.
    host.run_action("cowork.permissions");
    host.prompt.as_mut().unwrap().input = "2 edit".to_string();
    enter(&mut host);
    pump(&mut host, &mut guest, |_, g| {
        g.editor.tabs.iter().any(|t| t.is_remote() && !t.read_only)
    });
    assert_eq!(fs::read_to_string(&file).unwrap(), "hello\n");
}

#[test]
fn declining_keeps_the_guest_out() {
    let hdir = unique_dir("cw-host-decline");
    let gdir = unique_dir("cw-guest-decline");
    fs::write(hdir.join("a.txt"), "secret\n").unwrap();
    let mut host = app_at(&hdir);
    let mut guest = app_at(&gdir);
    host.editor.open(&hdir.join("a.txt"), false).unwrap();
    host.run_action("cowork.host");
    enter(&mut host);
    let invite = host.dialog.take().unwrap().body;
    host.run_action("cowork.share");
    guest.run_action("cowork.join");
    guest.prompt.as_mut().unwrap().input = invite;
    enter(&mut guest);
    pump(&mut host, &mut guest, |h, _| {
        h.prompt
            .as_ref()
            .is_some_and(|p| p.kind == PromptKind::CoworkConsent)
    });
    host.prompt.as_mut().unwrap().input = "no".to_string();
    enter(&mut host);
    // The guest hears a refusal and never receives the file.
    pump(&mut host, &mut guest, |_, g| !g.cowork_active());
    assert!(
        guest
            .editor
            .tabs
            .iter()
            .all(|t| !t.text().contains("secret"))
    );
    assert!(!guest.cowork_active());
}

#[test]
fn leaving_turns_shared_buffers_into_local_copies() {
    let (mut host, mut guest, _file) = connected("leave", "edit");
    guest.run_action("cowork.leave");
    let tab = guest
        .editor
        .tabs
        .iter()
        .find(|t| t.text() == "hello\n")
        .unwrap();
    assert!(tab.path.is_none() && !tab.is_remote() && !tab.read_only);
    // The host notices and keeps working.
    for _ in 0..40 {
        host.poll_cowork();
        std::thread::sleep(Duration::from_millis(5));
    }
    host.run_action("cowork.leave");
    assert!(!host.cowork_active());
}

#[test]
fn the_cowork_menu_is_available_with_the_feature() {
    assert!(vix::capability::is_available("cowork.host"));
}

/// T692: Undo in a shared buffer reverses only your own text, on both sides.
#[test]
fn undo_reverts_only_your_own_edit() {
    let (mut host, mut guest, _file) = connected("undo", "edit");
    guest.on_key(key('X'));
    pump(&mut host, &mut guest, |h, _| tab_text(h).contains('X'));
    host.on_key(key('Y'));
    pump(&mut host, &mut guest, |h, g| {
        tab_text(g).contains('Y') && tab_text(h) == tab_text(g)
    });
    guest.run_action("edit.undo");
    pump(&mut host, &mut guest, |h, g| {
        !tab_text(h).contains('X') && tab_text(h) == tab_text(g)
    });
    assert!(tab_text(&host).contains('Y'), "the host's text survives");
    host.run_action("edit.undo");
    pump(&mut host, &mut guest, |h, g| {
        tab_text(h) == "hello\n" && tab_text(g) == "hello\n"
    });
    guest.run_action("edit.redo");
    pump(&mut host, &mut guest, |h, g| {
        tab_text(h).contains('X') && !tab_text(h).contains('Y') && tab_text(h) == tab_text(g)
    });
}

/// T693: following a participant moves your caret with theirs; any key of
/// your own stops it.
#[test]
fn follow_tracks_the_host_until_a_local_key() {
    let (mut host, mut guest, _file) = connected("follow", "view");
    guest.run_action("cowork.follow");
    let prompt = guest.prompt.as_ref().expect("a follow prompt");
    assert!(prompt.kind == PromptKind::CoworkFollow);
    assert_eq!(prompt.input, "1", "the host is offered first");
    enter(&mut guest);
    host.on_key(keycode(KeyCode::Right));
    host.on_key(keycode(KeyCode::Right));
    host.on_key(keycode(KeyCode::Right));
    pump(&mut host, &mut guest, |_, g| {
        g.editor.active_tab().unwrap().editor.get_cursor() == 3
    });
    guest.on_key(keycode(KeyCode::Left));
    host.on_key(keycode(KeyCode::Right));
    for _ in 0..30 {
        host.poll_cowork();
        guest.poll_cowork();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        guest.editor.active_tab().unwrap().editor.get_cursor(),
        2,
        "no longer following after a local key"
    );
}

/// T696: the file tree is closed until the host shares it; then the guest
/// lists it (private files left out), picks a file and it opens shared.
#[test]
fn a_guest_browses_and_opens_files_only_once_the_tree_is_shared() {
    let (mut host, mut guest, file) = connected("tree", "view");
    let root = file.parent().unwrap().to_path_buf();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/b.txt"), "bee\n").unwrap();
    fs::write(root.join("server.pem"), "-----BEGIN-----\n").unwrap();

    // Closed: the host refuses, and the guest's palette never opens.
    guest.run_action("cowork.files");
    for _ in 0..30 {
        host.poll_cowork();
        guest.poll_cowork();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(guest.palette.is_none());
    assert!(guest.cowork_active(), "a refused request is not fatal");

    host.run_action("cowork.share_workspace");
    guest.run_action("cowork.files");
    pump(&mut host, &mut guest, |_, g| g.palette.is_some());
    let labels: Vec<String> = guest
        .palette
        .as_ref()
        .unwrap()
        .entries
        .iter()
        .map(|e| e.label.clone())
        .collect();
    assert!(labels.iter().any(|l| l == "src/b.txt"), "{labels:?}");
    assert!(!labels.iter().any(|l| l.contains("pem")), "{labels:?}");
    let host_tabs = host.editor.tabs.len();

    type_str(&mut guest, "b.txt");
    enter(&mut guest);
    pump(&mut host, &mut guest, |_, g| {
        g.editor.tabs.iter().any(|t| {
            t.text() == "bee\n"
                && t.path.as_deref() == Some(Path::new("cowork:src/b.txt"))
                && t.read_only
        })
    });
    assert_eq!(
        host.editor.tabs.len(),
        host_tabs + 1,
        "the host sees what it shares"
    );
    assert!(
        !host.editor.active_tab().unwrap().text().contains("bee"),
        "the host's focus did not move"
    );
}

/// Run both pumps (and the host's terminal poll) for a while, for checks
/// that something does *not* happen.
#[cfg(unix)]
fn settle(host: &mut App, guest: &mut App) {
    for _ in 0..60 {
        host.tick();
        guest.poll_cowork();
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Tick both apps until `pred` holds, failing after 10 s with both screens.
#[cfg(unix)]
fn until_screens(host: &mut App, guest: &mut App, pred: impl Fn(&str, &str) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let h = host.terminal_screen_text().unwrap_or_default();
        let g = guest.cowork_terminal_text().unwrap_or_default();
        if pred(&h, &g) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "terminal wait timed out\nhost:\n{h}\nguest:\n{g}"
        );
        host.tick();
        guest.poll_cowork();
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// T697: the host's terminal reaches a guest only after the host confirms
/// sharing it; the guest sees it read-only until the host also confirms
/// input; then keystrokes from a guest with terminal permission run in the
/// host's shell. Closing the terminal ends the share. A real shell on a real
/// PTY, over the encrypted loopback link.
#[cfg(unix)]
#[test]
fn a_shared_terminal_is_read_only_until_the_host_allows_input() {
    use vix::app::ConfirmKind;
    let (mut host, mut guest, _file) = connected("term", "terminal");

    // Sharing needs an open terminal, and then the host's confirmation.
    host.run_action("cowork.share_terminal");
    assert!(
        host.confirm.is_none(),
        "no terminal open: nothing to confirm"
    );
    host.run_action("tools.terminal");
    assert!(host.terminal_running());
    host.run_action("cowork.share_terminal");
    assert_eq!(
        host.confirm.as_ref().map(|c| c.kind),
        Some(ConfirmKind::CoworkShareTerminal)
    );
    host.on_key(key('n'));
    assert!(host.confirm.is_none());
    settle(&mut host, &mut guest);
    assert_eq!(guest.cowork_terminal_text(), None, "declined: nothing sent");

    host.run_action("cowork.share_terminal");
    host.on_key(key('y'));
    pump(&mut host, &mut guest, |_, g| {
        g.cowork_terminal_text().is_some()
    });
    assert!(
        !guest.cowork_terminal_can_type(),
        "a new share is read-only"
    );

    // The host's output streams to the guest.
    type_str(&mut host, "expr 123400 + 56");
    host.on_key(keycode(KeyCode::Enter));
    until_screens(&mut host, &mut guest, |_, g| g.contains("123456"));

    // Read-only: the guest's typing never reaches the host's shell.
    guest.run_action("cowork.terminal");
    type_str(&mut guest, "expr 500000 + 5");
    guest.on_key(keycode(KeyCode::Enter));
    settle(&mut host, &mut guest);
    let screen = host.terminal_screen_text().unwrap_or_default();
    assert!(!screen.contains("500000"), "{screen}");

    // The host allows input (confirmed); now the guest's command runs.
    host.run_action("cowork.terminal_input");
    assert_eq!(
        host.confirm.as_ref().map(|c| c.kind),
        Some(ConfirmKind::CoworkTerminalInput)
    );
    host.on_key(key('y'));
    pump(&mut host, &mut guest, |_, g| g.cowork_terminal_can_type());
    type_str(&mut guest, "expr 777000 + 777");
    guest.on_key(keycode(KeyCode::Enter));
    until_screens(&mut host, &mut guest, |h, g| {
        h.contains("777777") && g.contains("777777")
    });

    // Ctrl+] only hides the guest's view; the share goes on.
    guest.on_key(ctrl(']'));
    assert!(guest.cowork_terminal_text().is_some());

    // Closing the host's terminal ends the share for everyone.
    host.run_action("tools.terminal");
    assert!(!host.terminal_running());
    pump(&mut host, &mut guest, |_, g| {
        g.cowork_terminal_text().is_none()
    });
    assert!(!guest.cowork_terminal_can_type());
}
