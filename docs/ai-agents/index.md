# Native coding agents: Codex and Claude Code

![Codex and Claude Code conversations in Vix: pick a provider, ask about the code, allow one edit](../demos/ai-agents.gif)

**AI → Agent Conversations…** runs a coding conversation with the **Codex**
or **Claude Code** program installed on your machine, inside Vix. Both use
their own login and settings; Vix never asks for or stores an API key for
them. The same conversation panel works in the terminal and in every desktop
GUI.

This is an opt-in build feature, not part of the released binaries.

## Set it up

1. Install the provider you want and sign in once in a terminal:
   `codex` (then `codex login`) or `claude` (then `/login`).
2. Build Vix with the feature:

   ```sh
   cargo run --release --features ai-agents
   ```

   For a desktop window, use any GUI package with the same feature, for
   example
   `cargo run --manifest-path gui/Cargo.toml -p vix-egui --features ai-agents`.
3. Keep the AI menu on (`ai = "on"`, the default). `ai = "off"` hides it,
   this item included.

## Use it

1. Open a local folder and choose **AI → Agent Conversations…**. The first
   time in a folder, Vix asks whether you trust it; accept, then choose the
   menu item again. Remote (SSH) folders are not supported.
2. Pick **New conversation · Codex**, **New conversation · Claude Code**, or one
   of your earlier conversations in this folder (newest first, titled by
   your first prompt). Opening the list or an old conversation sends nothing.
3. Type a prompt and press **Enter**. Replies appear as they arrive, with one
   line for each tool the agent starts.
4. When the agent asks to run a command or change a file, the panel shows the
   whole request. Scroll it with the arrow keys, then press **Ctrl+Y** to
   allow that one request, or **N** or **Esc** to decline it.
5. Press **Esc** (outside a request) to close the panel. This stops the
   agent, including a reply in progress. Reopen the conversation from the
   same menu to continue where you left off; the agent resumes its own
   session.

## Give a conversation its own Git worktree

In a Git repository with at least one commit, pick **New worktree
conversation** for Codex or Claude Code (the rows are missing in a folder
that is not committed yet). The first prompt starts a new branch at your current commit. Uncommitted
edits, untracked files and unsaved Vix buffers stay in your original checkout.
If you opened a subfolder, the provider starts in its counterpart in the new
worktree. The ordinary **New conversation** rows use the current folder.

The conversation panel shows its `vix-agent-<UUID>` branch. Close the panel
and select its saved conversation in the original workspace to continue with
the same files. Use **Git → Worktree → List…** or `git worktree list` to find
the checkout and review or commit edits there. Merge the branch when ready.
The checkout persists until you remove it. Removing it or switching its
branch prevents further turns in that conversation.

Worktrees require the default `git` feature alongside `ai-agents`. The
checkout lives beside the private conversation file, outside the repository.

## What to know

- **The agent can change your files.** It runs in the folder with its own
  sandbox and permission rules (Codex: `workspace-write`, asking on
  request; Claude Code: its `default` permission mode plus your settings).
  Vix shows each request it is asked about and never answers one for you.
- **Conversations are per provider.** A Claude Code conversation cannot be
  continued with Codex or the other way round; pick the provider again for
  a new conversation.
- **Where it is stored.** Your prompts, the visible replies and the
  provider's resume ID are kept in private files beside Vix's session file,
  one folder per workspace. The agent keeps its own full history in its
  own directory, as it does in a terminal.
- **One window at a time.** A conversation open in one Vix cannot be opened
  in another until the first closes it.

## See also

- [Agent panel](../agent-panel/index.md) for ACP agents (**AI → Start Agent**).
- [MCP server](../mcp-server/index.md) to give an agent a read-only view of
  a workspace.
- [Threat model](../threat-model.md#native-codex-and-claude-code-conversations).
- The specification: [`spec/ai-agents`](../../spec/ai-agents/index.md).

---

Vix™ and Vix IDE™ are trademarks.
