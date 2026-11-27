# Tutorial 14: AI and Agents

Vix can send editor text to an AI assistant and show you the reply, drive a
coding agent that runs as a child process, and serve a read-only view of your
workspace to an AI tool over the Model Context Protocol (MCP). All three are
optional, and none of them runs until you ask it to.

This tutorial sets up a provider, uses the **AI** menu on the demo
workspace, reviews an AI edit hunk by hunk, starts an agent over the Agent
Client Protocol (ACP), and connects an MCP client with `vix --mcp`. Read the
privacy section before you send anything you would not want to leave your
machine.

Open the demo workspace:

```sh
cd tui/examples/demo-workspace
vix .
```

## What is sent where

Every AI feature sends text somewhere. Know where before you start:

| Feature | What leaves Vix | Where it goes |
| --- | --- | --- |
| **Summarize**, **Explain**, **Annotate**, **Improve** | The selection, or the **whole file** when nothing is selected | Your configured provider |
| **Define** | The selection, or one word at the cursor | Your configured provider |
| **Edit Selection with Instruction…** | Your instruction and the selection (a selection is required) | Your configured provider |
| **Generate Doc Comment** | The cursor line and up to 40 lines after it | Your configured provider |
| **Chat…** | Your message and the conversation so far | Your configured provider |
| Ghost text with `inline_completion = "ai"` | Up to 2000 characters before the cursor and 500 after it | Your configured provider |
| An ACP agent | Your prompts, and any workspace file the agent asks to read | The agent process on your machine, which then talks to its own service |
| `vix --mcp` | Nothing by itself; it answers the MCP client that started it | That client decides what it sends on |

"Your configured provider" is either a command-line tool (the default) or an
HTTPS API, chosen by `ai_provider` below. The Git panel's generated commit
message and the DB workbench assistant use the same provider.

To turn all of this off, set `ai = "off"` in `config.toml` and restart Vix.
The **AI** menu and every AI command palette entry disappear, and any AI
request that is still reached (a key binding, a script) shows *"AI is off
(ai = "off" in settings); nothing was sent"*.

## Step 1: Choose a provider

AI settings live in `config.toml`: `~/.config/vix/config.toml` on Linux,
`~/Library/Application Support/rs.vix/config.toml` on macOS, or
`%APPDATA%\vix\config\config.toml` on Windows.

| Key | Default | Meaning |
| --- | --- | --- |
| `ai` | `"on"` | Master switch. Any value other than `"on"` counts as off. |
| `ai_provider` | `"cli"` | `"cli"` runs `ai_command`. `"anthropic"`, `"openai"` or `"ollama"` call that HTTP API directly. An unknown value falls back to `"cli"`. |
| `ai_command` | `"claude -p {prompt}"` | The command the `cli` provider runs. |
| `ai_endpoint` | `""` | Overrides the HTTP provider's endpoint URL. Empty uses the default. |
| `ai_model` | `""` | Overrides the HTTP provider's model id. Empty uses the default. |
| `ai_api_key_command` | `""` | A command that prints the API key. |
| `ai_diff_review` | `true` | Show Annotate, Improve and Generate Doc Comment as a reviewable diff. |

### The default: a command-line assistant

With `ai_provider = "cli"`, Vix holds no API key at all. It runs the CLI you
already installed and signed in to, and that CLI handles authentication and
the network. The default runs the `claude` CLI, so if `claude -p "hello"`
works in your shell, the AI menu already works.

To use another CLI, change the template:

```toml
ai_command = "codex exec {prompt}"
```

`{prompt}` becomes the action's instruction and the editor text arrives on
stdin. If the template contains `{file}`, that is replaced with the path of a
temporary file holding the text instead. Vix quotes both placeholders itself,
so do **not** put quotes around them.

### An HTTP API: Anthropic, OpenAI or Ollama

To call a provider's API directly, set the provider and, ideally, the model:

```toml
ai_provider = "anthropic"
ai_model = "claude-sonnet-5"
```

| Provider | Default endpoint | Default model |
| --- | --- | --- |
| `anthropic` | `https://api.anthropic.com/v1/messages` | `claude-sonnet-5` |
| `openai` | `https://api.openai.com/v1/chat/completions` | `gpt-4o-mini` |
| `ollama` | `http://localhost:11434/api/generate` | `llama3.2` |

<!-- Facts: crates/vix-ai-core/src/lib.rs (Provider::parse, default_endpoint,
default_model, requires_api_key); crates/vix-ai-core/src/ollama.rs (headers);
tui/src/app/ai.rs (the msg.ai_no_api_key check). -->

Requests to `anthropic` and `openai` go over HTTPS to that endpoint, with
your key in a header. The `ollama` provider talks to an Ollama server's
`/api/generate` endpoint, by default on your own machine over plain HTTP. It
needs no key: when one is found (next step) it is sent as a bearer token, for
an Ollama server behind authentication.

### A local model endpoint

For an Ollama server, `ai_provider = "ollama"` is all it takes; set
`ai_model` to a model the server has. The `openai` provider works with anything that speaks the OpenAI Chat
Completions API, including a model server on your own machine. Point
`ai_endpoint` at its full chat-completions URL:

```toml
ai_provider = "openai"
ai_endpoint = "http://localhost:8080/v1/chat/completions"
ai_model = "your-local-model-name"
ai_api_key_command = "echo not-needed"
```

The `openai` provider always requires a key, so a server that needs none
still needs `ai_api_key_command` to print something. That placeholder is not
a secret; for a real key, use the next step. With a local endpoint, the text
stays on your machine.

## Step 2: Give Vix the API key without writing it down

Never paste an API key into `config.toml`. Vix looks for the key in two
places, in order, and needs no key at all for the `cli` and `ollama` providers:

1. **`ai_api_key_command`.** Its standard output is the key. It runs through
   `sh -c`, so a password manager works:

   ```toml
   ai_api_key_command = "pass show anthropic-api-key"
   ```

2. **The OS keyring**, service `vix-ai`, account set to the provider name
   (`anthropic`, `openai` or `ollama`). Store the key once, from your shell:

   ```sh
   # macOS (login Keychain); -w with no value asks for the key
   security add-generic-password -s vix-ai -a anthropic -w

   # Linux (Secret Service); asks for the key
   secret-tool store --label="Vix Anthropic key" service vix-ai account anthropic
   ```

   On Windows, Vix reads Windows Credential Manager; using
   `ai_api_key_command` there is the simplest route.

Vix never prompts for a key. If neither place has one, an `anthropic` or `openai`
request fails with
*"No API key for anthropic (set ai_api_key_command, or store one in the OS
keyring)"* in the message drawer.

## Step 3: Ask about code

Open `tui/examples/demo-workspace/rust-app/src/main.rs`. The **AI** menu
(`Alt+A`) has the one-shot commands at the top:

1. Select the `word_counts` function, from `fn word_counts` to its closing
   `}`.
2. Choose **AI → Explain** (`ai.explain`). The status line shows *"AI:
   Explain…"* while the request runs in the background.
3. When it finishes, the explanation opens in a new, unsaved editor tab.

**Summarize** (`ai.summarize`) works the same way. **Define** (`ai.define`)
sends only the selection or the word at the cursor, never the whole file. Only
one AI request runs at a time; a second one is declined until the first
returns.

## Step 4: Chat

Choose **AI → Chat…** (`ai.chat`). If text is selected, it seeds the input
line.

1. Type `Why does this count "The" and "the" separately?` and press
   **Enter**.
2. The title shows **Thinking…** until the reply arrives.
3. Ask a follow-up. Each turn includes the earlier conversation, so the
   assistant remembers what you discussed.

| Key | Action |
| --- | --- |
| `Enter` | Send the input line |
| `↑` / `↓`, `PageUp` / `PageDown` | Scroll the transcript |
| `Alt+T` | Open the last reply in a new tab |
| `Alt+C` | Copy the last reply to the clipboard |
| `Esc` | Close the panel |

## Step 5: Review an AI edit before it lands

The commands that change your text do not overwrite it. They open a
**Review AI edit** window with the proposed change split into hunks.

1. Select the body of `word_counts`.
2. Choose **AI → Edit Selection with Instruction…**
   (`ai.edit_with_instruction`). At the *"Instruction for the AI"* prompt,
   type `Lowercase each word before counting it` and press **Enter**.
3. The review opens. Its title counts accepted hunks against total hunks.

| Key | Action |
| --- | --- |
| `↑` / `↓` | Move between hunks |
| `Space` | Toggle the highlighted hunk |
| `a` / `r` | Accept all / reject all |
| `Enter` | Apply the accepted hunks as one undoable edit |
| `Esc` | Discard the whole proposal |

Accept the hunk, press **Enter**, then **Ctrl+Z** once to see that the whole
change undoes as one step.

The other editing commands use the same review:

- **AI → Annotate** (`ai.annotate`) and **AI → Improve** (`ai.improve`) act on
  the selection, or the whole file when nothing is selected.
- **AI → Generate Doc Comment** (`ai.generate_doc_comment`): put the cursor on
  the `fn word_counts` line and run it. The proposal only inserts a comment
  above that line; it never changes existing text.

Setting `ai_diff_review = false` applies Annotate, Improve and Generate Doc
Comment directly. **Edit Selection with Instruction…** always opens the
review, whatever that setting says.

## Step 6: Run a coding agent over ACP

An ACP agent is a coding agent (Gemini CLI, Codex and others that speak the
Agent Client Protocol) that Vix starts as a child process and talks to over
stdin and stdout. It can propose file edits and ask to run commands, so Vix
gates it.

### Configure the agent

List each agent's command line in `ai_agents`. It is split on whitespace and
run without a shell. Check your agent's own docs for its ACP flag; for
example:

```toml
ai_agents = ["gemini --experimental-acp"]
```

### Trust the workspace

An agent only starts in a trusted workspace. Otherwise **AI → Start Agent**
says *"Agent not started: trust this workspace first."*

Vix records trust when you answer **y** at the **Trust Workspace?** prompt.
That prompt appears at startup when the workspace has a `.vix/settings.toml`,
scripts under `.vix/scripts/`, or an environment source such as `.envrc`. The
demo workspace has none of these, so give it an empty project settings file
and restart Vix:

```sh
mkdir -p .vix
echo "# project settings" > .vix/settings.toml
vix .
```

Press **y** at the prompt. **n** or **Esc** declines, and Vix remembers that
answer too. Keys typed in the first moments after the prompt opens are
ignored, so a stray **Enter** cannot approve it.

### Start it and give it a task

1. Choose **AI → Start Agent** (`ai.agent_start`). With several entries in
   `ai_agents`, Vix asks *"Start which agent?"* and you type a number.
2. The chat panel opens and the status line shows *"Agent started: …"*. While
   an agent runs, the chat panel talks to the agent instead of the provider.
3. Type `Make word_counts ignore case` and press **Enter**. You can also send
   prompts with **AI → Prompt Agent…** (`ai.agent_prompt`).
4. Watch its progress with **AI → Agent Plan** (`ai.agent_plan`, a read-only
   tab) or **AI → Toggle Agent Plan Pane** (`ai.agent_plan_dock`, a side
   pane). Both show the plan, each tool call's status, and any approved
   commands' output.

### What the agent can and cannot do

- **Permissions are deny by default.** A permission request shows *"Agent
  wants to: … Allow once? (y/N)"*. Only `y` or `yes` allows it. Anything
  else, **Esc**, or closing the prompt denies it.
- **Commands run only on a yes.** *"Agent wants to run: … Allow? (y/N)"*
  names the full command line. It runs without a shell, inside the
  workspace.
- **File writes are diffs, not writes.** When the agent writes a file, Vix
  opens the new content in the same **Review AI edit** window from Step 5,
  and the status line says *"The agent proposes an edit to …; review it."*
  Nothing reaches the disk until you accept hunks and save the file. Only one
  proposal can wait for review at a time.
- **Reads stay inside the workspace.** Paths outside the workspace root,
  including through a symbolic link, are refused. A read inside it is not
  prompted, and it sees an open buffer's unsaved text.

Stop the agent with **AI → Stop Agent** (`ai.agent_stop`). Vix saves the
agent's session id, so **AI → Resume Agent** (`ai.agent_resume`) can pick it
up again later, if the agent supports resuming and is still listed in
`ai_agents`.

## Step 7: Let an AI tool read your workspace with `vix --mcp`

The other direction: an MCP client, such as Claude Code, starts `vix --mcp`
and asks it about the workspace. It does not open the editor.

Register it with Claude Code:

```sh
claude mcp add vix -- vix --mcp /path/to/project
```

Or, for a client that takes a JSON server list:

```json
{
  "mcpServers": {
    "vix": { "command": "vix", "args": ["--mcp", "/path/to/project"] }
  }
}
```

The server is read-only. Its tools are:

| Tool | Returns |
| --- | --- |
| `list_open_buffers` | The files your last Vix session had open in this workspace |
| `read_buffer` | One file's saved text |
| `document_symbols` | Declarations with line numbers, from Vix's offline scan |
| `outline` | Markdown headings, Org headlines, or a code file's symbols |
| `diagnostics` | Unresolved merge-conflict blocks (no language server runs) |
| `db_schema` | Tables and columns of SQLite files in the workspace |
| `db_connections`, `db_tables`, `db_describe`, `db_query` | Read-only access to DB workbench connections you marked **Share with MCP** |

It sees files as saved on disk, not unsaved edits in a running Vix. It
refuses paths outside the workspace and files that may hold secrets, such as
`.env` files, private keys, `.netrc`, `.pgpass`, and anything under `.git/`
or `.ssh/`. It never reads API keys. It opens no network connection except to
the server of a database connection you shared, during a call on it.

## Where to go next

- [AI Chat Panel](../../agent-panel/index.md): the chat panel and the
  `ai = "off"` switch.
- [MCP server](../../mcp-server/index.md): setting up `vix --mcp` and sharing
  database connections.
- [Inline Suggestions (Ghost Text)](../../ghost-text/index.md): AI or
  language-server suggestions as you type.
- [`crates/vix-acp/spec/index.md`](../../../crates/vix-acp/spec/index.md):
  the full ACP trust model, terminals and resume.
- [`crates/vix-ai-core/spec/index.md`](../../../crates/vix-ai-core/spec/index.md):
  providers and API-key resolution.
- [Configuration](../../configuration/index.md): every AI setting.

---

**Previous:** [Tutorial 13 — Scripting with Rhai](../13-scripting-with-rhai/index.md)  
**Next:** [Tutorial 15 — Large Files and Logs](../15-large-files-and-logs/index.md)

---

Vix™ and Vix IDE™ are trademarks.
