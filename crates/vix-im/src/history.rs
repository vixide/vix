//! Conversation history as JSONL: one JSON message object per line.
//!
//! Decoding is tolerant: a line that is blank or not a message is skipped,
//! so one damaged line (a crash mid-write) never loses the rest. Decoded
//! text is sanitized, since history files live outside Vix's control.

use crate::Message;

/// Milliseconds in a day.
const DAY_MS: i64 = 86_400_000;

/// One message as one JSON line, without the trailing newline.
///
/// # Errors
///
/// Only if serialization fails, which a well-formed message never does.
pub fn encode_line(message: &Message) -> Result<String, serde_json::Error> {
    serde_json::to_string(message)
}

/// Many messages as JSONL, each line ending in `'\n'`.
///
/// # Errors
///
/// Only if serialization fails, which a well-formed message never does.
pub fn encode(messages: &[Message]) -> Result<String, serde_json::Error> {
    let mut out = String::new();
    for message in messages {
        out.push_str(&encode_line(message)?);
        out.push('\n');
    }
    Ok(out)
}

/// Every message in `text` that parses, in order, sanitized. Bad lines are
/// skipped.
#[must_use]
pub fn decode(text: &str) -> Vec<Message> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<Message>(line).ok())
        .map(|mut message| {
            message.sanitize();
            message
        })
        .collect()
}

/// Drop messages older than `days` days before `now_ms` (Unix
/// milliseconds), keeping order. Returns how many were dropped.
pub fn prune(messages: &mut Vec<Message>, now_ms: i64, days: u32) -> usize {
    let cutoff = now_ms.saturating_sub(i64::from(days).saturating_mul(DAY_MS));
    let before = messages.len();
    messages.retain(|m| m.time_utc >= cutoff);
    before - messages.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MessageKind;

    fn sample() -> Vec<Message> {
        vec![
            Message::text("ana", 1_000, "see src/a.rs:3"),
            Message::action("bo", 2_000, "waves"),
            Message::system(3_000, "bo joined"),
            Message {
                author: "ana".into(),
                time_utc: 4_000,
                body: String::new(),
                kind: MessageKind::Snippet {
                    language: "rust".into(),
                    path: "src/a.rs".into(),
                    start_line: 3,
                    end_line: 4,
                    code: "let a = 1;\nlet b = 2;".into(),
                },
            },
            Message {
                author: "ana".into(),
                time_utc: 5_000,
                body: String::new(),
                kind: MessageKind::Location {
                    path: "src/b.rs".into(),
                    line: 9,
                    column: 2,
                },
            },
        ]
    }

    #[test]
    fn round_trip() {
        let text = encode(&sample()).unwrap();
        assert_eq!(text.lines().count(), 5);
        assert_eq!(decode(&text), sample());
    }

    #[test]
    fn bad_lines_are_skipped() {
        let mut text = encode(&sample()[..2]).unwrap();
        text.push_str("{\"author\":\"x\"\n\nnot json\n");
        text.push_str(&encode(&sample()[2..3]).unwrap());
        assert_eq!(decode(&text), sample()[..3]);
    }

    #[test]
    fn decoded_text_is_sanitized() {
        let line = "{\"author\":\"e\\u001b[2Jvil\",\"time_utc\":1,\"body\":\"a\\u202eb\",\"kind\":{\"type\":\"text\"}}";
        let got = decode(line);
        assert_eq!(got[0].author, "evil");
        assert_eq!(got[0].body, "ab");
    }

    #[test]
    fn prune_by_age() {
        let now = 10 * DAY_MS;
        let mut messages = vec![
            Message::text("a", now - 31 * DAY_MS, "old"),
            Message::text("a", now - 30 * DAY_MS, "edge"),
            Message::text("a", now, "new"),
        ];
        assert_eq!(prune(&mut messages, now, 30), 1);
        assert_eq!(messages.len(), 2);
        assert_eq!(prune(&mut messages, now, 0), 1);
        assert_eq!(messages[0].body, "new");
        assert_eq!(prune(&mut messages, i64::MIN, u32::MAX), 0);
    }
}
