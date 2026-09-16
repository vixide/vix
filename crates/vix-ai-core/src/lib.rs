//! Direct HTTP clients for AI providers, as an alternative to the CLI
//! shell-out `vix`'s AI features have always used (see [`Settings::ai_command`]
//! in `vix-settings` — `ai_provider = "cli"` is still the default, unchanged).
//!
//! Selecting a provider here (`"anthropic"`, `"openai"`, or `"ollama"`)
//! makes the AI menu, chat panel, and DB assistant call that provider's
//! HTTP API directly instead of shelling out — no CLI tool required, at the
//! cost of Vix now needing to hold an API key itself (Anthropic and
//! `OpenAI`-compatible only; Ollama's local server needs none by default). See
//! [`secret`] for how that key is resolved.
//!
//! Each provider module ([`anthropic`], [`openai`], [`ollama`]) is a pure
//! request-builder/response-parser pair, unit-tested with fixture JSON and
//! no network I/O — [`complete`] is the one function that actually performs
//! the HTTP call (`ureq`, blocking), matching the split `vix-lsp-core`/
//! `vix-lsp` and `vix-db`'s `ai`/`app.rs` already use elsewhere in this
//! workspace.
//!
//! [`Settings::ai_command`]: https://docs.rs/vix-settings (see `crates/vix-settings/src/lib.rs`)

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod anthropic;
pub mod ollama;
pub mod openai;
pub mod secret;

/// An HTTP AI backend `vix-ai-core` can talk to directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    /// Anthropic's Messages API (`https://api.anthropic.com/v1/messages`).
    Anthropic,
    /// The `OpenAI` Chat Completions API, or anything wire-compatible with it
    /// (a local gateway, a self-hosted model server, …) — the endpoint is
    /// always configurable, so this is not limited to `api.openai.com`.
    OpenAi,
    /// A local (or remote) Ollama server's `/api/generate` endpoint.
    Ollama,
}

impl Provider {
    /// Parse a [`Settings::ai_provider`] value (`"anthropic"`, `"openai"`,
    /// `"ollama"`); `None` for `"cli"` (the default, handled entirely by the
    /// host's own CLI shell-out, never routed through this crate) or an
    /// unrecognized string.
    ///
    /// [`Settings::ai_provider`]: https://docs.rs/vix-settings
    #[must_use]
    pub fn parse(name: &str) -> Option<Provider> {
        match name {
            "anthropic" => Some(Provider::Anthropic),
            "openai" => Some(Provider::OpenAi),
            "ollama" => Some(Provider::Ollama),
            _ => None,
        }
    }

    /// The canonical lowercase name — the same string [`Provider::parse`]
    /// accepts, and the keyring account name [`secret::resolve`] looks up.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Provider::Anthropic => "anthropic",
            Provider::OpenAi => "openai",
            Provider::Ollama => "ollama",
        }
    }

    /// The endpoint used when [`Settings::ai_endpoint`] is empty.
    ///
    /// [`Settings::ai_endpoint`]: https://docs.rs/vix-settings
    #[must_use]
    pub fn default_endpoint(self) -> &'static str {
        match self {
            Provider::Anthropic => "https://api.anthropic.com/v1/messages",
            Provider::OpenAi => "https://api.openai.com/v1/chat/completions",
            Provider::Ollama => "http://localhost:11434/api/generate",
        }
    }

    /// The model id used when [`Settings::ai_model`] is empty. A reasonable
    /// starting point, not a recommendation to stay on it forever — set
    /// `ai_model` explicitly once you know what you want.
    ///
    /// [`Settings::ai_model`]: https://docs.rs/vix-settings
    #[must_use]
    pub fn default_model(self) -> &'static str {
        match self {
            Provider::Anthropic => "claude-sonnet-5",
            Provider::OpenAi => "gpt-4o-mini",
            Provider::Ollama => "llama3.2",
        }
    }

    /// Whether this provider needs an API key at all — Ollama's default
    /// (a local server) does not, though [`secret::resolve`] is still
    /// consulted for a self-hosted Ollama behind auth.
    #[must_use]
    pub fn requires_api_key(self) -> bool {
        matches!(self, Provider::Anthropic | Provider::OpenAi)
    }

    fn request_body(self, model: &str, instruction: &str, input: &str) -> serde_json::Value {
        match self {
            Provider::Anthropic => anthropic::request_body(model, instruction, input),
            Provider::OpenAi => openai::request_body(model, instruction, input),
            Provider::Ollama => ollama::request_body(model, instruction, input),
        }
    }

    fn headers(self, api_key: &str) -> Vec<(&'static str, String)> {
        match self {
            Provider::Anthropic => anthropic::headers(api_key),
            Provider::OpenAi => openai::headers(api_key),
            Provider::Ollama => ollama::headers(api_key),
        }
    }

    fn parse_response(self, body: &serde_json::Value) -> Result<String, String> {
        match self {
            Provider::Anthropic => anthropic::parse_response(body),
            Provider::OpenAi => openai::parse_response(body),
            Provider::Ollama => ollama::parse_response(body),
        }
    }
}

/// Send `instruction` (the fixed system-level directions — what the CLI path
/// puts in `{prompt}`) and `input` (the schema/selection/question — what the
/// CLI path feeds on stdin) to `provider` at `endpoint` using `model`,
/// blocking until a reply or a clear error message.
///
/// Call from a background thread — like `vix-http-client::send`, this blocks
/// on network I/O and must never run on the render/event-loop thread. An
/// HTTP error status is not treated as a transport failure: the response
/// body is still parsed, since every provider here returns a JSON `error`
/// object worth surfacing rather than a bare status code.
///
/// # Errors
///
/// Returns `Err` with a human-readable reason on a transport failure (DNS,
/// connection, TLS), a non-JSON body, or a JSON body the provider's own
/// [`parse_response`](Provider) rejects (an `error` field, no content, or an
/// empty reply).
pub fn complete(
    provider: Provider,
    endpoint: &str,
    model: &str,
    api_key: &str,
    instruction: &str,
    input: &str,
) -> Result<String, String> {
    let body = provider.request_body(model, instruction, input);
    let mut req = ureq::post(endpoint);
    for (name, value) in provider.headers(api_key) {
        req = req.set(name, &value);
    }
    let json = match req.send_json(body) {
        Ok(resp) | Err(ureq::Error::Status(_, resp)) => resp
            .into_json::<serde_json::Value>()
            .map_err(|e| e.to_string())?,
        Err(e) => return Err(e.to_string()),
    };
    provider.parse_response(&json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_round_trips_through_name() {
        for p in [Provider::Anthropic, Provider::OpenAi, Provider::Ollama] {
            assert_eq!(Provider::parse(p.name()), Some(p));
        }
    }

    #[test]
    fn cli_and_unknown_names_are_not_a_provider() {
        assert_eq!(Provider::parse("cli"), None);
        assert_eq!(Provider::parse(""), None);
        assert_eq!(Provider::parse("gpt5"), None);
    }

    #[test]
    fn only_the_key_bearing_providers_require_a_key() {
        assert!(Provider::Anthropic.requires_api_key());
        assert!(Provider::OpenAi.requires_api_key());
        assert!(!Provider::Ollama.requires_api_key());
    }
}
