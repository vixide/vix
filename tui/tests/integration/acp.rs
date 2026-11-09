//! T635: the Agent Client Protocol host. The "agent" is a scripted fake
//! (`crates/vix-acp/tests/fake_agent.py`) run over real pipes; nothing here
//! touches a network or a real assistant. The trust tests prove an untrusted
//! workspace never spawns the agent at all. Everything the agent runs is
//! Python too, so the suite needs no POSIX shell and runs on Windows.

use crate::integration::common::*;

/// The first of `python3` and `python` that runs and is Python 3: Windows
/// usually has only `python` (and its `python3` may be the Store stub).
fn python() -> Option<&'static str> {
    ["python3", "python"].into_iter().find(|p| {
        std::process::Command::new(p)
            .arg("--version")
            .output()
            .is_ok_and(|o| {
                o.status.success()
                    && String::from_utf8_lossy(&[o.stdout, o.stderr].concat()).contains("Python 3")
            })
    })
}

fn python_available() -> bool {
    python().is_some()
}

/// The interpreter for commands the tests build (`python3` when none is found;
/// such tests return early anyway).
fn py() -> &'static str {
    python().unwrap_or("python3")
}

fn fake_agent_command(log: &Path) -> String {
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/vix-acp/tests/fake_agent.py");
    format!("{} {} {}", py(), script.display(), log.display())
}

/// Write a Python script named `name` into `dir` for the agent to run.
fn py_script(dir: &Path, name: &str, code: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, code).unwrap();
    path
}

/// A script that creates the file named by its argument.
const TOUCH: &str = "import sys\nopen(sys.argv[1], 'w').close()\n";

/// A script that prints `tick` until it is stopped.
const TICKER: &str = "import sys, time\nwhile True:\n    sys.stdout.buffer.write(b'tick\\n')\n    sys.stdout.flush()\n    time.sleep(0.05)\n";

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

#[test]
fn a_terminal_runs_only_after_the_user_says_yes() {
    if !python_available() {
        return;
    }
    let dir = unique_dir("acp-term-yes");
    let (mut app, log) = agent_app(&dir, true);
    start(&mut app, &log);
    let marker = dir.join("made-by-agent");
    let touch = py_script(&dir, "touch.py", TOUCH);
    send_prompt(
        &mut app,
        &format!("term {} {} {}", py(), touch.display(), marker.display()),
    );
    tick_until(&mut app, |a| {
        a.prompt
            .as_ref()
            .is_some_and(|p| p.kind == PromptKind::AcpPermission)
    });
    let asked = app.prompt.as_ref().unwrap().title.clone();
    assert!(
        asked.contains("touch"),
        "the prompt names the command: {asked}"
    );
    assert!(!marker.exists(), "nothing runs before the answer");
    type_str(&mut app, "y");
    app.on_key(keycode(KeyCode::Enter));
    tick_until(&mut app, |_| log_text(&log).contains("TERM released"));
    assert!(marker.exists(), "the approved command ran");
    assert!(log_text(&log).contains("TERM exit {\"exitCode\": 0"));
    app.run_action("ai.agent_stop");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_denied_terminal_runs_nothing() {
    if !python_available() {
        return;
    }
    let dir = unique_dir("acp-term-no");
    let (mut app, log) = agent_app(&dir, true);
    start(&mut app, &log);
    let marker = dir.join("made-by-agent");
    let touch = py_script(&dir, "touch.py", TOUCH);
    send_prompt(
        &mut app,
        &format!("term {} {} {}", py(), touch.display(), marker.display()),
    );
    tick_until(&mut app, |a| a.prompt.is_some());
    app.on_key(esc());
    tick_until(&mut app, |_| log_text(&log).contains("TERM denied"));
    assert!(!marker.exists(), "a declined command never starts");
    app.run_action("ai.agent_stop");
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_terminal_cannot_run_outside_the_workspace() {
    if !python_available() {
        return;
    }
    let dir = unique_dir("acp-term-cwd");
    let (mut app, log) = agent_app(&dir, true);
    start(&mut app, &log);
    // The workspace's parent directory is outside it on every OS.
    let outside = dir.parent().unwrap().to_path_buf();
    let touch = py_script(&dir, "touch.py", TOUCH);
    let marker = outside.join("acp-should-never-exist");
    send_prompt(
        &mut app,
        &format!(
            "termin {} {} {} {}",
            outside.display(),
            py(),
            touch.display(),
            marker.display()
        ),
    );
    tick_until(&mut app, |_| log_text(&log).contains("TERM denied"));
    assert!(
        app.prompt.is_none(),
        "an escaping directory is refused without even asking"
    );
    assert!(!marker.exists());
    app.run_action("ai.agent_stop");
    fs::remove_dir_all(&dir).ok();
}

/// Answer the open terminal prompt with yes.
fn approve_terminal(app: &mut App) {
    tick_until(app, |a| {
        a.prompt
            .as_ref()
            .is_some_and(|p| p.kind == PromptKind::AcpPermission)
    });
    type_str(app, "y");
    app.on_key(keycode(KeyCode::Enter));
}

/// The plan tab's text (empty when it is not open).
fn plan_tab_text(app: &App) -> String {
    app.editor
        .tabs
        .iter()
        .find(|t| t.path.as_deref() == Some(Path::new("*agent plan*")))
        .map(vix_editor::editor::Tab::text)
        .unwrap_or_default()
}

/// Start the fake agent with the plan tab open, and write the Python `script`
/// to run.
fn terminal_app(tag: &str, script: &str) -> (App, PathBuf, PathBuf) {
    let dir = unique_dir(tag);
    let (mut app, log) = agent_app(&dir, true);
    start(&mut app, &log);
    app.run_action("ai.agent_plan");
    let path = py_script(&dir, "out.py", script);
    (app, log, path)
}

#[test]
fn terminal_output_shows_live_in_order_without_control_sequences() {
    if !python_available() {
        return;
    }
    let script = "import sys, time\n\
                  sys.stdout.buffer.write(b'first\\n\\x1b[31msecond\\x1b[0m\\n\\x1b]0;pwned\\x07')\n\
                  sys.stdout.flush()\n\
                  time.sleep(2)\n\
                  sys.stderr.buffer.write(b'third\\n')\n\
                  sys.exit(4)\n";
    let (mut app, log, script) = terminal_app("acp-term-live", script);
    send_prompt(&mut app, &format!("term {} {}", py(), script.display()));
    approve_terminal(&mut app);
    // Live: the first lines show while the command still sleeps.
    tick_until(&mut app, |a| plan_tab_text(a).contains("second"));
    assert!(
        !log_text(&log).contains("TERM exit"),
        "shown before the command ended"
    );
    assert!(!plan_tab_text(&app).contains("third"));
    tick_until(&mut app, |_| log_text(&log).contains("TERM released"));
    app.poll_acp();
    let text = plan_tab_text(&app);
    let at = |needle: &str| {
        text.find(needle)
            .unwrap_or_else(|| panic!("{needle:?} missing from {text}"))
    };
    assert!(
        at("$ python") < at("first") && at("first") < at("second") && at("second") < at("third")
    );
    assert!(text.contains("4]"), "the exit status is shown: {text}");
    assert!(
        !text.chars().any(|c| c.is_control() && c != '\n'),
        "no control characters reach the display: {text:?}"
    );
    assert!(!text.contains("pwned"), "an OSC title is removed whole");
    // The docked pane's terminal section holds the same, sanitized, output.
    let docked = app.acp_terminal_lines().join("\n");
    assert!(
        docked.contains("  third") && !docked.contains('\u{1b}'),
        "{docked}"
    );
    app.run_action("ai.agent_stop");
    fs::remove_dir_all(script.parent().unwrap()).ok();
}

#[test]
fn terminal_output_shown_respects_the_byte_limit() {
    if !python_available() {
        return;
    }
    let (mut app, log, script) = terminal_app(
        "acp-term-cap",
        "import sys\nsys.stdout.buffer.write(b'0123456789abcdefghij')\n",
    );
    send_prompt(
        &mut app,
        &format!("termcap 5 {} {}", py(), script.display()),
    );
    approve_terminal(&mut app);
    tick_until(&mut app, |_| log_text(&log).contains("TERM released"));
    app.poll_acp();
    let text = plan_tab_text(&app);
    assert!(text.contains("  fghij"), "{text}");
    assert!(
        !text.contains("abcde"),
        "only the newest bytes are kept: {text}"
    );
    app.run_action("ai.agent_stop");
    fs::remove_dir_all(script.parent().unwrap()).ok();
}

/// Poll a while and return the plan tab, to show it no longer changes.
fn settled_plan(app: &mut App) -> String {
    for _ in 0..5 {
        app.poll_acp();
        std::thread::sleep(Duration::from_millis(100));
    }
    plan_tab_text(app)
}

#[test]
fn a_killed_terminal_stops_updating() {
    if !python_available() {
        return;
    }
    let (mut app, log, script) = terminal_app("acp-term-kill", TICKER);
    send_prompt(&mut app, &format!("termkill {} {}", py(), script.display()));
    approve_terminal(&mut app);
    tick_until(&mut app, |_| log_text(&log).contains("TERM killed"));
    let before = settled_plan(&mut app);
    assert!(before.contains("  tick"), "{before}");
    assert_eq!(settled_plan(&mut app), before, "no output after the kill");
    app.run_action("ai.agent_stop");
    fs::remove_dir_all(script.parent().unwrap()).ok();
}

#[test]
fn a_released_terminal_stops_updating() {
    if !python_available() {
        return;
    }
    let (mut app, log, script) = terminal_app("acp-term-release", TICKER);
    send_prompt(&mut app, &format!("termrel {} {}", py(), script.display()));
    approve_terminal(&mut app);
    tick_until(&mut app, |_| log_text(&log).contains("TERM released"));
    let before = settled_plan(&mut app);
    assert!(
        before.contains("  tick"),
        "the final output is kept: {before}"
    );
    assert_eq!(settled_plan(&mut app), before, "a released view is frozen");
    app.run_action("ai.agent_stop");
    fs::remove_dir_all(script.parent().unwrap()).ok();
}
