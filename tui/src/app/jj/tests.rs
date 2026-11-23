//! Behavior tests for the `jj.*` dispatcher and its prompt handlers. No real
//! `jj` is needed: a `.jj` directory makes the workspace a jj repo, and the
//! command that would run is observed as the `$ ...` line pushed to the dock
//! (then cancelled).

use super::*;
use crate::settings::Settings;
use std::path::PathBuf;

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("vix-jjt-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn app_in(d: &std::path::Path) -> App {
    App::new(d.to_path_buf(), Settings::default())
}

fn jj_repo(tag: &str) -> (PathBuf, App) {
    let d = dir(tag);
    std::fs::create_dir_all(d.join(".jj")).unwrap();
    let app = app_in(&d);
    (d, app)
}

fn dock_has(app: &App, line: &str) -> bool {
    app.bottom_dock.lines.iter().any(|l| l == line)
}

#[test]
fn unknown_action_is_not_handled() {
    let mut app = app_in(&dir("unknown"));
    assert!(!app.run_jj_action("jj.nope"));
    assert!(app.prompt.is_none());
}

#[test]
fn outside_a_repo_commands_and_prompts_report_not_a_repo() {
    let d = dir("norepo");
    let mut app = app_in(&d);
    assert!(!app.jj_is_repo());
    for action in [
        "jj.status",
        "jj.describe",
        "jj.commit",
        "jj.edit",
        "jj.rebase",
    ] {
        app.status.clear();
        assert!(app.run_jj_action(action));
        assert_eq!(app.status, t!("status.jj_not_repo").to_string(), "{action}");
        assert!(app.prompt.is_none(), "{action}");
        assert!(app.running_command.is_none(), "{action}");
    }
}

#[test]
fn a_repo_is_found_from_a_subdirectory() {
    let (d, _) = jj_repo("ancestor");
    let sub = d.join("a/b");
    std::fs::create_dir_all(&sub).unwrap();
    assert!(app_in(&sub).jj_is_repo());
}

#[test]
fn prompt_actions_open_the_matching_prompt_in_a_repo() {
    let (_d, mut app) = jj_repo("prompts");
    for (action, kind) in [
        ("jj.describe", PromptKind::JjDescribe),
        ("jj.commit", PromptKind::JjCommit),
        ("jj.edit", PromptKind::JjEdit),
        ("jj.rebase", PromptKind::JjRebase),
        ("jj.bookmark_create", PromptKind::JjBookmarkCreate),
        ("jj.bookmark_set", PromptKind::JjBookmarkSet),
        ("jj.bookmark_delete", PromptKind::JjBookmarkDelete),
    ] {
        app.prompt = None;
        assert!(app.run_jj_action(action));
        let p = app.prompt.as_ref().unwrap_or_else(|| panic!("{action}"));
        assert!(p.kind == kind, "{action}");
        assert_eq!(p.input, "");
    }
}

#[test]
fn clone_prompt_opens_even_outside_a_repo() {
    let mut app = app_in(&dir("clone-prompt"));
    assert!(app.run_jj_action("jj.clone"));
    assert!(app.prompt.as_ref().unwrap().kind == PromptKind::JjClone);
}

#[test]
fn simple_actions_stream_their_command_into_the_dock() {
    for (action, cmd) in [
        ("jj.status", "jj --no-pager status"),
        ("jj.diff", "jj --no-pager diff"),
        ("jj.show", "jj --no-pager show"),
        ("jj.log", "jj --no-pager log"),
        ("jj.log_all", "jj --no-pager log -r ::"),
        ("jj.op_log", "jj --no-pager op log"),
        ("jj.new", "jj new"),
        ("jj.squash", "jj squash"),
        ("jj.abandon", "jj abandon"),
        ("jj.restore", "jj restore"),
        ("jj.undo", "jj undo"),
        ("jj.bookmark_list", "jj --no-pager bookmark list"),
        ("jj.git_push", "jj git push"),
        ("jj.git_fetch", "jj git fetch"),
    ] {
        let (_d, mut app) = jj_repo(&format!("simple-{}", action.replace('.', "-")));
        assert!(app.run_jj_action(action));
        assert!(dock_has(&app, &format!("$ {cmd}")), "{action}");
        assert!(app.visible.contains(crate::app::Visible::BOTTOM_DOCK));
        app.cancel_command();
    }
}

#[test]
fn init_refuses_an_existing_repo() {
    let (_d, mut app) = jj_repo("init-exists");
    assert!(app.run_jj_action("jj.init"));
    assert_eq!(app.status, t!("status.jj_already_init").to_string());
    assert!(app.running_command.is_none());
}

#[test]
fn init_colocates_with_an_existing_git_dir() {
    let d = dir("init-colocate");
    std::fs::create_dir_all(d.join(".git")).unwrap();
    let mut app = app_in(&d);
    app.jj_init();
    assert!(dock_has(&app, "$ jj git init --colocate"));
    app.cancel_command();
}

#[test]
fn init_without_git_creates_a_plain_repo() {
    let d = dir("init-plain");
    let mut app = app_in(&d);
    app.jj_init();
    assert!(dock_has(&app, "$ jj git init"));
    app.cancel_command();
}

#[test]
fn clone_validates_the_url() {
    let mut app = app_in(&dir("clone-validate"));
    app.jj_clone("   ");
    assert_eq!(app.status, t!("status.git_empty_url").to_string());
    app.jj_clone("--upload-pack=x");
    assert_eq!(app.status, t!("status.git_bad_argument").to_string());
    assert!(app.running_command.is_none());
}

#[test]
fn message_commands_reject_empty_and_pass_the_message_as_one_argument() {
    let mut app = app_in(&dir("message"));
    app.jj_describe("  ");
    assert_eq!(app.status, t!("status.jj_empty_message").to_string());
    app.status.clear();
    app.jj_commit("");
    assert_eq!(app.status, t!("status.jj_empty_message").to_string());
    app.jj_describe(" -fix: thing ");
    assert!(dock_has(&app, "$ jj describe --message=-fix: thing"));
    app.cancel_command();
}

#[test]
fn edit_and_rebase_validate_their_revision() {
    let mut app = app_in(&dir("rev"));
    app.jj_edit(" ");
    assert_eq!(app.status, t!("status.jj_empty_revision").to_string());
    app.jj_rebase("");
    assert_eq!(app.status, t!("status.jj_empty_revision").to_string());
    app.jj_edit("-x");
    assert_eq!(app.status, t!("status.git_bad_argument").to_string());
    app.status.clear();
    app.jj_rebase("--onto");
    assert_eq!(app.status, t!("status.git_bad_argument").to_string());
    assert!(app.running_command.is_none());

    app.jj_edit("abc123");
    assert!(dock_has(&app, "$ jj edit -- abc123"));
    app.cancel_command();
}

#[test]
fn rebase_runs_with_destination() {
    let mut app = app_in(&dir("rebase"));
    app.jj_rebase("main");
    assert!(dock_has(&app, "$ jj rebase -d main"));
    app.cancel_command();
}

#[test]
fn bookmark_validates_and_runs() {
    let mut app = app_in(&dir("bookmark"));
    app.jj_bookmark("create", " ");
    assert_eq!(app.status, t!("status.jj_empty_bookmark").to_string());
    app.jj_bookmark("create", "-bad");
    assert_eq!(app.status, t!("status.git_bad_argument").to_string());
    assert!(app.running_command.is_none());
    app.jj_bookmark("set", "feature");
    assert!(dock_has(&app, "$ jj bookmark set -- feature"));
    app.cancel_command();
}

#[test]
fn accepting_each_jj_prompt_runs_its_command() {
    for (kind, input, expect) in [
        (
            PromptKind::JjClone,
            "https://example.invalid/r.git",
            "$ jj git clone -- https://example.invalid/r.git",
        ),
        (PromptKind::JjDescribe, "msg", "$ jj describe --message=msg"),
        (PromptKind::JjCommit, "msg", "$ jj commit --message=msg"),
        (PromptKind::JjEdit, "r1", "$ jj edit -- r1"),
        (PromptKind::JjRebase, "r2", "$ jj rebase -d r2"),
        (
            PromptKind::JjBookmarkCreate,
            "b",
            "$ jj bookmark create -- b",
        ),
        (PromptKind::JjBookmarkSet, "b", "$ jj bookmark set -- b"),
        (
            PromptKind::JjBookmarkDelete,
            "b",
            "$ jj bookmark delete -- b",
        ),
    ] {
        let mut app = app_in(&dir("accept"));
        app.accept_jj_prompt(kind, input);
        assert!(dock_has(&app, expect), "{expect}");
        app.cancel_command();
    }
    let mut app = app_in(&dir("accept-other"));
    app.accept_jj_prompt(PromptKind::Open, "x");
    assert!(app.running_command.is_none());
    assert_eq!(app.bottom_dock.lines.len(), 0);
}
