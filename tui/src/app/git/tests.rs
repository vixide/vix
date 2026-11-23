//! Behavior tests for the `git.*` App paths. Every test builds a throwaway
//! repository in a temp dir (through the `vix_git::run` helper, local config
//! only, no signing, no network) and drives the real `App`.

use super::*;
use crate::settings::Settings;
use ratatui::layout::Rect;
use vix_input::{KeyEvent, KeyModifiers, MouseButton, MouseEventKind};

/// A fresh, canonicalized temp directory unique to `tag`.
pub(crate) fn temp(tag: &str) -> PathBuf {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!("vix-gt-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

/// Run `git args` in `dir`, panicking with git's message on failure.
pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
    crate::git::run(dir, args).unwrap_or_else(|e| panic!("git {args:?}: {e}"))
}

/// An initialized repo on branch `main` holding `files`, all committed.
pub(crate) fn repo(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    let d = temp(tag);
    git(&d, &["init", "-q", "-b", "main"]);
    git(&d, &["config", "user.name", "Test"]);
    git(&d, &["config", "user.email", "test@example.invalid"]);
    git(&d, &["config", "commit.gpgsign", "false"]);
    git(&d, &["config", "core.hooksPath", "/dev/null"]);
    for (name, body) in files {
        let p = d.join(name);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, body).unwrap();
    }
    git(&d, &["add", "-A"]);
    git(&d, &["commit", "-q", "-m", "seed"]);
    d
}

/// An app rooted at `dir`, with a headless editor viewport, git state loaded.
pub(crate) fn app_at(dir: &Path) -> App {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let session = std::env::temp_dir().join(format!("vix-gt-sess-{}-{n}.toml", std::process::id()));
    let mut app = App::new(dir.to_path_buf(), Settings::default()).with_session_path(session);
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app.refresh_git();
    app
}

/// A repo with `files` committed and an app with the first file open.
pub(crate) fn app_with(tag: &str, files: &[(&str, &str)]) -> (PathBuf, App) {
    let d = repo(tag, files);
    let mut app = app_at(&d);
    if let Some((first, _)) = files.first() {
        app.open_initial(&d.join(first));
    }
    app.refresh_git();
    (d, app)
}

pub(crate) fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

pub(crate) fn ch(c: char) -> KeyEvent {
    key(KeyCode::Char(c))
}

pub(crate) fn click(col: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: col,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

/// Replace the active buffer's text, as if the user had typed it.
pub(crate) fn set_text(app: &mut App, text: &str) {
    let t = app.editor.active_tab_mut().unwrap();
    t.editor.set_content(text);
    t.dirty = true;
}

pub(crate) fn text(app: &App) -> String {
    app.editor.active_tab().unwrap().text()
}

pub(crate) fn cursor_line(app: &App) -> usize {
    app.editor.active_tab().unwrap().editor.cursor_line()
}

pub(crate) fn goto_line(app: &mut App, line: usize) {
    app.editor
        .active_tab_mut()
        .unwrap()
        .editor
        .set_cursor_line(line);
}

pub(crate) fn last_error(app: &App) -> String {
    app.messages
        .items
        .last()
        .map(|m| m.text.clone())
        .unwrap_or_default()
}

pub(crate) fn dock_has(app: &App, line: &str) -> bool {
    app.bottom_dock.lines.iter().any(|l| l == line)
}

const FIVE: &str = "one\ntwo\nthree\nfour\nfive\n";
const FIVE_EDITED: &str = "one\nTWO\nthree\nfour\nFIVE\n";

// ----- repo guard, gutter ---------------------------------------------------

#[test]
fn outside_a_repo_the_guard_reports_and_actions_bail() {
    let d = temp("norepo");
    let mut app = app_at(&d);
    assert!(!app.require_git_repo());
    assert_eq!(app.status, t!("status.git_not_repo").to_string());
    for action in [
        "git.push",
        "git.fetch",
        "git.switch_branch",
        "git.merge_branch",
        "git.gutter_base",
        "git.new_branch",
        "git.log",
        "git.log_graph",
        "git.log_since_1_day_ago",
        "git.browse_log",
        "git.file_history",
        "git.open_at_revision",
        "git.status",
        "git.blame",
        "git.diff_head",
    ] {
        app.status.clear();
        assert!(app.run_git_action(action), "{action}");
        assert_eq!(app.status, t!("status.git_not_repo").to_string(), "{action}");
        assert!(app.running_command.is_none(), "{action}");
        assert!(app.prompt.is_none(), "{action}");
    }
    app.status.clear();
    app.run_git_action("git.stash");
    assert_eq!(app.status, t!("status.git_not_repo").to_string());
    assert!(!app.run_git_action("git.no_such_action"));
    // The changes panel refuses outside a repo too.
    app.status.clear();
    app.run_git_action("git.changes");
    assert!(app.git_panel.is_none());
    assert_eq!(app.status, t!("status.git_not_repo").to_string());
    assert!(!app.git_dirty());
    assert_eq!(app.git_branch, None);
}

#[test]
fn gutter_is_cleared_outside_a_repo_and_for_unsaved_buffers() {
    let d = temp("gutter-none");
    let mut app = app_at(&d);
    app.editor.new_tab_with_content("hello\n");
    app.refresh_git_gutter();
    assert!(app.editor.active_tab().unwrap().editor.gutter_marks().is_none_or(|m| m.is_empty()));

    let (_d, mut app) = app_with("gutter-unsaved", &[("a.txt", FIVE)]);
    app.editor.new_tab_with_content("scratch\n");
    app.refresh_git_gutter();
    assert!(app.editor.active_tab().unwrap().editor.gutter_marks().is_none_or(|m| m.is_empty()));
}

#[test]
fn gutter_base_choices_change_what_the_gutter_compares_against() {
    let (d, mut app) = app_with("gutter-base", &[("a.txt", FIVE)]);
    set_text(&mut app, FIVE_EDITED);
    app.refresh_git_gutter();
    let marked = |app: &App| {
        app.editor
            .active_tab()
            .unwrap()
            .editor
            .gutter_marks()
            .map_or(0, |m| m.len())
    };
    assert_eq!(marked(&app), 2, "two lines differ from HEAD");

    // Stage the edit: against the index the buffer is clean.
    std::fs::write(d.join("a.txt"), FIVE_EDITED).unwrap();
    git(&d, &["add", "a.txt"]);
    app.set_gutter_base("index");
    assert_eq!(app.settings.git_gutter_base, "index");
    assert_eq!(app.status, t!("status.gutter_base_set", base = "index").to_string());
    app.refresh_git_gutter();
    assert_eq!(marked(&app), 0);

    // Disk: the buffer matches the file just written.
    app.set_gutter_base("disk");
    app.refresh_git_gutter();
    assert_eq!(marked(&app), 0);

    // A branch base: the seed commit on `main`.
    app.set_gutter_base("branch:main");
    app.refresh_git_gutter();
    assert_eq!(marked(&app), 2);
}

#[test]
fn gutter_base_chooser_lists_builtins_and_branches_and_applies() {
    let (d, mut app) = app_with("gutter-chooser", &[("a.txt", FIVE)]);
    git(&d, &["branch", "topic"]);
    app.settings.git_gutter_base = "branch:topic".into();
    app.run_git_action("git.gutter_base");
    {
        let c = app.chooser.branch_mut().unwrap();
        assert_eq!(&c.branches[..3], ["head", "index", "disk"]);
        assert!(c.branches.contains(&"branch:topic".to_string()));
        assert_eq!(c.branches[c.selected], "branch:topic");
        assert!(c.gutter && !c.merge);
    }
    // Move to "head" and apply.
    app.branch_key(key(KeyCode::Down));
    app.branch_key(key(KeyCode::Up));
    app.chooser.branch_mut().unwrap().selected = 1;
    app.branch_key(key(KeyCode::Enter));
    assert!(app.chooser.branch_mut().is_none());
    assert_eq!(app.settings.git_gutter_base, "index");
}

// ----- hunks ---------------------------------------------------------------

#[test]
fn diff_navigation_wraps_in_both_directions() {
    let (_d, mut app) = app_with("diff-nav", &[("a.txt", FIVE)]);
    app.run_git_action("git.diff_next");
    assert_eq!(app.status, t!("status.no_changes").to_string());
    set_text(&mut app, FIVE_EDITED);
    goto_line(&mut app, 0);
    app.run_git_action("git.diff_next");
    assert_eq!(cursor_line(&app), 1);
    app.run_git_action("git.diff_next");
    assert_eq!(cursor_line(&app), 4);
    app.run_git_action("git.diff_next");
    assert_eq!(cursor_line(&app), 1, "wraps to the first hunk");
    app.run_git_action("git.diff_prev");
    assert_eq!(cursor_line(&app), 4, "wraps to the last hunk");
    app.run_git_action("git.diff_prev");
    assert_eq!(cursor_line(&app), 1);
}

#[test]
fn revert_hunk_restores_only_the_hunk_under_the_cursor() {
    let (_d, mut app) = app_with("revert", &[("a.txt", FIVE)]);
    app.run_git_action("git.revert_hunk");
    assert_eq!(app.status, t!("status.no_changes").to_string());
    set_text(&mut app, FIVE_EDITED);
    goto_line(&mut app, 2);
    app.run_git_action("git.revert_hunk");
    assert_eq!(app.status, t!("status.no_hunk").to_string());
    assert_eq!(text(&app), FIVE_EDITED);
    goto_line(&mut app, 1);
    app.run_git_action("git.revert_hunk");
    assert_eq!(app.status, t!("status.hunk_reverted").to_string());
    assert_eq!(text(&app), "one\ntwo\nthree\nfour\nFIVE\n");
}

#[test]
fn stage_and_unstage_hunk_touch_only_the_index() {
    let (d, mut app) = app_with("stage-hunk", &[("a.txt", FIVE)]);
    app.run_git_action("git.stage_hunk");
    assert_eq!(app.status, t!("status.no_changes").to_string());
    set_text(&mut app, FIVE_EDITED);
    goto_line(&mut app, 2);
    app.run_git_action("git.stage_hunk");
    assert_eq!(app.status, t!("status.no_hunk").to_string());

    goto_line(&mut app, 1);
    app.run_git_action("git.stage_hunk");
    assert_eq!(app.status, t!("status.hunk_staged").to_string());
    assert_eq!(
        crate::git::index_blob(&d, "a.txt").unwrap(),
        "one\nTWO\nthree\nfour\nfive\n"
    );
    // The working buffer is untouched.
    assert_eq!(text(&app), FIVE_EDITED);

    // Editing the staged line again makes the HEAD region diverge from the index.
    goto_line(&mut app, 1);
    set_text(&mut app, "one\nTWO!\nthree\nfour\nFIVE\n");
    goto_line(&mut app, 1);
    app.run_git_action("git.stage_hunk");
    assert_eq!(
        app.status,
        t!("status.stage_hunk_failed", error = "index diverged").to_string()
    );

    // Restore the staged line's text and unstage it.
    set_text(&mut app, FIVE_EDITED);
    goto_line(&mut app, 1);
    app.run_git_action("git.unstage_hunk");
    assert_eq!(app.status, t!("status.hunk_unstaged").to_string());
    assert_eq!(crate::git::index_blob(&d, "a.txt").unwrap(), FIVE);

    // Unstaging a hunk that is not staged is refused.
    goto_line(&mut app, 1);
    app.run_git_action("git.unstage_hunk");
    assert_eq!(
        app.status,
        t!("status.unstage_hunk_failed", error = "hunk not staged").to_string()
    );
}

#[test]
fn hunk_actions_refuse_a_file_outside_the_workspace() {
    let (_d, mut app) = app_with("hunk-outside", &[("a.txt", FIVE)]);
    let outside = temp("hunk-outside-file").join("o.txt");
    std::fs::write(&outside, "x\n").unwrap();
    app.editor.new_tab_with_content("changed\n");
    app.editor.active_tab_mut().unwrap().path = Some(outside);
    app.run_git_action("git.stage_hunk");
    assert_eq!(
        app.status,
        t!("status.stage_hunk_failed", error = "outside workspace").to_string()
    );
}

// ----- conflicts -----------------------------------------------------------

const CONFLICTED: &str =
    "a\n<<<<<<< HEAD\nours1\n=======\ntheirs1\n>>>>>>> x\nmid\n<<<<<<< HEAD\nours2\n=======\ntheirs2\n>>>>>>> x\nz\n";

#[test]
fn conflict_resolution_keeps_the_chosen_side() {
    for (action, expected) in [
        (
            "git.conflict_ours",
            "a\nours1\nmid\n<<<<<<< HEAD\nours2\n=======\ntheirs2\n>>>>>>> x\nz\n",
        ),
        (
            "git.conflict_theirs",
            "a\ntheirs1\nmid\n<<<<<<< HEAD\nours2\n=======\ntheirs2\n>>>>>>> x\nz\n",
        ),
        (
            "git.conflict_both",
            "a\nours1\ntheirs1\nmid\n<<<<<<< HEAD\nours2\n=======\ntheirs2\n>>>>>>> x\nz\n",
        ),
    ] {
        let (_d, mut app) = app_with("conflict", &[("a.txt", "x\n")]);
        set_text(&mut app, CONFLICTED);
        goto_line(&mut app, 2);
        app.run_git_action(action);
        assert_eq!(text(&app), expected, "{action}");
        assert_eq!(app.status, t!("status.conflict_resolved").to_string());
        assert!(app.editor.active_tab().unwrap().dirty);
    }
}

#[test]
fn conflict_actions_report_when_there_is_no_conflict() {
    let (_d, mut app) = app_with("no-conflict", &[("a.txt", "x\ny\n")]);
    for action in ["git.conflict_ours", "git.conflict_next", "git.conflict_list"] {
        app.status.clear();
        app.run_git_action(action);
        assert_eq!(app.status, t!("status.no_conflict").to_string(), "{action}");
    }
    assert!(app.conflict_list.is_none());
}

#[test]
fn conflict_next_advances_and_wraps() {
    let (_d, mut app) = app_with("conflict-next", &[("a.txt", "x\n")]);
    set_text(&mut app, CONFLICTED);
    goto_line(&mut app, 0);
    app.run_git_action("git.conflict_next");
    assert_eq!(cursor_line(&app), 1);
    app.run_git_action("git.conflict_next");
    assert_eq!(cursor_line(&app), 7);
    app.run_git_action("git.conflict_next");
    assert_eq!(cursor_line(&app), 1, "wraps to the first conflict");
}

#[test]
fn conflict_overlay_navigates_jumps_and_resolves() {
    let (_d, mut app) = app_with("conflict-list", &[("a.txt", "x\n")]);
    set_text(&mut app, CONFLICTED);
    app.layout.conflict_list = Rect::new(0, 0, 40, 10);
    app.run_git_action("git.conflict_list");
    assert_eq!(app.conflict_list.as_ref().unwrap().entries.len(), 2);

    // Down then Enter jumps to the second conflict and closes the overlay.
    app.conflict_list_key(key(KeyCode::Down));
    assert_eq!(app.conflict_list.as_ref().unwrap().selected, 1);
    app.conflict_list_key(key(KeyCode::Enter));
    assert!(app.conflict_list.is_none());
    assert_eq!(cursor_line(&app), 7);
    assert!(matches!(app.focus, crate::app::Focus::Editor));

    // Resolve both with the keys; the overlay closes when none are left.
    app.run_git_action("git.conflict_list");
    app.conflict_list_key(ch('o'));
    assert_eq!(app.conflict_list.as_ref().unwrap().entries.len(), 1);
    app.conflict_list_key(ch('t'));
    assert!(app.conflict_list.is_none());
    assert_eq!(text(&app), "a\nours1\nmid\ntheirs2\nz\n");

    set_text(&mut app, CONFLICTED);
    app.run_git_action("git.conflict_list");
    app.conflict_list_key(ch('b'));
    assert!(text(&app).starts_with("a\nours1\ntheirs1\nmid\n"));
    app.conflict_list_key(key(KeyCode::Esc));
    assert!(app.conflict_list.is_none());
    // Keys with no overlay open do nothing harmful.
    app.conflict_list_key(ch('z'));
}

#[test]
fn clicking_a_conflict_row_selects_and_jumps() {
    let (_d, mut app) = app_with("conflict-click", &[("a.txt", "x\n")]);
    set_text(&mut app, CONFLICTED);
    app.layout.conflict_list = Rect::new(2, 3, 40, 10);
    app.run_git_action("git.conflict_list");
    app.conflict_list_mouse(click(0, 0)); // outside
    assert!(app.conflict_list.is_some());
    app.conflict_list_mouse(click(5, 4)); // second row
    assert!(app.conflict_list.is_none());
    assert_eq!(cursor_line(&app), 7);
}

// ----- Changes panel ----------------------------------------------------------

/// Repo with a modified tracked file and an untracked one, panel open.
fn panel_app(tag: &str) -> (PathBuf, App) {
    let d = repo(tag, &[("a.txt", "one\n"), ("b.txt", "two\n")]);
    let mut app = app_at(&d);
    std::fs::write(d.join("a.txt"), "one!\n").unwrap();
    std::fs::write(d.join("c.txt"), "new\n").unwrap();
    app.run_git_action("git.changes");
    app.layout.git_panel = Rect::new(0, 0, 40, 10);
    (d, app)
}

#[test]
fn the_panel_stages_unstages_and_navigates() {
    let (_d, mut app) = panel_app("panel-stage");
    assert_eq!(app.git_status.len(), 2);
    assert!(app.git_dirty());
    assert_eq!(app.git_branch.as_deref(), Some("main"));

    app.git_panel_key(key(KeyCode::Up)); // already at the top
    assert_eq!(app.git_panel.as_ref().unwrap().selected, 0);
    app.git_panel_key(ch('s'));
    assert!(app.git_status[0].is_staged());
    app.git_panel_key(ch('u'));
    assert!(!app.git_status[0].is_staged());
    app.git_panel_key(ch(' '));
    assert!(app.git_status[0].is_staged(), "space toggles to staged");
    app.git_panel_key(ch(' '));
    assert!(!app.git_status[0].is_staged(), "and back");

    app.git_panel_key(key(KeyCode::Down));
    assert_eq!(app.git_panel.as_ref().unwrap().selected, 1);
    app.git_panel_key(key(KeyCode::Down)); // clamped at the last row
    assert_eq!(app.git_panel.as_ref().unwrap().selected, 1);
    app.git_panel_key(ch('r'));
    assert_eq!(app.git_panel.as_ref().unwrap().selected, 1);
    app.git_panel_key(ch('?'));
    app.git_panel_key(key(KeyCode::Esc));
    assert!(app.git_panel.is_none());
}

#[test]
fn clicking_a_panel_row_selects_and_toggles_it() {
    let (_d, mut app) = panel_app("panel-click");
    app.git_panel_mouse(click(50, 50)); // outside
    assert!(!app.git_status[0].is_staged());
    app.git_panel_mouse(click(1, 1)); // row 1: c.txt
    assert_eq!(app.git_panel.as_ref().unwrap().selected, 1);
    assert!(app.git_status[1].is_staged());
    app.git_panel_mouse(click(1, 9)); // beyond the list
    assert_eq!(app.git_panel.as_ref().unwrap().selected, 1);
}

#[test]
fn the_panel_opens_a_diff_and_confirms_discards() {
    let (d, mut app) = panel_app("panel-diff");
    app.git_panel_key(key(KeyCode::Enter));
    let dv = app.diff_view.as_ref().unwrap();
    assert!(dv.title.contains("HEAD") && dv.title.contains("a.txt"));
    app.diff_view = None;

    // Discarding asks first; `n` cancels and leaves the file alone.
    app.git_panel_key(ch('d'));
    assert!(matches!(
        app.confirm.as_ref().map(|c| c.kind),
        Some(ConfirmKind::GitDiscard)
    ));
    app.git_confirm_key(ConfirmKind::GitDiscard, ch('n'));
    assert!(app.confirm.is_none());
    assert_eq!(std::fs::read_to_string(d.join("a.txt")).unwrap(), "one!\n");
    app.git_panel_key(ch('d'));
    app.git_confirm_key(ConfirmKind::GitDiscard, key(KeyCode::Esc));
    assert!(app.confirm.is_none());

    // `y` restores it.
    app.git_panel_key(ch('d'));
    app.git_confirm_key(ConfirmKind::GitDiscard, ch('x')); // ignored
    assert!(app.confirm.is_some());
    app.git_confirm_key(ConfirmKind::GitDiscard, ch('y'));
    assert_eq!(std::fs::read_to_string(d.join("a.txt")).unwrap(), "one\n");
    assert_eq!(app.status, t!("status.git_discarded", path = "a.txt").to_string());
    assert_eq!(app.git_status.len(), 1);

    // The remaining row is the untracked file: it is refused, not deleted.
    app.git_panel_key(key(KeyCode::Up));
    app.git_panel_key(ch('d'));
    assert_eq!(app.status, t!("status.git_discard_untracked").to_string());
    assert!(d.join("c.txt").exists());
}

#[test]
fn a_failed_discard_reports_an_error() {
    let (d, mut app) = panel_app("discard-fail");
    app.git_discard_confirmed(Path::new("does-not-exist.txt"));
    assert!(!last_error(&app).is_empty());
    assert_eq!(std::fs::read_to_string(d.join("a.txt")).unwrap(), "one!\n");
}

#[test]
fn identical_diff_reports_no_difference() {
    let (_d, mut app) = app_with("diff-same", &[("a.txt", FIVE)]);
    app.run_git_action("git.diff_head");
    assert_eq!(app.status, t!("status.diff_identical").to_string());
    assert!(app.diff_view.is_none());
    // The index diff of an edited, unstaged buffer shows the edit.
    set_text(&mut app, FIVE_EDITED);
    app.run_git_action("git.diff_index");
    let dv = app.diff_view.as_ref().unwrap();
    assert!(dv.title.contains("a.txt"));
    assert!(!dv.lines.is_empty());
}

#[test]
fn diff_without_an_active_file_reports_it() {
    let d = repo("diff-nofile", &[("a.txt", "x\n")]);
    let mut app = app_at(&d);
    app.run_git_action("git.diff_head");
    assert_eq!(app.status, t!("status.blame_no_file").to_string());
}

// ----- commit flow ---------------------------------------------------------------

#[test]
fn commit_prompt_requires_staged_changes_or_smart_commit() {
    // Clean tree: nothing staged.
    let (_d, mut app) = app_with("commit-clean", &[("a.txt", "x\n")]);
    app.run_git_action("git.changes");
    app.git_panel_key(ch('c'));
    assert_eq!(app.status, t!("status.git_nothing_staged").to_string());
    assert!(app.prompt.is_none());

    // Dirty and unstaged: "never" refuses, default asks, "always" stages all.
    for (mode, tag) in [("never", "sc-never"), ("ask", "sc-ask"), ("always", "sc-always")] {
        let (d, mut app) = app_with(tag, &[("a.txt", "x\n")]);
        std::fs::write(d.join("a.txt"), "y\n").unwrap();
        app.settings.smart_commit = mode.into();
        app.run_git_action("git.changes");
        app.git_panel_key(ch('c'));
        match mode {
            "never" => {
                assert!(app.prompt.is_none() && app.confirm.is_none());
                assert_eq!(app.status, t!("status.git_nothing_staged").to_string());
            }
            "ask" => {
                assert!(matches!(
                    app.confirm.as_ref().map(|c| c.kind),
                    Some(ConfirmKind::StageAllAndCommit)
                ));
                app.git_confirm_key(ConfirmKind::StageAllAndCommit, ch('n'));
                assert!(app.confirm.is_none());
                assert_eq!(app.status, t!("status.git_nothing_staged").to_string());
                app.git_panel_key(ch('c'));
                app.git_confirm_key(ConfirmKind::StageAllAndCommit, ch('y'));
                assert!(app.prompt.as_ref().unwrap().kind == PromptKind::GitCommit);
                assert!(app.git_status[0].is_staged());
            }
            _ => {
                assert!(app.prompt.as_ref().unwrap().kind == PromptKind::GitCommit);
                assert!(app.git_status[0].is_staged());
                assert!(app.git_panel.is_none());
            }
        }
    }
}

#[test]
fn committing_validates_the_message_and_reports_the_outcome() {
    let (d, mut app) = app_with("commit", &[("a.txt", "x\n")]);
    app.git_commit("   ");
    assert_eq!(app.status, t!("status.git_empty_message").to_string());

    // Nothing to commit: git fails, the error reaches the message drawer.
    let before = app.messages.items.len();
    app.git_commit("noop");
    assert_eq!(app.messages.items.len(), before + 1);
    assert!(last_error(&app).contains(&t!("msg.git_commit_failed", error = "").to_string()[..8]));

    std::fs::write(d.join("a.txt"), "y\n").unwrap();
    git(&d, &["add", "a.txt"]);
    app.git_commit(" second ");
    assert_eq!(app.status, t!("status.git_committed").to_string());
    assert_eq!(git(&d, &["log", "-1", "--format=%s"]).trim(), "second");
    assert!(!app.git_dirty());
}

#[test]
fn stage_all_failure_and_success_paths() {
    let (d, mut app) = app_with("stage-all", &[("a.txt", "x\n")]);
    std::fs::write(d.join("a.txt"), "y\n").unwrap();
    app.refresh_git();
    app.git_stage_all_then_commit();
    assert!(app.prompt.as_ref().unwrap().kind == PromptKind::GitCommit);
    // Not a repo any more: `git add -A` fails and is reported.
    let plain = temp("stage-all-fail");
    let mut app2 = app_at(&plain);
    app2.git_stage_all_then_commit();
    assert!(!last_error(&app2).is_empty());
}

#[test]
fn amend_and_stash_run_through_git_op() {
    let (d, mut app) = app_with("amend-stash", &[("a.txt", "x\n")]);
    app.run_git_action("git.amend");
    assert_eq!(app.status, t!("status.git_amended").to_string());

    std::fs::write(d.join("a.txt"), "y\n").unwrap();
    app.refresh_git();
    assert!(app.git_dirty());
    app.run_git_action("git.stash");
    assert_eq!(app.status, t!("status.git_stashed").to_string());
    assert!(!app.git_dirty());
    assert_eq!(std::fs::read_to_string(d.join("a.txt")).unwrap(), "x\n");
    app.run_git_action("git.stash_pop");
    assert_eq!(app.status, t!("status.git_stash_popped").to_string());
    assert!(app.git_dirty());
    // Popping with nothing stashed fails and says why.
    app.run_git_action("git.stash_pop");
    assert!(app.status.starts_with(&t!("msg.git_failed_reason", error = "").to_string()[..6]));
}

// ----- branches ---------------------------------------------------------------

#[test]
fn creating_and_switching_branches() {
    let (d, mut app) = app_with("branches", &[("a.txt", "x\n")]);
    app.run_git_action("git.new_branch");
    assert!(app.prompt.as_ref().unwrap().kind == PromptKind::GitNewBranch);
    app.git_create_branch("  ");
    assert_eq!(app.status, t!("status.git_empty_branch").to_string());
    app.git_create_branch("topic");
    assert_eq!(app.status, t!("status.git_switched", branch = "topic").to_string());
    assert_eq!(app.git_branch.as_deref(), Some("topic"));
    // A duplicate name fails with an error message.
    let before = app.messages.items.len();
    app.git_create_branch("topic");
    assert_eq!(app.messages.items.len(), before + 1);

    // Chooser: Up wraps to the last, Enter checks it out.
    app.run_git_action("git.switch_branch");
    let n = app.chooser.branch_mut().unwrap().branches.len();
    assert_eq!(n, 2);
    app.branch_key(key(KeyCode::Up));
    let sel = app.chooser.branch_mut().unwrap().selected;
    app.branch_key(key(KeyCode::Down));
    assert_ne!(app.chooser.branch_mut().unwrap().selected, sel);
    app.branch_key(ch('q')); // ignored
    let main = app
        .chooser
        .branch_mut()
        .unwrap()
        .branches
        .iter()
        .position(|b| b == "main")
        .unwrap();
    app.chooser.branch_mut().unwrap().selected = main;
    app.branch_key(key(KeyCode::Enter));
    assert!(app.chooser.branch_mut().is_none());
    assert_eq!(app.git_branch.as_deref(), Some("main"));
    assert_eq!(app.status, t!("status.git_switched", branch = "main").to_string());

    app.run_git_action("git.switch_branch");
    app.branch_key(key(KeyCode::Esc));
    assert!(app.chooser.branch_mut().is_none());
    drop(d);
}

#[test]
fn branch_checkout_reloads_clean_buffers_and_reports_it() {
    let (d, mut app) = app_with("checkout-reload", &[("a.txt", "x\n")]);
    git(&d, &["checkout", "-q", "-b", "other"]);
    std::fs::write(d.join("a.txt"), "other\n").unwrap();
    git(&d, &["commit", "-qam", "other"]);
    git(&d, &["checkout", "-q", "main"]);
    app.refresh_git();
    app.open_initial(&d.join("a.txt"));
    app.run_git_action("git.switch_branch");
    let idx = app
        .chooser
        .branch_mut()
        .unwrap()
        .branches
        .iter()
        .position(|b| b == "other")
        .unwrap();
    app.chooser.branch_mut().unwrap().selected = idx;
    app.branch_key(key(KeyCode::Enter));
    assert_eq!(text(&app), "other\n");
    assert!(
        app.messages
            .items
            .iter()
            .any(|m| m.text == t!("status.git_reloaded", count = 1).to_string())
    );
}

#[test]
fn branch_checkout_failure_and_merge_mode() {
    let (d, mut app) = app_with("checkout-fail", &[("a.txt", "x\n")]);
    git(&d, &["branch", "other"]);
    app.run_git_action("git.switch_branch");
    app.chooser.branch_mut().unwrap().branches = vec!["no-such-branch".into()];
    app.chooser.branch_mut().unwrap().selected = 0;
    let before = app.messages.items.len();
    app.branch_key(key(KeyCode::Enter));
    assert_eq!(app.messages.items.len(), before + 1);

    app.run_git_action("git.merge_branch");
    assert!(app.chooser.branch_mut().unwrap().merge);
    app.chooser.branch_mut().unwrap().selected = 0;
    app.branch_key(key(KeyCode::Enter));
    let branch = "main"; // first listed
    assert!(
        app.bottom_dock
            .lines
            .iter()
            .any(|l| l.starts_with("$ git merge ")),
        "{branch}: merge is streamed into the dock"
    );
    app.cancel_command();
}

#[test]
fn clicking_a_branch_row_checks_it_out() {
    let (d, mut app) = app_with("branch-click", &[("a.txt", "x\n")]);
    git(&d, &["branch", "other"]);
    app.run_git_action("git.switch_branch");
    app.layout.chooser = Rect::new(0, 0, 30, 5);
    app.branch_mouse(click(60, 60)); // outside
    assert!(app.chooser.branch_mut().is_some());
    app.branch_mouse(click(1, 4)); // beyond the rows
    assert!(app.chooser.branch_mut().is_some());
    let names = app.chooser.branch_mut().unwrap().branches.clone();
    let row = u16::try_from(names.iter().position(|b| b == "other").unwrap()).unwrap();
    app.branch_mouse(click(1, row));
    assert_eq!(app.git_branch.as_deref(), Some("other"));
}

#[test]
fn no_local_branches_reports_it() {
    let d = temp("no-branches");
    git(&d, &["init", "-q", "-b", "main"]);
    let mut app = app_at(&d);
    app.run_git_action("git.switch_branch");
    assert_eq!(app.status, t!("status.git_no_branches").to_string());
    assert!(app.chooser.branch_mut().is_none());
}

// ----- logs and history -----------------------------------------------------------

#[test]
fn log_actions_stream_their_commands() {
    let (_d, mut app) = app_with("log-cmds", &[("a.txt", "x\n")]);
    for (action, expect) in [
        ("git.log", "$ git --no-pager log"),
        ("git.log_since_1_day_ago", "$ git --no-pager log --since=1-day-ago"),
        ("git.log_since_1_week_ago", "$ git --no-pager log --since=1-week-ago"),
        ("git.log_since_1_month_ago", "$ git --no-pager log --since=1-month-ago"),
        ("git.status", "$ git --no-pager status"),
        ("git.push", "$ git push"),
        ("git.pull_ff", "$ git pull --ff-only"),
        ("git.pull_rebase", "$ git pull --rebase"),
        ("git.pull_merge", "$ git pull --no-rebase"),
        ("git.pull_squash", "$ git pull --squash"),
        ("git.fetch", "$ git fetch"),
    ] {
        app.cancel_command();
        app.running_command = None;
        app.bottom_dock.clear();
        app.run_git_action(action);
        assert!(dock_has(&app, expect), "{action}: {:?}", app.bottom_dock.lines);
    }
    app.cancel_command();
    app.running_command = None;
    app.bottom_dock.clear();
    app.run_git_action("git.log_graph");
    assert!(app.bottom_dock.lines[0].starts_with("$ git --no-pager log --graph"));
    app.cancel_command();
}

#[test]
fn init_refuses_an_existing_repo_and_otherwise_runs_git_init() {
    let (_d, mut app) = app_with("init-exists", &[("a.txt", "x\n")]);
    app.run_git_action("git.init");
    assert_eq!(app.status, t!("status.git_already_init").to_string());
    assert!(app.running_command.is_none());

    let plain = temp("init-new");
    let mut app = app_at(&plain);
    app.run_git_action("git.init");
    assert!(dock_has(&app, "$ git init"));
    app.cancel_command();
}

#[test]
fn browse_log_lists_commits_and_opens_a_diff() {
    let (d, mut app) = app_with("browse-log", &[("a.txt", "x\n")]);
    std::fs::write(d.join("a.txt"), "y\n").unwrap();
    git(&d, &["commit", "-qam", "second"]);
    app.layout.editor = Rect::new(0, 0, 80, 24);
    app.layout.git_log = Rect::new(0, 0, 60, 10);
    app.run_git_action("git.browse_log");
    assert_eq!(app.git_log.as_ref().unwrap().entries.len(), 2);
    app.git_log_key(key(KeyCode::Down));
    assert_eq!(app.git_log.as_ref().unwrap().selected, 1);
    app.git_log_key(key(KeyCode::Up));
    app.git_log_key(key(KeyCode::PageDown));
    app.git_log_key(key(KeyCode::PageUp));
    app.git_log_key(ch('c')); // only meaningful in the pick scope
    assert!(app.git_log.is_some());
    app.git_log_key(key(KeyCode::Enter));
    assert!(app.git_log.is_none());
    let tab = app.editor.active_tab().unwrap();
    assert!(tab.read_only);
    assert!(tab.title().ends_with(" log"), "{}", tab.title());
    assert!(tab.text().contains("second"));

    app.run_git_action("git.browse_log");
    app.git_log_key(key(KeyCode::Esc));
    assert!(app.git_log.is_none());
    app.git_log_key(key(KeyCode::Enter)); // nothing open: no-op
}

#[test]
fn clicking_a_log_row_opens_that_commit() {
    let (d, mut app) = app_with("log-click", &[("a.txt", "x\n")]);
    std::fs::write(d.join("a.txt"), "y\n").unwrap();
    git(&d, &["commit", "-qam", "second"]);
    app.layout.git_log = Rect::new(0, 0, 60, 10);
    app.run_git_action("git.browse_log");
    app.git_log_mouse(click(70, 1)); // outside
    assert!(app.git_log.is_some());
    app.git_log_mouse(click(1, 8)); // beyond the entries
    assert!(app.git_log.is_some());
    app.git_log_mouse(click(1, 1));
    assert!(app.git_log.is_none());
    assert!(app.editor.active_tab().unwrap().text().contains("seed"));
}

#[test]
fn file_history_scopes_to_the_active_file() {
    let (d, mut app) = app_with("file-history", &[("a.txt", "x\n"), ("b.txt", "b\n")]);
    std::fs::write(d.join("a.txt"), "y\n").unwrap();
    git(&d, &["commit", "-qam", "touch a"]);
    app.run_git_action("git.file_history");
    assert_eq!(app.git_log.as_ref().unwrap().entries.len(), 2);
    app.git_log_key(key(KeyCode::Enter));
    let tab = app.editor.active_tab().unwrap();
    assert!(tab.title().ends_with(" a.txt"), "{}", tab.title());
    assert!(tab.text().contains("a.txt"));

    // A file with no history yet (untracked) says so.
    std::fs::write(d.join("new.txt"), "n\n").unwrap();
    app.open_initial(&d.join("new.txt"));
    app.status.clear();
    app.run_git_action("git.file_history");
    assert_eq!(app.status, t!("status.git_no_file_history").to_string());
    assert!(app.git_log.is_none());
}

#[test]
fn file_history_and_revision_open_need_an_active_file() {
    let d = repo("history-nofile", &[("a.txt", "x\n")]);
    let mut app = app_at(&d);
    app.run_git_action("git.file_history");
    assert!(app.git_log.is_none());
    app.run_git_action("git.open_at_revision");
    assert!(app.prompt.is_none());
    let tabs = app.editor.tabs.len();
    app.git_open_at_revision("HEAD");
    assert_eq!(app.editor.tabs.len(), tabs, "no file, so nothing opens");
}

#[test]
fn open_file_at_revision_prompts_and_opens_read_only() {
    let (d, mut app) = app_with("at-revision", &[("a.txt", "old\n")]);
    std::fs::write(d.join("a.txt"), "new\n").unwrap();
    git(&d, &["commit", "-qam", "second"]);
    app.run_git_action("git.open_at_revision");
    assert!(app.prompt.as_ref().unwrap().kind == PromptKind::GitOpenAtRevision);
    app.git_open_at_revision("");
    app.git_open_at_revision("HEAD~1");
    let tab = app.editor.active_tab().unwrap();
    assert!(tab.read_only);
    assert!(tab.title().contains("a.txt @ "), "{}", tab.title());
    assert_eq!(tab.text(), "old\n");

    app.git_open_at_revision("no-such-rev");
    assert_eq!(
        last_error(&app),
        t!("msg.git_revision_not_found", revision = "no-such-rev").to_string()
    );
}

// ----- clone / description / delete / grep -------------------------------------------

#[test]
fn clone_prompt_and_validation() {
    let (_d, mut app) = app_with("clone", &[("a.txt", "x\n")]);
    app.run_git_action("git.clone");
    assert!(app.prompt.as_ref().unwrap().kind == PromptKind::GitClone);
    app.git_clone("  ");
    assert_eq!(app.status, t!("status.git_empty_url").to_string());
    app.git_clone("--upload-pack=x");
    assert_eq!(app.status, t!("status.git_bad_argument").to_string());
    assert!(app.running_command.is_none());
    app.git_clone("https://example.invalid/r.git");
    assert!(dock_has(
        &app,
        "$ git -c protocol.ext.allow=never -c protocol.fd.allow=never clone -- https://example.invalid/r.git"
    ));
    app.cancel_command();
}

#[test]
fn simple_prompt_actions_open_their_prompts() {
    let (_d, mut app) = app_with("prompts", &[("a.txt", "x\n")]);
    for (action, kind) in [
        ("git.edit_description", PromptKind::GitEditDescription),
        ("git.delete_branch", PromptKind::GitDeleteBranch),
        ("git.grep", PromptKind::GitGrep),
    ] {
        app.prompt = None;
        app.run_git_action(action);
        assert!(app.prompt.as_ref().unwrap().kind == kind, "{action}");
    }
}

#[test]
fn delete_branch_and_grep_validate_and_run() {
    let (_d, mut app) = app_with("delete-grep", &[("a.txt", "x\n")]);
    app.git_delete_branch("  ");
    assert!(app.running_command.is_none());
    app.git_delete_branch("-d");
    assert_eq!(app.status, t!("status.git_bad_argument").to_string());
    app.git_delete_branch("topic");
    assert!(dock_has(&app, "$ git branch --delete -- topic"));
    app.cancel_command();
    app.running_command = None;
    app.bottom_dock.clear();

    app.git_grep("");
    assert!(app.running_command.is_none());
    app.git_grep("a\nb");
    assert_eq!(app.status, t!("status.git_bad_argument").to_string());
    app.git_grep("-foo");
    assert!(dock_has(&app, "$ git --no-pager grep -n -e -foo"));
    app.cancel_command();

    let plain = temp("grep-norepo");
    let mut app = app_at(&plain);
    app.git_delete_branch("x");
    app.git_grep("x");
    assert_eq!(app.status, t!("status.git_not_repo").to_string());
    app.git_edit_description("desc");
    assert!(app.running_command.is_none());
}

#[test]
fn edit_description_feeds_text_through_a_throwaway_editor() {
    let (_d, mut app) = app_with("description", &[("a.txt", "x\n")]);
    app.git_edit_description("my description");
    assert!(
        app.bottom_dock
            .lines
            .iter()
            .any(|l| l.contains("git branch --edit-description") && l.contains("GIT_EDITOR=")),
        "{:?}",
        app.bottom_dock.lines
    );
    app.cancel_command();
}

// ----- blame ---------------------------------------------------------------------------

#[test]
fn blame_reports_committed_uncommitted_and_missing() {
    let (_d, mut app) = app_with("blame", &[("a.txt", "one\ntwo\n")]);
    goto_line(&mut app, 0);
    app.run_git_action("git.blame");
    assert!(app.status.contains("Test"), "{}", app.status);
    assert!(app.status.contains("seed"), "{}", app.status);

    set_text(&mut app, "one\nINSERTED\ntwo\n");
    goto_line(&mut app, 1);
    app.run_git_action("git.blame");
    assert_eq!(app.status, t!("status.blame_uncommitted", line = 2).to_string());

    goto_line(&mut app, 9);
    app.run_git_action("git.blame");
    assert!(!app.status.is_empty());
}

#[test]
fn blame_without_a_file_or_for_an_untracked_file() {
    let d = repo("blame-nofile", &[("a.txt", "x\n")]);
    let mut app = app_at(&d);
    app.run_git_action("git.blame");
    assert_eq!(app.status, t!("status.blame_no_file").to_string());
    std::fs::write(d.join("u.txt"), "u\n").unwrap();
    app.open_initial(&d.join("u.txt"));
    app.run_git_action("git.blame");
    assert_eq!(app.status, t!("status.blame_none").to_string());
}

/// Draw one frame and flatten it to text.
pub(crate) fn render(app: &mut App) -> String {
    let mut term =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    term.draw(|f| crate::ui::draw(app, f)).unwrap();
    term.backend()
        .buffer()
        .content
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect()
}

#[test]
fn inline_blame_toggles_and_follows_the_cursor() {
    let (_d, mut app) = app_with("inline-blame", &[("a.txt", "one\ntwo\n")]);
    app.refresh_inline_blame(); // off: nothing happens
    assert!(app.blame_cache.is_none());

    app.run_git_action("git.blame_inline");
    assert_eq!(app.status, t!("status.inline_blame_on").to_string());
    app.refresh_inline_blame();
    assert_eq!(app.blame_cache.as_ref().map(|c| c.1), Some(1));
    assert!(render(&mut app).contains("seed"), "committed note drawn");
    app.refresh_inline_blame(); // cached: same line
    goto_line(&mut app, 1);
    app.refresh_inline_blame();
    assert_eq!(app.blame_cache.as_ref().map(|c| c.1), Some(2));

    set_text(&mut app, "one\ntwo\nthree\n");
    goto_line(&mut app, 2);
    app.refresh_inline_blame();
    assert!(render(&mut app).contains(&t!("blame.uncommitted").to_string()));

    // Turning it off clears the annotation at once.
    app.run_git_action("git.blame_inline");
    assert_eq!(app.status, t!("status.inline_blame_off").to_string());
    assert!(app.blame_cache.is_none());
    assert!(!render(&mut app).contains(&t!("blame.uncommitted").to_string()));

    // A path-less buffer has nothing to blame.
    app.run_git_action("git.blame_inline");
    app.editor.new_tab_with_content("scratch\n");
    app.refresh_inline_blame();
    // Turning it off with a cache held drops the cache and the note.
    app.blame_cache = Some((PathBuf::from("x"), 1));
    app.settings.subsystems.inline_blame = false;
    app.refresh_inline_blame();
    assert!(app.blame_cache.is_none());
}

#[test]
fn git_change_for_maps_status_to_a_file() {
    let (d, mut app) = app_with("change-for", &[("a.txt", "x\n"), ("b.txt", "b\n")]);
    std::fs::write(d.join("a.txt"), "y\n").unwrap();
    std::fs::write(d.join("n.txt"), "n\n").unwrap();
    app.refresh_git();
    assert_eq!(
        app.git_change_for(&d.join("a.txt")),
        Some(crate::git::Change::Modified)
    );
    assert_eq!(
        app.git_change_for(&d.join("n.txt")),
        Some(crate::git::Change::Untracked)
    );
    assert_eq!(app.git_change_for(&d.join("b.txt")), None);
    assert_eq!(app.git_change_for(Path::new("/elsewhere/a.txt")), None);
    let plain = temp("change-for-plain");
    assert_eq!(app_at(&plain).git_change_for(&plain.join("a.txt")), None);
}

#[test]
fn error_text_has_an_install_hint_for_a_missing_git() {
    assert_eq!(
        git_error_text(&crate::git::GitError::NotInstalled),
        t!("msg.git_not_installed").to_string()
    );
}
