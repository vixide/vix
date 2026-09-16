//! Anthropic's Messages API (pure request building and response parsing —
//! see [`crate::complete`] for the actual HTTP call).

use serde_json::{Value, json};

/// The API version header Anthropic's Messages API requires.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// A reasonable ceiling on the reply length — generous for the summarize/
/// explain/define/chat/NL-to-SQL use cases this crate serves, without
/// risking a runaway response.
const MAX_TOKENS: u32 = 4096;

/// Build a Messages API request body: `instruction` becomes the top-level
/// `system` prompt, `input` a single `user` turn.
#[must_use]
pub fn request_body(model: &str, instruction: &str, input: &str) -> Value {
    json!({
        "model": model,
        "max_tokens": MAX_TOKENS,
        "system": instruction,
        "messages": [{"role": "user", "content": input}],
    })
}

/// The `x-api-key`/`anthropic-version` headers Anthropic requires on every
/// request (unlike `OpenAI`'s/Ollama's single `Authorization` header).
#[must_use]
pub fn headers(api_key: &str) -> Vec<(&'static str, String)> {
    vec![
        ("x-api-key", api_key.to_string()),
        ("anthropic-version", ANTHROPIC_VERSION.to_string()),
    ]
}

/// Parse a Messages API response body into the reply text: concatenates
/// every `text` content block (a reply is normally exactly one). An `error`
/// field (present on a non-2xx response) takes priority and its `message`
/// is returned as `Err`.
///
/// # Errors
///
/// Returns `Err` when the body carries an `error` object, has no usable
/// `content` array, or the concatenated text is empty.
pub fn parse_response(body: &Value) -> Result<String, String> {
    if let Some(error) = body.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("the server returned an error");
        return Err(message.to_string());
    }
    let content = body
        .get("content")
        .and_then(Value::as_array)
        .ok_or_else(|| "response had no content array".to_string())?;
    let text: String = content
        .iter()
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect();
    if text.is_empty() {
        return Err("response had no text content".to_string());
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_body_carries_system_and_one_user_turn() {
        let body = request_body("claude-sonnet-5", "Be terse.", "Summarize this.");
        assert_eq!(body["model"], "claude-sonnet-5");
        assert_eq!(body["system"], "Be terse.");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "Summarize this.");
    }

    #[test]
    fn headers_carry_the_key_and_a_fixed_api_version() {
        let h = headers("sk-ant-abc");
        assert!(h.contains(&("x-api-key", "sk-ant-abc".to_string())));
        assert!(h.contains(&("anthropic-version", ANTHROPIC_VERSION.to_string())));
    }

    #[test]
    fn parse_response_concatenates_text_blocks() {
        let body = json!({"content": [{"type": "text", "text": "Hello, "}, {"type": "text", "text": "world."}]});
        assert_eq!(parse_response(&body).unwrap(), "Hello, world.");
    }

    #[test]
    fn parse_response_surfaces_the_error_message() {
        let body =
            json!({"error": {"type": "authentication_error", "message": "invalid x-api-key"}});
        assert_eq!(parse_response(&body).unwrap_err(), "invalid x-api-key");
    }

    #[test]
    fn parse_response_rejects_missing_or_empty_content() {
        assert!(parse_response(&json!({})).is_err());
        assert!(parse_response(&json!({"content": []})).is_err());
        assert!(parse_response(&json!({"content": [{"type": "text", "text": ""}]})).is_err());
    }
}
