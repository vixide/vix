# Coding agents in Vix, Orbit and T3 Code

At Vix revision `e78fdb58`, **AI → Start Agent** launches a configured
ACP program in a trusted workspace. The editor owns its process, permission
prompts, plan and edit review. **Resume Agent** remembers the last command
and provider session ID for each workspace. The fake-agent integration
suite exercises this path over real pipes, including after restarting Vix.
[Vix ACP contract](../../crates/vix-acp/spec/index.md) ·
[Integration tests](../../tui/tests/integration/acp.rs)

## Two design decisions

Orbit gives a conversation its own thread ID. Its provider adapter receives
both that ID and an optional provider-specific resume token, plus the working
directory. The adapter reports events tagged with the thread. When a user
switches providers, Orbit prepares a bounded text summary for the next agent.
For Vix, a bookmark for an ACP session can support resuming with its original
provider. A cross-provider handoff needs its own explicit context transfer.
[Orbit provider interface](https://github.com/countradooku/orbit/blob/main/crates/orbit-core/src/provider/mod.rs) ·
[Orbit session implementation](https://github.com/countradooku/orbit/blob/main/crates/orbit-core/src/engine/sessions.rs)

T3 Code puts processes, files, Git and approvals on the machine hosting the
workspace. Clients reconnect to that server. It also distinguishes a completed
provider turn from its pending Git checkpoint. Both distinctions matter in
Vix. Saving a resume token lets the user restart an agent. Keeping a running
command or an unanswered approval alive after closing Vix needs a separate
process. Restoring a checkpoint would have to reconcile the provider's
history, the worktree and Vix's unsaved buffers.
[T3 Code architecture](https://github.com/pingdotgg/t3code/blob/main/docs/internals/01_architecture.md)

## Recommended sequence

| User outcome | Starting point in Vix | Design decision |
| --- | --- | --- |
| Return to an earlier conversation | ACP already implements start, prompt, load, stop and reviewed edits | Expose workspace-scoped conversation selection through the AI menu. Native Claude Code and Codex require dedicated adapters. Keep credentials with the installed agent. |
| Work on two changes at once | Git already supports adding and switching worktrees | Bind each running thread, request for approval and pending diff to its own workspace before allowing concurrent edits. |
| Review and publish a turn | Vix has Git diffs, staging and a terminal | Capture turn boundaries and test restoration with dirty buffers before offering rewind. Keep publishing an explicit user action. |
| Use a different provider or delegate | Vix can start several configured ACP programs one at a time | Add a provider-neutral thread ID and let the user inspect the outgoing handoff context. |
| Reconnect from another device or an MCP client | The existing MCP server exposes read-only tools over stdio | Give a local service ownership of processes and approval state before adding authenticated remote clients or mutating MCP tools. |
| Add secret entry, usage, voice, text generation and device previews | These require different provider capabilities and platform integrations | Prioritize each by an actual workflow. A secret prompt needs a private reply channel, and usage needs a provider-reported unit and time period. |

See the [Git worktree contract](../../crates/vix-git/spec/index.md),
[Vix MCP contract](../mcp-server/index.md) and
[T3 Code feature overview](https://github.com/pingdotgg/t3code).

## First implementation on ai-agents

The selected scope is a single Vix-drawn conversation UI for native Codex
and Claude Code in the TUI and all seven GUI front ends. Both providers use
their existing logins. The optional `vix-ai-agents` crate owns their message
formats and saved conversations. The ACP client continues to serve configured
ACP agents. A future cross-provider handoff needs a user-visible context
transfer even when both providers share the same UI.
[Native conversation contract](../../crates/vix-ai-agents/spec/index.md)
