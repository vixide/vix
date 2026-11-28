//! Provider-free checks of every message shape the turn reads or writes.
use super::*;

/// A Codex turn that has started its thread "t" and turn "u".
fn codex_turn() -> Turn {
    let mut turn = Turn::new(Provider::Codex, "/w".into(), None, "go".into());
    let _ = turn.receive(&json!({"id":1,"result":{}}));
    let _ = turn.receive(&json!({"id":2,"result":{"thread":{"id":"t"}}}));
    let _ = turn.receive(&json!({"id":3,"result":{"turn":{"id":"u"}}}));
    turn
}

fn sends(events: &[Event]) -> Vec<&Value> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Send(v) => Some(v),
            _ => None,
        })
        .collect()
}

fn ask(mut events: Vec<Event>) -> Approval {
    match events.pop() {
        Some(Event::Ask(a)) => a,
        other => panic!("expected an approval, got {other:?}"),
    }
}

#[test]
fn codex_thread_start_and_resume_leave_sandbox_and_approvals_to_the_user() {
    let mut fresh = Turn::new(Provider::Codex, "/w".into(), None, "go".into());
    assert_eq!(fresh.argv(), ["codex", "app-server"]);
    assert_eq!(fresh.initialize()["method"], "initialize");
    let events = fresh.receive(&json!({"id":1,"result":{}}));
    let out = sends(&events);
    assert_eq!(out[0], &json!({"method":"initialized"}));
    assert_eq!(
        out[1],
        &json!({"method":"thread/start","id":2,"params":{"cwd":"/w"}})
    );
    let mut resumed = Turn::new(Provider::Codex, "/w".into(), Some("t".into()), "go".into());
    let events = resumed.receive(&json!({"id":1,"result":{}}));
    assert_eq!(
        sends(&events)[1],
        &json!({"method":"thread/resume","id":2,"params":{"cwd":"/w","threadId":"t"}})
    );
    let events = resumed.receive(&json!({"id":2,"result":{"thread":{"id":"t"}}}));
    assert!(matches!(&events[0], Event::Session(id) if id == "t"));
    assert_eq!(
        sends(&events)[0],
        &json!({"method":"turn/start","id":3,"params":{"threadId":"t","input":[{"type":"text","text":"go"}]}})
    );
}

#[test]
fn claude_argv_keeps_the_users_permission_mode() {
    let fresh = Turn::new(Provider::Claude, "/w".into(), None, "go".into());
    let argv = fresh.argv();
    assert_eq!(
        argv,
        [
            "claude",
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--permission-prompt-tool",
            "stdio"
        ]
    );
    assert!(!argv.iter().any(|a| a == "--permission-mode"));
    let resumed = Turn::new(Provider::Claude, "/w".into(), Some("s".into()), "go".into());
    assert_eq!(resumed.argv()[9..], ["--resume", "s"]);
}

#[test]
fn a_foreign_codex_thread_cannot_request_approval_or_finish_our_turn() {
    let mut turn = Turn::new(
        Provider::Codex,
        ".".into(),
        Some("ours".into()),
        "go".into(),
    );
    let msg = json!({"id":42,"method":"item/commandExecution/requestApproval","params":{"threadId":"other"}});
    assert!(
        matches!(&turn.receive(&msg)[..], [Event::Send(reply)] if reply.get("error").is_some())
    );
    let done = json!({"method":"turn/completed","params":{"threadId":"other","turn":{"status":"completed"}}});
    assert!(turn.receive(&done).is_empty());
}

#[test]
fn claude_unknown_requests_fail_and_a_decline_has_no_updated_input() {
    let mut turn = Turn::new(Provider::Claude, ".".into(), None, "go".into());
    let msg = json!({"type":"control_request","request_id":"r","request":{"subtype":"can_use_tool","tool_name":"Edit","input":{"x":1}}});
    let approval = ask(turn.receive(&msg));
    assert_eq!(approval.id(), "r");
    let reply = approval.answer(false);
    assert_eq!(reply["response"]["request_id"], "r");
    assert_eq!(reply["response"]["response"]["behavior"], "deny");
    assert!(reply.pointer("/response/response/updatedInput").is_none());
    let unknown = json!({"type":"control_request","request_id":"x","request":{"subtype":"future"}});
    assert!(
        matches!(&turn.receive(&unknown)[..],[Event::Send(reply)] if reply["response"]["subtype"]=="error")
    );
}

#[test]
fn claude_allow_returns_the_unchanged_input_and_a_cancel_names_the_request() {
    let mut turn = Turn::new(Provider::Claude, ".".into(), None, "go".into());
    let msg = json!({"type":"control_request","request_id":"r","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"ls"}}});
    let reply = ask(turn.receive(&msg)).answer(true);
    assert_eq!(
        reply["response"]["response"],
        json!({"behavior":"allow","updatedInput":{"command":"ls"}})
    );
    let cancel = json!({"type":"control_cancel_request","request_id":"r"});
    assert!(matches!(&turn.receive(&cancel)[..], [Event::Cancelled(id)] if id == "r"));
}

#[test]
fn claude_session_text_tools_and_results() {
    let mut turn = Turn::new(Provider::Claude, ".".into(), None, "go".into());
    let ok = json!({"type":"control_response","response":{"subtype":"success","request_id":"init","response":{}}});
    let out = turn.receive(&ok);
    assert_eq!(sends(&out)[0]["message"]["content"], "go");
    let bad = json!({"type":"control_response","response":{"subtype":"error","request_id":"init","error":"no login"}});
    assert!(matches!(&turn.receive(&bad)[..], [Event::Failed(e)] if e == "no login"));
    let init = json!({"type":"system","subtype":"init","session_id":"s"});
    assert!(matches!(&turn.receive(&init)[..], [Event::Session(s)] if s == "s"));
    let reply = json!({"type":"assistant","message":{"content":[{"type":"text","text":"hi"},{"type":"tool_use","name":"Read"},{"type":"thinking"}]}});
    assert!(matches!(
        &turn.receive(&reply)[..],
        [Event::Text(t), Event::Tool(n)] if t == "hi" && n == "Read"
    ));
    let done = json!({"type":"result","subtype":"success","is_error":false});
    assert!(matches!(&turn.receive(&done)[..], [Event::Done]));
    let refused = json!({"type":"result","subtype":"success","is_error":true,"result":"Credit balance is too low"});
    assert!(
        matches!(&turn.receive(&refused)[..], [Event::Failed(e)] if e == "Credit balance is too low")
    );
    let errors = json!({"type":"result","subtype":"error_during_execution","errors":["one","two"]});
    assert!(matches!(&turn.receive(&errors)[..], [Event::Failed(e)] if e == "- one\n- two"));
    let bare = json!({"type":"result","subtype":"error_max_turns"});
    assert!(matches!(&turn.receive(&bare)[..], [Event::Failed(e)] if e == "error_max_turns"));
    assert!(turn.receive(&json!({"type":"stream_event"})).is_empty());
}

#[test]
fn claude_summaries_are_readable() {
    let mut turn = Turn::new(Provider::Claude, ".".into(), None, "go".into());
    let bash = json!({"type":"control_request","request_id":"1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"cargo test\n--all","description":"Run tests"}}});
    assert_eq!(
        ask(turn.receive(&bash)).summary(),
        [
            (Field::Tool, "Bash".into()),
            (Field::Command, "cargo test\n--all".into()),
            (Field::Other("description".into()), "Run tests".into()),
        ]
    );
    let edit = json!({"type":"control_request","request_id":"2","request":{"subtype":"can_use_tool","tool_name":"Edit","input":{"file_path":"/w/a.rs","old_string":"a\nb","new_string":"c"}}});
    assert_eq!(
        ask(turn.receive(&edit)).summary(),
        [
            (Field::Tool, "Edit".into()),
            (Field::File, "/w/a.rs".into()),
            (Field::Old, "a\nb".into()),
            (Field::New, "c".into()),
        ]
    );
    let other = json!({"type":"control_request","request_id":"3","request":{"subtype":"can_use_tool","tool_name":"mcp__x","input":{"query":{"text":"line 1\nline \"2\"","n":2},"tags":["a"]}}});
    assert_eq!(
        ask(turn.receive(&other)).summary(),
        [
            (Field::Tool, "mcp__x".into()),
            (
                Field::Other("query".into()),
                "n: 2\ntext:\n  line 1\n  line \"2\"".into()
            ),
            (Field::Other("tags".into()), "- a".into()),
        ]
    );
}

#[test]
fn codex_command_approval_shows_command_directory_and_reason() {
    let mut turn = codex_turn();
    let msg = json!({"id":7,"method":"item/commandExecution/requestApproval","params":{"threadId":"t","turnId":"u","itemId":"i","command":"rm -rf target","cwd":"/w","reason":"clean"}});
    let approval = ask(turn.receive(&msg));
    assert_eq!(approval.id(), 7);
    assert_eq!(
        approval.summary(),
        [
            (Field::Command, "rm -rf target".into()),
            (Field::Directory, "/w".into()),
            (Field::Reason, "clean".into()),
        ]
    );
    assert_eq!(
        approval.answer(true),
        json!({"id":7,"result":{"decision":"accept"}})
    );
}

#[test]
fn codex_file_changes_are_shown_and_attached_to_their_approval() {
    let mut turn = codex_turn();
    let started = json!({"method":"item/started","params":{"threadId":"t","turnId":"u","item":{"type":"fileChange","id":"f","status":"inProgress","changes":[{"path":"a.rs","kind":{"type":"add"},"diff":"+one\n"}]}}});
    assert!(matches!(&turn.receive(&started)[..], [Event::Tool(t)] if t == "a.rs\n+one"));
    let updated = json!({"method":"item/fileChange/patchUpdated","params":{"threadId":"t","turnId":"u","itemId":"f","changes":[{"path":"b.rs","kind":{"type":"update","move_path":"c.rs"},"diff":"-x\n+y\n"}]}});
    assert!(turn.receive(&updated).is_empty());
    let request = json!({"id":8,"method":"item/fileChange/requestApproval","params":{"threadId":"t","turnId":"u","itemId":"f","startedAtMs":0}});
    let approval = ask(turn.receive(&request));
    assert_eq!(approval.details["changes"][0]["path"], "b.rs");
    assert_eq!(
        approval.summary(),
        [(Field::Change, "b.rs → c.rs\n-x\n+y".into())]
    );
    assert_eq!(
        approval.answer(false),
        json!({"id":8,"result":{"decision":"decline"}})
    );
    let unknown_item = json!({"id":9,"method":"item/fileChange/requestApproval","params":{"threadId":"t","turnId":"u","itemId":"zz","startedAtMs":0}});
    assert!(
        ask(turn.receive(&unknown_item))
            .details
            .get("changes")
            .is_none()
    );
}

#[test]
fn codex_questions_are_declined_in_the_schemas_own_shape() {
    let mut turn = codex_turn();
    let input = json!({"id":10,"method":"item/tool/requestUserInput","params":{"threadId":"t","turnId":"u","itemId":"i","isBlocking":true,"questions":[{"id":"q","header":"H","question":"Which?"}]}});
    let events = turn.receive(&input);
    assert_eq!(sends(&events)[0], &json!({"id":10,"result":{"answers":{}}}));
    assert!(matches!(&events[1], Event::Declined(q) if q == "Which?"));
    let elicit = json!({"id":11,"method":"mcpServer/elicitation/request","params":{"threadId":"t","message":"Token?"}});
    let events = turn.receive(&elicit);
    assert_eq!(
        sends(&events)[0],
        &json!({"id":11,"result":{"action":"decline"}})
    );
    assert!(matches!(&events[1], Event::Declined(q) if q == "Token?"));
    let other = json!({"id":12,"method":"item/tool/call","params":{"threadId":"t"}});
    assert_eq!(sends(&turn.receive(&other))[0]["error"]["code"], -32601);
}

#[test]
fn codex_turn_outcomes() {
    let mut turn = codex_turn();
    let text = json!({"method":"item/completed","params":{"threadId":"t","item":{"type":"agentMessage","text":"hi"}}});
    assert!(matches!(&turn.receive(&text)[..], [Event::Text(t)] if t == "hi"));
    let command = json!({"method":"item/started","params":{"threadId":"t","item":{"type":"commandExecution","command":"ls"}}});
    assert!(matches!(&turn.receive(&command)[..], [Event::Tool(t)] if t == "ls"));
    let end = |status: &str| json!({"method":"turn/completed","params":{"threadId":"t","turn":{"id":"u","status":status,"error":{"message":"quota exceeded"}}}});
    assert!(matches!(
        &turn.receive(&end("completed"))[..],
        [Event::Done]
    ));
    assert!(matches!(
        &turn.receive(&end("interrupted"))[..],
        [Event::Interrupted]
    ));
    assert!(
        matches!(&turn.receive(&end("failed"))[..], [Event::Failed(e)] if e == "quota exceeded")
    );
    let bare =
        json!({"method":"turn/completed","params":{"threadId":"t","turn":{"status":"failed"}}});
    assert!(matches!(&turn.receive(&bare)[..], [Event::Failed(e)] if e == "failed"));
    let error = json!({"id":3,"error":{"code":-1,"message":"not logged in"}});
    assert!(matches!(&turn.receive(&error)[..], [Event::Failed(e)] if e == "not logged in"));
    // A late interrupt error is not a failure.
    assert!(
        turn.receive(&json!({"id":4,"error":{"code":-1,"message":"no turn"}}))
            .is_empty()
    );
    let no_thread = json!({"id":2,"result":{}});
    assert!(matches!(&turn.receive(&no_thread)[..], [Event::Failed(_)]));
}

#[test]
fn interrupts_name_the_running_turn() {
    let mut codex = Turn::new(Provider::Codex, "/w".into(), None, "go".into());
    assert!(codex.interrupt().is_none());
    let _ = codex.receive(&json!({"id":2,"result":{"thread":{"id":"t"}}}));
    assert!(codex.interrupt().is_none());
    let _ = codex
        .receive(&json!({"method":"turn/started","params":{"threadId":"t","turn":{"id":"u"}}}));
    assert_eq!(
        codex.interrupt(),
        Some(json!({"method":"turn/interrupt","id":4,"params":{"threadId":"t","turnId":"u"}}))
    );
    let claude = Turn::new(Provider::Claude, "/w".into(), None, "go".into());
    assert_eq!(
        claude.interrupt().unwrap()["request"]["subtype"],
        "interrupt"
    );
}
