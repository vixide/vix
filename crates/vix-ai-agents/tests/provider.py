"""Scripted peers for the native conversation UI tests. No model or network."""
import json
import sys


def recv():
    return json.loads(sys.stdin.readline())


def send(message):
    print(json.dumps(message), flush=True)


provider, mode, *args = sys.argv[1:]
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
    send({"id": thread["id"], "result": {"thread": {"id": "codex-id"}}})
    turn = recv()
    assert turn["params"]["input"][0]["text"] == "one\ntwo"
    send({"id": turn["id"], "result": {"turn": {"id": "turn-1"}}})
    send({"id": 98, "method": "item/commandExecution/requestApproval",
          "params": {"threadId": "codex-id", "command": "test command"}})
    answer = recv()
    assert answer == {"id": 98, "result": {"decision": "decline"}}
    send({"method": "item/completed", "params": {"threadId": "codex-id",
          "item": {"type": "agentMessage", "text": "codex reply"}}})
    send({"method": "turn/completed", "params": {"threadId": "codex-id",
          "turn": {"status": "completed"}}})
else:
    assert provider == "claude"
    assert "stream-json" in args and "stdio" in args
    if mode == "resume":
        assert args[-2:] == ["--resume", "claude-id"]
    init = recv()
    assert init["request"]["subtype"] == "initialize"
    send({"type": "control_response", "response": {"subtype": "success",
          "request_id": init["request_id"], "response": {}}})
    assert recv()["message"]["content"] == "one\ntwo"
    send({"type": "system", "subtype": "init", "session_id": "claude-id"})
    send({"type": "control_request", "request_id": "98", "request": {
          "subtype": "can_use_tool", "tool_name": "Edit", "input": {"x": 1}}})
    response = recv()["response"]
    assert response["request_id"] == "98"
    assert response["response"] == {"behavior": "allow", "updatedInput": {"x": 1}}
    send({"type": "assistant", "message": {"content": [
          {"type": "text", "text": "claude reply"}]}})
    send({"type": "result", "subtype": "success", "is_error": False})
