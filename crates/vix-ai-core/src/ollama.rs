//! A local (or remote) [Ollama](https://ollama.com) server's `/api/generate`
//! endpoint (pure request building and response parsing — see
//! [`crate::complete`] for the actual HTTP call).

use serde_json::{Value, json};

/// Build a non-streaming `/api/generate` request body. Ollama's own default
/// server needs no authentication at all (see [`headers`]), so this is the
/// simplest of the three providers.
#[must_use]
pub fn request_body(model: &str, instruction: &str, input: &str) -> Value {
    json!({
        "model": model,
        "system": instruction,
        "prompt": input,
        "stream": false,
    })
}

/// An `Authorization: Bearer` header when `api_key` is non-empty (a
/// self-hosted Ollama behind an authenticating proxy), otherwise none — the
/// stock local server expects no header at all.
#[must_use]
pub fn headers(api_key: &str) -> Vec<(&'static str, String)> {
    if api_key.is_empty() {
        Vec::new()
    } else {
        vec![("Authorization", format!("Bearer {api_key}"))]
    }
}

/// Parse a `/api/generate` response body into the reply text (the
/// `response` field). An `error` field takes priority and is returned as
/// `Err` directly — Ollama's error body is a bare string, not a nested
/// object like the other two providers.
///
/// # Errors
///
/// Returns `Err` when the body carries an `error` string, has no usable
/// `response` string, or that string is empty.
pub fn parse_response(body: &Value) -> Result<String, String> {
    if let Some(error) = body.get("error").and_then(Value::as_str) {
        return Err(error.to_string());
    }
    let text = body
        .get("response")
        .and_then(Value::as_str)
        .ok_or_else(|| "response had no `response` field".to_string())?;
    if text.trim().is_empty() {
        return Err("response had no text content".to_string());
    }
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_body_is_non_streaming_with_system_and_prompt() {
        let body = request_body("llama3.2", "Be terse.", "Summarize this.");
        assert_eq!(body["model"], "llama3.2");
        assert_eq!(body["system"], "Be terse.");
        assert_eq!(body["prompt"], "Summarize this.");
        assert_eq!(body["stream"], false);
    }

    #[test]
    fn headers_are_empty_without_a_key_and_bearer_with_one() {
        assert!(headers("").is_empty());
        assert_eq!(
            headers("tok"),
            vec![("Authorization", "Bearer tok".to_string())]
        );
    }

    #[test]
    fn parse_response_reads_the_response_field() {
        let body = json!({"response": "Hello, world.", "done": true});
        assert_eq!(parse_response(&body).unwrap(), "Hello, world.");
    }

    #[test]
    fn parse_response_surfaces_a_bare_string_error() {
        let body = json!({"error": "model 'llama3.2' not found"});
        assert_eq!(
            parse_response(&body).unwrap_err(),
            "model 'llama3.2' not found"
        );
    }

    #[test]
    fn parse_response_rejects_missing_or_empty_text() {
        assert!(parse_response(&json!({})).is_err());
        assert!(parse_response(&json!({"response": ""})).is_err());
    }
}
