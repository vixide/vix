//! Discovery of `deno.json`/`deno.jsonc` tasks.

use crate::task::{NamedTask, TaskSource};

/// Parse the `tasks` object of a `deno.json`/`deno.jsonc`, prefixing each
/// task name with `deno:` and its command with `deno task NAME`. `deno.jsonc`
/// allows `//` line comments, which plain JSON does not, so this strips them
/// first with a pragmatic line-based heuristic (`strip_line_comments`, private) —
/// it does not handle `/* */` block comments or trailing commas, both of
/// which JSONC also permits but real-world `deno.json` files rarely use.
/// Malformed JSON or a missing/non-object `tasks` key returns an empty
/// vector rather than erroring.
#[must_use]
pub fn discover_deno_tasks(deno_json: &str) -> Vec<NamedTask> {
    let stripped = strip_line_comments(deno_json);
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&stripped) else {
        return Vec::new();
    };
    let Some(tasks) = value.get("tasks").and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };
    tasks
        .keys()
        .map(|name| NamedTask {
            name: format!("deno:{name}"),
            command: format!("deno task {name}"),
            source: TaskSource::Discovered,
        })
        .collect()
}

/// Strip `//` line comments from JSONC text, one line at a time. A `//`
/// immediately preceded by `:` is treated as part of a URL (`https://…`)
/// and left alone — a pragmatic heuristic, not a real tokenizer, so a `//`
/// inside a quoted string that is *not* preceded by `:` (e.g. a Windows
/// path fragment) would still be incorrectly treated as a comment start.
fn strip_line_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for line in input.lines() {
        let bytes = line.as_bytes();
        let mut cut = None;
        for i in 0..bytes.len().saturating_sub(1) {
            if bytes[i] == b'/' && bytes[i + 1] == b'/' && !(i > 0 && bytes[i - 1] == b':') {
                cut = Some(i);
                break;
            }
        }
        out.push_str(cut.map_or(line, |c| &line[..c]));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_deno_tasks_with_prefix() {
        let json = r#"{"tasks": {"build": "deno compile main.ts", "test": "deno test"}}"#;
        let tasks = discover_deno_tasks(json);
        assert_eq!(tasks.len(), 2);
        assert!(
            tasks
                .iter()
                .any(|t| t.name == "deno:build" && t.command == "deno task build")
        );
    }

    #[test]
    fn strips_line_comments_but_keeps_urls() {
        let jsonc = "{\n  // a comment\n  \"tasks\": {\"serve\": \"deno run --allow-net=https://example.com main.ts\"}\n}";
        let tasks = discover_deno_tasks(jsonc);
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].name, "deno:serve");
    }

    #[test]
    fn malformed_or_missing_tasks_is_empty() {
        assert!(discover_deno_tasks("not json").is_empty());
        assert!(discover_deno_tasks("{}").is_empty());
        assert!(discover_deno_tasks(r#"{"tasks": "oops"}"#).is_empty());
    }
}
