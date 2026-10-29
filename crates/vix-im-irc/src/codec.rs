//! The IRC line codec: RFC 1459/2812 framing with IRCv3 message tags.
//!
//! [`Line::parse`] reads one line as received (any trailing CR/LF is ignored)
//! and never panics: everything a server sends is untrusted. [`Line::to_wire`]
//! writes one line without its CRLF and refuses CR, LF and NUL anywhere, so no
//! text can smuggle in a second command.

use std::fmt;

/// One IRCv3 message tag: a key and an optional, already-unescaped value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    /// The tag key, e.g. `time`, `account` or `+example.com/foo`.
    pub key: String,
    /// The unescaped value; `None` for a bare key (`@key` or `@key=`).
    pub value: Option<String>,
}

/// The source of a line: `nick!user@host`, or a bare server name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prefix {
    /// The nick, or the server name when there is no `!`/`@`.
    pub nick: String,
    /// The user (ident) part after `!`, if present.
    pub user: Option<String>,
    /// The host part after `@`, if present.
    pub host: Option<String>,
}

impl Prefix {
    /// Split `nick!user@host` (each of `!user` and `@host` optional).
    #[must_use]
    pub fn parse(text: &str) -> Prefix {
        let (rest, host) = match text.split_once('@') {
            Some((r, h)) => (r, Some(h.to_string())),
            None => (text, None),
        };
        let (nick, user) = match rest.split_once('!') {
            Some((n, u)) => (n.to_string(), Some(u.to_string())),
            None => (rest.to_string(), None),
        };
        Prefix { nick, user, host }
    }
}

impl fmt::Display for Prefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.nick)?;
        if let Some(user) = &self.user {
            write!(f, "!{user}")?;
        }
        if let Some(host) = &self.host {
            write!(f, "@{host}")?;
        }
        Ok(())
    }
}

/// One IRC line: tags, an optional prefix, a command and its parameters.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Line {
    /// IRCv3 message tags, in the order received.
    pub tags: Vec<Tag>,
    /// The source, when the line had a `:prefix`.
    pub prefix: Option<Prefix>,
    /// The command, upper-cased (`PRIVMSG`), or a three-digit numeric (`001`).
    pub command: String,
    /// The parameters; the trailing (`:`) parameter, if any, is the last one.
    pub params: Vec<String>,
}

/// Why a received line could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseError {
    /// The line is empty (or only tags/prefix with no command).
    NoCommand,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::NoCommand => f.write_str("line has no command"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Why a line could not be written.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EncodeError {
    /// A command, parameter, tag or prefix holds CR, LF or NUL.
    ForbiddenChar,
    /// The command is empty or contains a space.
    BadCommand,
    /// A parameter other than the last is empty, contains a space or starts
    /// with `:`.
    BadMiddleParam,
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            EncodeError::ForbiddenChar => "line contains CR, LF or NUL",
            EncodeError::BadCommand => "bad command",
            EncodeError::BadMiddleParam => "bad middle parameter",
        })
    }
}

impl std::error::Error for EncodeError {}

/// Whether `text` holds a character that would end or corrupt a line.
#[must_use]
pub fn has_forbidden(text: &str) -> bool {
    text.contains(['\r', '\n', '\0'])
}

/// Undo IRCv3 tag-value escaping: `\:` `;`, `\s` space, `\\` `\`, `\r` CR,
/// `\n` LF; any other escaped character stands for itself and a lone trailing
/// backslash is dropped.
#[must_use]
pub fn unescape_tag_value(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some(':') => out.push(';'),
            Some('s') => out.push(' '),
            Some('\\') => out.push('\\'),
            Some('r') => out.push('\r'),
            Some('n') => out.push('\n'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// Apply IRCv3 tag-value escaping, the inverse of [`unescape_tag_value`].
#[must_use]
pub fn escape_tag_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            ';' => out.push_str("\\:"),
            ' ' => out.push_str("\\s"),
            '\\' => out.push_str("\\\\"),
            '\r' => out.push_str("\\r"),
            '\n' => out.push_str("\\n"),
            '\0' => {}
            other => out.push(other),
        }
    }
    out
}

fn parse_tags(raw: &str) -> Vec<Tag> {
    raw.split(';')
        .filter(|t| !t.is_empty())
        .map(|t| match t.split_once('=') {
            Some((k, v)) if !v.is_empty() => Tag {
                key: k.to_string(),
                value: Some(unescape_tag_value(v)),
            },
            Some((k, _)) => Tag {
                key: k.to_string(),
                value: None,
            },
            None => Tag {
                key: t.to_string(),
                value: None,
            },
        })
        .collect()
}

/// Split off the next space-delimited token, skipping runs of spaces.
fn next_token(rest: &str) -> (&str, &str) {
    let rest = rest.trim_start_matches(' ');
    match rest.split_once(' ') {
        Some((tok, after)) => (tok, after),
        None => (rest, ""),
    }
}

impl Line {
    /// A tagless, prefixless line.
    #[must_use]
    pub fn new(command: &str, params: &[&str]) -> Line {
        Line {
            tags: Vec::new(),
            prefix: None,
            command: command.to_string(),
            params: params.iter().map(|p| (*p).to_string()).collect(),
        }
    }

    /// Read one line. Leading/trailing CR and LF are ignored; runs of spaces
    /// between parts are tolerated; the command is upper-cased.
    ///
    /// # Errors
    /// [`ParseError::NoCommand`] when there is no command.
    pub fn parse(line: &str) -> Result<Line, ParseError> {
        let mut rest = line.trim_matches(['\r', '\n']);
        let mut out = Line::default();
        if let Some(after) = rest.strip_prefix('@') {
            let (tags, after) = next_token(after);
            out.tags = parse_tags(tags);
            rest = after;
        }
        rest = rest.trim_start_matches(' ');
        if let Some(after) = rest.strip_prefix(':') {
            let (prefix, after) = next_token(after);
            out.prefix = Some(Prefix::parse(prefix));
            rest = after;
        }
        let (command, after) = next_token(rest);
        if command.is_empty() {
            return Err(ParseError::NoCommand);
        }
        out.command = command.to_ascii_uppercase();
        rest = after;
        loop {
            rest = rest.trim_start_matches(' ');
            if rest.is_empty() {
                break;
            }
            if let Some(trailing) = rest.strip_prefix(':') {
                out.params.push(trailing.to_string());
                break;
            }
            let (param, after) = next_token(rest);
            out.params.push(param.to_string());
            rest = after;
        }
        Ok(out)
    }

    /// The value of tag `key`, if present and non-empty.
    #[must_use]
    pub fn tag(&self, key: &str) -> Option<&str> {
        self.tags
            .iter()
            .find(|t| t.key == key)
            .and_then(|t| t.value.as_deref())
    }

    /// Parameter `i`, or `""` when absent.
    #[must_use]
    pub fn param(&self, i: usize) -> &str {
        self.params.get(i).map_or("", String::as_str)
    }

    /// The source nick (or server name), or `""` without a prefix.
    #[must_use]
    pub fn source_nick(&self) -> &str {
        self.prefix.as_ref().map_or("", |p| p.nick.as_str())
    }

    /// Write the line without its CRLF. The last parameter gets a `:` when
    /// it is empty, contains a space or starts with `:`.
    ///
    /// # Errors
    /// An [`EncodeError`] when any part holds CR, LF or NUL, the command is
    /// malformed, or a non-final parameter cannot be written as a middle one.
    pub fn to_wire(&self) -> Result<String, EncodeError> {
        let mut out = String::new();
        if !self.tags.is_empty() {
            out.push('@');
            for (i, tag) in self.tags.iter().enumerate() {
                if has_forbidden(&tag.key) || tag.key.contains([' ', ';', '=']) {
                    return Err(EncodeError::ForbiddenChar);
                }
                if i > 0 {
                    out.push(';');
                }
                out.push_str(&tag.key);
                if let Some(v) = &tag.value {
                    out.push('=');
                    out.push_str(&escape_tag_value(v));
                }
            }
            out.push(' ');
        }
        if let Some(prefix) = &self.prefix {
            let p = prefix.to_string();
            if has_forbidden(&p) || p.contains(' ') {
                return Err(EncodeError::ForbiddenChar);
            }
            out.push(':');
            out.push_str(&p);
            out.push(' ');
        }
        if has_forbidden(&self.command) {
            return Err(EncodeError::ForbiddenChar);
        }
        if self.command.is_empty() || self.command.contains(' ') {
            return Err(EncodeError::BadCommand);
        }
        out.push_str(&self.command);
        let last = self.params.len().saturating_sub(1);
        for (i, param) in self.params.iter().enumerate() {
            if has_forbidden(param) {
                return Err(EncodeError::ForbiddenChar);
            }
            out.push(' ');
            let needs_colon = param.is_empty() || param.contains(' ') || param.starts_with(':');
            if needs_colon {
                if i != last {
                    return Err(EncodeError::BadMiddleParam);
                }
                out.push(':');
            }
            out.push_str(param);
        }
        Ok(out)
    }
}

/// Read an IRCv3 `server-time` value (`YYYY-MM-DDThh:mm:ss[.sss]Z`) as
/// milliseconds since the Unix epoch, UTC. `None` for anything else.
#[must_use]
pub fn server_time_millis(value: &str) -> Option<i64> {
    let value = value.strip_suffix('Z')?;
    let (date, time) = value.split_once('T')?;
    let mut d = date.splitn(3, '-');
    let year: i64 = num(d.next()?, 4)?;
    let month: i64 = num(d.next()?, 2)?;
    let day: i64 = num(d.next()?, 2)?;
    let (hms, frac) = match time.split_once('.') {
        Some((h, f)) => (h, Some(f)),
        None => (time, None),
    };
    let mut t = hms.splitn(3, ':');
    let hour: i64 = num(t.next()?, 2)?;
    let minute: i64 = num(t.next()?, 2)?;
    let second: i64 = num(t.next()?, 2)?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let millis = match frac {
        None => 0,
        Some(f) if !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit()) => {
            // Take the first three digits, padding a short fraction.
            let digits: String = f.chars().chain("000".chars()).take(3).collect();
            digits.parse::<i64>().ok()?
        }
        Some(_) => return None,
    };
    let days = vix_civil_date::days_from_civil(year, month, day);
    Some(((days * 24 + hour) * 60 + minute) * 60_000 + second * 1000 + millis)
}

fn num(text: &str, width: usize) -> Option<i64> {
    if text.len() != width || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_line() {
        let l = Line::parse(
            "@time=2024-01-02T03:04:05.678Z;account=ana :ana!a@host PRIVMSG #vix :hello there\r\n",
        )
        .unwrap();
        assert_eq!(l.tag("time"), Some("2024-01-02T03:04:05.678Z"));
        assert_eq!(l.tag("account"), Some("ana"));
        assert_eq!(l.source_nick(), "ana");
        let p = l.prefix.as_ref().unwrap();
        assert_eq!(p.user.as_deref(), Some("a"));
        assert_eq!(p.host.as_deref(), Some("host"));
        assert_eq!(l.command, "PRIVMSG");
        assert_eq!(l.params, vec!["#vix", "hello there"]);
    }

    #[test]
    fn parses_numeric_and_middle_params() {
        let l = Line::parse(":irc.example 001 ana :Welcome").unwrap();
        assert_eq!(l.command, "001");
        assert_eq!(l.params, vec!["ana", "Welcome"]);
        let l = Line::parse("ping   x  y").unwrap();
        assert_eq!(l.command, "PING");
        assert_eq!(l.params, vec!["x", "y"]);
        assert_eq!(Line::parse(""), Err(ParseError::NoCommand));
        assert_eq!(Line::parse("@a=b :pre"), Err(ParseError::NoCommand));
        let l = Line::parse("CMD :").unwrap();
        assert_eq!(l.params, vec![""]);
    }

    #[test]
    fn tag_escapes_round_trip() {
        let raw = "a\\:b\\sc\\\\d\\re\\nf\\xg\\";
        assert_eq!(unescape_tag_value(raw), "a;b c\\d\re\nfxg");
        let v = "semi;space \\back\r\n";
        assert_eq!(unescape_tag_value(&escape_tag_value(v)), v);
        let l = Line::parse("@k=;flag;x=1 CMD").unwrap();
        assert_eq!(l.tags[0].value, None);
        assert_eq!(l.tags[1].value, None);
        assert_eq!(l.tag("x"), Some("1"));
    }

    #[test]
    fn writes_lines() {
        let l = Line::new("PRIVMSG", &["#vix", "hi there"]);
        assert_eq!(l.to_wire().unwrap(), "PRIVMSG #vix :hi there");
        assert_eq!(Line::new("NICK", &["ana"]).to_wire().unwrap(), "NICK ana");
        assert_eq!(
            Line::new("PRIVMSG", &["#a", ":)"]).to_wire().unwrap(),
            "PRIVMSG #a ::)"
        );
        assert_eq!(
            Line::new("PRIVMSG", &["#a\r\nQUIT", "x"]).to_wire(),
            Err(EncodeError::ForbiddenChar)
        );
        assert_eq!(
            Line::new("PRIVMSG", &["#a", "x\0"]).to_wire(),
            Err(EncodeError::ForbiddenChar)
        );
        assert_eq!(
            Line::new("PRIVMSG", &["a b", "x"]).to_wire(),
            Err(EncodeError::BadMiddleParam)
        );
        let mut tagged = Line::new("TAGMSG", &["#a"]);
        tagged.tags.push(Tag {
            key: "+x".into(),
            value: Some("a b;c".into()),
        });
        let wire = tagged.to_wire().unwrap();
        assert_eq!(wire, "@+x=a\\sb\\:c TAGMSG #a");
        assert_eq!(Line::parse(&wire).unwrap(), tagged);
    }

    #[test]
    fn server_time() {
        assert_eq!(server_time_millis("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(
            server_time_millis("2011-10-19T16:40:51.620Z"),
            Some(1_319_042_451_620)
        );
        assert_eq!(
            server_time_millis("2011-10-19T16:40:51Z"),
            Some(1_319_042_451_000)
        );
        assert_eq!(
            server_time_millis("2011-10-19T16:40:51.6Z"),
            Some(1_319_042_451_600)
        );
        assert_eq!(server_time_millis("2011-10-19 16:40:51Z"), None);
        assert_eq!(server_time_millis("2011-13-19T16:40:51Z"), None);
        assert_eq!(server_time_millis("garbage"), None);
    }
}
