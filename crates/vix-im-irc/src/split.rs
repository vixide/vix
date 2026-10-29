//! Outgoing text: validate names, clean message bodies, and split a long
//! body into lines that each fit IRC's 512-byte limit.

use std::fmt;

/// IRC's maximum line length in bytes, including the closing CRLF.
pub const MAX_LINE: usize = 512;

/// Why a command could not be turned into lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TextError {
    /// A nick, channel or target is empty, or holds a space, comma, `:` at
    /// the start, CR, LF, NUL, or another control character.
    BadName,
    /// The body is empty after cleaning.
    Empty,
    /// The target is so long that no text fits on a line.
    TooLong,
    /// The command needs a registered connection and there is none yet.
    NotRegistered,
}

impl fmt::Display for TextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TextError::BadName => "bad nick, channel or target",
            TextError::Empty => "empty message",
            TextError::TooLong => "target too long",
            TextError::NotRegistered => "not connected yet",
        })
    }
}

impl std::error::Error for TextError {}

/// Check a nick, channel name or message target before it is sent: it must
/// be non-empty, at most 200 bytes, not start with `:`, and hold no space,
/// comma or control character (CR, LF and NUL included). Names are never
/// "cleaned" -- a bad one is refused, so nothing is sent to someone else.
///
/// # Errors
/// [`TextError::BadName`] for a name that breaks any rule above.
pub fn check_name(name: &str) -> Result<(), TextError> {
    let bad = name.is_empty()
        || name.len() > 200
        || name.starts_with(':')
        || name.chars().any(|c| c == ' ' || c == ',' || c.is_control());
    if bad { Err(TextError::BadName) } else { Ok(()) }
}

/// Remove CR, LF and NUL from free text that must stay on one line (a part
/// or quit reason, a topic). Line breaks become spaces.
#[must_use]
pub fn clean_single_line(text: &str) -> String {
    text.chars()
        .filter(|c| *c != '\0' && *c != '\r')
        .map(|c| if c == '\n' { ' ' } else { c })
        .collect()
}

/// The non-empty lines of a message body, with NUL and CR removed: each
/// line break in the body starts a separate message.
#[must_use]
pub fn body_lines(text: &str) -> Vec<String> {
    text.split('\n')
        .map(|l| {
            l.chars()
                .filter(|c| *c != '\0' && *c != '\r')
                .collect::<String>()
        })
        .filter(|l| !l.is_empty())
        .collect()
}

/// Split `text` into chunks of at most `budget` bytes, never inside a UTF-8
/// character, preferring to break after the last space that fits. The
/// chunks concatenate back to `text` exactly (a breaking space stays at the
/// end of its chunk). `budget` below 4 is raised to 4 so any character fits.
#[must_use]
pub fn split_utf8(text: &str, budget: usize) -> Vec<&str> {
    let budget = budget.max(4);
    let mut out = Vec::new();
    let mut rest = text;
    while rest.len() > budget {
        let mut cut = budget;
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        // Prefer the last space inside the window (keep it on this chunk).
        if let Some(space) = rest[..cut].rfind(' ')
            && space > 0
        {
            cut = space + 1;
        }
        let (head, tail) = rest.split_at(cut);
        out.push(head);
        rest = tail;
    }
    if !rest.is_empty() {
        out.push(rest);
    }
    out
}

/// The `PRIVMSG` lines (without CRLF) that send `text` to `target`, each at
/// most `max_line - 2` bytes so the line with its CRLF fits `max_line`. When
/// `action` is set each chunk is wrapped as a CTCP `ACTION`. Line breaks in
/// `text` start new messages; NUL and CR are dropped.
///
/// # Errors
/// [`TextError::BadName`] for a bad target, [`TextError::Empty`] for a body
/// with no text, [`TextError::TooLong`] when the target leaves no room.
pub fn privmsg_lines(
    target: &str,
    text: &str,
    action: bool,
    max_line: usize,
) -> Result<Vec<String>, TextError> {
    check_name(target)?;
    let overhead = "PRIVMSG ".len()
        + target.len()
        + " :".len()
        + 2
        + if action {
            crate::ctcp::ACTION_OVERHEAD
        } else {
            0
        };
    let budget = max_line.saturating_sub(overhead);
    if budget < 4 {
        return Err(TextError::TooLong);
    }
    let mut lines = Vec::new();
    for body in body_lines(text) {
        for chunk in split_utf8(&body, budget) {
            let payload = if action {
                crate::ctcp::encode_action(chunk)
            } else {
                chunk.to_string()
            };
            lines.push(format!("PRIVMSG {target} :{payload}"));
        }
    }
    if lines.is_empty() {
        return Err(TextError::Empty);
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(check_name("#vix").is_ok());
        assert!(check_name("ana_").is_ok());
        for bad in ["", "a b", "#a,#b", ":x", "a\r\nQUIT", "a\0", "a\u{1}"] {
            assert_eq!(check_name(bad), Err(TextError::BadName), "{bad:?}");
        }
        assert!(check_name(&"x".repeat(201)).is_err());
    }

    #[test]
    fn cleaning() {
        assert_eq!(clean_single_line("a\r\nQUIT\0"), "a QUIT");
        assert_eq!(body_lines("one\r\n\ntwo\0\n"), vec!["one", "two"]);
    }

    #[test]
    fn split_prefers_spaces_and_char_boundaries() {
        assert_eq!(split_utf8("hello world", 8), vec!["hello ", "world"]);
        assert_eq!(split_utf8("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        // "é" is two bytes: never cut between them.
        let s = "éééé";
        let parts = split_utf8(s, 5);
        assert_eq!(parts, vec!["éé", "éé"]);
        assert_eq!(split_utf8("", 10), Vec::<&str>::new());
    }

    #[test]
    fn privmsg_lines_fit() {
        let text = "word ".repeat(300);
        let lines = privmsg_lines("#vix", &text, false, MAX_LINE).unwrap();
        assert!(lines.len() > 1);
        for l in &lines {
            assert!(l.len() + 2 <= MAX_LINE);
            assert!(l.starts_with("PRIVMSG #vix :"));
        }
        let joined: String = lines.iter().map(|l| &l["PRIVMSG #vix :".len()..]).collect();
        assert_eq!(joined, text);
        let action = privmsg_lines("#vix", "waves", true, MAX_LINE).unwrap();
        assert_eq!(action, vec!["PRIVMSG #vix :\u{1}ACTION waves\u{1}"]);
        assert_eq!(
            privmsg_lines("#vix", "\r\n\0", false, MAX_LINE),
            Err(TextError::Empty)
        );
        assert_eq!(
            privmsg_lines("#a\r\nQUIT", "x", false, MAX_LINE),
            Err(TextError::BadName)
        );
        assert_eq!(
            privmsg_lines(&"x".repeat(200), "x", false, 210),
            Err(TextError::TooLong)
        );
        let two = privmsg_lines("ana", "a\nb", false, MAX_LINE).unwrap();
        assert_eq!(two, vec!["PRIVMSG ana :a", "PRIVMSG ana :b"]);
    }
}
