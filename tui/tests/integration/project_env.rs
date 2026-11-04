//! Project environment activation (T667): the app-level trust gate.

use std::fs;

use vix::app::App;
use vix::settings::Settings;

use super::common::{isolated_session_path, unique_dir};

fn make_venv(dir: &std::path::Path) {
    fs::create_dir_all(dir.join(".venv/bin")).unwrap();
    fs::write(dir.join(".venv/pyvenv.cfg"), "home = /usr/bin\n").unwrap();
}

#[test]
fn venv_activates_without_trust_and_reaches_spawned_commands() {
    let dir = unique_dir("env-venv");
    make_venv(&dir);
    let mut app =
        App::new(dir.clone(), Settings::default()).with_session_path(isolated_session_path());
    app.run_action("project.reload_env");
    assert_eq!(
        app.project_env_source,
        Some(vix_project_env::Source::Venv),
        "the venv is activated"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn untrusted_workspace_does_not_load_a_code_running_environment() {
    let dir = unique_dir("env-untrusted");
    // An .envrc and a mise config, and nothing the file-only source could use.
    fs::write(dir.join(".envrc"), "export EVIL=1\n").unwrap();
    fs::write(dir.join("mise.toml"), "").unwrap();
    let mut app =
        App::new(dir.clone(), Settings::default()).with_session_path(isolated_session_path());
    app.run_action("project.reload_env");
    assert_eq!(
        app.project_env_source, None,
        "nothing may be loaded from an untrusted workspace"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn untrusted_envrc_falls_back_to_the_file_only_venv() {
    let dir = unique_dir("env-fallback");
    fs::write(dir.join(".envrc"), "export EVIL=1\n").unwrap();
    make_venv(&dir);
    let mut app =
        App::new(dir.clone(), Settings::default()).with_session_path(isolated_session_path());
    app.run_action("project.reload_env");
    assert_eq!(
        app.project_env_source,
        Some(vix_project_env::Source::Venv),
        "the file-only venv still applies, not direnv"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn envrc_alone_raises_the_trust_prompt_and_an_empty_workspace_does_not() {
    let empty = unique_dir("env-prompt-none");
    let mut app =
        App::new(empty.clone(), Settings::default()).with_session_path(isolated_session_path());
    app.maybe_prompt_script_trust();
    assert!(app.script_trust.is_none());
    fs::remove_dir_all(&empty).ok();

    let dir = unique_dir("env-prompt-envrc");
    fs::write(dir.join(".envrc"), "export EVIL=1\n").unwrap();
    let mut app =
        App::new(dir.clone(), Settings::default()).with_session_path(isolated_session_path());
    app.maybe_prompt_script_trust();
    let prompt = app.script_trust.as_ref().expect("prompt raised");
    assert!(prompt.env && prompt.count == 0 && !prompt.settings);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn user_detector_is_blocked_until_trusted_and_raises_the_prompt() {
    let dir = unique_dir("env-custom");
    fs::create_dir_all(dir.join(".vix")).unwrap();
    fs::write(
        dir.join(".vix/env.toml"),
        "[[detector]]\nname = \"x\"\ndetect = [\"marker\"]\ncommand = [\"/nonexistent/should-not-run\"]\n",
    )
    .unwrap();
    fs::write(dir.join("marker"), "").unwrap();
    let mut app =
        App::new(dir.clone(), Settings::default()).with_session_path(isolated_session_path());
    app.maybe_prompt_script_trust();
    assert!(app.script_trust.as_ref().is_some_and(|p| p.env));
    app.run_action("project.reload_env");
    assert_eq!(
        app.project_env_source, None,
        "an untrusted workspace never runs the detector"
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn detector_env_with_a_loader_variable_invalidates_the_file() {
    let dir = unique_dir("env-custom-deny");
    fs::create_dir_all(dir.join(".vix")).unwrap();
    fs::write(
        dir.join(".vix/env.toml"),
        "[[detector]]\nname = \"x\"\ndetect = [\"marker\"]\ncommand = [\"/nonexistent/x\"]\n[detector.env]\nLD_PRELOAD = \"/tmp/evil.so\"\n",
    )
    .unwrap();
    fs::write(dir.join("marker"), "").unwrap();
    let mut app =
        App::new(dir.clone(), Settings::default()).with_session_path(isolated_session_path());
    // The whole file is ignored, so it does not even raise the trust prompt.
    app.maybe_prompt_script_trust();
    assert!(app.script_trust.is_none());
    fs::remove_dir_all(&dir).ok();
}
