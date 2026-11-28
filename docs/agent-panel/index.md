# AI Chat Panel and Agents

Vix™ can send editor text to an AI assistant and show you the reply, keep a
conversation with it in the **AI chat panel**, and drive a coding agent over the
Agent Client Protocol (ACP). Everything is on the **AI** menu (`Alt+A`) and in
the command palette, and nothing is sent until you run one of these commands.

This page is the reference. For a guided first session, see
[Tutorial 14: AI and Agents](../tutorials/14-ai-and-agents/index.md). The other
direction, an AI tool reading your workspace through `vix --mcp`, is
[MCP Server](../mcp-server/index.md); AI ghost text is
[Ghost Text](../ghost-text/index.md).

<!-- Sources: crates/vix-menu/src/lib.rs (AI); crates/vix-menu/spec/index.md
(AI Alt+A); crates/vix-ai-panel/spec/ai/index.md;
crates/vix-ai-panel/spec/vix-agent-panel/index.md; crates/vix-acp/spec/index.md. -->

## Turning AI off

AI is optional. Set `ai = "off"` in `config.toml` and restart Vix:

- The **AI** menu disappears (`Alt+A` opens nothing) and no `ai.*` command is
  listed in the palette.
- Every AI request is refused before it is built, however it is reached (a key
  binding, a script), including the Git panel's generated commit message and
  the DB workbench assistant. Each says *"AI is off (ai = "off" in settings);
  nothing was sent"*. Agents cannot be started either.
- Any value other than `"on"` counts as off, so a typo fails closed.

This is a run-time switch. The `ai` Cargo feature removes the AI code from a
build entirely.

<!-- Sources: crates/vix-ai-panel/spec/ai/index.md (Turning AI off);
tui/src/app/ai.rs (spawn_ai_cmd refuses when ai_refused);
crates/vix-i18n/locales/msg.yml (msg.ai_off); tui/tests/integration/ai.rs
(ai_off_hides_the_ai_menu, ai_off_lists_no_ai_command_in_the_palette,
ai_off_refuses_every_ai_action_without_running_anything);
tui/tests/integration/acp.rs (ai_off_refuses_the_agent_actions). -->

## Providers

The `ai_provider` setting picks how Vix talks to the assistant. The AI menu, the
chat panel, the Git commit message, the DB workbench assistant and AI ghost text
all use it.

| `ai_provider` | What Vix does | API key in Vix |
| --- | --- | --- |
| `"cli"` (default) | Runs the `ai_command` template, by default `claude -p {prompt}` | None: the CLI signs in by itself |
| `"anthropic"` | Calls `https://api.anthropic.com/v1/messages`, model `claude-sonnet-5` | Required |
| `"openai"` | Calls `https://api.openai.com/v1/chat/completions`, model `gpt-4o-mini` | Required |
| `"ollama"` | Calls a local Ollama server at `http://localhost:11434/api/generate`, model `llama3.2` | Optional |

`ai_endpoint` and `ai_model` override the endpoint URL (give the full URL) and
the model id; empty uses the defaults above. An unknown provider name falls
back to `"cli"`.

```toml
ai_provider = "anthropic"
ai_model = "claude-sonnet-5"
ai_api_key_command = "pass show anthropic-api-key"
```

### The command-line provider

`ai_command` is a template. `{prompt}` becomes the command's instruction, and
the editor text goes to the command's standard input from a private temporary
file. If the template contains `{file}` instead, that is replaced with the
temporary file's path. Vix quotes both placeholders itself, so do not put quotes
around them; quotes written right around a placeholder are dropped.

| Assistant | `ai_command` |
| --- | --- |
| Claude | `claude -p {prompt}` (default) |
| Codex | `codex exec {prompt}` |
| Mistral | `mistral chat -m {prompt}` |
| Ollama | `ollama run llama3 {prompt}` |

The command always runs on your own machine, also when the workspace is remote
(over SSH or in a container); it then runs in the local temporary directory. A
command that exits with an error, or prints nothing, counts as a failure.

### The HTTP providers

For `anthropic`, `openai` and `ollama`, Vix makes one HTTPS (or, for the
local Ollama default, plain HTTP) request per command, without streaming. The
command's instruction is sent as the system prompt and the editor text as the
user message. A provider's error message is shown as the failure reason.

`openai` works with anything that speaks the OpenAI Chat Completions API (a
local gateway, a self-hosted model server, a proxy): point `ai_endpoint` at it.
It always needs a key, so for a server that needs none, set
`ai_api_key_command = "echo not-needed"`. `ollama` needs no key; a key you do
give it is sent as `Authorization: Bearer`, for an Ollama behind an
authenticating proxy.

<!-- Sources: crates/vix-ai-core/spec/index.md (Providers, Configuration,
Relationship to the CLI path); crates/vix-ai-core/src/lib.rs (Provider::parse,
default_endpoint, default_model, requires_api_key, complete);
crates/vix-ai-core/src/anthropic.rs and ollama.rs (request_body: instruction
as system, input as the user turn; ollama headers); tui/src/app/ai.rs
(spawn_ai_cmd, spawn_ai_cli: private temp file, local authority, temp dir for
a remote workspace, failure on a non-zero exit; spawn_ai_http);
crates/vix-ai-panel/spec/ai/index.md (Configuration, quoting);
docs/reference/settings.md (ai_provider, ai_endpoint, ai_model, ai_command). -->

## API keys

Never put an API key in `config.toml`; there is no setting for it. For an HTTP
provider Vix looks for the key in two places, in order:

1. **`ai_api_key_command`**: a command whose standard output is the key, run
   through `sh -c`, so a password manager works (`pass show …`, `op read …`).
2. **The OS keyring**, service `vix-ai`, account set to the provider name
   (`anthropic`, `openai` or `ollama`): the login Keychain on macOS, Credential
   Manager on Windows, the Secret Service (`secret-tool`) on Linux.

Store a key once, from your shell:

```sh
# macOS; -w with no value asks for the key
security add-generic-password -s vix-ai -a anthropic -w

# Linux; asks for the key
secret-tool store --label="Vix Anthropic key" service vix-ai account anthropic
```

Vix only reads the keyring; it never writes a key there and never prompts for
one. When a provider needs a key and neither place has one, the request is not
sent and the message drawer says *"No API key for anthropic (set
ai_api_key_command, or store one in the OS keyring)"*. An imported settings
bundle never imports `ai_command` or other command-bearing fields.

<!-- Sources: crates/vix-ai-core/spec/index.md (API-key resolution);
crates/vix-ai-core/src/secret.rs (keyring_get and resolve only read);
tui/src/app/ai.rs (spawn_ai_http:
requires_api_key and msg.ai_no_api_key); crates/vix-i18n/locales/msg.yml
(msg.ai_no_api_key); tui/src/app/ai.rs test
the_http_provider_without_an_api_key_reports_and_starts_nothing;
docs/threat-model.md (AI providers and their API keys);
crates/vix-settings-bundle/src/lib.rs (command-bearing fields);
docs/tutorials/14-ai-and-agents/index.md (Step 2 commands). -->

## What is sent, and when

Nothing leaves Vix until you run an AI command. Then:

| Command | What is sent |
| --- | --- |
| **AI → Summarize**, **Explain**, **Annotate**, **Improve** | The selection, or the **whole file** when nothing is selected |
| **AI → Define** | The selection, or the word at (or after) the cursor; never the whole file |
| **AI → Edit Selection with Instruction…** | Your instruction and the selection (a selection is required) |
| **AI → Generate Doc Comment** | The cursor line and the next 40 lines |
| **AI → Chat…** | Your message and the whole conversation so far |
| Ghost text, `inline_completion = "ai"` | Up to 2000 characters before the cursor and 500 after it, each time you pause typing |
| Git panel commit message (`g`) | The staged diff |
| DB workbench assistant (`Ctrl+A` and the other AI keys) | Your question or statement and the schema, never a row (see [DB Workbench](../db/index.md)) |
| An ACP agent | Your prompts, and each workspace file the agent asks to read |

The text goes to the provider you configured: the CLI you chose (which then talks
to its own service), the HTTPS endpoint, or your local Ollama. Vix does not strip
secrets from what you select, so do not send what you would not paste into that
service. Replies are treated as untrusted text: they are shown, opened in a tab
or offered as a diff, never run.

<!-- Sources: crates/vix-ai-panel/spec/ai/index.md (Commands, Input selection);
tui/src/app/ai.rs (ai_generate_doc_comment CONTEXT_LINES = 40,
selected_or_word_text, ai_begin_edit_with_instruction, ai_panel_send with
panel.context()); tui/src/app/git.rs (git_generate_commit_message:
staged_diff); docs/db/index.md (AI helpers see only the schema);
crates/vix-ai-core/spec/index.md (Fill-in-the-middle prompts:
2000 and 500 characters); docs/threat-model.md (AI providers: nothing is sent
until you invoke an AI action, open risks); tui/tests/integration/ai.rs
(generate_doc_comment_inserts_just_above_the_cursor_line,
edit_with_instruction_does_nothing_without_a_selection). -->

## The AI menu

| Item | Result |
| --- | --- |
| **Chat…** (`ai.chat`) | Opens the [chat panel](#the-chat-panel) |
| **Summarize**, **Explain**, **Define** | The reply opens in a new, unsaved tab |
| **Annotate**, **Improve** | The reply replaces the text, through a [diff review](#reviewing-ai-edits) |
| **Edit Selection with Instruction…** | Asks *"Instruction for the AI"*, then always opens a diff review |
| **Generate Doc Comment** | Proposes a comment above the cursor line, through a diff review |
| **Start Agent**, **Prompt Agent…**, **Resume Agent**, **Agent Plan**, **Toggle Agent Plan Pane**, **Stop Agent** | Control an [ACP agent](#coding-agents-acp) |

The request runs in the background. The status line shows *"AI: Explain…"*
while it runs, then *"AI: Explain done"* or *"AI: Explain failed"* (with the
reason, for an HTTP provider). Only one AI request runs at a time: a second says
*"An AI task is already running"*. With nothing to send it says *"Nothing to send
to the AI"*; Edit Selection with Instruction… without a selection says *"Select
text first"*.

<!-- Sources: crates/vix-menu/src/lib.rs (AI); crates/vix-ai-panel/spec/ai/index.md
(Commands, Output modes); tui/src/app/ai.rs (run_ai_action, ai_to_new_tab,
ai_replace_or_diff_dest, ai_begin_edit_with_instruction, report_ai_failure);
crates/vix-i18n/locales/status.yml (status.ai_running, ai_done, ai_failed,
ai_failed_detail, ai_busy, ai_no_input, ai_selection_required);
crates/vix-i18n/locales/prompt.yml (prompt.ai_instruction);
tui/tests/integration/ai.rs (ai_actions_report_missing_input_and_a_busy_assistant,
a_failing_ai_command_is_reported_and_leaves_the_text). -->

## The chat panel

**AI → Chat…** opens a conversation with the assistant. Unlike the one-shot
commands, each turn sends the conversation so far (as `User: …` and
`Assistant: …` turns), so you can ask follow-up questions. If text is selected
when you open the panel, it seeds the input line.

- Type a message and press **Enter**. The title shows **Thinking…** until the
  reply arrives; a failed or empty reply appears as an error turn.
- Blank input is not sent. While a reply is pending, further sends are declined
  with *"An AI task is already running"*.
- While an [ACP agent](#coding-agents-acp) runs, the panel talks to the agent
  instead of the provider.

| Key | Action |
| --- | --- |
| `Enter` | Send the input line |
| `↑` / `↓` | Scroll the transcript one line |
| `PageUp` / `PageDown` | Scroll ten lines |
| `Alt+T` | Open the last reply in a new tab and close the panel |
| `Alt+C` | Copy the last reply to the clipboard (*"Copied reply to clipboard"*) |
| `Esc` | Close the panel |

The hint line under the panel lists the same keys. Closing the panel ends the
conversation; it is not saved.

<!-- Sources: crates/vix-ai-panel/spec/vix-agent-panel/index.md (Behavior,
Keys); crates/vix-ai-panel/src/lib.rs (Panel::context, context_skips_errors_and_formats_turns);
tui/src/app/ai.rs (open_ai_panel, ai_panel_key: Esc drops the panel,
ai_panel_send: acp first, busy,
blank input; ai_panel_last_to_tab closes the panel; ai_panel_copy_last);
crates/vix-i18n/locales/ui.yml (ui.ai_thinking, ui.ai_panel_hint);
crates/vix-i18n/locales/status.yml (status.ai_copied);
tui/tests/integration/ai.rs (chat_panel_sends_shows_the_reply_and_moves_it_to_a_tab);
tui/src/app/ai.rs tests (chat_panel_keys_edit_the_input_scroll_and_close,
sending_ignores_blank_input_and_a_busy_assistant). -->

## Reviewing AI edits

Commands that change your text do not overwrite it. They open **Review AI
edit**, with the proposed change split into hunks:

| Key | Action |
| --- | --- |
| `↑` / `↓` | Move between hunks |
| `Space` | Toggle the highlighted hunk |
| `a` / `r` | Accept all / reject all |
| `Enter` | Apply the accepted hunks as one undoable edit |
| `Esc` | Discard the whole proposal |

Accepted hunks take the new text and rejected hunks keep the old. When the reply changes nothing, no review opens and the
status line says *"AI: no changes proposed"*.

`ai_diff_review = false` applies **Annotate**, **Improve** and **Generate Doc
Comment** directly, as one undoable edit. **Edit Selection with Instruction…**
and agent file writes always open the review.

<!-- Sources: crates/vix-ai-panel/spec/ai/index.md (Reviewing AI edits);
crates/vix-ai-diff/spec/index.md; tui/src/app/ai.rs (open_ai_diff, ai_diff_key,
ai_diff_apply, with_original_line_breaks, ai_replace_or_diff_dest,
ai_begin_edit_with_instruction); crates/vix-i18n/locales/ui.yml (ui.ai_diff,
ui.ai_diff_hint); crates/vix-i18n/locales/status.yml (status.ai_no_change);
tui/tests/integration/ai.rs (ai_diff_review_navigates_toggles_and_applies,
edit_with_instruction_runs_the_typed_text_and_always_opens_a_diff);
tui/src/app/ai.rs tests (a_diff_reply_opens_a_review_that_applies_only_accepted_hunks,
a_doc_comment_reply_inserts_a_line_above_directly_or_through_a_review). -->

## Coding agents (ACP)

An ACP agent (Gemini CLI, Codex and other agents that speak the Agent Client
Protocol) runs as a child process that Vix talks to over its standard input
and output. List each agent's command line in `ai_agents`:

```toml
ai_agents = ["gemini --experimental-acp"]
```

The command is split on whitespace and run without a shell. The agent's own
network traffic and its own tools are its business; Vix only controls what it
can do through ACP.

### Starting and stopping

- **AI → Start Agent** starts it, **only in a trusted workspace**; otherwise
  *"Agent not started: trust this workspace first."* With nothing in
  `ai_agents`: *"No agent configured. Add a command to ai_agents in
  settings."* With several entries it asks *"Start which agent? …"*; type a
  number (empty means the first).
- The chat panel opens and the status line says *"Agent started: …"*, then
  *"Agent ready."* Prompts typed in the panel, or given at **AI → Prompt
  Agent…** (*"Prompt for the agent"*), go to the agent; the reply streams into
  the transcript, and a finished turn reports *"Agent finished (…)."*
- **AI → Agent Plan** opens a read-only tab with the agent's plan and each tool
  call with its status; **AI → Toggle Agent Plan Pane** shows the same beside the
  editor. Both update live and end with a **Terminals** section for approved
  commands.
- **AI → Stop Agent** ends the process (*"Agent stopped."*). Vix saves the
  agent and its session id with the workspace session, so **AI → Resume Agent**
  can continue it after a restart, if the command is still in `ai_agents` and
  the agent supports resuming; otherwise it starts a fresh session.

### What an agent can do

- **Permissions default to deny.** A request shows *"Agent wants to: ….
  Allow once? (y/N)"*. Only `y` or `yes` allows; anything else, Esc, or the
  agent stopping denies.
- **File writes become diffs.** The new content opens in **Review AI edit** on
  that file (*"The agent proposes an edit to …; review it."*). Nothing reaches
  the disk until you accept hunks and save. A new file opens as an unsaved tab
  and exists on disk only when you save it.
- **Commands run only on a yes.** *"Agent wants to run: …. Allow? (y/N)"* names
  the full command line. It runs without a shell, inside the workspace; at most
  eight run at once, and each keeps only the newest 1 MiB of output unless the
  agent asks for more (8 MiB at most).
- **Paths stay inside the workspace.** Reads and writes must be absolute paths
  inside the workspace root, also after following symbolic links. Reads see an
  open buffer's unsaved text and are not prompted, but files that may hold
  secrets (`.env` files, private keys, `.netrc`, `.pgpass`, `credentials*`,
  anything under `.git/` or `.ssh/`) are refused: *"Agent request refused:
  …"*.
- Agent text shown in Vix (replies, plan entries, tool titles, terminal
  output, prompts) has terminal control sequences removed first.

<!-- Sources: crates/vix-acp/spec/index.md (Conversation, Trust model,
Settings, Plan pane, Agent terminals, Resume); tui/src/app/acp.rs
(run_acp_action, acp_start: workspace_trusted, open_ai_panel;
acp_begin_prompt; acp_send_prompt; submit_acp_permission: y/yes only;
displayable); crates/vix-i18n/locales/status.yml (status.acp_untrusted,
acp_no_agent, acp_started, acp_ready, acp_turn_ended, acp_stopped,
acp_edit_pending, acp_refused); crates/vix-i18n/locales/prompt.yml
(prompt.acp_agent, acp_prompt, acp_permission, acp_terminal);
docs/threat-model.md (ACP agents); tui/tests/integration/acp.rs
(an_untrusted_workspace_never_spawns_the_agent,
a_trusted_workspace_streams_a_turn_into_the_ai_panel,
only_y_allows_a_permission_request,
an_agent_write_becomes_a_pending_diff_and_writes_nothing,
an_agent_write_outside_the_workspace_is_refused,
a_terminal_runs_only_after_the_user_says_yes,
a_stopped_agent_can_be_resumed_into_its_session,
the_session_id_survives_a_restart). -->

## Specifications

- [`crates/vix-ai-panel/spec/ai/index.md`](../../crates/vix-ai-panel/spec/ai/index.md):
  the AI menu, the off switch, diff review.
- [`crates/vix-ai-panel/spec/vix-agent-panel/index.md`](../../crates/vix-ai-panel/spec/vix-agent-panel/index.md):
  the chat panel.
- [`crates/vix-ai-core/spec/index.md`](../../crates/vix-ai-core/spec/index.md):
  HTTP providers and API keys.
- [`crates/vix-acp/spec/index.md`](../../crates/vix-acp/spec/index.md): the
  ACP client and its trust model.
- [Threat model](../threat-model.md): what AI features and agents may and may
  not reach.

---

Vix™ and Vix IDE™ are trademarks.

## Native Codex and Claude Code

Build with `cargo run --features ai-agents`. For a desktop window, use
`cargo run --manifest-path gui/Cargo.toml -p vix-egui --features ai-agents`
or select any of the other GUI packages. Install the desired CLI and sign in
on this machine. In a trusted local workspace, open **AI → Agent Conversations…**
and choose a provider or a previous conversation. Type your prompt and press
Enter. To review a tool request, scroll with the arrow keys. Ctrl+Y allows that
request once. N or Esc declines it. Esc outside a request closes the conversation
and stops any active turn. Open it again from the same menu to continue.

In a new local workspace, opening the menu item offers Vix’s workspace trust
dialog. After accepting, open **Agent Conversations…** again to choose a provider.

Native agents use their own tool permissions and can change files in the
workspace. Vix stores your visible messages and the provider resume ID alongside
the editor session file. [Conversation spec](../../crates/vix-ai-agents/spec/index.md)
