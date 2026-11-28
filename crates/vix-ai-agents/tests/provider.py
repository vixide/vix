"""Scripted peers for the native conversation UI tests. No model or network.

Usage: provider.py <codex|claude> <mode> <the provider's own argv...>
Modes: new, resume, queue, filechange, failed, interrupted, is_error, stop,
exit, garbage. Markers for the test are written to the working directory.
"""
import json
import os
import sys


def recv():
    line = sys.stdin.readline()
    if not line:
        sys.exit(0)
    return json.loads(line)


def send(message):
    print(json.dumps(message), flush=True)


def marker(name):
    with open(name, "w") as f:
        f.write(name)


def wait_for_eof():
    """After an interrupt: note it, then wait for Vix to close stdin."""
    marker("fake-interrupted")
    while sys.stdin.readline():
        pass
    marker("fake-eof")
    sys.exit(0)


def misbehave(mode):
    """Modes shared by both providers once the prompt has arrived."""
    if mode in ("new", "resume") and os.path.isfile("worktree-test.txt"):
        with open("worktree-test.txt", "a") as tracked:
            tracked.write(provider + "\n")
    if mode == "exit":
        sys.stderr.write("noise\n" * 2000 + "fatal: boom\n")
        sys.stderr.flush()
        sys.exit(3)
    if mode == "garbage":
        print("this is not json", flush=True)
        sys.stdout.write("[" * (17 * 1024 * 1024) + "\n")
        sys.stdout.flush()


provider, mode, *args = sys.argv[1:]


def note(method, **params):
    send({"method": method, "params": {"threadId": "codex-id", "turnId": "turn-1", **params}})


if provider == "codex":
    assert args == ["app-server"]
    init = recv()
    assert init["method"] == "initialize"
    send({"id": init["id"], "result": {}})
    assert recv()["method"] == "initialized"
    thread = recv()
    if mode == "resume":
        assert thread["method"] == "thread/resume"
        assert thread["params"]["threadId"] == "codex-id"
    else:
        assert thread["method"] == "thread/start"
    assert os.path.samefile(thread["params"]["cwd"], os.getcwd())
    # The user's Codex config decides the sandbox and approval policy.
    assert set(thread["params"]) <= {"cwd", "threadId"}, thread
    send({"id": thread["id"], "result": {"thread": {"id": "codex-id"}}})
    turn = recv()
    assert turn["params"]["input"][0]["text"] == "one\ntwo"
    send({"id": turn["id"], "result": {"turn": {"id": "turn-1"}}})
    misbehave(mode)
    if mode == "stop":
        note("item/started", item={"type": "commandExecution", "command": "working"})
        stop = recv()
        assert stop["method"] == "turn/interrupt", stop
        assert stop["params"] == {"threadId": "codex-id", "turnId": "turn-1"}
        wait_for_eof()
    if mode == "filechange":
        note("item/started", item={"type": "fileChange", "id": "fc", "status": "inProgress",
             "changes": [{"path": "src/a.rs", "kind": {"type": "add"}, "diff": "+hello"}]})
        note("item/fileChange/patchUpdated", itemId="fc",
             changes=[{"path": "src/b.rs", "kind": {"type": "update"}, "diff": "-old\n+new"}])
        send({"id": 97, "method": "item/fileChange/requestApproval",
              "params": {"threadId": "codex-id", "turnId": "turn-1", "itemId": "fc",
                         "startedAtMs": 0, "reason": "apply the patch"}})
        assert recv() == {"id": 97, "result": {"decision": "accept"}}
    elif mode in ("new", "resume"):
        send({"id": 98, "method": "item/commandExecution/requestApproval",
              "params": {"threadId": "codex-id", "command": "test command"}})
        answer = recv()
        assert answer == {"id": 98, "result": {"decision": "decline"}}
    if mode == "failed":
        note("turn/completed", turn={"id": "turn-1", "status": "failed",
             "error": {"message": "quota exceeded"}})
    elif mode == "interrupted":
        note("turn/completed", turn={"id": "turn-1", "status": "interrupted"})
    else:
        note("item/completed", item={"type": "agentMessage", "text": "codex reply"})
        note("turn/completed", turn={"id": "turn-1", "status": "completed"})
else:
    assert provider == "claude"
    # Only the permission prompts move to Vix; the user's mode stays.
    assert "--permission-mode" not in args
    assert args[:8] == ["-p", "--input-format", "stream-json", "--output-format",
                        "stream-json", "--verbose", "--permission-prompt-tool", "stdio"], args
    if mode == "resume":
        assert args[-2:] == ["--resume", "claude-id"]
    init = recv()
    assert init["request"]["subtype"] == "initialize"
    send({"type": "control_response", "response": {"subtype": "success",
          "request_id": init["request_id"], "response": {}}})
    assert recv()["message"]["content"] == "one\ntwo"
    send({"type": "system", "subtype": "init", "session_id": "claude-id"})
    misbehave(mode)
    if mode == "stop":
        send({"type": "assistant", "message": {"content": [
              {"type": "tool_use", "name": "working"}]}})
        stop = recv()
        assert stop["type"] == "control_request", stop
        assert stop["request"]["subtype"] == "interrupt"
        wait_for_eof()
    if mode == "queue":
        for rid, command in (("98", "echo one\necho two"), ("99", "rm -rf /tmp/x"),
                             ("100", "never shown")):
            send({"type": "control_request", "request_id": rid, "request": {
                  "subtype": "can_use_tool", "tool_name": "Bash", "input": {"command": command}}})
        send({"type": "control_cancel_request", "request_id": "100"})
        first, second = recv()["response"], recv()["response"]
        assert first["request_id"] == "98" and first["response"]["behavior"] == "allow", first
        assert second["request_id"] == "99" and second["response"]["behavior"] == "deny", second
    elif mode in ("new", "resume"):
        send({"type": "control_request", "request_id": "98", "request": {
              "subtype": "can_use_tool", "tool_name": "Edit", "input": {"x": 1}}})
        response = recv()["response"]
        assert response["request_id"] == "98"
        assert response["response"] == {"behavior": "allow", "updatedInput": {"x": 1}}
    send({"type": "assistant", "message": {"content": [
          {"type": "text", "text": "claude reply"}]}})
    if mode == "is_error":
        send({"type": "result", "subtype": "success", "is_error": True,
              "result": "Credit balance is too low"})
    else:
        send({"type": "result", "subtype": "success", "is_error": False})
