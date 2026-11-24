//! Round 4: the `git.*` glue's remaining refusals that a real scratch
//! repository can reach -- unstaging a hunk of a file the index no longer
//! holds, and opening the diff of a commit that has vanished.

use super::tests::{app_with as base_app_with, git, goto_line, key, set_text};
use super::*;
use vix_input::KeyCode;

fn app_with(tag: &str, files: &[(&str, &str)]) -> (PathBuf, App) {
    let (d, mut app) = base_app_with(tag, files);
    app.settings_path = Some(d.join("vix-test-config.toml"));
    (d, app)
}

const FIVE: &str = "one\ntwo\nthree\nfour\nfive\n";
const FIVE_EDITED: &str = "one\nTWO\nthree\nfour\nFIVE\n";

#[test]
fn unstaging_a_hunk_of_a_file_removed_from_the_index_says_nothing_is_staged() {
    let (d, mut app) = app_with("g4-unstage", &[("a.txt", FIVE)]);
    git(&d, &["rm", "-q", "--cached", "a.txt"]);
    set_text(&mut app, FIVE_EDITED);
    goto_line(&mut app, 1);
    app.run_git_action("git.unstage_hunk");
    assert_eq!(
        app.status,
        t!("status.unstage_hunk_failed", error = "nothing staged").to_string()
    );
}

#[test]
fn the_diff_of_a_commit_that_vanished_is_reported_and_the_log_stays_open() {
    let (d, mut app) = app_with("g4-vanished", &[("a.txt", "one\n")]);
    std::fs::write(d.join("b.txt"), "bee\n").unwrap();
    git(&d, &["add", "-A"]);
    git(&d, &["commit", "-q", "-m", "second"]);
    app.refresh_git();
    app.run_git_action("git.browse_log");
    let panel = app.git_log.as_mut().unwrap();
    let abbrev = panel.entries[0].abbrev.clone();
    panel.entries[0].sha = "0".repeat(40);
    let errors_before = app.messages.items.len();
    let tabs = app.editor.tabs.len();
    app.on_key(key(KeyCode::Enter));
    assert!(app.git_log.is_some(), "the panel stays for another try");
    assert_eq!(app.editor.tabs.len(), tabs, "no diff tab was opened");
    let last = app.messages.items.last().unwrap();
    assert_eq!(app.messages.items.len(), errors_before + 1);
    assert_eq!(
        last.text,
        t!("msg.git_show_failed", sha = abbrev).to_string()
    );
}
