//! Round-4 behavior tests for `app/compare.rs`, `app/doc_links.rs`,
//! `app/keymap.rs` and `app/session.rs` (test-only).

use super::*;
use crate::app::cov4::{app_at, buffer_with, code, content, key, temp};

/// Write `old.txt` and `new.txt` into a fresh workspace and return it.
fn two_files(tag: &str, old: &str, new: &str) -> (App, std::path::PathBuf) {
    let dir = temp(tag);
    std::fs::write(dir.join("old.txt"), old).unwrap();
    std::fs::write(dir.join("new.txt"), new).unwrap();
    (app_at(&dir), dir)
}

fn k(c: KeyCode) -> KeyEvent {
    code(c)
}

fn cursor_line(app: &App) -> usize {
    app.editor.active_tab().unwrap().editor.cursor_line()
}

// ----- compare ---------------------------------------------------------

/// A file that cannot be opened is reported and no comparison starts, for
/// either side.
#[test]
fn comparing_an_unopenable_file_reports_and_opens_no_split() {
    let (mut a, dir) = two_files("r4-cmp-bad", "x\n", "y\n");
    let missing = dir.join("no-such-dir").join("gone.txt");
    a.open_compare_files(&missing, &dir.join("new.txt"));
    assert!(a.compare.is_none());
    assert!(a.editor.split_root.is_none());
    let before = a.messages.items.len();
    a.open_compare_files(&dir.join("old.txt"), &missing);
    assert!(a.compare.is_none());
    assert!(a.editor.split_root.is_none());
    assert!(a.messages.items.len() > before, "the failure is logged");
}

/// Comparing a file with itself, or two files with equal text, says they are
/// identical; only the second shape opens a split.
#[test]
fn identical_comparisons_say_so() {
    let (mut a, dir) = two_files("r4-cmp-same", "same\n", "same\n");
    a.open_compare_files(&dir.join("old.txt"), &dir.join("old.txt"));
    assert_eq!(a.status, t!("status.diff_identical").to_string());
    assert!(a.compare.is_none() && a.editor.split_root.is_none());

    a.open_compare_files(&dir.join("old.txt"), &dir.join("new.txt"));
    assert_eq!(a.status, t!("status.diff_identical").to_string());
    assert!(a.compare.is_some(), "the equal files are still compared");
    assert!(a.editor.split_root.is_some());
    // Nothing to jump to, nothing to copy, and nothing to show unified.
    a.run_action("diff.next");
    assert_eq!(a.status, t!("status.compare_no_more").to_string());
    a.run_action("diff.prev");
    assert_eq!(a.status, t!("status.compare_no_more").to_string());
    a.run_action("diff.copy_to_right");
    assert_eq!(a.status, t!("status.compare_no_change_here").to_string());
    a.run_action("diff.unified");
    assert_eq!(a.status, t!("status.diff_identical").to_string());
    assert!(a.diff_view.is_none());
}

/// Without a comparison every `diff.*` action says none is active, and an
/// unrelated action id is not claimed.
#[test]
fn compare_actions_without_a_comparison_say_so() {
    let mut a = app_at(&temp("r4-cmp-none"));
    for id in [
        "diff.next",
        "diff.prev",
        "diff.copy_to_left",
        "diff.copy_to_right",
        "diff.unified",
        "diff.close",
    ] {
        a.status.clear();
        a.run_action(id);
        assert_eq!(a.status, t!("status.compare_inactive").to_string(), "{id}");
    }
    assert!(!a.run_compare_action("diff.bogus"));
    assert!(!a.refresh_compare());
}

/// The unified view lists the differences under both file names; closing the
/// comparison clears the split and the gutters but keeps both tabs.
#[test]
fn unified_view_and_close() {
    let (mut a, dir) = two_files("r4-cmp-uni", "a\nb\n", "a\nB\n");
    a.open_compare_files(&dir.join("old.txt"), &dir.join("new.txt"));
    assert_eq!(a.status, t!("status.compare_opened").to_string());
    a.run_action("diff.unified");
    let view = a.diff_view.as_ref().expect("unified overlay");
    assert!(view.title.contains("old.txt") && view.title.contains("new.txt"));
    assert!(
        view.lines.len() >= 2,
        "a removed and an added line at least"
    );
    a.diff_view = None;

    let tabs = a.editor.tabs.len();
    a.run_action("diff.close");
    assert_eq!(a.status, t!("status.compare_closed").to_string());
    assert!(a.compare.is_none() && a.editor.split_root.is_none());
    assert_eq!(a.editor.tabs.len(), tabs);
    assert!(
        a.editor
            .tabs
            .iter()
            .all(|t| t.editor.gutter_marks().is_none_or(Vec::is_empty))
    );
}

/// Closing either compared tab ends the comparison the next time it is
/// consulted.
#[test]
fn closing_a_compared_tab_ends_the_comparison() {
    let (mut a, dir) = two_files("r4-cmp-closed", "a\nb\n", "a\nB\n");
    a.open_compare_files(&dir.join("old.txt"), &dir.join("new.txt"));
    assert!(a.compare.is_some());
    let right = a
        .editor
        .tabs
        .iter()
        .position(|t| t.path.as_deref().and_then(Path::file_name) == Some("new.txt".as_ref()))
        .unwrap();
    a.editor.tabs.remove(right);
    a.editor.active = 0;
    assert!(!a.refresh_compare());
    assert!(a.compare.is_none());
    a.run_action("diff.next");
    assert_eq!(a.status, t!("status.compare_inactive").to_string());
}

/// Copying into a read-only side is refused with the read-only message, and
/// the target text is unchanged.
#[test]
fn copying_into_a_read_only_side_is_refused() {
    let (mut a, dir) = two_files("r4-cmp-ro", "a\nb\n", "a\nB\n");
    a.open_compare_files(&dir.join("old.txt"), &dir.join("new.txt"));
    a.run_action("diff.next");
    let right = a
        .editor
        .tabs
        .iter()
        .position(|t| t.path.as_deref().and_then(Path::file_name) == Some("new.txt".as_ref()))
        .unwrap();
    a.editor.tabs[right].read_only = true;
    a.run_action("diff.copy_to_right");
    assert_eq!(a.status, t!("status.read_only_blocked").to_string());
    assert_eq!(a.editor.tabs[right].text(), "a\nB\n");
}

/// Focusing a tab outside the comparison reports that none is active here.
#[test]
fn an_unrelated_tab_is_not_part_of_the_comparison() {
    let (mut a, dir) = two_files("r4-cmp-third", "a\nb\n", "a\nB\n");
    std::fs::write(dir.join("other.txt"), "z\n").unwrap();
    a.open_compare_files(&dir.join("old.txt"), &dir.join("new.txt"));
    a.open_path(&dir.join("other.txt"), false);
    assert!(!a.refresh_compare());
    a.run_action("diff.next");
    assert_eq!(a.status, t!("status.compare_inactive").to_string());
    // The comparison itself is still open for when its tabs are focused again.
    assert!(a.compare.is_some());
    a.run_action("diff.unified");
    assert!(a.diff_view.is_some(), "unified needs only the two tabs");
    assert_eq!(cursor_line(&a), 0);
}

// ----- session ---------------------------------------------------------

/// Applying a recorded session reopens the files, restores cursors and the
/// active tab, drops the blank tab, and skips files that no longer exist.
#[test]
fn applying_a_session_reopens_files_and_ignores_missing_ones() {
    let dir = temp("r4-sess-apply");
    std::fs::write(dir.join("a.txt"), "alpha\nbeta\n").unwrap();
    std::fs::write(dir.join("b.txt"), "gamma\n").unwrap();
    let mut a = app_at(&dir);
    let ws = crate::session::WorkspaceSession {
        files: [dir.join("a.txt"), dir.join("gone.txt"), dir.join("b.txt")]
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
        cursors: vec![3, 0, 99],
        scrolls: vec![0, 0, 0],
        active: 7,
        ..crate::session::WorkspaceSession::default()
    };
    let opened = a.apply_session(&ws);
    assert_eq!(opened, 2);
    assert_eq!(a.editor.tabs.len(), 2);
    assert!(
        a.editor.tabs.iter().all(|t| t.path.is_some()),
        "blank tab dropped"
    );
    assert_eq!(a.editor.active, 1, "active clamped to the last tab");
    let first = &a.editor.tabs[0];
    assert_eq!(first.editor.get_cursor(), 3);
    assert_eq!(
        a.editor.tabs[1].editor.get_cursor(),
        "gamma\n".chars().count().min(99),
        "cursor clamped into the file"
    );
    assert_eq!(content(&mut a), "gamma\n");
}

// ----- app.rs ----------------------------------------------------------

fn error_count(app: &App) -> usize {
    app.messages
        .items
        .iter()
        .filter(|m| m.level == crate::messages::Level::Error)
        .count()
}

/// Folding where nothing can fold says so, for toggle, fold and unfold.
#[test]
fn folding_without_a_foldable_range_reports_it() {
    let mut a = app_at(&temp("r4-fold"));
    buffer_with(&mut a, "one\ntwo\n", 0);
    for id in ["editor.fold_toggle", "editor.fold", "editor.unfold"] {
        a.status.clear();
        a.run_action(id);
        assert_eq!(a.status, t!("status.no_fold").to_string(), "{id}");
    }
}

/// The comment banner borders the line with rules at least eight wide, uses a
/// placeholder title for an empty line, and refuses a read-only buffer.
#[test]
fn comment_banner_shapes_the_current_line() {
    let mut a = app_at(&temp("r4-banner"));
    buffer_with(&mut a, "  Setup\nrest", 0);
    let p = a.editor.active_tab().unwrap().editor.comment_prefix();
    assert_ne!(p, "");
    a.run_action("edit.comment_banner");
    assert_eq!(
        content(&mut a),
        format!("  {p} ========\n  {p} Setup\n  {p} ========\nrest")
    );
    assert!(a.editor.active_tab().unwrap().dirty);

    buffer_with(&mut a, "\nrest", 0);
    a.run_action("edit.comment_banner");
    assert_eq!(
        content(&mut a),
        format!("{p} ========\n{p} Section\n{p} ========\nrest")
    );

    let long = "x".repeat(70);
    buffer_with(&mut a, &long, 0);
    a.run_action("edit.comment_banner");
    assert!(content(&mut a).starts_with(&format!("{p} {}\n", "=".repeat(60))));

    buffer_with(&mut a, "locked", 0);
    a.editor.active_tab_mut().unwrap().read_only = true;
    a.run_action("edit.comment_banner");
    assert_eq!(a.status, t!("status.read_only_blocked").to_string());
    assert_eq!(content(&mut a), "locked");
}

/// An abbreviation Emmet cannot parse leaves the text alone and says so.
#[test]
fn emmet_with_nothing_to_expand_says_so() {
    let mut a = app_at(&temp("r4-emmet"));
    buffer_with(&mut a, "(((", 3);
    a.run_action("edit.emmet_expand");
    assert_eq!(a.status, t!("status.emmet_none").to_string());
    assert_eq!(content(&mut a), "(((");
    buffer_with(&mut a, "ul>li*2", 7);
    a.run_action("edit.emmet_expand");
    assert_eq!(a.status, t!("status.emmet_expanded").to_string());
    assert!(content(&mut a).contains("<li>"));
}

/// Buffer-word completion with no prefix or no candidates says so.
#[test]
fn autocomplete_without_candidates_says_so() {
    let mut a = app_at(&temp("r4-autocomplete"));
    buffer_with(&mut a, "alpha ", 6);
    a.run_action("autocomplete");
    assert_eq!(a.status, t!("status.no_completions").to_string());
    buffer_with(&mut a, "alpha zzz", 9);
    a.status.clear();
    a.run_action("autocomplete");
    assert_eq!(a.status, t!("status.no_completions").to_string());
    assert_eq!(content(&mut a), "alpha zzz");
}

/// Enter stops a running countdown and keeps the dialog; Esc cancels a
/// running one and drops the timer; a tick that does not reach the end of the
/// phase changes nothing but the remaining time.
#[test]
fn pomodoro_stop_cancel_and_partial_ticks() {
    use crate::pomodoro_tool::Phase;
    let mut a = app_at(&temp("r4-pomo"));
    a.run_action("tools.pomodoro");
    a.pomodoro_primary(); // start: closes the dialog, runs in the background
    assert!(a.pomodoro_running());
    assert!(!a.visible.contains(Visible::POMODORO));

    // 5 s later: just the remaining time moves.
    a.pomodoro_last_tick = std::time::Instant::now().checked_sub(std::time::Duration::from_secs(5));
    let before = a.pomodoro.as_ref().unwrap().remaining;
    a.poll_pomodoro();
    assert_eq!(a.pomodoro.as_ref().unwrap().remaining, before - 5);
    assert_eq!(a.pomodoro.as_ref().unwrap().phase, Phase::Work);

    // Enter while running stops it and keeps the timer around.
    a.visible.set(Visible::POMODORO, true);
    a.pomodoro_key(k(KeyCode::Enter));
    assert_eq!(a.pomodoro.as_ref().unwrap().phase, Phase::Idle);
    assert!(a.visible.contains(Visible::POMODORO));

    // Esc while idle drops the timer and closes; Esc with no timer is fine.
    a.pomodoro_key(k(KeyCode::Esc));
    assert!(a.pomodoro.is_none());
    assert!(!a.visible.contains(Visible::POMODORO));
    a.pomodoro_key(k(KeyCode::Esc));
    a.pomodoro_primary();
    assert!(a.pomodoro.is_none());

    // Esc during a run cancels it.
    a.run_action("tools.pomodoro");
    a.pomodoro_primary();
    a.visible.set(Visible::POMODORO, true);
    a.pomodoro_key(k(KeyCode::Esc));
    assert!(a.pomodoro.is_none() && !a.pomodoro_running());
}

/// The work length refuses to leave its 1-180 minute range and says why.
#[test]
fn pomodoro_length_out_of_range_is_reported() {
    let mut a = app_at(&temp("r4-pomo-range"));
    a.run_action("tools.pomodoro");
    a.pomodoro_key(k(KeyCode::Down));
    for _ in 0..40 {
        a.pomodoro_key(k(KeyCode::Down));
    }
    let minutes = a.pomodoro.as_ref().unwrap().work_minutes();
    assert!(minutes >= 1);
    let before = error_count(&a);
    for _ in 0..200 {
        a.pomodoro_key(k(KeyCode::Down));
    }
    assert_eq!(a.pomodoro.as_ref().unwrap().work_minutes(), 1);
    assert!(error_count(&a) > before, "the refused step is logged");
    for _ in 0..200 {
        a.pomodoro_key(k(KeyCode::Up));
    }
    assert_eq!(a.pomodoro.as_ref().unwrap().work_minutes(), 180);
}

/// The keybinding editor's keys do nothing once it is closed.
#[test]
fn keybinding_editor_keys_without_the_panel_are_harmless() {
    let mut a = app_at(&temp("r4-kbe"));
    assert!(a.keybinding_editor.is_none());
    for c in [
        KeyCode::Up,
        KeyCode::Down,
        KeyCode::PageUp,
        KeyCode::PageDown,
        KeyCode::Enter,
        KeyCode::Delete,
        KeyCode::Backspace,
        KeyCode::Char('x'),
    ] {
        a.keybinding_editor_key(k(c));
    }
    assert!(a.keybinding_editor.is_none());
    a.reset_selected_keybinding();
    a.refresh_keybinding_editor();
}

/// A save that cannot write names the file in the error and keeps the buffer
/// dirty.
#[test]
fn a_failed_save_names_the_path_and_keeps_the_buffer_dirty() {
    let dir = temp("r4-save-fail");
    let mut a = app_at(&dir);
    buffer_with(&mut a, "unsaved", 0);
    let target = dir.join("missing-dir").join("x.txt");
    {
        let t = a.editor.active_tab_mut().unwrap();
        t.path = Some(target.clone());
        t.dirty = true;
    }
    let before = error_count(&a);
    a.run_action("file.save");
    assert_eq!(error_count(&a), before + 1);
    let last = a.messages.items.last().unwrap().text.clone();
    assert!(last.contains("x.txt"), "{last}");
    assert!(a.editor.active_tab().unwrap().dirty);
    assert!(!target.exists());
}

/// Naming a macro with nothing recorded, or an empty name, saves nothing.
#[test]
fn saving_a_macro_needs_a_name_and_recorded_keys() {
    let mut a = app_at(&temp("r4-macro"));
    a.status = "untouched".to_string();
    a.macro_keys.clear();
    a.save_macro("empty");
    a.macro_keys.push(key('x'));
    a.save_macro("   ");
    assert_eq!(a.status, "untouched");
}

/// A recorded split is rebuilt over the reopened files, focused on the
/// recorded leaf; a layout with a single leaf is not restored.
#[test]
fn a_recorded_split_is_restored_only_when_it_has_two_panes() {
    use crate::session::{PaneNode, SplitSession, WorkspaceSession};
    let dir = temp("r4-sess-split");
    std::fs::write(dir.join("a.txt"), "a\n").unwrap();
    std::fs::write(dir.join("b.txt"), "b\n").unwrap();
    let files: Vec<String> = ["a.txt", "b.txt"]
        .iter()
        .map(|n| dir.join(n).to_string_lossy().into_owned())
        .collect();
    let split = |tree| SplitSession { tree, focused: 1 };
    let two = PaneNode::Split {
        dir: "vertical".into(),
        ratio: 40,
        first: Box::new(PaneNode::Leaf(0)),
        second: Box::new(PaneNode::Leaf(1)),
    };

    let mut a = app_at(&dir);
    let ws = WorkspaceSession {
        files: files.clone(),
        cursors: vec![0, 0],
        scrolls: vec![0, 0],
        split: Some(split(two)),
        ..WorkspaceSession::default()
    };
    assert_eq!(a.apply_session(&ws), 2);
    let root = a.editor.split_root.as_ref().expect("split restored");
    assert_eq!(root.leaf_count(), 2);
    assert_eq!(a.editor.focused_leaf, 1);
    assert_eq!(a.editor.active, 1, "the focused leaf's tab is active");

    let mut b = app_at(&dir);
    let single = WorkspaceSession {
        files,
        cursors: vec![0, 0],
        scrolls: vec![0, 0],
        split: Some(split(PaneNode::Leaf(0))),
        ..WorkspaceSession::default()
    };
    assert_eq!(b.apply_session(&single), 2);
    assert!(b.editor.split_root.is_none());

    // No files reopened: nothing changes and no split is built.
    let mut c = app_at(&dir);
    let empty = WorkspaceSession {
        files: vec![dir.join("gone.txt").to_string_lossy().into_owned()],
        ..WorkspaceSession::default()
    };
    assert_eq!(c.apply_session(&empty), 0);
    assert!(c.editor.split_root.is_none());
    assert_eq!(c.editor.tabs.len(), 1, "the blank tab stays");
}

/// An unknown keymap id in new settings is reported and replaced with the
/// default, and a changed keymap clears the chord state.
#[test]
fn applying_settings_with_an_unknown_keymap_falls_back_with_an_error() {
    let mut a = app_at(&temp("r4-sess-keymap"));
    let old = a.settings.clone();
    let mut new = old.clone();
    new.keymap = "no-such-keymap".to_string();
    let before = error_count(&a);
    a.apply_settings(&old, new);
    assert_eq!(a.settings.keymap, "apple");
    assert_eq!(error_count(&a), before + 1);
}

/// Editing helpers do nothing, and say nothing, when no tab is open.
#[test]
fn editing_helpers_without_a_tab_leave_everything_alone() {
    let mut a = app_at(&temp("r4-notab"));
    a.editor.tabs.clear();
    a.settings.editor_visual.highlight_word = true;
    a.status = "untouched".to_string();
    a.change_case(str::to_uppercase);
    a.transform_selection_or_buffer_try(|s| Ok::<_, String>(s.to_uppercase()));
    a.toggle_wrap("*", "*");
    a.toggle_fold_at_cursor();
    a.refresh_word_highlight();
    a.mark_active_dirty();
    a.replace_range_chars(0, 5, "x");
    a.apply_selected_code_action();
    a.paste_selected_clipboard();
    a.run_selected_macro();
    a.run_selected_task();
    a.unsaved_discard();
    assert!(a.editor.tabs.is_empty());
    // Folding is the one that reports: there is nothing to fold.
    assert_eq!(a.status, t!("status.no_fold").to_string());

    a.status.clear();
    a.request_code_action();
    assert_eq!(a.status, t!("status.lsp_inactive").to_string());
    a.nav_back();
    assert_eq!(a.status, t!("status.no_earlier").to_string());
    a.nav_forward();
    assert_eq!(a.status, t!("status.no_later").to_string());
    a.bookmark_goto(true);
    assert_eq!(a.status, t!("status.no_bookmarks").to_string());
    a.workspace_add_folder("no-such-folder");
    assert_eq!(a.status, t!("status.workspace_not_a_folder").to_string());
    assert!(a.editor.tabs.is_empty());
}

/// A code-action menu with an out-of-range selection applies nothing and is
/// consumed.
#[test]
fn an_empty_code_action_menu_is_consumed_without_effect() {
    let mut a = app_at(&temp("r4-codeaction"));
    buffer_with(&mut a, "keep", 0);
    a.code_actions = Some(crate::app::CodeActionMenu {
        actions: Vec::new(),
        selected: 3,
        refactor: false,
    });
    a.apply_selected_code_action();
    assert!(a.code_actions.is_none());
    assert_eq!(content(&mut a), "keep");
}
