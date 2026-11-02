//! `textDocument/inlineCompletion` (T618): the request parameters and the reply
//! parser behind ghost-text suggestions.

use serde_json::{Value, json};

/// The `InlineCompletionParams` body for a request at `(line, character)`.
/// `automatic` is `context.triggerKind` 2 (the editor asked on its own after a
/// pause), otherwise 1 (the user invoked it).
#[must_use]
pub fn params(uri: &str, line: u32, character: u32, automatic: bool) -> Value {
    json!({
        "textDocument": { "uri": uri },
        "position": { "line": line, "character": character },
        "context": { "triggerKind": if automatic { 2 } else { 1 } }
    })
}

/// Parse a `textDocument/inlineCompletion` result (an `InlineCompletionItem[]`
/// or an `InlineCompletionList`) into the suggested texts, in server order.
/// A snippet `insertText` is flattened to plain text ([`flatten_snippet`]);
/// items whose text is empty are dropped.
#[must_use]
pub fn parse(result: &Value) -> Vec<String> {
    let items = match result {
        Value::Array(items) => items.as_slice(),
        Value::Object(_) => match result.get("items") {
            Some(Value::Array(items)) => items.as_slice(),
            _ => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    items
        .iter()
        .filter_map(|item| {
            let text = item.get("insertText")?;
            let text = match text {
                Value::String(s) => s.clone(),
                Value::Object(_) => {
                    let value = text.get("value")?.as_str()?;
                    if text.get("kind").and_then(Value::as_str) == Some("snippet") {
                        flatten_snippet(value)
                    } else {
                        value.to_string()
                    }
                }
                _ => return None,
            };
            (!text.is_empty()).then_some(text)
        })
        .collect()
}

/// Flatten a snippet body to plain text: `$1`, `${1}` and `$0` vanish,
/// `${1:name}` becomes `name`, and `\$`, `\}` and `\\` are unescaped.
#[must_use]
pub fn flatten_snippet(snippet: &str) -> String {
    let chars: Vec<char> = snippet.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\\' if i + 1 < chars.len() && matches!(chars[i + 1], '$' | '}' | '\\') => {
                out.push(chars[i + 1]);
                i += 2;
            }
            '$' if chars.get(i + 1).is_some_and(char::is_ascii_digit) => {
                i += 1;
                while chars.get(i).is_some_and(char::is_ascii_digit) {
                    i += 1;
                }
            }
            '$' if chars.get(i + 1) == Some(&'{') => {
                let mut j = i + 2;
                while chars.get(j).is_some_and(char::is_ascii_digit) {
                    j += 1;
                }
                match chars.get(j) {
                    Some('}') => i = j + 1,
                    Some(':') => {
                        let (text, end) = placeholder_body(&chars, j + 1);
                        out.push_str(&flatten_snippet(&text));
                        i = end;
                    }
                    _ => {
                        out.push('$');
                        i += 1;
                    }
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// The raw text of a `${n:...}` placeholder starting at `from` (just after the
/// colon) up to its matching `}`, and the index after that brace.
fn placeholder_body(chars: &[char], from: usize) -> (String, usize) {
    let mut depth = 1;
    let mut j = from;
    let mut text = String::new();
    while j < chars.len() {
        match chars[j] {
            '\\' if j + 1 < chars.len() => {
                text.push('\\');
                text.push(chars[j + 1]);
                j += 2;
                continue;
            }
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return (text, j + 1);
                }
            }
            _ => {}
        }
        text.push(chars[j]);
        j += 1;
    }
    (text, j)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_carry_the_trigger_kind() {
        let auto = params("file:///a.rs", 3, 7, true);
        assert_eq!(auto["context"]["triggerKind"], 2);
        assert_eq!(auto["position"]["character"], 7);
        assert_eq!(
            params("file:///a.rs", 0, 0, false)["context"]["triggerKind"],
            1
        );
    }

    #[test]
    fn parse_accepts_an_array_and_a_list() {
        let arr = json!([{"insertText": "foo()"}, {"insertText": ""}, {"nope": 1}]);
        assert_eq!(parse(&arr), ["foo()"]);
        let list = json!({"items": [{"insertText": "a"}, {"insertText": "b"}]});
        assert_eq!(parse(&list), ["a", "b"]);
        assert_eq!(parse(&Value::Null), Vec::<String>::new());
        assert_eq!(parse(&json!({"items": 3})), Vec::<String>::new());
    }

    #[test]
    fn parse_flattens_snippet_values_only() {
        let r = json!([
            {"insertText": {"kind": "snippet", "value": "f(${1:x}, $2)$0"}},
            {"insertText": {"kind": "plaintext", "value": "$1 stays"}}
        ]);
        assert_eq!(parse(&r), ["f(x, )", "$1 stays"]);
    }

    #[test]
    fn snippets_flatten_nested_placeholders_and_escapes() {
        assert_eq!(flatten_snippet("a${1:b${2:c}d}e"), "abcde");
        assert_eq!(flatten_snippet(r"cost \$5 \\ \}"), r"cost $5 \ }");
        assert_eq!(flatten_snippet("${1}x$12y"), "xy");
        assert_eq!(flatten_snippet("a$b"), "a$b");
        assert_eq!(flatten_snippet("${x"), "${x");
    }
}
