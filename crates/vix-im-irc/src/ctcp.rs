//! CTCP `ACTION` (`/me`): `\x01ACTION text\x01` inside a `PRIVMSG`.
//!
//! Other CTCP requests (`VERSION`, `TIME`, ...) are recognized only so they
//! can be ignored: answering them would tell strangers about the user's
//! machine, so this crate never does.

const DELIM: char = '\u{1}';

/// Wrap `text` as a CTCP `ACTION` body.
#[must_use]
pub fn encode_action(text: &str) -> String {
    format!("{DELIM}ACTION {text}{DELIM}")
}

/// The text of a CTCP `ACTION` body, or `None` when `body` is not one. The
/// closing `\x01` is optional, as many clients omit it.
#[must_use]
pub fn decode_action(body: &str) -> Option<&str> {
    let inner = body.strip_prefix(DELIM)?;
    let inner = inner.strip_suffix(DELIM).unwrap_or(inner);
    if inner == "ACTION" {
        return Some("");
    }
    inner.strip_prefix("ACTION ")
}

/// Whether `body` is any CTCP message (starts with `\x01`).
#[must_use]
pub fn is_ctcp(body: &str) -> bool {
    body.starts_with(DELIM)
}

/// The bytes [`encode_action`] adds around the text.
pub(crate) const ACTION_OVERHEAD: usize = "\u{1}ACTION \u{1}".len();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_round_trip() {
        let body = encode_action("waves");
        assert_eq!(body, "\u{1}ACTION waves\u{1}");
        assert_eq!(decode_action(&body), Some("waves"));
        assert_eq!(decode_action("\u{1}ACTION waves"), Some("waves"));
        assert_eq!(decode_action("\u{1}ACTION\u{1}"), Some(""));
        assert_eq!(decode_action("\u{1}VERSION\u{1}"), None);
        assert_eq!(decode_action("ACTION waves"), None);
        assert!(is_ctcp("\u{1}VERSION\u{1}"));
        assert_eq!(ACTION_OVERHEAD, 9);
    }
}
