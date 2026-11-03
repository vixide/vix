//! The input line's slash commands.
//!
//! `/me`, `/join`, `/part`, `/msg`, `/nick`, `/topic`, `/status`,
//! `/follow` and `/clear`. Command names are matched without regard to ASCII case. An
//! unknown command is an error and is never sent as text; to send text that
//! starts with `/`, double it (`//etc/hosts` sends `/etc/hosts`).
//!
//! Parsing is purely syntactic: whether a room exists, a nick is taken or a
//! `/follow` target is real is the App's business. Failures are returned as
//! [`CommandError`] values (never text) so the App can translate them. Owning
//! spec: `crates/vix-im/spec/index.md`.
//!
//! ```
//! use vix_im::{parse_input, Command, Input};
//! assert_eq!(parse_input("/me waves"), Ok(Input::Command(Command::Me("waves".into()))));
//! assert_eq!(parse_input("//etc/hosts"), Ok(Input::Text("/etc/hosts".into())));
//! ```

use crate::Presence;

/// What the user typed, once parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Input {
    /// Plain text to send to the selected conversation.
    Text(String),
    /// A slash command.
    Command(Command),
}

/// A slash command and its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Command {
    /// `/me TEXT` — an action ("* ana waves").
    Me(String),
    /// `/join ROOM [KEY]`.
    Join {
        /// The room, as typed (`#vix`).
        room: String,
        /// The room key, if one was given.
        key: Option<String>,
    },
    /// `/part [ROOM] [REASON]` — leave `room`, or the selected room when it
    /// is `None`. The first word is the room only when it starts with a
    /// channel prefix (`#`, `&`, `+`, `!`).
    Part {
        /// The room to leave, if named.
        room: Option<String>,
        /// The parting message.
        reason: Option<String>,
    },
    /// `/msg NICK TEXT` — a direct message.
    Msg {
        /// Who to send to.
        target: String,
        /// What to send.
        text: String,
    },
    /// `/nick NEW`.
    Nick(String),
    /// `/topic [TEXT]` — show the topic, or set it to `TEXT`.
    Topic(Option<String>),
    /// `/status available|away|busy`.
    Status(Presence),
    /// `/clear` — clear the selected conversation's messages on screen.
    Clear,
    /// `/follow [PEER]` — follow a Cowork participant (T759) by the number
    /// the session shows for them; with nothing, stop following.
    Follow(Option<String>),
}

/// Why a line could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CommandError {
    /// A `/name` Vix does not know (the name without the slash; empty for a
    /// lone `/`).
    Unknown(String),
    /// A command missing a required argument (the command's name).
    MissingArgument(&'static str),
    /// `/status` with a value other than `available`, `away` or `busy`.
    InvalidStatus(String),
}

/// Parse one input line.
///
/// # Errors
///
/// [`CommandError`] for an unknown command, a missing argument or a bad
/// `/status` value. Text that does not start with `/` always parses.
pub fn parse_input(line: &str) -> Result<Input, CommandError> {
    // Only a `/` in column 0 starts a command; leading whitespace makes the
    // line ordinary text (so pasted indented `/path` text is not a command).
    let Some(rest) = line.strip_prefix('/') else {
        return Ok(Input::Text(line.to_string()));
    };
    // `//x` is the escape for text that really starts with a slash.
    if rest.starts_with('/') {
        return Ok(Input::Text(rest.to_string()));
    }
    let (name, args) = rest
        .split_once(char::is_whitespace)
        .map_or((rest, ""), |(n, a)| (n, a.trim()));
    let command = match name.to_ascii_lowercase().as_str() {
        "me" => Command::Me(required("me", args)?.to_string()),
        "join" => {
            let (room, key) = split_word(required("join", args)?);
            Command::Join {
                room: room.to_string(),
                key: key.map(str::to_string),
            }
        }
        "part" => {
            let (first, more) = split_word(args);
            // IRC channel prefixes. Anything else is the start of a reason
            // for leaving the selected room, so `/part bye all` works.
            if first.starts_with(['#', '&', '+', '!']) {
                Command::Part {
                    room: Some(first.to_string()),
                    reason: more.map(str::to_string),
                }
            } else {
                Command::Part {
                    room: None,
                    reason: (!args.is_empty()).then(|| args.to_string()),
                }
            }
        }
        "msg" => {
            let (target, text) = split_word(required("msg", args)?);
            let text = text.ok_or(CommandError::MissingArgument("msg"))?;
            Command::Msg {
                target: target.to_string(),
                text: text.to_string(),
            }
        }
        "nick" => Command::Nick(split_word(required("nick", args)?).0.to_string()),
        "topic" => Command::Topic((!args.is_empty()).then(|| args.to_string())),
        "status" => {
            let value = required("status", args)?;
            Command::Status(
                Presence::parse(value).ok_or_else(|| CommandError::InvalidStatus(value.into()))?,
            )
        }
        "clear" => Command::Clear,
        "follow" => Command::Follow((!args.is_empty()).then(|| split_word(args).0.to_string())),
        _ => return Err(CommandError::Unknown(name.to_string())),
    };
    Ok(Input::Command(command))
}

/// `args` if non-empty, else [`CommandError::MissingArgument`] for `name`.
fn required<'a>(name: &'static str, args: &'a str) -> Result<&'a str, CommandError> {
    if args.is_empty() {
        Err(CommandError::MissingArgument(name))
    } else {
        Ok(args)
    }
}

/// The first word and the trimmed rest, if any.
fn split_word(args: &str) -> (&str, Option<&str>) {
    match args.split_once(char::is_whitespace) {
        Some((word, rest)) => {
            let rest = rest.trim();
            (word, (!rest.is_empty()).then_some(rest))
        }
        None => (args, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(line: &str) -> Command {
        match parse_input(line) {
            Ok(Input::Command(c)) => c,
            other => panic!("{line}: {other:?}"),
        }
    }

    #[test]
    fn text_and_escaped_slash() {
        assert_eq!(parse_input("hello"), Ok(Input::Text("hello".into())));
        assert_eq!(
            parse_input("//etc/hosts"),
            Ok(Input::Text("/etc/hosts".into()))
        );
        assert_eq!(parse_input(" /me x"), Ok(Input::Text(" /me x".into())));
    }

    #[test]
    fn commands() {
        assert_eq!(cmd("/me waves  hi"), Command::Me("waves  hi".into()));
        assert_eq!(cmd("/ME waves"), Command::Me("waves".into()));
        assert_eq!(
            cmd("/join #vix secret"),
            Command::Join {
                room: "#vix".into(),
                key: Some("secret".into())
            }
        );
        assert_eq!(
            cmd("/part #vix bye all"),
            Command::Part {
                room: Some("#vix".into()),
                reason: Some("bye all".into())
            }
        );
        assert_eq!(
            cmd("/part bye all"),
            Command::Part {
                room: None,
                reason: Some("bye all".into())
            }
        );
        assert_eq!(
            cmd("/part"),
            Command::Part {
                room: None,
                reason: None
            }
        );
        assert_eq!(
            cmd("/msg bo look at a.rs:3"),
            Command::Msg {
                target: "bo".into(),
                text: "look at a.rs:3".into()
            }
        );
        assert_eq!(cmd("/nick ana_"), Command::Nick("ana_".into()));
        assert_eq!(cmd("/topic"), Command::Topic(None));
        assert_eq!(
            cmd("/topic Release 2.0"),
            Command::Topic(Some("Release 2.0".into()))
        );
        assert_eq!(cmd("/status Busy"), Command::Status(Presence::Busy));
        assert_eq!(cmd("/clear"), Command::Clear);
        assert_eq!(cmd("/follow 2"), Command::Follow(Some("2".into())));
        assert_eq!(cmd("/Follow  3 now"), Command::Follow(Some("3".into())));
        assert_eq!(cmd("/follow"), Command::Follow(None));
    }

    #[test]
    fn errors() {
        assert_eq!(
            parse_input("/frobnicate x"),
            Err(CommandError::Unknown("frobnicate".into()))
        );
        assert_eq!(parse_input("/"), Err(CommandError::Unknown(String::new())));
        assert_eq!(parse_input("/me"), Err(CommandError::MissingArgument("me")));
        assert_eq!(
            parse_input("/msg bo"),
            Err(CommandError::MissingArgument("msg"))
        );
        assert_eq!(
            parse_input("/join   "),
            Err(CommandError::MissingArgument("join"))
        );
        assert_eq!(
            parse_input("/status lunch"),
            Err(CommandError::InvalidStatus("lunch".into()))
        );
    }
}
