//! T635: the Agent Client Protocol host. The "agent" is a scripted fake
//! (`crates/vix-acp/tests/fake_agent.py`) run over real pipes; nothing here
//! touches a network or a real assistant. The trust tests prove an untrusted
//! workspace never spawns the agent at all.
#![cfg(unix)]

use crate::integration::common::*;

fn python_available() -> bool {
    std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_ok()
}

fn fake_agent_command(log: &Path) -> String {
    format!(
        "python3 {}/../crates/vix-acp/tests/fake_agent.py {}",
        env!("CARGO_MANIFEST_DIR"),
        log.display()
    )
}

/// An app rooted at `dir` whose only configured agent is the fake one,
/// with this workspace recorded as trusted when `trusted`.
fn agent_app(dir: &Path, trusted: bool) -> (App, PathBuf) {
    agent_app_with_session(dir, trusted, isolated_session_path())
}

/// [`agent_app`] on a given session file, so a second app can share it.
fn agent_app_with_session(dir: &Path, trusted: bool, session: PathBuf) -> (App, PathBuf) {
    let log = dir.join("agent.log");
    let mut settings = Settings::default();
    settings.ai_agents = vec![fake_agent_command(&log)];
    if trusted {
        let mut s = vix_session::Session::default();
        let key = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        s.set_scripts_trusted(&key.to_string_lossy(), Some(true));
        s.save_to(&session).unwrap();
    }
    let mut app = App::new(dir.to_path_buf(), settings).with_session_path(session);
    app.layout.editor = Rect::new(0, 0, 80, 24);
    (app, log)
}

fn tick_until(app: &mut App, pred: impl FnMut(&App) -> bool) {
    wait_until(app, App::poll_acp, pred, Duration::from_secs(10));
}

fn log_text(log: &Path) -> String {
    fs::read_to_string(log).unwrap_or_default()
}

/// Start the agent and wait until it has a session (the log shows `session/new`
/// was received, and one more poll processed the reply).
fn start(app: &mut App, log: &Path) {
    app.run_action("ai.agent_start");
    assert!(app.acp_running());
    tick_until(app, |_| log_text(log).contains("session/new"));
    std::thread::sleep(Duration::from_millis(100));
    app.poll_acp();
}

fn send_prompt(app: &mut App, text: &str) {
    app.run_action("ai.agent_prompt");
    assert!(app.prompt.is_some(), "the prompt opens while an agent runs");
    type_str(app, text);
    app.on_key(keycode(KeyCode::Enter));
}

fn transcript(app: &App) -> String {
    app.ai_panel
        .as_ref()
        .map(|p| {
            p.turns
                .iter()
                .map(|t| t.text.clone())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

#[test]
fn an_untrusted_workspace_never_spawns_the_agent() {
    let dir = unique_dir("acp-untrusted");
    let (mut app, log) = agent_app(&dir, false);
    app.run_action("ai.agent_start");
    assert!(
        !app.acp_running(),
        "no agent process for an untrusted workspace"
    );
    // The fake agent opens its log file the moment it starts, so no file
    // means no process was ever spawned.
    std::thread::sleep(Duration::from_millis(300));
    assert!(!log.exists(), "the agent command must not have run");
    // Prompting and stopping with no agent are harmless.
    app.run_action("ai.agent_prompt");
    assert!(app.prompt.is_none());
    app.run_action("ai.agent_stop");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_trusted_workspace_without_a_configured_agent_spawns_nothing() {
    let dir = unique_dir("acp-noagent");
    let (mut app, _log) = agent_app(&dir, true);
    app.settings.ai_agents.clear();
    app.run_action("ai.agent_start");
    assert!(!app.acp_running());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn ai_off_refuses_the_agent_actions() {
    let dir = unique_dir("acp-ai-off");
    let (mut app, log) = agent_app(&dir, true);
    app.settings.ai = "off".to_string();
    app.run_action("ai.agent_start");
    assert!(!app.acp_running());
    std::thread::sleep(Duration::from_millis(200));
    assert!(!log.exists());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_trusted_workspace_streams_a_turn_into_the_ai_panel() {
    if !python_available() {
        return;
    }
    let dir = unique_dir("acp-stream");
    let (mut app, log) = agent_app(&dir, true);
    start(&mut app, &log);
    send_prompt(&mut app, "plain");
    tick_until(&mut app, |a| transcript(a).contains("hello"));
    tick_until(&mut app, |a| transcript(a).contains("step one"));
    let text = transcript(&app);
    assert!(text.contains("Reading"), "tool call is shown: {text}");
    assert!(
        text.contains("plain"),
        "the user's prompt is in the transcript"
    );
    app.run_action("ai.agent_stop");
    assert!(!app.acp_running());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn escaping_a_permission_prompt_denies_it() {
    if !python_available() {
        return;
    }
    let dir = unique_dir("acp-perm-esc");
    let (mut app, log) = agent_app(&dir, true);
    start(&mut app, &log);
    send_prompt(&mut app, "perm");
    tick_until(&mut app, |a| {
        a.prompt
            .as_ref()
            .is_some_and(|p| p.kind == PromptKind::AcpPermission)
    });
    app.on_key(esc());
    tick_until(&mut app, |_| log_text(&log).contains("OUTCOME"));
    let text = log_text(&log);
    assert!(text.contains("\"optionId\": \"no\""), "rejected: {text}");
    assert!(!text.contains("\"optionId\": \"yes\""));
    app.run_action("ai.agent_stop");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn only_y_allows_a_permission_request() {
    if !python_available() {
        return;
    }
    for (answer, expect) in [("n", "no"), ("", "no"), ("maybe", "no"), ("y", "yes")] {
        let dir = unique_dir(&format!("acp-perm-{expect}-{}", answer.len()));
        let (mut app, log) = agent_app(&dir, true);
        start(&mut app, &log);
        send_prompt(&mut app, "perm");
        tick_until(&mut app, |a| {
            a.prompt
                .as_ref()
                .is_some_and(|p| p.kind == PromptKind::AcpPermission)
        });
        type_str(&mut app, answer);
        app.on_key(keycode(KeyCode::Enter));
        tick_until(&mut app, |_| log_text(&log).contains("OUTCOME"));
        let text = log_text(&log);
        assert!(
            text.contains(&format!("\"optionId\": \"{expect}\"")),
            "answer {answer:?} -> {expect}: {text}"
        );
        app.run_action("ai.agent_stop");
        fs::remove_dir_all(&dir).ok();
    }
}

#[test]
fn an_agent_write_becomes_a_pending_diff_and_writes_nothing() {
    if !python_available() {
        return;
    }
    let dir = unique_dir("acp-write");
    let file = dir.join("a.txt");
    fs::write(&file, "old\n").unwrap();
    let (mut app, log) = agent_app(&dir, true);
    start(&mut app, &log);
    send_prompt(&mut app, &format!("write {} NEW", file.display()));
    tick_until(&mut app, |a| a.ai_diff_review().is_some());
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "old\n",
        "disk untouched"
    );
    // Rejecting the review leaves the buffer and the disk as they were.
    app.on_key(esc());
    assert!(app.ai_diff_review().is_none());
    assert_eq!(fs::read_to_string(&file).unwrap(), "old\n");
    let tab = app.editor.active_tab().unwrap();
    assert_eq!(tab.text(), "old\n");
    app.run_action("ai.agent_stop");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_agent_write_outside_the_workspace_is_refused() {
    if !python_available() {
        return;
    }
    let dir = unique_dir("acp-write-out");
    let outside = unique_dir("acp-write-out-elsewhere").join("victim.txt");
    fs::write(&outside, "keep\n").unwrap();
    let (mut app, log) = agent_app(&dir, true);
    start(&mut app, &log);
    send_prompt(&mut app, &format!("write {} PWNED", outside.display()));
    tick_until(&mut app, |_| log_text(&log).contains("WRITE"));
    assert!(log_text(&log).contains("WRITE error"));
    assert!(app.ai_diff_review().is_none());
    assert_eq!(fs::read_to_string(&outside).unwrap(), "keep\n");
    app.run_action("ai.agent_stop");
    fs::remove_dir_all(&dir).ok();
    fs::remove_dir_all(outside.parent().unwrap()).ok();
}

#[test]
fn an_agent_read_is_answered_from_the_unsaved_buffer() {
    if !python_available() {
        return;
    }
    let dir = unique_dir("acp-read");
    let file = dir.join("r.txt");
    fs::write(&file, "on disk\n").unwrap();
    let (mut app, log) = agent_app(&dir, true);
    app.editor.open(&file, false).unwrap();
    buffer_with(&mut app, "unsaved edit\n", 0);
    start(&mut app, &log);
    send_prompt(&mut app, &format!("read {}", file.display()));
    tick_until(&mut app, |a| transcript(a).contains("unsaved edit"));
    assert!(!transcript(&app).contains("on disk"));
    app.run_action("ai.agent_stop");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn several_agents_ask_which_to_start() {
    if !python_available() {
        return;
    }
    let dir = unique_dir("acp-choose");
    let (mut app, log_a) = agent_app(&dir, true);
    let log_b = dir.join("agent-b.log");
    app.settings.ai_agents.push(fake_agent_command(&log_b));
    app.run_action("ai.agent_start");
    assert!(!app.acp_running(), "nothing starts before a choice");
    assert!(
        app.prompt
            .as_ref()
            .is_some_and(|p| p.kind == PromptKind::AcpAgent)
    );
    // A number outside the list starts nothing.
    type_str(&mut app, "9");
    app.on_key(keycode(KeyCode::Enter));
    assert!(!app.acp_running());
    // The second entry runs, and the first never does.
    app.run_action("ai.agent_start");
    type_str(&mut app, "2");
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.acp_running());
    tick_until(&mut app, |_| log_text(&log_b).contains("initialize"));
    assert!(!log_a.exists(), "the first agent was not spawned");
    app.run_action("ai.agent_stop");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn choosing_an_agent_in_an_untrusted_workspace_opens_nothing() {
    let dir = unique_dir("acp-choose-untrusted");
    let (mut app, log) = agent_app(&dir, false);
    app.settings.ai_agents.push("never-run".to_string());
    app.run_action("ai.agent_start");
    assert!(app.prompt.is_none(), "no chooser before trust");
    assert!(!app.acp_running());
    std::thread::sleep(Duration::from_millis(200));
    assert!(!log.exists());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_agent_write_can_create_a_new_file_but_only_in_memory() {
    if !python_available() {
        return;
    }
    let dir = unique_dir("acp-create");
    let file = dir.join("fresh.txt");
    let (mut app, log) = agent_app(&dir, true);
    start(&mut app, &log);
    send_prompt(&mut app, &format!("write {} NEW", file.display()));
    tick_until(&mut app, |a| a.ai_diff_review().is_some());
    assert!(!file.exists(), "nothing on disk before review and save");
    let tab = app.editor.active_tab().unwrap();
    assert_eq!(tab.path.as_deref(), Some(file.as_path()));
    // Accepting fills the buffer; the disk is still untouched until a save.
    app.on_key(keycode(KeyCode::Enter));
    assert_eq!(app.editor.active_tab().unwrap().text(), "NEW");
    assert!(!file.exists());
    app.run_action("ai.agent_stop");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_new_file_needs_an_existing_parent_directory() {
    if !python_available() {
        return;
    }
    let dir = unique_dir("acp-create-parent");
    let file = dir.join("no-such-dir").join("x.txt");
    let (mut app, log) = agent_app(&dir, true);
    start(&mut app, &log);
    send_prompt(&mut app, &format!("write {} NEW", file.display()));
    tick_until(&mut app, |_| log_text(&log).contains("WRITE"));
    assert!(log_text(&log).contains("WRITE error"));
    assert!(app.ai_diff_review().is_none());
    app.run_action("ai.agent_stop");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_plan_opens_in_its_own_read_only_tab() {
    if !python_available() {
        return;
    }
    let dir = unique_dir("acp-plan");
    let (mut app, log) = agent_app(&dir, true);
    start(&mut app, &log);
    send_prompt(&mut app, "plain");
    tick_until(&mut app, |a| transcript(a).contains("step one"));
    app.run_action("ai.agent_plan");
    let tab = app.editor.active_tab().unwrap();
    assert!(tab.read_only);
    assert_eq!(tab.path.as_deref(), Some(Path::new("*agent plan*")));
    let text = tab.text();
    assert!(
        text.contains("step one") && text.contains("Reading"),
        "{text}"
    );
    app.run_action("ai.agent_stop");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_plan_action_without_an_agent_opens_nothing() {
    let dir = unique_dir("acp-plan-none");
    let (mut app, _log) = agent_app(&dir, true);
    let before = app.editor.tabs.len();
    app.run_action("ai.agent_plan");
    assert_eq!(app.editor.tabs.len(), before);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_stopped_agent_can_be_resumed_into_its_session() {
    if !python_available() {
        return;
    }
    let dir = unique_dir("acp-resume");
    let (mut app, log) = agent_app(&dir, true);
    app.run_action("ai.agent_resume");
    assert!(!app.acp_running(), "nothing to resume yet");
    start(&mut app, &log);
    app.run_action("ai.agent_stop");
    app.run_action("ai.agent_resume");
    assert!(app.acp_running());
    tick_until(&mut app, |_| log_text(&log).contains("session/load"));
    assert!(log_text(&log).contains("\"sessionId\":\"S1\""));
    app.run_action("ai.agent_stop");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn resume_requires_a_still_configured_agent() {
    if !python_available() {
        return;
    }
    let dir = unique_dir("acp-resume-gates");
    let (mut app, log) = agent_app(&dir, true);
    start(&mut app, &log);
    app.run_action("ai.agent_stop");
    // The command was removed from settings: it may not run again.
    app.settings.ai_agents.clear();
    app.run_action("ai.agent_resume");
    assert!(!app.acp_running());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_session_id_survives_a_restart() {
    if !python_available() {
        return;
    }
    let dir = unique_dir("acp-persist");
    let session = isolated_session_path();
    let (mut app, log) = agent_app_with_session(&dir, true, session.clone());
    start(&mut app, &log);
    app.run_action("ai.agent_stop");
    drop(app);
    // A brand new App (a restart) on the same session file.
    let (mut app, log) = agent_app_with_session(&dir, false, session);
    app.run_action("ai.agent_resume");
    assert!(app.acp_running());
    tick_until(&mut app, |_| log_text(&log).contains("session/load"));
    assert!(log_text(&log).contains("\"sessionId\":\"S1\""));
    app.run_action("ai.agent_stop");
    fs::remove_dir_all(&dir).ok();
}

fn screen(app: &mut App) -> String {
    use ratatui::{Terminal, backend::TestBackend};
    let mut term = Terminal::new(TestBackend::new(140, 30)).unwrap();
    term.draw(|f| crate::ui::draw(app, f)).unwrap();
    term.backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect()
}

#[test]
fn the_plan_can_be_docked_beside_the_editor() {
    if !python_available() {
        return;
    }
    let dir = unique_dir("acp-dock");
    let (mut app, log) = agent_app(&dir, true);
    app.run_action("ai.agent_plan_dock");
    assert!(
        screen(&mut app).contains("No agent is running"),
        "the dock opens even with no agent, saying so"
    );
    start(&mut app, &log);
    send_prompt(&mut app, "plain");
    tick_until(&mut app, |a| transcript(a).contains("step one"));
    let shown = screen(&mut app);
    assert!(
        shown.contains("step one") && shown.contains("Reading"),
        "the plan and tool calls are docked: {shown}"
    );
    assert!(
        app.editor
            .tabs
            .iter()
            .all(|t| t.path.as_deref() != Some(Path::new("*agent plan*"))),
        "docking does not open a tab"
    );
    app.run_action("ai.agent_plan_dock");
    assert!(!screen(&mut app).contains("Agent plan"), "toggled off");
    app.run_action("ai.agent_stop");
    fs::remove_dir_all(&dir).ok();
}
