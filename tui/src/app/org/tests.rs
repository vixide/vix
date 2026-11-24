//! Behavior tests for the Org actions, prompts, capture, agenda and links,
//! driving a real `App` over temp-dir `.org` files.

use super::*;
use crate::settings::Settings;
use ratatui::layout::Rect;
use vix_input::{KeyModifiers, MouseButton, MouseEventKind};

fn temp(tag: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("vix-ot-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn app_at(dir: &Path) -> App {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let session = std::env::temp_dir().join(format!("vix-ot-sess-{}-{n}.toml", std::process::id()));
    let mut app = App::new(dir.to_path_buf(), Settings::default()).with_session_path(session);
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app
}

/// An app in a fresh dir with `files` written; the first is opened.
fn app_with(tag: &str, files: &[(&str, &str)]) -> (PathBuf, App) {
    let d = temp(tag);
    for (name, body) in files {
        std::fs::write(d.join(name), body).unwrap();
    }
    let mut app = app_at(&d);
    app.build_file_index();
    if let Some((first, _)) = files.first() {
        app.open_initial(&d.join(first));
    }
    (d, app)
}

/// An app with an unsaved scratch buffer holding `text`.
fn scratch(text: &str) -> App {
    let mut app = app_at(&temp("scratch"));
    app.editor.new_tab_with_content(text);
    app
}

fn text(app: &App) -> String {
    app.editor.active_tab().unwrap().text()
}

fn line(app: &App) -> usize {
    app.editor.active_tab().unwrap().editor.cursor_line()
}

fn goto(app: &mut App, l: usize) {
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_cursor_line(l);
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn click(col: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn dirty(app: &App) -> bool {
    app.editor.active_tab().unwrap().dirty
}

/// Answer the open prompt the way `accept_prompt` does: take it, then accept.
fn answer(app: &mut App, kind: PromptKind, input: &str) {
    app.prompt = None;
    app.accept_roam_prompt(kind, input);
}

fn is_prompt(app: &App, kind: PromptKind) -> bool {
    app.prompt.as_ref().is_some_and(|p| p.kind == kind)
}

// ----- with no buffer open ----------------------------------------------------

#[test]
fn org_actions_with_no_buffer_do_nothing() {
    let mut app = app_at(&temp("nobuf"));
    app.editor.tabs.clear();
    assert!(app.editor.active_tab().is_none());
    for action in [
        "org.promote",
        "org.demote",
        "org.cycle_todo",
        "org.priority.up",
        "org.toggle_checkbox",
        "org.ctrl_c_ctrl_c",
        "org.update_statistics",
        "org.move_up",
        "org.export_markdown",
        "org.new_heading",
        "org.nav.up",
        "org.subtree.copy",
        "org.subtree.paste",
        "org.refile",
        "org.sparse.todo",
        "org.sparse.match",
        "org.footnote",
        "org.edit_src",
        "org.set_tags",
        "org.set_property",
        "org.schedule",
        "org.link.insert",
        "org.link.next",
        "org.link.follow",
        "org.clock_out",
        "org.time_report",
        "roam.backlinks",
        "node.nodeify",
        "node.extract_subtree",
        "node.rename_by_title",
    ] {
        app.status.clear();
        assert!(app.run_action_known(action), "{action}");
        assert!(app.prompt.is_none(), "{action}");
        assert!(app.editor.active_tab().is_none(), "{action}");
        assert!(app.chooser.is_none(), "{action}");
        assert!(app.status.is_empty(), "{action}: {}", app.status);
    }
    app.org_close_note("n");
    app.org_set_tags("a");
    app.org_set_property("A b");
    app.org_plan("SCHEDULED", "2030-01-01");
    app.org_edit_src_finish();
    app.org_agenda_lock();
    assert_eq!(app.status, t!("status.org_archive_unsaved").to_string());
}

impl App {
    /// Test shim: dispatch through the Org dispatcher and say if it knew `action`.
    fn run_action_known(&mut self, action: &str) -> bool {
        self.org_action(action)
    }
}

// ----- headline editing -----------------------------------------------------------

#[test]
fn promote_demote_cycle_and_priority_rewrite_the_headline() {
    let mut app = scratch("* TODO task\nplain\n");
    app.org_action("org.demote");
    assert_eq!(text(&app), "** TODO task\nplain\n");
    app.org_action("org.promote");
    assert_eq!(text(&app), "* TODO task\nplain\n");
    app.org_action("org.cycle_todo");
    assert_eq!(text(&app), "* DONE task\nplain\n");
    assert!(dirty(&app));
    app.org_action("org.priority.up");
    assert!(text(&app).starts_with("* DONE [#"), "{}", text(&app));
    app.org_action("org.priority.down");

    // Off a headline: reported, text untouched.
    goto(&mut app, 1);
    let before = text(&app);
    app.status.clear();
    app.org_action("org.demote");
    assert_eq!(app.status, t!("status.org_not_headline").to_string());
    assert_eq!(text(&app), before);
    app.status.clear();
    app.org_action("org.priority.up");
    assert_eq!(app.status, t!("status.org_not_headline").to_string());
    app.status.clear();
    app.org_action("org.priority.down");
    assert_eq!(app.status, t!("status.org_not_headline").to_string());
}

#[test]
fn checkboxes_update_their_parents_and_ctrl_c_ctrl_c_picks_the_right_job() {
    let mut app = scratch("* Task [0/2]\n- [ ] one\n- [ ] two\n");
    goto(&mut app, 1);
    app.org_action("org.toggle_checkbox");
    assert!(text(&app).contains("- [x] one"), "{}", text(&app));
    assert!(text(&app).contains("[1/2]"), "{}", text(&app));
    // C-c C-c on a checkbox line toggles it back.
    app.org_action("org.ctrl_c_ctrl_c");
    assert!(text(&app).contains("- [ ] one"));
    // Off a checkbox it only recomputes statistics.
    goto(&mut app, 0);
    let before = text(&app);
    app.org_action("org.ctrl_c_ctrl_c");
    assert_eq!(text(&app), before);
    app.org_action("org.update_statistics");
    assert_eq!(text(&app), before);
}

#[test]
fn closing_with_a_note_needs_a_headline() {
    let mut app = scratch("* TODO job\nbody\n");
    app.org_action("org.close_note");
    assert!(is_prompt(&app, PromptKind::OrgCloseNote));
    app.prompt = None;
    app.org_close_note("all done");
    assert_eq!(app.status, t!("status.org_closed").to_string());
    assert!(text(&app).contains("DONE job"));
    assert!(text(&app).contains("all done"));

    goto(&mut app, 3);
    app.status.clear();
    app.org_action("org.close_note");
    assert_eq!(app.status, t!("status.org_not_headline").to_string());
    assert!(app.prompt.is_none());
    app.status.clear();
    app.org_close_note("x");
    assert_eq!(app.status, t!("status.org_not_headline").to_string());
}

#[test]
fn subtrees_move_among_their_siblings() {
    let mut app = scratch("* A\n* B\n* C\n");
    goto(&mut app, 1);
    app.org_action("org.move_up");
    assert_eq!(text(&app), "* B\n* A\n* C\n");
    assert_eq!(line(&app), 0);
    app.org_action("org.move_down");
    assert_eq!(text(&app), "* A\n* B\n* C\n");
    assert_eq!(line(&app), 1);
    goto(&mut app, 0);
    app.status.clear();
    app.org_action("org.move_up");
    assert_eq!(app.status, t!("status.org_no_sibling").to_string());
}

#[test]
fn exports_open_a_new_tab_with_the_converted_text() {
    let mut app = scratch("* Title\nbody\n");
    let tabs = app.editor.tabs.len();
    let src = app.editor.active;
    for (action, ext) in [
        ("org.export_markdown", "md"),
        ("org.export_html", "html"),
        ("org.export_latex", "tex"),
        ("org.column_view_export", "org"),
        ("org.export_ics", "ics"),
    ] {
        app.editor.active = src;
        app.org_action(action);
        assert_eq!(
            app.status,
            t!("status.org_exported", ext = ext).to_string(),
            "{action}"
        );
    }
    assert_eq!(app.editor.tabs.len(), tabs + 5);
    app.editor.active = src;
    app.org_action("org.export_markdown");
    assert_eq!(text(&app), crate::org::to_markdown("* Title\nbody\n"));
}

#[test]
fn new_heading_navigation_and_missing_targets() {
    let mut app = scratch("* One\n** Child\n* Two\n");
    goto(&mut app, 0);
    app.org_action("org.new_heading");
    assert!(text(&app).starts_with("* One\n"));
    assert_eq!(text(&app).lines().count(), 4);

    let mut app = scratch("* One\n** Child\n* Two\n");
    goto(&mut app, 1);
    app.org_action("org.nav.up");
    assert_eq!(line(&app), 0);
    app.org_action("org.nav.next");
    assert_eq!(line(&app), 1);
    app.org_action("org.nav.forward");
    assert_eq!(line(&app), 1, "no later sibling at that level");
    assert_eq!(app.status, t!("status.org_no_heading").to_string());
    app.org_action("org.nav.previous");
    assert_eq!(line(&app), 0);
    app.status.clear();
    app.org_action("org.nav.up");
    assert_eq!(app.status, t!("status.org_no_heading").to_string());
    goto(&mut app, 2);
    app.org_action("org.nav.backward");
    assert_eq!(line(&app), 0);
}

#[test]
fn subtree_copy_cut_and_paste_roundtrip_through_the_clipboard() {
    static CLIP: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _g = CLIP
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut app = scratch("* A\nbody\n* B\n");
    app.org_action("org.subtree.copy");
    assert_eq!(app.status, t!("status.org_subtree_copied").to_string());
    goto(&mut app, 2);
    app.org_action("org.subtree.paste");
    assert!(text(&app).contains("* A\nbody\n"));
    assert_eq!(text(&app).matches("* A").count(), 2);

    goto(&mut app, 0);
    app.org_action("org.subtree.cut");
    assert_eq!(app.status, t!("status.org_subtree_cut").to_string());
    assert_eq!(text(&app).matches("* A").count(), 1);

    let mut plain = scratch("no headline\n");
    plain.org_action("org.subtree.copy");
    assert_eq!(plain.status, t!("status.org_not_headline").to_string());
    plain.status.clear();
    plain.org_action("org.subtree.cut");
    assert_eq!(plain.status, t!("status.org_not_headline").to_string());
}

// ----- refile, sparse trees, source blocks -------------------------------------------

#[test]
fn refile_chooser_lists_targets_and_moves_the_subtree() {
    let mut app = scratch("* A\n* B\n* C\n");
    goto(&mut app, 0);
    app.org_action("org.refile");
    {
        let c = app.chooser.refile_mut().unwrap();
        assert_eq!(c.targets.len(), 2, "the subtree itself is not a target");
    }
    app.refile_chooser_key(key(KeyCode::Up)); // wraps to the last
    assert_eq!(app.chooser.refile_mut().unwrap().selected, 1);
    app.refile_chooser_key(key(KeyCode::Down));
    assert_eq!(app.chooser.refile_mut().unwrap().selected, 0);
    app.refile_chooser_key(key(KeyCode::Char('x')));
    app.refile_chooser_key(key(KeyCode::Esc));
    assert!(app.chooser.is_none());

    app.org_action("org.refile");
    app.refile_chooser_key(key(KeyCode::Enter));
    assert!(app.chooser.is_none());
    assert_eq!(app.status, t!("status.org_refiled").to_string());
    assert_eq!(text(&app), "* B\n** A\n* C\n", "A became a child of B");
}

#[test]
fn refile_by_click_and_refusals() {
    let mut app = scratch("* A\n* B\n");
    app.org_action("org.refile");
    app.layout.chooser = Rect::new(0, 0, 30, 5);
    app.refile_chooser_mouse(click(60, 60)); // outside
    assert!(app.chooser.refile_mut().is_some());
    app.refile_chooser_mouse(click(1, 4)); // below the rows
    assert!(app.chooser.refile_mut().is_some());
    app.refile_chooser_mouse(click(1, 0));
    assert!(app.chooser.is_none());
    assert!(
        text(&app).starts_with("* B\n") && text(&app).contains("** A"),
        "{}",
        text(&app)
    );

    // Only one headline: nowhere to refile.
    let mut lone = scratch("* Only\n");
    lone.org_action("org.refile");
    assert_eq!(lone.status, t!("status.org_no_refile_target").to_string());
    assert!(lone.chooser.is_none());
    // Not on a headline.
    let mut plain = scratch("text\n");
    plain.org_action("org.refile");
    assert_eq!(plain.status, t!("status.org_not_headline").to_string());
    // No chooser open: a stale accept does nothing; a vanished target is reported.
    plain.accept_refile_chooser();
    let mut app = scratch("* A\n* B\n");
    app.org_refile_apply(0);
    assert_eq!(app.status, t!("status.org_no_refile_target").to_string());
}

#[test]
fn sparse_trees_fold_and_prompt() {
    let mut app = scratch("* TODO a\nx\n* b\ny\n* TODO c\n");
    app.org_action("org.sparse.todo");
    assert!(
        app.status
            .contains(&t!("status.org_sparse", count = 1).to_string()[..4])
    );
    app.org_action("org.sparse.match");
    assert!(is_prompt(&app, PromptKind::OrgSparseMatch));
    app.prompt = None;
    app.accept_org_prompt(PromptKind::OrgSparseMatch, "");
    app.status.clear();
    app.accept_org_prompt(PromptKind::OrgSparseMatch, "y");
    assert_ne!(app.status, "");
    app.org_sparse_match("");
}

#[test]
fn editing_a_source_block_in_its_own_tab_and_writing_it_back() {
    let mut app = scratch("* S\n#+begin_src rust\nfn a() {}\n#+end_src\n");
    goto(&mut app, 0);
    app.org_action("org.edit_src");
    assert_eq!(app.status, t!("status.org_no_src_block").to_string());
    goto(&mut app, 2);
    app.org_action("org.edit_src");
    assert_eq!(app.status, t!("status.org_src_editing").to_string());
    assert!(app.src_edit.is_some());
    assert_eq!(text(&app), "fn a() {}\n");
    set_active(&mut app, "fn b() {}\n");
    app.org_action("org.edit_src");
    assert_eq!(app.status, t!("status.org_src_applied").to_string());
    assert!(text(&app).contains("fn b() {}"));
    assert!(app.src_edit.is_none());
}

fn set_active(app: &mut App, s: &str) {
    app.editor.active_tab_mut().unwrap().editor.set_content(s);
}

#[test]
fn a_source_edit_whose_source_vanished_is_reported() {
    let mut app = scratch("#+begin_src sh\necho\n#+end_src\n");
    goto(&mut app, 1);
    app.org_action("org.edit_src");
    app.src_edit.as_mut().unwrap().source_index = 99;
    app.org_edit_src_finish();
    assert_eq!(app.status, t!("status.org_src_gone").to_string());

    // The block's markers were destroyed meanwhile: also "gone".
    let mut app = scratch("#+begin_src sh\necho\n#+end_src\n");
    goto(&mut app, 1);
    app.org_action("org.edit_src");
    app.editor.active = 0;
    set_active(&mut app, "no block now\n");
    app.editor.active = 1;
    app.org_edit_src_finish();
    assert_eq!(app.status, t!("status.org_src_gone").to_string());

    // A stale session with a file-backed active tab is abandoned.
    let (_d, mut app) = app_with("src-stale", &[("a.org", "x\n")]);
    app.src_edit = Some(SrcEdit {
        source: None,
        source_index: 0,
        begin_line: 0,
    });
    app.org_action("org.edit_src");
    assert!(app.src_edit.is_none());
}

#[test]
fn footnote_creates_and_navigates() {
    let mut app = scratch("Text here.\n");
    app.editor.active_tab_mut().unwrap().editor.set_cursor(4);
    app.org_action("org.footnote");
    assert!(text(&app).contains("[fn:1]"), "{}", text(&app));
    assert!(dirty(&app));
    // On the reference, the action jumps to the definition without editing.
    let before = text(&app);
    app.editor.active_tab_mut().unwrap().editor.set_cursor(5);
    app.org_action("org.footnote");
    assert_eq!(text(&app), before);
}

// ----- archive, tags, properties, planning -----------------------------------------------

#[test]
fn archiving_moves_a_subtree_to_the_sibling_archive_file() {
    let (d, mut app) = app_with("archive", &[("n.org", "* Keep\n* Gone\nbody\n")]);
    goto(&mut app, 1);
    app.org_action("org.archive.subtree");
    assert!(app.status.contains("n.org_archive"), "{}", app.status);
    assert_eq!(text(&app), "* Keep");
    let archived = std::fs::read_to_string(d.join("n.org_archive")).unwrap();
    assert!(archived.contains("* Gone"), "{archived}");
    // A second archive appends to the same file.
    app.org_action("org.archive.subtree");
    assert!(
        std::fs::read_to_string(d.join("n.org_archive"))
            .unwrap()
            .contains("* Keep")
    );
    assert_eq!(text(&app), "");

    let mut plain = app_at(&temp("archive-plain"));
    plain.editor.new_tab_with_content("* x\n");
    plain.org_action("org.archive.subtree");
    assert_eq!(plain.status, t!("status.org_archive_unsaved").to_string());
}

#[test]
fn archiving_off_a_headline_and_with_an_unwritable_archive() {
    let (d, mut app) = app_with("archive-bad", &[("n.org", "text only\n* H\n")]);
    goto(&mut app, 0);
    app.org_action("org.archive.subtree");
    assert_eq!(app.status, t!("status.org_not_headline").to_string());
    // A directory squatting on the archive path makes the write fail.
    std::fs::create_dir(d.join("n.org_archive")).unwrap();
    goto(&mut app, 1);
    let before = app.messages.items.len();
    app.org_action("org.archive.subtree");
    assert_eq!(app.messages.items.len(), before + 1);
    assert_eq!(text(&app), "text only\n* H\n", "the buffer is left alone");
}

#[test]
fn archive_tag_tags_and_properties() {
    let mut app = scratch("* Task\n");
    app.org_action("org.archive.tag");
    assert!(text(&app).contains(":ARCHIVE:"), "{}", text(&app));
    app.org_action("org.set_tags");
    assert!(is_prompt(&app, PromptKind::OrgSetTags));
    assert_eq!(app.prompt.as_ref().unwrap().input, "ARCHIVE");
    app.prompt = None;
    app.org_set_tags("work urgent");
    assert!(text(&app).contains(":work:urgent:"), "{}", text(&app));
    app.org_action("org.set_property");
    assert!(is_prompt(&app, PromptKind::OrgSetProperty));
    app.prompt = None;
    app.accept_org_prompt(PromptKind::OrgSetProperty, "OWNER  me");
    assert!(text(&app).contains(":OWNER:"), "{}", text(&app));
    assert!(text(&app).contains("me"));
    let before = text(&app);
    app.org_set_property("");
    assert_eq!(text(&app), before);

    goto(&mut app, 9);
    let mut plain = scratch("not a headline\n");
    plain.org_set_tags("a");
    assert_eq!(plain.status, t!("status.org_not_headline").to_string());
    plain.status.clear();
    plain.org_set_property("A b");
    assert_eq!(plain.status, t!("status.org_not_headline").to_string());
}

#[test]
fn planning_and_timestamps() {
    let mut app = scratch("* Task\n");
    app.org_action("org.schedule");
    assert!(is_prompt(&app, PromptKind::OrgSchedule));
    assert_ne!(app.prompt.as_ref().unwrap().input, "");
    app.prompt = None;
    app.org_action("org.deadline");
    assert!(is_prompt(&app, PromptKind::OrgDeadline));
    app.prompt = None;
    app.accept_org_prompt(PromptKind::OrgSchedule, "2030-03-04");
    assert!(
        text(&app).contains("SCHEDULED: <2030-03-04"),
        "{}",
        text(&app)
    );
    app.accept_org_prompt(PromptKind::OrgDeadline, "2030-04-05");
    assert!(text(&app).contains("DEADLINE: <2030-04-05"));
    app.status.clear();
    app.org_plan("DEADLINE", "not a date");
    assert_eq!(app.status, t!("status.org_bad_date").to_string());

    let at = text(&app).find("2030-03-04").unwrap() + 2;
    app.editor.active_tab_mut().unwrap().editor.set_cursor(at);
    app.org_action("org.date_up");
    assert!(text(&app).contains("2030-03-05"), "{}", text(&app));
    app.org_action("org.date_down");
    assert!(text(&app).contains("2030-03-04"), "{}", text(&app));

    let mut plain = scratch("text\n");
    plain.org_plan("SCHEDULED", "2030-03-04");
    assert_eq!(plain.status, t!("status.org_not_headline").to_string());
    plain.org_action("org.timestamp");
    assert!(text(&plain).starts_with('<'), "{}", text(&plain));
    let mut inactive = scratch("");
    inactive.org_action("org.timestamp_inactive");
    assert!(text(&inactive).starts_with('['), "{}", text(&inactive));
}

// ----- links ------------------------------------------------------------------------------

#[test]
fn storing_and_inserting_a_link() {
    let (_d, mut app) = app_with("link-store", &[("n.org", "line one\nline two\n")]);
    goto(&mut app, 1);
    app.org_action("org.link.store");
    let stored = app.stored_org_link.clone().unwrap();
    assert!(stored.contains("n.org::2"), "{stored}");

    app.org_action("org.link.insert");
    assert!(is_prompt(&app, PromptKind::OrgLinkTarget));
    assert_eq!(app.prompt.as_ref().unwrap().input, stored);
    app.prompt = None;
    app.accept_org_prompt(PromptKind::OrgLinkTarget, "");
    assert!(app.prompt.is_none());
    app.accept_org_prompt(PromptKind::OrgLinkTarget, "https://example.invalid");
    assert!(is_prompt(&app, PromptKind::OrgLinkDesc));
    app.prompt = None;
    app.accept_org_prompt(PromptKind::OrgLinkDesc, "Example");
    assert!(text(&app).contains("[[https://example.invalid][Example]]"));
    // Without a description: the bare form. Without a pending target: no-op.
    app.pending_link_target = Some("x".into());
    app.org_insert_link("");
    assert!(text(&app).contains("[[x]]"));
    let before = text(&app);
    app.org_insert_link("late");
    assert_eq!(text(&app), before);

    let mut plain = scratch("x\n");
    plain.org_action("org.link.store");
    assert_eq!(plain.status, t!("status.org_archive_unsaved").to_string());
}

#[test]
fn link_navigation_and_following() {
    let mut app = scratch("a [[https://example.invalid][web]] b [[*Target]] c\n* Target\n");
    app.org_action("org.link.next");
    assert!(app.editor.active_tab().unwrap().editor.get_cursor() > 0);
    app.org_action("org.link.prev");
    app.org_action("org.link.next");
    // On the URL link: copied, not opened.
    app.editor.active_tab_mut().unwrap().editor.set_cursor(5);
    app.org_action("org.link.follow");
    assert!(app.status.contains("example.invalid"), "{}", app.status);
    // On the internal link: jumps to the headline.
    let at = text(&app).find("[[*Target]]").unwrap() + 3;
    app.editor.active_tab_mut().unwrap().editor.set_cursor(at);
    app.org_action("org.link.follow");
    assert_eq!(line(&app), 1);
    // A heading link with no target.
    set_active(&mut app, "[[*Missing]] and no link here\n");
    app.editor.active_tab_mut().unwrap().editor.set_cursor(3);
    app.status.clear();
    app.org_action("org.link.follow");
    assert_eq!(app.status, t!("status.org_no_link_target").to_string());
    // Not on a link.
    app.editor.active_tab_mut().unwrap().editor.set_cursor(20);
    app.status.clear();
    app.org_action("org.link.follow");
    assert_eq!(app.status, t!("status.org_no_link").to_string());
    // No links at all.
    let mut none = scratch("nothing\n");
    none.org_action("org.link.next");
    assert_eq!(none.status, t!("status.org_no_link").to_string());
}

#[test]
fn following_file_and_id_links_opens_the_target() {
    let (d, mut app) = app_with(
        "link-follow",
        &[
            (
                "a.org",
                "[[file:b.org::2][b]] [[id:abc-123][node]] [[id:nope][gone]]\n",
            ),
            ("b.org", "first\nsecond\n"),
            ("c.org", "* Node\n:PROPERTIES:\n:ID: abc-123\n:END:\n"),
        ],
    );
    app.editor.active_tab_mut().unwrap().editor.set_cursor(3);
    app.org_action("org.link.follow");
    assert_eq!(app.active_path().unwrap(), d.join("b.org"));
    assert_eq!(line(&app), 1);

    app.open_initial(&d.join("a.org"));
    let at = text(&app).find("[[id:abc").unwrap() + 3;
    app.editor.active_tab_mut().unwrap().editor.set_cursor(at);
    app.org_action("org.link.follow");
    assert_eq!(app.active_path().unwrap(), d.join("c.org"));

    app.open_initial(&d.join("a.org"));
    let at = text(&app).find("[[id:nope").unwrap() + 3;
    app.editor.active_tab_mut().unwrap().editor.set_cursor(at);
    app.status.clear();
    app.org_action("org.link.follow");
    assert_eq!(app.status, t!("status.org_no_link_target").to_string());
}

// ----- emphasis, blocks, clock -------------------------------------------------------------

#[test]
fn emphasis_wraps_the_selection_and_blocks_insert_templates() {
    for (action, marker) in [
        ("org.emphasis.bold", "*"),
        ("org.emphasis.italic", "/"),
        ("org.emphasis.underline", "_"),
        ("org.emphasis.code", "~"),
        ("org.emphasis.verbatim", "="),
        ("org.emphasis.strike", "+"),
    ] {
        let mut app = scratch("word\n");
        app.editor
            .active_tab_mut()
            .unwrap()
            .editor
            .set_selection_range(0, 4);
        assert!(app.org_action(action), "{action}");
        assert_eq!(text(&app), format!("{marker}word{marker}\n"), "{action}");
    }
    let mut app = scratch("");
    assert!(!app.org_action("org.emphasis.nope"));
    assert!(!app.org_action("org.block.nope"));
    for kind in ["src", "example", "quote", "center", "verse", "comment"] {
        let mut app = scratch("");
        assert!(app.org_action(&format!("org.block.{kind}")));
        assert!(text(&app).contains(&format!("#+begin_{kind}")), "{kind}");
        assert!(text(&app).contains(&format!("#+end_{kind}")));
    }
}

#[test]
fn clocking_in_and_out_and_the_time_report() {
    let mut app = scratch("* Task\n");
    app.org_action("org.clock_in");
    assert!(text(&app).contains("CLOCK: ["), "{}", text(&app));
    app.org_action("org.clock_out");
    assert_eq!(app.status, t!("status.org_clocked_out").to_string());
    assert!(text(&app).contains("=>"), "{}", text(&app));
    app.org_action("org.clock_out");
    assert_eq!(app.status, t!("status.org_no_clock").to_string());
    let tabs = app.editor.tabs.len();
    app.org_action("org.time_report");
    assert_eq!(app.status, t!("status.org_time_report").to_string());
    assert_eq!(app.editor.tabs.len(), tabs + 1);
}

#[test]
fn org_dispatcher_declines_unknown_actions_and_opens_agenda_prompts() {
    let mut app = scratch("* x\n");
    assert!(!app.org_action("nope.nothing"));
    app.org_action("org.agenda.match");
    assert!(is_prompt(&app, PromptKind::OrgAgendaMatch));
    app.org_action("org.agenda.search");
    assert!(is_prompt(&app, PromptKind::OrgAgendaSearch));
    // Unrelated prompt kinds are ignored by the Org accept handlers.
    app.accept_org_prompt(PromptKind::Open, "x");
    app.accept_org_agenda_prompt(PromptKind::Open, "x");
    app.accept_roam_prompt(PromptKind::Open, "x");
}

// ----- cycle visibility / org_view_key ---------------------------------------------------------

#[test]
fn tab_folds_drawers_and_shift_tab_cycles_globally() {
    let (_d, mut app) = app_with(
        "fold",
        &[(
            "n.org",
            "* H\n:PROPERTIES:\n:ID: x\n:END:\nbody\n* I\nmore\n",
        )],
    );
    goto(&mut app, 1);
    assert!(app.org_view_key(key(KeyCode::Tab)), "on a drawer header");
    assert!(app.org_view_key(key(KeyCode::Tab)), "and back");
    goto(&mut app, 4);
    assert!(!app.org_view_key(key(KeyCode::Tab)));
    app.org_action("org.cycle_visibility");
    assert!(app.org_view_key(key(KeyCode::BackTab)));
    assert!(app.org_view_key(key(KeyCode::BackTab)));
    // Modified keys fall through.
    let ctrl_tab = KeyEvent::new(KeyCode::Tab, KeyModifiers::CONTROL);
    assert!(!app.org_view_key(ctrl_tab));
    assert!(!app.org_view_key(key(KeyCode::Char('q'))));
    // With a selection S-Tab still outdents.
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_selection_range(0, 3);
    assert!(!app.org_view_key(key(KeyCode::BackTab)));
    assert!(!app.org_toggle_drawer_fold());
}

// ----- capture -------------------------------------------------------------------------------

fn template(key: &str, target: &str, body: &str) -> vix_org_capture::CaptureTemplate {
    vix_org_capture::CaptureTemplate {
        key: key.into(),
        description: format!("desc {key}"),
        entry_type: vix_org_capture::EntryType::Entry,
        target: target.into(),
        template: body.into(),
        prepend: false,
        empty_lines: 0,
        immediate_finish: true,
        clock_in: false,
    }
}

#[test]
fn capture_by_key_runs_the_wizard_into_the_cursor() {
    let mut app = scratch("");
    app.org_action("org.capture");
    // Built-in "a": one `%^{Task}` field, immediate finish.
    assert!(is_prompt(&app, PromptKind::OrgCaptureField));
    assert!(app.prompt.as_ref().unwrap().preview.is_some());
    answer(&mut app, PromptKind::OrgCaptureField, "write tests");
    assert!(app.prompt.is_none());
    assert!(text(&app).contains("* TODO write tests"), "{}", text(&app));

    // "t": no fields, so a review prompt pre-filled with the entry.
    app.org_action("org.capture.task");
    assert!(is_prompt(&app, PromptKind::OrgCaptureReview));
    assert!(app.capture_review.is_some());
    assert!(app.prompt.as_ref().unwrap().input.contains("* TODO"));
    answer(
        &mut app,
        PromptKind::OrgCaptureReview,
        "* TODO edited in review",
    );
    assert!(text(&app).contains("edited in review"));
    assert!(app.capture_review.is_none());
    // A stale review accept does nothing.
    let before = text(&app);
    answer(&mut app, PromptKind::OrgCaptureReview, "* ignored");
    assert_eq!(text(&app), before);

    app.org_action("org.capture.babel");
    assert!(is_prompt(&app, PromptKind::OrgCaptureField));
    answer(&mut app, PromptKind::OrgCaptureField, "rust");
    assert!(is_prompt(&app, PromptKind::OrgCaptureReview));
    assert!(
        app.prompt
            .as_ref()
            .unwrap()
            .input
            .contains("#+begin_src rust")
    );
    app.prompt = None;
    app.capture_review = None;

    app.org_action("org.capture.note");
    answer(&mut app, PromptKind::OrgCaptureField, "a note");
    assert!(text(&app).contains("a note"));
}

#[test]
fn capture_with_a_missing_key_or_no_templates_reports() {
    let mut app = scratch("");
    app.start_capture_by_key("zz");
    assert_eq!(
        app.status,
        t!("status.org_capture_missing_key", key = "zz").to_string()
    );
    app.settings.org_capture_templates.clear();
    app.org_action("org.capture.select");
    assert_eq!(
        app.status,
        t!("status.org_capture_no_templates").to_string()
    );
    assert!(app.chooser.is_none());
    // A stray field answer with no wizard running does nothing.
    app.accept_capture_field("x");
    app.advance_capture();
    app.finish_capture();
    assert!(app.prompt.is_none());
}

#[test]
fn capture_asks_for_tags_when_the_template_wants_them() {
    let mut app = scratch("");
    app.settings.org_capture_templates = vec![template("g", "cursor", "* %^{Title} %^g")];
    app.org_action("org.capture.select");
    app.capture_chooser_key(key(KeyCode::Enter));
    assert!(is_prompt(&app, PromptKind::OrgCaptureField));
    answer(&mut app, PromptKind::OrgCaptureField, "Item");
    // Second prompt is the tag prompt, with a preview.
    assert!(is_prompt(&app, PromptKind::OrgCaptureField));
    assert!(app.prompt.as_ref().unwrap().preview.is_some());
    answer(&mut app, PromptKind::OrgCaptureField, "work urgent");
    assert!(text(&app).contains("Item"), "{}", text(&app));
    assert!(text(&app).contains(":work:urgent:"), "{}", text(&app));
    // Empty tag input yields no tag text at all.
    assert_eq!(App::format_capture_tags("  "), "");
    assert_eq!(App::format_capture_tags("a b"), ":a:b:");
}

#[test]
fn the_capture_chooser_navigates_and_clicks() {
    let mut app = scratch("");
    app.settings.org_capture_templates = vec![
        template("x", "cursor", "* first"),
        template("y", "cursor", "* second"),
    ];
    app.org_action("org.capture.select");
    app.capture_chooser_key(key(KeyCode::Up));
    assert_eq!(app.chooser.capture_mut().unwrap().selected, 1);
    app.capture_chooser_key(key(KeyCode::Down));
    assert_eq!(app.chooser.capture_mut().unwrap().selected, 0);
    app.capture_chooser_key(key(KeyCode::Char('q')));
    app.capture_chooser_key(key(KeyCode::Esc));
    assert!(app.chooser.is_none());

    app.org_action("org.capture.select");
    app.layout.chooser = Rect::new(0, 0, 30, 5);
    app.capture_chooser_mouse(click(60, 60));
    app.capture_chooser_mouse(click(1, 4)); // below the two rows
    assert!(app.chooser.capture_mut().is_some());
    app.capture_chooser_mouse(click(1, 1));
    assert!(app.chooser.is_none());
    assert!(text(&app).contains("second"), "{}", text(&app));
}

#[test]
fn captures_file_into_files_headlines_datetrees_and_ids() {
    let (d, mut app) = app_with(
        "capture-files",
        &[
            ("inbox.org", "* Inbox\n* Other\n"),
            ("nodes.org", "* Node\n:PROPERTIES:\n:ID: node-1\n:END:\n"),
        ],
    );
    app.settings.org_capture_templates = vec![
        template("f", "file:new.org", "* appended"),
        template("h", "file+headline:inbox.org#Inbox", "** under inbox"),
        template("d", "file+datetree:journal.org", "* journaled"),
        template("i", "id:node-1", "** under node"),
        template("m", "id:no-such-id", "** lost"),
    ];
    app.start_capture_by_key("f");
    assert!(
        std::fs::read_to_string(d.join("new.org"))
            .unwrap()
            .contains("* appended")
    );
    app.start_capture_by_key("h");
    let inbox = std::fs::read_to_string(d.join("inbox.org")).unwrap();
    assert!(inbox.find("under inbox").unwrap() > inbox.find("* Inbox").unwrap());
    assert!(inbox.find("under inbox").unwrap() < inbox.find("* Other").unwrap());
    app.start_capture_by_key("d");
    let journal = std::fs::read_to_string(d.join("journal.org")).unwrap();
    assert!(journal.contains("* journaled"), "{journal}");
    assert!(
        journal.lines().count() >= 4,
        "year/month/day tree: {journal}"
    );
    app.start_capture_by_key("i");
    assert!(
        std::fs::read_to_string(d.join("nodes.org"))
            .unwrap()
            .contains("under node")
    );
    app.start_capture_by_key("m");
    assert_eq!(
        app.status,
        t!("status.org_capture_id_not_found", id = "no-such-id").to_string()
    );
}

#[test]
fn clocked_captures_get_a_clock_line() {
    let mut app = scratch("");
    let mut t = template("k", "cursor", "* Timed\nbody");
    t.clock_in = true;
    app.settings.org_capture_templates = vec![t];
    app.start_capture_by_key("k");
    assert!(text(&app).contains("CLOCK: ["), "{}", text(&app));
    assert!(text(&app).find("* Timed").unwrap() < text(&app).find("CLOCK").unwrap());
    assert_eq!(App::insert_capture_clock("only", "C"), "only\n  C");
    assert_eq!(App::insert_capture_clock("h\nrest", "C"), "h\n  C\nrest");
}

// ----- agenda -----------------------------------------------------------------------------------

const AGENDA_FILE: &str = "* TODO write docs :doc:\n* TODO ship it\n** NEXT sub\n* Done thing\n";

#[test]
fn agenda_views_list_tasks_and_search() {
    let (_d, mut app) = app_with("agenda-views", &[("t.org", AGENDA_FILE)]);
    for action in ["org.agenda.todo", "org.agenda", "org.agenda.stuck"] {
        app.agenda = None;
        app.org_action(action);
        assert!(app.agenda.is_some(), "{action}");
        assert!(app.editor.active_tab().unwrap().read_only, "{action}");
        let count = app.agenda.as_ref().unwrap().items.len();
        assert_eq!(
            app.status,
            t!("status.org_agenda_view", count = count).to_string(),
            "{action}"
        );
    }
    app.org_action("org.agenda.todo");
    assert!(text(&app).contains("write docs"), "{}", text(&app));
    app.accept_org_agenda_prompt(PromptKind::OrgAgendaMatch, "doc");
    assert!(text(&app).contains("write docs"));
    assert!(!text(&app).contains("ship it"), "{}", text(&app));
    app.accept_org_agenda_prompt(PromptKind::OrgAgendaSearch, "ship");
    assert!(text(&app).contains("ship it"));
    assert!(!text(&app).contains("write docs"));
}

#[test]
fn the_agenda_t_key_cycles_the_task_under_the_cursor() {
    let (d, mut app) = app_with("agenda-t", &[("t.org", AGENDA_FILE)]);
    // Not an agenda yet: `t` is not consumed.
    assert!(!app.org_view_key(key(KeyCode::Char('t'))));
    app.org_action("org.agenda.todo");
    let row = text(&app)
        .lines()
        .position(|l| l.contains("write docs"))
        .unwrap();
    goto(&mut app, row);
    assert!(app.org_view_key(key(KeyCode::Char('t'))));
    assert_eq!(app.status, t!("status.org_agenda_toggled").to_string());
    let src = std::fs::read_to_string(d.join("t.org")).unwrap();
    assert!(src.contains("DONE write docs"), "{src}");
    // The agenda was rebuilt in place, still read-only and not dirty.
    assert!(app.editor.active_tab().unwrap().read_only);
    assert!(!dirty(&app));

    // On a header row there is no task.
    goto(&mut app, 0);
    assert!(app.org_view_key(key(KeyCode::Char('t'))));
    assert_eq!(app.status, t!("status.org_agenda_no_task").to_string());

    // A source file that disappeared is reported.
    let row = text(&app)
        .lines()
        .position(|l| l.contains("ship it"))
        .unwrap();
    goto(&mut app, row);
    std::fs::remove_file(d.join("t.org")).unwrap();
    assert!(app.org_view_key(key(KeyCode::Char('t'))));
    assert_eq!(app.status, t!("status.org_agenda_error").to_string());

    // Once another buffer is active, `t` types normally again.
    app.editor.new_tab_with_content("other");
    assert!(!app.org_view_key(key(KeyCode::Char('t'))));
    app.rebuild_agenda(0);
    app.agenda = None;
    app.rebuild_agenda(0);
}

#[test]
fn the_agenda_scope_follows_lock_file_list_and_default() {
    let (d, mut app) = app_with(
        "agenda-scope",
        &[("a.org", "* TODO in a\n"), ("b.org", "* TODO in b\n")],
    );
    app.org_action("org.agenda.file_list");
    assert_eq!(app.status, t!("status.org_agenda_files_all").to_string());
    app.org_action("org.agenda.lock");
    assert!(app.agenda_restriction.is_some());
    app.org_action("org.agenda.file_list");
    assert!(app.status.contains("a.org"), "{}", app.status);
    let files = app.org_agenda_files();
    assert_eq!(files.len(), 1);
    app.org_action("org.agenda.unlock");
    assert_eq!(app.status, t!("status.org_agenda_unlocked").to_string());
    assert!(app.agenda_restriction.is_none());
    assert_eq!(app.org_agenda_files().len(), 2);

    // The explicit list.
    app.org_action("org.agenda.file_add");
    assert_eq!(
        app.status,
        t!("status.org_agenda_file_added", path = "a.org").to_string()
    );
    app.org_action("org.agenda.file_add");
    assert_eq!(
        app.status,
        t!("status.org_agenda_file_present", path = "a.org").to_string()
    );
    app.org_action("org.agenda.file_list");
    assert!(app.status.contains("a.org"));
    assert_eq!(app.org_agenda_files().len(), 1);
    app.org_action("org.agenda.file_remove");
    assert_eq!(
        app.status,
        t!("status.org_agenda_file_removed", path = "a.org").to_string()
    );
    app.org_action("org.agenda.file_remove");
    assert_eq!(
        app.status,
        t!("status.org_agenda_file_absent", path = "a.org").to_string()
    );
    app.settings.org_agenda_files = vec!["b.org".into(), "gone.org".into()];
    assert_eq!(
        app.org_agenda_files().len(),
        1,
        "unreadable entries are skipped"
    );
    app.org_action("org.agenda.file_clear");
    assert_eq!(
        app.status,
        t!("status.org_agenda_files_cleared").to_string()
    );
    assert_eq!(app.settings.org_agenda_files.len(), 0);

    // An unsaved buffer has no path to add or remove.
    let mut plain = scratch("x\n");
    plain.org_action("org.agenda.file_add");
    assert_eq!(plain.status, t!("status.org_archive_unsaved").to_string());
    plain.status.clear();
    plain.org_action("org.agenda.file_remove");
    assert_eq!(plain.status, t!("status.org_archive_unsaved").to_string());
    drop(d);
}

// ----- contact links, roam prompts ----------------------------------------------------------------------

#[test]
fn contact_link_completion_only_triggers_after_the_marker_in_org_files() {
    let (d, mut app) = app_with("contact", &[("c.org", "[[mailto:")]);
    let end = text(&app).chars().count();
    app.editor.active_tab_mut().unwrap().editor.set_cursor(end);
    assert!(app.maybe_complete_org_contact_link());
    app.completion = None;
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_content("plain text");
    let end = text(&app).chars().count();
    app.editor.active_tab_mut().unwrap().editor.set_cursor(end);
    assert!(!app.maybe_complete_org_contact_link());

    // Not an .org file.
    std::fs::write(d.join("x.txt"), "[[mailto:").unwrap();
    app.open_initial(&d.join("x.txt"));
    let end = text(&app).chars().count();
    app.editor.active_tab_mut().unwrap().editor.set_cursor(end);
    assert!(!app.maybe_complete_org_contact_link());
    // No file at all.
    let mut scratch_app = scratch("[[mailto:");
    assert!(!scratch_app.maybe_complete_org_contact_link());
}

#[test]
fn roam_prompts_edit_the_active_node() {
    let (_d, mut app) = app_with("roam-prompts", &[("n.org", "#+title: Node\n")]);
    app.accept_roam_prompt(PromptKind::RoamTag, "idea");
    assert!(text(&app).contains("#+filetags:"), "{}", text(&app));
    app.accept_roam_prompt(PromptKind::RoamAlias, "Other Name");
    assert!(text(&app).contains("ROAM_ALIASES"), "{}", text(&app));
    app.accept_roam_prompt(PromptKind::RoamRef, "https://example.invalid");
    assert!(text(&app).contains("ROAM_REFS"), "{}", text(&app));
    let before = text(&app);
    app.accept_roam_prompt(PromptKind::RoamTag, "");
    app.accept_roam_prompt(PromptKind::RoamAlias, "");
    app.accept_roam_prompt(PromptKind::RoamRef, "");
    assert_eq!(text(&app), before);
    app.accept_roam_prompt(PromptKind::RoamDailyDate, "");
    app.accept_roam_prompt(PromptKind::GotoByte, "1");
}

// ----- round 3: gaps -------------------------------------------------------------------

#[test]
fn pasting_something_that_is_not_a_subtree_says_so_and_leaves_the_buffer() {
    static CLIP: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _g = CLIP
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut app = scratch("* A\n* B\n");
    // Whatever the clipboard holds, a plain line of prose is no subtree.
    vix_clipboard::set("just some prose").ok();
    app.org_action("org.subtree.paste");
    if vix_clipboard::get().is_ok_and(|c| c == "just some prose") {
        assert_eq!(app.status, t!("status.org_no_subtree_clip").to_string());
        assert_eq!(text(&app), "* A\n* B\n");
    }
}

#[test]
fn sorting_children_reorders_the_subtree_under_the_cursor() {
    let mut app = scratch("* Parent\n** beta\n** alpha");
    app.org_action("org.sort_children");
    assert_eq!(text(&app), "* Parent\n** alpha\n** beta");
}

#[test]
fn archiving_into_a_file_without_a_trailing_newline_starts_a_fresh_line() {
    let (d, mut app) = app_with(
        "archive-nonl",
        &[("n.org", "* Keep\n* Gone\n"), ("n.org_archive", "* Old")],
    );
    goto(&mut app, 1);
    app.org_action("org.archive.subtree");
    let archived = std::fs::read_to_string(d.join("n.org_archive")).unwrap();
    assert!(archived.starts_with("* Old\n* Gone"), "{archived}");
    assert!(archived.ends_with('\n'));
}

#[test]
fn the_table_sort_prompt_reports_a_cursor_outside_any_table() {
    let mut app = scratch("* Not a table\n");
    app.accept_org_prompt(PromptKind::OrgTableSort, "1");
    assert_eq!(app.status, t!("status.org_table_no_table").to_string());
}

#[test]
fn an_unreadable_org_file_in_the_index_is_skipped_when_following_an_id() {
    let (d, mut app) = app_with(
        "id-skip",
        &[
            ("a.org", "[[id:target-1][t]]\n"),
            ("z.org", "* Z\n:PROPERTIES:\n:ID: target-1\n:END:\n"),
        ],
    );
    // A directory named like an org file cannot be read as text, and a
    // non-org file is not even tried.
    std::fs::create_dir(d.join("dir.org")).unwrap();
    std::fs::write(d.join("notes.txt"), ":ID: target-1\n").unwrap();
    app.build_file_index();
    app.editor.active_tab_mut().unwrap().editor.set_cursor(3);
    app.org_action("org.link.follow");
    assert_eq!(app.active_path().unwrap(), d.join("z.org"));
}
