# Agent Client Protocol client (`vix-acp`)

An Agent Client Protocol (ACP) client for Vix (T635). ACP is the JSON-RPC
protocol Zed defined so an editor can drive a coding agent (Claude Code,
Gemini CLI, Codex, ...) that runs as a child process. Actions:
`ai.agent_start` (AI → Start Agent), `ai.agent_prompt` (AI → Agent Prompt…),
`ai.agent_stop` (AI → Stop Agent), `ai.agent_resume` (AI → Resume Agent),
`ai.agent_plan` (AI → Agent Plan), `ai.agent_plan_dock` (AI → Toggle Agent
Plan Pane). The agents come from the `ai_agents`
setting.

The crate is split so the protocol is pure and the process is optional:

| Module    | Needs `process`? | Contents |
| --------- | ---------------- | -------- |
| `wire`    | no | Newline-delimited JSON framing, JSON-RPC builders, `Incoming` parsing |
| `client`  | no | `Client`, a state machine: no IO, no clock, no threads |
| `policy`  | no | Path containment, and picking a rejecting permission option |
| `display` | no | `sanitize` (strip control sequences) and `TerminalViews`, what the user sees of each terminal |
| `process` | yes | `AgentProcess`: spawn, stdin writer, stdout reader thread |
| `terminal` | yes | `Terminal`: one approved agent command, merged capped output, exit status |

## Framing

ACP over stdio is **one JSON object per line** (`\n`-terminated, no embedded
newlines), not the `Content-Length` envelope LSP uses, so `vix-lsp-core`'s
framer is not reused; only the JSON-RPC 2.0 message shapes are alike. A line
over 16 MiB is dropped (the decoder resynchronizes at the next newline), and
a line that is not JSON is skipped and counted, never fatal.

## Conversation

1. `Client::start` produces `initialize` (protocol version 1; we advertise
   `fs.readTextFile`, `fs.writeTextFile` and `terminal`).
2. The initialize response makes the client send `session/new` with the
   workspace root as `cwd` and no MCP servers.
   With a session to resume (`Client::resuming`) and an agent that advertised
   `agentCapabilities.loadSession`, it sends `session/load` (with the old
   `sessionId`) instead; the agent replays history as `session/update`s and the
   reply yields `Event::Ready` with `Client::resumed()` true. An agent without
   the capability, or one that refuses the id, gets `session/new` instead: a
   resume never fails, it degrades to a fresh session.
3. The session response yields `Event::Ready`; `Client::prompt` then sends
   `session/prompt` and the turn ends with `Event::TurnEnded(stop_reason)`.
4. While a turn runs the agent streams `session/update` notifications:
   `agent_message_chunk`, `agent_thought_chunk`, `tool_call`,
   `tool_call_update`, `plan`. They surface as events. An update whose
   `sessionId` is not ours is ignored.
5. The agent may call back: `session/request_permission`,
   `fs/read_text_file`, `fs/write_text_file`, and the `terminal/*` methods
   (`create`, `output`, `wait_for_exit`, `kill`, `release`). They surface as events that the
   host must answer (`answer_permission`, `answer_read`, `answer_write`). Any
   other agent request is answered `-32601` (method not found) by the client
   itself, so an unknown method never hangs the agent.
6. `Client::cancel` sends `session/cancel`.

## Trust model

Agents can write files and run commands, so:

- **An agent is only ever spawned in a trusted workspace**
  (`App::workspace_trusted`, T132/T627; a remote workspace also needs its own
  trust). An untrusted workspace gets a status message and no process.
- The command comes only from the user's `ai_agents` setting. It is split on
  whitespace and executed as argv with no shell.
- The spawn site (`process.rs`) is behind the `process` feature, which only
  the root `ai` feature enables, and is listed in `tui/tests/spawn_sites.rs`.
- **Nothing the agent asks for is applied silently.**
  - A permission request opens a prompt; only `y`/`yes` selects an allow
    option. Anything else, Esc, a closed prompt, or the agent stopping
    answers with a reject option (or `cancelled`). The default is deny.
  - `fs/write_text_file` never touches the disk. The new content opens as a
    pending `vix-ai-diff` review on the target buffer; hunks are applied only
    when the user accepts them. The agent is told the write succeeded once it
    is queued (ACP has no "pending" reply). A path that does not exist yet is
    allowed when its parent directory exists and the path passes the same
    containment checks: it opens an empty tab that carries the path, the diff
    adds the content, and the file exists on disk only when the user accepts
    and saves. A directory, or a missing parent, is refused.
  - Paths must be absolute and lexically inside the workspace root (no `..`),
    and on disk must also canonicalize inside it, so a symlink cannot lead out.
    Reads prefer the open buffer's unsaved text.
- **Terminals run only on a yes.** `terminal/create` asks to run a command.
  The host checks the working directory (absolute, inside the workspace, an
  existing directory; none given means the root) and refuses an escaping one
  without asking. Otherwise it opens the same prompt as a permission request,
  naming the full command line (`prompt.acp_terminal`); only `y`/`yes` starts
  it, and Esc, anything else, or the agent stopping answers with an error and
  starts nothing. The command is the program plus its arguments, executed as
  argv with no shell and no stdin; stdout and stderr merge into one buffer kept
  to the newest `outputByteLimit` bytes (default 1 MiB, never above 8 MiB, the
  cut moved to a character boundary). At most 8 terminals are open at once.
  `terminal/output` and `terminal/wait_for_exit` (answered when the command
  ends) work only on terminals the user approved; `kill` ends the command and
  keeps the output; `release`, stopping the agent, or dropping the host kills
  and forgets it. An unknown terminal id is an error. The spawn site
  (`terminal.rs`) is listed in `tui/tests/spawn_sites.rs` beside `process.rs`.
- The agent's stderr is discarded. Dropping or stopping the agent kills the
  process.

## Settings

`ai_agents` (list of strings, default empty): each entry is a command line,
for example `"gemini --experimental-acp"`. With one entry `ai.agent_start`
starts it; with several it opens a prompt listing them numbered and starts the
one picked (empty input means the first, anything unlisted starts nothing).
The trust check runs before the prompt opens and again immediately before the
spawn, so no route reaches a process without it.

## Plan pane

`ai.agent_plan` opens a read-only tab (synthetic path `*agent plan*`) holding
the agent's latest plan and every tool call with its status (updates merge by
tool-call id). While the tab is open it is rewritten as events arrive. The AI
panel still gets the transcript lines.

`ai.agent_plan_dock` toggles the same content as a docked, read-only side pane
(`Visible::AGENT_PLAN`, a fixed 36 columns beside the editor like the IM dock;
drawn by `draw_agent_plan_dock` in `tui/src/ui/ai.rs`): the plan, then the tool
calls with their status, newest last, following the agent live. With no agent
running it says so rather than staying blank. It is display only: no key or
click does anything in it.

### Agent terminals

Both surfaces end with a **Terminals** section once the agent has had a command
approved: one entry per terminal, oldest first, `$ <command>  [<status>]`
(running, the exit code, the signal that killed it, or released), a note when
older output was dropped, then the output indented. The output is what the
terminal captured, stdout and stderr merged in arrival order and already capped
at the terminal's byte limit; the plan tab shows each terminal's newest 1000
lines, the dock its newest 12. It updates live: every tick the host copies a
terminal's output into its view only when new output arrived
(`Terminal::output_generation`) or its exit status first became known, and
rewrites the plan tab only when a view changed. No thread is added; the two
reader threads `Terminal` already has do the capture.

Agent output never reaches Vix's own terminal raw. `display::sanitize` runs on
the output (and on the command line, here and in the approval prompt) before it
is stored for display: CSI, OSC, DCS/SOS/PM/APC and two-byte escape sequences
are removed whole (an unterminated string sequence swallows the rest), the
one-byte C1 forms too, and every other control character is dropped except
newline; a tab becomes four spaces, `\r\n` a newline, and a lone `\r` starts the
line over so a progress bar shows its last state. The agent itself still gets
the raw output from `terminal/output`.

The same cleaning applies to everything else the agent sends that Vix shows:
reply text (`agent_message_chunk`), tool-call titles and statuses, plan entries,
and the permission prompt's title and option labels. The host runs every
incoming event through one function (`displayable` in `tui/src/app/acp.rs`)
before it reaches the transcript, the status line, the plan tab or a prompt;
ids, paths and other protocol fields are left as they are.

`kill` keeps the view (it shows the exit status; a dead command prints nothing
more). `release` copies the final output, then freezes the view: a released
view never changes again. At most 16 views are kept; past that the oldest
released one is forgotten. Stopping the agent drops them all.

## Resume

When an agent reports its session (`Event::Ready`) the host remembers the
command and session id, in memory and in the workspace's entry of
`session.toml` (`acp_agent`, `acp_session`; see
[`vix-session`](../../vix-session/spec/index.md)). The saved pair survives
restarts and later workspace saves. `ai.agent_resume` starts that command again
(only if it is still an entry of `ai_agents`, and only in a trusted workspace)
via `Client::resuming`. A saved id never widens what may run: the command is
re-checked against the current setting, and an agent that has forgotten the id
degrades to a fresh session.

## Tests on every OS

The fake agent (`tests/fake_agent.py`) and every command the tests ask it to
run are Python 3 scripts, found as `python3` or else `python` (Windows usually
has only `python`, and its `python3` can be the Store stub). No test needs a
POSIX shell or tools such as `touch`, so the fake-agent tests in this crate and
in `tui/tests/integration/acp.rs` build and run on Windows as well. A line
ending in `\r\n`, as a Windows agent writing in text mode sends, decodes the
same as one ending in `\n`.

## Not yet done

Terminal output is shown in its own section rather than
under the tool call that embedded it (ACP tool-call `content` of type
`terminal` is not parsed yet).

## Implemented by

`crates/vix-acp/src/{wire,client,policy,process,terminal,display}.rs`; the host
is `tui/src/app/acp.rs`, the dock is drawn in `tui/src/ui/ai.rs`. Tests: unit tests in each module,
`crates/vix-acp/tests/fake_agent.rs` (a scripted fake agent over real pipes),
and `tui/tests/integration/acp.rs`.
