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
