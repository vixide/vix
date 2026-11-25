//! Behavior tests for Org-roam / Org-node actions over real temp-dir notes.

use super::*;
use crate::settings::Settings;
use ratatui::layout::Rect;

fn temp(tag: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("vix-rt-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

/// An app over a dir holding `files`; the first is opened.
fn app_with(tag: &str, files: &[(&str, &str)]) -> (PathBuf, App) {
    let d = temp(tag);
    for (name, body) in files {
        std::fs::write(d.join(name), body).unwrap();
    }
    let mut app = App::new(d.clone(), Settings::default());
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app.build_file_index();
    if let Some((first, _)) = files.first() {
        app.open_initial(&d.join(first));
    }
    (d, app)
}

fn text(app: &App) -> String {
    app.editor.active_tab().unwrap().text()
}

fn is_prompt(app: &App, kind: PromptKind) -> bool {
    app.prompt.as_ref().is_some_and(|p| p.kind == kind)
}

fn node(title: &str, id: &str) -> String {
    crate::roam::new_node(title, id)
}

#[test]
fn prompt_actions_open_their_prompts() {
    let (_d, mut app) = app_with("prompts", &[("a.org", "x\n")]);
    for (action, kind) in [
        ("roam.node_find", PromptKind::RoamFind),
        ("roam.node_insert", PromptKind::RoamInsert),
        ("roam.capture", PromptKind::RoamCapture),
        ("roam.dailies_capture", PromptKind::RoamDailyCapture),
        ("roam.dailies_date", PromptKind::RoamDailyDate),
        ("roam.tag_add", PromptKind::RoamTag),
        ("roam.alias_add", PromptKind::RoamAlias),
        ("roam.ref_add", PromptKind::RoamRef),
        ("node.insert_transclusion", PromptKind::NodeTransclusion),
    ] {
        app.prompt = None;
        assert!(app.roam_action(action), "{action}");
        assert!(is_prompt(&app, kind), "{action}");
    }
    // The date prompt is pre-filled with a valid date.
    app.roam_action("roam.dailies_date");
    let seed = app.prompt.as_ref().unwrap().input.clone();
    assert!(crate::roam::is_valid_daily_date(&seed), "{seed}");
    assert!(!app.roam_action("roam.nope"));
}

#[test]
fn finding_a_node_opens_it_or_creates_it() {
    let existing = node("Existing Note", "id-1");
    let (d, mut app) = app_with("find", &[("existing.org", &existing)]);
    app.editor.new_tab_with_content("scratch");
    // Case-insensitive title match opens the existing file and returns its id.
    let id = app.roam_visit_or_create("  existing NOTE ");
    assert_eq!(id.as_deref(), Some("id-1"));
    assert_eq!(app.active_path().unwrap(), d.join("existing.org"));

    // A new title creates and opens `<slug>.org` with a fresh id.
    let id = app.roam_visit_or_create("Brand New").unwrap();
    assert_eq!(
        app.status,
        t!("status.roam_created", title = "Brand New").to_string()
    );
    let path = d.join(format!("{}.org", crate::roam::slugify("Brand New")));
    assert_eq!(app.active_path().unwrap(), path);
    let body = std::fs::read_to_string(&path).unwrap();
    assert!(
        body.contains(&id) && body.contains("#+title: Brand New"),
        "{body}"
    );

    assert_eq!(app.roam_visit_or_create("   "), None);
    // Through the prompt handlers.
    app.accept_roam_prompt(PromptKind::RoamFind, "Existing Note");
    assert_eq!(app.active_path().unwrap(), d.join("existing.org"));
    app.accept_roam_prompt(PromptKind::RoamCapture, "Another One");
    assert!(d.join("another-one.org").exists());
}

#[test]
fn writing_a_node_reports_a_failure() {
    let (d, mut app) = app_with("write-fail", &[("a.org", "x\n")]);
    // A file squatting where the parent directory would go.
    std::fs::write(d.join("blocker"), "file").unwrap();
    let ok = app.roam_write_and_open(&d.join("blocker").join("child.org"), "body");
    assert!(!ok);
    let prefix = t!("msg.save_failed", error = "");
    let last = &app.messages.items.last().unwrap().text;
    assert!(last.starts_with(prefix.trim_end()), "{last}");
    // A directory squatting at the file path: create_dir_all passes, write fails.
    std::fs::create_dir(d.join("taken.org")).unwrap();
    let before = app.messages.items.len();
    assert!(!app.roam_write_and_open(&d.join("taken.org"), "body"));
    assert_eq!(app.messages.items.len(), before + 1);
    // Creating a node whose file path is blocked yields no id.
    let before = app.messages.items.len();
    assert_eq!(app.roam_visit_or_create("taken"), None);
    assert_eq!(app.messages.items.len(), before + 1);
}

#[test]
fn inserting_a_link_finds_or_creates_without_leaving_the_buffer() {
    let target = node("Target Note", "tid-9");
    let (d, mut app) = app_with(
        "insert",
        &[("main.org", "start\n"), ("target.org", &target)],
    );
    let main = d.join("main.org");
    app.roam_insert_link("target note");
    assert!(
        text(&app).contains("[[id:tid-9][target note]]"),
        "{}",
        text(&app)
    );
    assert_eq!(app.active_path().unwrap(), main, "still in the same buffer");

    app.roam_insert_link("Fresh Idea");
    let created = d.join("fresh-idea.org");
    assert!(created.exists(), "the node file is written");
    assert_eq!(app.active_path().unwrap(), main, "but not opened");
    let id = crate::roam::node_id(&std::fs::read_to_string(&created).unwrap()).unwrap();
    assert!(
        text(&app).contains(&format!("[[id:{id}][Fresh Idea]]")),
        "{}",
        text(&app)
    );

    let before = text(&app);
    app.roam_insert_link("   ");
    assert_eq!(text(&app), before);
    app.accept_roam_prompt(PromptKind::RoamInsert, "Target Note");
    assert!(text(&app).matches("tid-9").count() >= 2);
}

#[test]
fn a_node_that_cannot_be_written_still_links_by_file() {
    let (d, mut app) = app_with("insert-fail", &[("main.org", "x\n")]);
    std::fs::create_dir(d.join("blocked.org")).unwrap();
    let before = app.messages.items.len();
    app.roam_insert_link("blocked");
    assert_eq!(app.messages.items.len(), before + 1);
    assert!(
        text(&app).contains("[[file:blocked.org][blocked]]"),
        "{}",
        text(&app)
    );
}

#[test]
fn transclusions_reference_a_found_or_created_node() {
    let target = node("Shared", "sh-1");
    let (d, mut app) = app_with(
        "transclude",
        &[("main.org", "x\n"), ("shared.org", &target)],
    );
    app.node_insert_transclusion("Shared");
    assert!(
        text(&app).contains("#+transclude: [[id:sh-1][Shared]]"),
        "{}",
        text(&app)
    );
    app.node_insert_transclusion("Newly Made");
    assert!(d.join("newly-made.org").exists());
    assert!(text(&app).contains("#+transclude:"));
    let before = text(&app);
    app.node_insert_transclusion("  ");
    assert_eq!(text(&app), before);
    // An unwritable new node inserts nothing.
    std::fs::create_dir(d.join("nope.org")).unwrap();
    app.node_insert_transclusion("nope");
    assert_eq!(text(&app), before);
}

#[test]
fn a_random_node_opens_one_of_the_nodes() {
    let (d, mut app) = app_with("random", &[("plain.org", "no title here\n")]);
    app.roam_action("roam.node_random");
    assert_eq!(app.status, t!("status.roam_no_nodes").to_string());
    std::fs::write(d.join("n1.org"), node("One", "1")).unwrap();
    std::fs::write(d.join("n2.org"), node("Two", "2")).unwrap();
    app.build_file_index();
    app.roam_action("roam.node_random");
    let opened = app.active_path().unwrap();
    assert!(
        opened == d.join("n1.org") || opened == d.join("n2.org"),
        "{opened:?}"
    );
}

#[test]
fn backlinks_list_who_links_to_the_node() {
    let target = node("Target", "tid-1");
    let linker = format!("{}* See [[id:tid-1][Target]]\n", node("Linker", "lid-1"));
    let (_d, mut app) = app_with(
        "backlinks",
        &[("target.org", &target), ("linker.org", &linker)],
    );
    let tabs = app.editor.tabs.len();
    app.roam_action("roam.backlinks");
    assert_eq!(
        app.status,
        t!("status.roam_backlinks", title = "Target").to_string()
    );
    assert_eq!(app.editor.tabs.len(), tabs + 1);
    assert!(text(&app).contains("linker.org"), "{}", text(&app));

    // Not a node: nothing to find backlinks for.
    let (_d, mut plain) = app_with("backlinks-plain", &[("p.org", "just prose\n")]);
    plain.roam_action("roam.backlinks");
    assert_eq!(plain.status, t!("status.roam_not_node").to_string());
}

#[test]
fn live_backlinks_follow_the_active_buffer() {
    let target = node("Target", "tid-1");
    let linker = format!("{}* See [[id:tid-1][Target]]\n", node("Linker", "lid-1"));
    let (d, mut app) = app_with("live", &[("target.org", &target), ("linker.org", &linker)]);
    app.refresh_backlinks_follow();
    assert!(app.bottom_dock.lines.is_empty(), "off by default");

    app.roam_action("roam.backlinks_follow");
    assert_eq!(
        app.status,
        t!("status.backlinks_follow", on = true).to_string()
    );
    assert!(
        app.bottom_dock
            .lines
            .iter()
            .any(|l| l.contains("linker.org")),
        "{:?}",
        app.bottom_dock.lines
    );
    assert!(app.visible.contains(crate::app::Visible::BOTTOM_DOCK));

    // Unchanged buffer and revision: no rebuild (the dock keeps its lines).
    app.bottom_dock.push("sentinel".to_string());
    app.refresh_backlinks_follow();
    assert!(app.bottom_dock.lines.iter().any(|l| l == "sentinel"));

    // Switching to a non-node buffer says so.
    std::fs::write(d.join("prose.org"), "no id\n").unwrap();
    app.open_initial(&d.join("prose.org"));
    app.refresh_backlinks_follow();
    assert!(
        app.bottom_dock
            .lines
            .iter()
            .any(|l| l.as_str() == t!("status.roam_not_node"))
    );

    app.roam_action("roam.backlinks_follow");
    assert_eq!(
        app.status,
        t!("status.backlinks_follow", on = false).to_string()
    );
}

#[test]
fn dailies_open_create_and_capture() {
    let (d, mut app) = app_with("dailies", &[("a.org", "x\n")]);
    app.roam_open_daily("not-a-date");
    assert_eq!(
        app.status,
        t!("status.roam_invalid_date", date = "not-a-date").to_string()
    );
    app.roam_open_daily("../../etc/x");
    assert!(!d.join("daily").exists());

    app.roam_open_daily("2031-02-03");
    assert_eq!(
        app.status,
        t!("status.roam_created", title = "2031-02-03").to_string()
    );
    let path = d.join("daily").join("2031-02-03.org");
    assert_eq!(app.active_path().unwrap(), path);
    assert!(text(&app).contains("2031-02-03"));
    // Opening it again just opens it.
    app.open_initial(&d.join("a.org"));
    app.roam_open_daily("2031-02-03");
    assert_eq!(app.active_path().unwrap(), path);
    assert_ne!(
        app.status,
        t!("status.roam_created", title = "2031-02-03").to_string()
    );

    app.roam_action("roam.dailies_today");
    let today = App::roam_today();
    assert!(d.join("daily").join(format!("{today}.org")).exists());

    app.roam_daily_capture("  remember the milk ");
    assert_eq!(app.status, t!("status.roam_daily_captured").to_string());
    let body = std::fs::read_to_string(d.join("daily").join(format!("{today}.org"))).unwrap();
    assert!(body.contains("remember the milk"), "{body}");
    app.roam_daily_capture("second thought");
    let body = std::fs::read_to_string(d.join("daily").join(format!("{today}.org"))).unwrap();
    assert!(body.contains("second thought") && body.contains("remember the milk"));
    app.status.clear();
    app.roam_daily_capture("   ");
    assert_eq!(app.status, "");
    app.accept_roam_prompt(PromptKind::RoamDailyCapture, "via prompt");
    app.accept_roam_prompt(PromptKind::RoamDailyDate, "2031-04-05");
    assert!(d.join("daily").join("2031-04-05.org").exists());
}

#[test]
fn daily_capture_into_a_file_without_trailing_newline_keeps_entries_apart() {
    let (d, mut app) = app_with("daily-nl", &[("a.org", "x\n")]);
    let today = App::roam_today();
    std::fs::create_dir_all(d.join("daily")).unwrap();
    let p = d.join("daily").join(format!("{today}.org"));
    std::fs::write(&p, "* existing without newline").unwrap();
    app.roam_daily_capture("added");
    let body = std::fs::read_to_string(&p).unwrap();
    assert!(body.starts_with("* existing without newline\n"), "{body}");
    assert!(body.contains("added"));
}

#[test]
fn the_calendar_dailies_action_opens_the_calendar() {
    let (_d, mut app) = app_with("calendar", &[("a.org", "x\n")]);
    app.roam_action("roam.dailies_calendar");
    assert!(app.flags.contains(AppFlags::CALENDAR_DAILIES));
    assert!(app.visible.contains(crate::app::Visible::CALENDAR));
}

#[test]
fn metadata_edits_rewrite_the_active_node_and_keep_the_cursor_line() {
    let (_d, mut app) = app_with(
        "rewrite",
        &[("n.org", "#+title: Node\nline two\nline three\n")],
    );
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_cursor_line(2);
    app.roam_rewrite_active(|t| crate::roam::add_filetag(t, "idea"));
    assert!(text(&app).contains("idea"), "{}", text(&app));
    assert!(app.editor.active_tab().unwrap().dirty);
    assert!(app.editor.active_tab().unwrap().editor.cursor_line() >= 1);
    // A rewrite that changes nothing leaves the buffer clean.
    app.editor.active_tab_mut().unwrap().dirty = false;
    app.roam_rewrite_active(str::to_string);
    assert!(!app.editor.active_tab().unwrap().dirty);
    // Without a buffer it is a no-op.
    app.editor.tabs.clear();
    app.roam_rewrite_active(|_| "x".into());
}

#[test]
fn graph_index_and_reset_summarise_the_notes() {
    let a = format!("{}See [[id:b-id][B]]\n", node("A", "a-id"));
    let (d, mut app) = app_with("graph", &[("a.org", &a), ("b.org", &node("B", "b-id"))]);
    let tabs = app.editor.tabs.len();
    app.roam_action("roam.graph");
    assert_eq!(app.status, t!("status.roam_graph").to_string());
    assert_eq!(app.editor.tabs.len(), tabs + 1);
    assert!(text(&app).contains("flowchart LR"), "{}", text(&app));
    assert!(
        text(&app).contains("-->"),
        "the A to B link is an edge: {}",
        text(&app)
    );

    std::fs::write(d.join("c.org"), node("C", "c-id")).unwrap();
    app.roam_action("roam.db_sync");
    assert_eq!(app.status, t!("status.roam_synced", count = 3).to_string());
    assert!(
        text(&app).contains('C'),
        "the new file is indexed: {}",
        text(&app)
    );

    std::fs::write(d.join("d.org"), node("D", "d-id")).unwrap();
    app.roam_action("node.reset");
    assert_eq!(app.status, t!("status.node_reset", count = 4).to_string());
}

#[test]
fn dead_links_are_reported() {
    let a = format!("{}See [[id:ghost][Gone]]\n", node("A", "a-id"));
    let (_d, mut app) = app_with("dead", &[("a.org", &a)]);
    app.roam_action("node.dead_links");
    assert_eq!(app.status, t!("status.node_dead_links").to_string());
    assert!(text(&app).contains("ghost"), "{}", text(&app));
}

#[test]
fn nodeify_gives_a_headline_an_id() {
    let (_d, mut app) = app_with("nodeify", &[("n.org", "* Heading\nbody\n")]);
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_cursor_line(0);
    app.roam_action("node.nodeify");
    assert_eq!(app.status, t!("status.node_nodeified").to_string());
    assert!(text(&app).contains(":ID:"), "{}", text(&app));
    assert!(app.editor.active_tab().unwrap().dirty);

    let (_d, mut plain) = app_with("nodeify-no", &[("n.org", "just text\n")]);
    plain.roam_action("node.nodeify");
    assert_eq!(plain.status, t!("status.node_not_headline").to_string());
    plain.editor.tabs.clear();
    plain.roam_action("node.nodeify");
}

#[test]
fn extracting_a_subtree_moves_it_to_its_own_node_file() {
    let body = "* Top\n* TODO Big idea\nfirst\n** Sub point\nmore\n* After\n";
    let (d, mut app) = app_with("extract", &[("n.org", body)]);
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_cursor_line(1);
    app.roam_action("node.extract_subtree");
    assert_eq!(
        app.status,
        t!("status.node_extracted", title = "Big idea").to_string()
    );
    let new = d.join(format!("{}.org", crate::roam::slugify("Big idea")));
    let file = std::fs::read_to_string(&new).unwrap();
    assert!(file.contains("#+title: Big idea"), "{file}");
    assert!(file.contains("first"), "{file}");
    assert!(
        file.contains("* Sub point"),
        "nested headline is re-levelled: {file}"
    );
    // The source now links to it, and the new file is the active buffer.
    assert_eq!(app.active_path().unwrap(), new);
    app.open_initial(&d.join("n.org"));
    assert!(text(&app).contains("[[id:"), "{}", text(&app));
    assert!(!text(&app).contains("Sub point"));
    assert!(text(&app).contains("* After"));

    // Off a headline.
    let (_d, mut plain) = app_with("extract-no", &[("n.org", "text\n")]);
    plain.roam_action("node.extract_subtree");
    assert_eq!(plain.status, t!("status.org_not_headline").to_string());
    plain.editor.tabs.clear();
    plain.roam_action("node.extract_subtree");
}

#[test]
fn a_subtree_with_no_body_extracts_to_a_bare_node() {
    let (d, mut app) = app_with("extract-bare", &[("n.org", "* Bare\n")]);
    app.roam_action("node.extract_subtree");
    let file = std::fs::read_to_string(d.join("bare.org")).unwrap();
    assert!(file.contains("#+title: Bare"));
    assert!(file.ends_with('\n'));
}

#[test]
fn renaming_a_file_to_its_title_slug() {
    let (d, mut app) = app_with(
        "rename",
        &[("old-name.org", "#+title: A Better Name\nbody\n")],
    );
    app.roam_action("node.rename_by_title");
    let target = d.join("a-better-name.org");
    assert!(target.exists());
    assert!(!d.join("old-name.org").exists());
    assert_eq!(app.active_path().unwrap(), target);
    assert_eq!(
        app.status,
        t!("status.node_renamed", path = target.display()).to_string()
    );
    // Already named by its title: nothing happens.
    app.status.clear();
    app.roam_action("node.rename_by_title");
    assert_eq!(app.status, "");

    let (_d, mut notitle) = app_with("rename-no", &[("x.org", "no title\n")]);
    notitle.roam_action("node.rename_by_title");
    assert_eq!(notitle.status, t!("status.node_no_title").to_string());
    let mut unsaved = App::new(temp("rename-unsaved"), Settings::default());
    unsaved.editor.new_tab_with_content("#+title: T\n");
    let status = unsaved.status.clone();
    unsaved.roam_action("node.rename_by_title");
    assert_eq!(
        unsaved.status, status,
        "an unsaved buffer has no file to rename"
    );
}

#[test]
fn a_rename_onto_a_taken_name_fails_cleanly_or_overwrites_nothing() {
    let (d, mut app) = app_with("rename-clash", &[("old.org", "#+title: Clash\n")]);
    std::fs::create_dir(d.join("clash.org")).unwrap();
    let before = app.messages.items.len();
    app.roam_action("node.rename_by_title");
    assert_eq!(app.messages.items.len(), before + 1);
    assert!(d.join("old.org").exists());
}

#[test]
fn node_link_completion_lists_titles_by_prefix() {
    let (_d, mut app) = app_with(
        "complete",
        &[
            ("a.org", &node("Alpha", "1")),
            ("b.org", &node("Beta", "2")),
        ],
    );
    app.editor.new_tab_with_content("[[al");
    let end = text(&app).chars().count();
    app.editor.active_tab_mut().unwrap().editor.set_cursor(end);
    app.roam_action("roam.link_complete");
    assert!(app.completion.is_some());
    app.completion = None;

    let (_d, mut none) = app_with("complete-none", &[("p.org", "prose\n")]);
    none.roam_action("roam.link_complete");
    assert_eq!(none.status, t!("status.roam_no_nodes").to_string());
}
