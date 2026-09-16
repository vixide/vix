# Ai Core

Direct HTTP clients for AI providers (Anthropic, OpenAI-compatible, Ollama)
plus keyring-backed API-key resolution. This is one of *two* ways Vix talks
to an AI assistant — see § Relationship to the CLI path below for how it
fits alongside the original, still-default approach.

## Providers

Three [`Provider`](../src/lib.rs) variants, each a pure request-builder /
response-parser pair (`anthropic`, `openai`, `ollama` modules) plus the one
function that actually performs the HTTP call, `complete` (blocking —
callers run it on a background thread, same as `vix-http-client::send`):

| Provider | Default endpoint | Auth |
| -------- | ----------------- | ---- |
| `anthropic` | `https://api.anthropic.com/v1/messages` | `x-api-key` header |
| `openai` | `https://api.openai.com/v1/chat/completions` | `Authorization: Bearer` |
| `ollama` | `http://localhost:11434/api/generate` | none by default (a self-hosted, authenticated Ollama can still supply a key, sent as `Authorization: Bearer`) |

`openai` is deliberately not limited to `api.openai.com` — the endpoint is
always configurable via `Settings::ai_endpoint`, so this is really "anything
wire-compatible with the OpenAI Chat Completions API": a local gateway, a
self-hosted model server, a proxy in front of another vendor.

Every provider's response parser checks for a JSON `error` field *before*
looking for the expected reply shape and surfaces its message as `Err` —
`complete` treats an HTTP error status the same as a success for this
purpose (the body is still parsed), since all three providers put a useful
message in the error body rather than just a status code.

## Configuration

Four `Settings` fields (`crates/vix-settings/src/lib.rs`, documented in
`docs/configuration/index.md`):

- `ai_provider` — `"cli"` (default) or one of `"anthropic"`/`"openai"`/
  `"ollama"`.
- `ai_endpoint` — overrides a provider's default endpoint; empty uses it.
- `ai_model` — overrides a provider's default model id; empty uses it.
- `ai_api_key_command` — a command whose stdout is the API key (see §
  API-key resolution); empty relies on the OS keyring alone.

## API-key resolution

`secret::resolve(provider, api_key_command)` is the same two-step waterfall
`vix-db`'s `secret` module uses for database passwords
(`crates/vix-db/src/secret.rs`), generalized from one saved connection to
one provider name:

1. `api_key_command`'s stdout, if the setting is non-empty (any command that
   prints the key — `pass show anthropic-api-key`, `op read op://…`, a
   wrapper script; run via `sh -c`, so shell syntax works).
2. The OS keyring, account name = the provider's name (`"anthropic"` /
   `"openai"` / `"ollama"`), service `"vix-ai"` — macOS via the native
   Security framework (the `keyring` crate), Linux via `secret-tool lookup`.

Unlike the DB workbench's credential flow, there is no third, interactive
step: an AI request that needs a key and resolves none simply fails
(`Provider::requires_api_key`) with a clear status-line message rather than
pausing to prompt, since a background AI request has no natural place to
block and ask. Store a key once, externally (`secret-tool store`/`security
add-generic-password`, or point `ai_api_key_command` at a password manager),
and every request after that resolves it automatically.

## Relationship to the CLI path

Before this crate existed, Vix's AI features (the AI menu's Summarize/
Explain/Define/Improve/Annotate, the chat panel, and the DB workbench's
natural-language-to-SQL assistant) worked exactly one way: shell out to
whatever CLI the user configures (`Settings::ai_command`, default `claude -p
{prompt}` — also `codex`, `mistral`, `ollama run …`, anything the user has
installed and already authenticated). That path needs no API key inside Vix
at all and remains the default (`ai_provider = "cli"`) — this crate adds a
second, opt-in path for talking to a provider's HTTP API directly, for
users who would rather configure an endpoint/model/key once than depend on
a separate CLI tool being installed. The host (`app.rs`'s `spawn_ai_cmd`)
branches on `ai_provider` and dispatches to whichever path is configured;
every call site above is unaffected either way, since both paths return the
same "reply text or a failure" shape.
