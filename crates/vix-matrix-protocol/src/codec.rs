//! JSON lines: one [`Request`] or [`Reply`] per line, each a JSON object
//! carrying `"v": 1` ([`PROTOCOL_VERSION`]). Pure; decoding never panics.
//!
//! An encoded line never holds a raw line break: JSON escapes every control
//! character inside strings, so a message body cannot start a second
//! message.

use std::fmt;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::protocol::{PROTOCOL_VERSION, Reply, Request};

/// The longest line either side accepts, in bytes (1 MiB). A longer line is
/// refused rather than buffered without bound; Matrix events are at most
/// 64 KiB.
pub const MAX_LINE: usize = 1 << 20;

/// Why a line did not decode.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// Longer than [`MAX_LINE`].
    TooLong,
    /// Not a JSON object.
    NotAnObject,
    /// `"v"` is missing or is not [`PROTOCOL_VERSION`].
    Version(Option<u64>),
    /// The object is not a known message (unknown `"type"`, a missing or
    /// mistyped field). Holds serde's description.
    Invalid(String),
}

impl fmt::Display for DecodeError {
    /// A short English description, for the helper's stderr and for tests.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::TooLong => write!(f, "line longer than {MAX_LINE} bytes"),
            DecodeError::NotAnObject => f.write_str("not a JSON object"),
            DecodeError::Version(Some(v)) => {
                write!(f, "protocol version {v}, expected {PROTOCOL_VERSION}")
            }
            DecodeError::Version(None) => f.write_str("no protocol version"),
            DecodeError::Invalid(why) => write!(f, "invalid message: {why}"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Write `message` as one line without its line break.
fn encode<T: Serialize>(message: &T) -> String {
    let mut object = match serde_json::to_value(message) {
        Ok(Value::Object(map)) => map,
        // Every Request and Reply is an internally tagged enum, which serde
        // always writes as an object.
        _ => Map::new(),
    };
    object.insert("v".to_string(), Value::from(PROTOCOL_VERSION));
    Value::Object(object).to_string()
}

/// Read one line (a trailing CR or LF is ignored).
fn decode<T: DeserializeOwned>(line: &str) -> Result<T, DecodeError> {
    if line.len() > MAX_LINE {
        return Err(DecodeError::TooLong);
    }
    let line = line.trim_end_matches(['\r', '\n']);
    let Ok(Value::Object(mut object)) = serde_json::from_str::<Value>(line) else {
        return Err(DecodeError::NotAnObject);
    };
    match object.remove("v").map(|v| v.as_u64()) {
        Some(Some(v)) if v == u64::from(PROTOCOL_VERSION) => {}
        Some(v) => return Err(DecodeError::Version(v)),
        None => return Err(DecodeError::Version(None)),
    }
    serde_json::from_value(Value::Object(object)).map_err(|e| DecodeError::Invalid(e.to_string()))
}

/// Write a request as one line, without the line break.
#[must_use]
pub fn encode_request(request: &Request) -> String {
    encode(request)
}

/// Read a request line (the helper's side).
///
/// # Errors
///
/// [`DecodeError`] when the line is too long, not an object, of another
/// protocol version, or not a known request.
pub fn decode_request(line: &str) -> Result<Request, DecodeError> {
    decode(line)
}

/// Write a reply as one line, without the line break (the helper's side).
#[must_use]
pub fn encode_reply(reply: &Reply) -> String {
    encode(reply)
}

/// Read a reply line.
///
/// # Errors
///
/// [`DecodeError`] when the line is too long, not an object, of another
/// protocol version, or not a known reply.
pub fn decode_reply(line: &str) -> Result<Reply, DecodeError> {
    decode(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ErrorKey, MessageKind, Password};

    #[test]
    fn a_request_line_carries_the_version_and_type() {
        let line = encode_request(&Request::Join {
            room: "#vix:example.org".into(),
        });
        assert_eq!(line, r##"{"room":"#vix:example.org","type":"join","v":1}"##);
        assert_eq!(
            decode_request(&format!("{line}\r\n")),
            Ok(Request::Join {
                room: "#vix:example.org".into()
            })
        );
    }

    #[test]
    fn unit_requests_round_trip() {
        for r in [Request::Rooms, Request::Logout, Request::Quit] {
            assert_eq!(decode_request(&encode_request(&r)), Ok(r));
        }
    }

    #[test]
    fn a_login_without_a_password_omits_the_field() {
        let login = Request::Login {
            account: "home".into(),
            homeserver: "https://example.org".into(),
            user: "@me:example.org".into(),
            password: None,
        };
        let line = encode_request(&login);
        assert!(!line.contains("password"));
        assert_eq!(decode_request(&line), Ok(login));
        let with = Request::Login {
            account: "home".into(),
            homeserver: "https://example.org".into(),
            user: "@me:example.org".into(),
            password: Some(Password::new("s3cret".into())),
        };
        assert_eq!(decode_request(&encode_request(&with)), Ok(with));
    }

    #[test]
    fn a_body_with_line_breaks_stays_on_one_line() {
        let send = Request::Send {
            room: "!r:h".into(),
            body: "one\ntwo\r\n{\"v\":1,\"type\":\"quit\"}".into(),
            kind: MessageKind::Emote,
        };
        let line = encode_request(&send);
        assert!(!line.contains('\n') && !line.contains('\r'));
        assert_eq!(decode_request(&line), Ok(send));
    }

    #[test]
    fn a_send_without_kind_is_text() {
        assert_eq!(
            decode_request(r#"{"v":1,"type":"send","room":"!r:h","body":"hi"}"#),
            Ok(Request::Send {
                room: "!r:h".into(),
                body: "hi".into(),
                kind: MessageKind::Text
            })
        );
    }

    #[test]
    fn bad_lines_are_refused() {
        assert_eq!(decode_reply("hello"), Err(DecodeError::NotAnObject));
        assert_eq!(decode_reply("[1]"), Err(DecodeError::NotAnObject));
        assert_eq!(
            decode_reply(r#"{"type":"closed"}"#),
            Err(DecodeError::Version(None))
        );
        assert_eq!(
            decode_reply(r#"{"v":2,"type":"closed"}"#),
            Err(DecodeError::Version(Some(2)))
        );
        assert_eq!(
            decode_reply(r#"{"v":"1","type":"closed"}"#),
            Err(DecodeError::Version(None))
        );
        assert!(matches!(
            decode_reply(r#"{"v":1,"type":"explode"}"#),
            Err(DecodeError::Invalid(_))
        ));
        assert!(matches!(
            decode_reply(r#"{"v":1,"type":"room","id":"!r:h"}"#),
            Err(DecodeError::Invalid(_))
        ));
        let long = format!("\"{}\"", "x".repeat(MAX_LINE));
        assert_eq!(decode_reply(&long), Err(DecodeError::TooLong));
    }

    #[test]
    fn replies_round_trip() {
        let replies = [
            Reply::Ready {
                helper: "2.0.2".into(),
                protocol: 1,
            },
            Reply::LoggedIn {
                user_id: "@me:h".into(),
                device_id: "ABCDEF".into(),
                restored: true,
            },
            Reply::Room {
                id: "!r:h".into(),
                name: "Vix".into(),
                encrypted: true,
                direct: false,
            },
            Reply::Left { id: "!r:h".into() },
            Reply::Message {
                room: "!r:h".into(),
                event_id: "$e".into(),
                sender: "@bob:h".into(),
                sender_name: Some("Bob".into()),
                time: 1_762_732_800_000,
                body: "see src/main.rs:12".into(),
                kind: MessageKind::Notice,
                encrypted: true,
                verified: false,
            },
            Reply::Undecryptable {
                room: "!r:h".into(),
                event_id: "$e".into(),
                sender: "@bob:h".into(),
                time: 0,
            },
            Reply::error(ErrorKey::NoSession),
            Reply::error_with(ErrorKey::Network, "M_LIMIT_EXCEEDED"),
            Reply::Closed,
        ];
        for r in replies {
            assert_eq!(decode_reply(&encode_reply(&r)), Ok(r));
        }
    }

    #[test]
    fn unknown_fields_are_ignored() {
        assert_eq!(
            decode_reply(r#"{"v":1,"type":"left","id":"!r:h","later":true}"#),
            Ok(Reply::Left { id: "!r:h".into() })
        );
    }
}
