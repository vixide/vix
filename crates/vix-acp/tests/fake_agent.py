#!/usr/bin/env python3
"""A scripted fake ACP agent for tests. Never touches the network.

Usage: fake_agent.py LOGFILE

Every line received from the client is appended to LOGFILE. The prompt text
chooses the behavior:

  plain          stream a message chunk, a tool call and a plan, then end the turn
  perm           ask permission, log the outcome, end the turn
  write PATH TXT ask the client to write TXT to PATH (via fs/write_text_file)
  read PATH      ask the client to read PATH, stream what came back
  termin DIR CMD ARGS  like term, with DIR as the working directory
  term CMD ARGS  ask the client for a terminal running CMD ARGS, wait for it to
                 exit, fetch its output, release it, and log every reply
"""
import json
import sys

log = open(sys.argv[1], "a", buffering=1)
next_id = [1000]


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def update(sid, u):
    send({"jsonrpc": "2.0", "method": "session/update",
          "params": {"sessionId": sid, "update": u}})


def call(method, params):
    next_id[0] += 1
    rid = next_id[0]
    send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params})
    while True:
        line = sys.stdin.readline()
        if not line:
            sys.exit(0)
        log.write(line)
        msg = json.loads(line)
        if msg.get("id") == rid and "method" not in msg:
            return msg


sid = "S1"
for line in sys.stdin:
    log.write(line)
    msg = json.loads(line)
    method = msg.get("method")
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {
            "protocolVersion": 1, "agentCapabilities": {"loadSession": True}}})
    elif method == "session/load":
        sid = msg["params"]["sessionId"]
        send({"jsonrpc": "2.0", "id": msg["id"], "result": None})
    elif method == "session/new":
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {"sessionId": sid}})
    elif method == "session/prompt":
        text = msg["params"]["prompt"][0]["text"]
        word = text.split(" ", 2)
        if text == "plain":
            update(sid, {"sessionUpdate": "agent_message_chunk",
                         "content": {"type": "text", "text": "hel"}})
            update(sid, {"sessionUpdate": "agent_message_chunk",
                         "content": {"type": "text", "text": "lo"}})
            update(sid, {"sessionUpdate": "tool_call", "toolCallId": "t1",
                         "title": "Reading", "status": "pending"})
            update(sid, {"sessionUpdate": "plan", "entries": [
                {"content": "step one", "priority": "high", "status": "pending"}]})
        elif text == "perm":
            r = call("session/request_permission", {
                "sessionId": sid,
                "toolCall": {"toolCallId": "t2", "title": "Run rm"},
                "options": [
                    {"optionId": "yes", "name": "Allow", "kind": "allow_once"},
                    {"optionId": "no", "name": "Reject", "kind": "reject_once"}]})
            log.write("OUTCOME " + json.dumps(r["result"]["outcome"]) + "\n")
        elif word[0] == "write":
            r = call("fs/write_text_file",
                     {"sessionId": sid, "path": word[1], "content": word[2]})
            log.write("WRITE " + ("error" if "error" in r else "ok") + "\n")
        elif word[0] in ("term", "termin"):
            argv = text.split(" ")[1:]
            params = {"sessionId": sid}
            if word[0] == "termin":
                params["cwd"] = argv.pop(0)
            params["command"] = argv[0]
            params["args"] = argv[1:]
            r = call("terminal/create", params)
            if "error" in r:
                log.write("TERM denied " + r["error"]["message"] + "\n")
            else:
                tid = r["result"]["terminalId"]
                w = call("terminal/wait_for_exit",
                         {"sessionId": sid, "terminalId": tid})
                o = call("terminal/output", {"sessionId": sid, "terminalId": tid})
                log.write("TERM exit " + json.dumps(w["result"]) + "\n")
                log.write("TERM output " + json.dumps(o["result"]["output"]) + "\n")
                call("terminal/release", {"sessionId": sid, "terminalId": tid})
                log.write("TERM released\n")
        elif word[0] == "read":
            r = call("fs/read_text_file", {"sessionId": sid, "path": word[1]})
            body = r.get("result", {}).get("content", "ERROR")
            update(sid, {"sessionUpdate": "agent_message_chunk",
                         "content": {"type": "text", "text": body}})
        send({"jsonrpc": "2.0", "id": msg["id"], "result": {"stopReason": "end_turn"}})
