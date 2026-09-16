//! The `OpenAI` Chat Completions API, or anything wire-compatible with it
//! (pure request building and response parsing — see [`crate::complete`]
//! for the actual HTTP call).

use serde_json::{Value, json};

/// Build a Chat Completions request body: `instruction` becomes the
/// `system` message, `input` the single `user` message that follows it.
#[must_use]
pub fn request_body(model: &str, instruction: &str, input: &str) -> Value {
    json!({
        "model": model,
        "messages": [
            {"role": "system", "content": instruction},
            {"role": "user", "content": input},
        ],
    })
}

/// The single `Authorization: Bearer` header this API's auth scheme uses.
#[must_use]
pub fn headers(api_key: &str) -> Vec<(&'static str, String)> {
    vec![("Authorization", format!("Bearer {api_key}"))]
}

/// Parse a Chat Completions response body into the reply text:
/// `choices[0].message.content`. An `error` field (present on a non-2xx
/// response) takes priority and its `message` is returned as `Err`.
///
/// # Errors
///
/// Returns `Err` when the body carries an `error` object, has no usable
/// `choices[0].message.content` string, or that string is empty.
pub fn parse_response(body: &Value) -> Result<String, String> {
    if let Some(error) = body.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("the server returned an error");
        return Err(message.to_string());
    }
    let text = body
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .ok_or_else(|| "response had no choices[0].message.content".to_string())?;
    if text.trim().is_empty() {
        return Err("response had no text content".to_string());
    }
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_body_carries_system_then_user_messages() {
        let body = request_body("gpt-4o-mini", "Be terse.", "Summarize this.");
        assert_eq!(body["model"], "gpt-4o-mini");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "Be terse.");
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][1]["content"], "Summarize this.");
    }

    #[test]
    fn headers_carry_a_bearer_token() {
        assert_eq!(
            headers("sk-abc"),
            vec![("Authorization", "Bearer sk-abc".to_string())]
        );
    }

    #[test]
    fn parse_response_reads_the_first_choice() {
        let body =
            json!({"choices": [{"message": {"role": "assistant", "content": "Hello, world."}}]});
        assert_eq!(parse_response(&body).unwrap(), "Hello, world.");
    }

    #[test]
    fn parse_response_surfaces_the_error_message() {
        let body =
            json!({"error": {"message": "invalid api key", "type": "invalid_request_error"}});
        assert_eq!(parse_response(&body).unwrap_err(), "invalid api key");
    }

    #[test]
    fn parse_response_rejects_missing_or_empty_content() {
        assert!(parse_response(&json!({})).is_err());
        assert!(parse_response(&json!({"choices": []})).is_err());
        assert!(parse_response(&json!({"choices": [{"message": {"content": "   "}}]})).is_err());
    }
}
