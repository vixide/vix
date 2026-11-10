//! Turning one timeline event's JSON into a [`Reply`] (`spec/index.md`,
//! "Sync"). Pure, so it is tested without a homeserver.
//!
//! Only `m.room.message` events with `msgtype` `m.text`, `m.emote` or
//! `m.notice` become [`Reply::Message`], with the plain `body` (a formatted
//! body is dropped). An encrypted event this device cannot decrypt becomes
//! [`Reply::Undecryptable`]. Everything else is skipped, and so is every
//! event this device sent itself: Vix already shows what it sent.

use serde::Deserialize;
use serde_json::Value;
use vix_im_matrix::{MessageKind, Reply};

/// Who "we" are: the logged-in user and this device.
#[derive(Debug, Clone, Copy)]
pub struct Me<'a> {
    /// Our full user id.
    pub user_id: &'a str,
    /// This device's id.
    pub device_id: &'a str,
}

/// How the event arrived.
#[derive(Debug, Clone, Copy)]
pub enum Encryption<'a> {
    /// In the clear.
    Plain,
    /// Encrypted and decrypted by this device.
    Decrypted {
        /// The sending device, as the decryption reported it.
        sender_device: Option<&'a str>,
        /// Whether that device is cross-signed by its owner.
        cross_signed: bool,
    },
    /// Encrypted, and this device has no key for it.
    Undecryptable,
}

/// The fields every timeline event has, and the ones we read.
#[derive(Deserialize)]
struct TimelineEvent {
    #[serde(rename = "type", default)]
    kind: String,
    event_id: String,
    sender: String,
    #[serde(default)]
    origin_server_ts: i64,
    #[serde(default)]
    content: Value,
    #[serde(default)]
    unsigned: Unsigned,
}

/// The event's `unsigned` block: `transaction_id` is present only for the
/// device that sent the event.
#[derive(Deserialize, Default)]
struct Unsigned {
    #[serde(default)]
    transaction_id: Option<Value>,
}

/// The reply for one timeline event `json` in `room`, or `None` when it is
/// not shown (not a supported message, our own echo, or malformed). The
/// [`Reply::Message`]'s `sender_name` is left `None` for the caller to fill.
#[must_use]
pub fn convert(room: &str, json: &str, encryption: Encryption<'_>, me: Me<'_>) -> Option<Reply> {
    let event: TimelineEvent = serde_json::from_str(json).ok()?;
    let ours = event.sender == me.user_id;
    if ours && event.unsigned.transaction_id.is_some() {
        return None;
    }
    if let Encryption::Decrypted {
        sender_device: Some(device),
        ..
    } = encryption
        && ours
        && device == me.device_id
    {
        return None;
    }
    if let Encryption::Undecryptable = encryption {
        return Some(Reply::Undecryptable {
            room: room.to_string(),
            event_id: event.event_id,
            sender: event.sender,
            time: event.origin_server_ts,
        });
    }
    if event.kind != "m.room.message" {
        return None;
    }
    let kind = match event.content.get("msgtype").and_then(Value::as_str)? {
        "m.text" => MessageKind::Text,
        "m.emote" => MessageKind::Emote,
        "m.notice" => MessageKind::Notice,
        _ => return None,
    };
    let body = event
        .content
        .get("body")
        .and_then(Value::as_str)?
        .to_string();
    let (encrypted, verified) = match encryption {
        Encryption::Decrypted { cross_signed, .. } => (true, cross_signed),
        Encryption::Plain | Encryption::Undecryptable => (false, false),
    };
    Some(Reply::Message {
        room: room.to_string(),
        event_id: event.event_id,
        sender: event.sender,
        sender_name: None,
        time: event.origin_server_ts,
        body,
        kind,
        encrypted,
        verified,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: Me<'static> = Me {
        user_id: "@me:h",
        device_id: "MYDEV",
    };

    fn message(sender: &str, msgtype: &str, unsigned: &str) -> String {
        format!(
            r#"{{"type":"m.room.message","event_id":"$e","sender":"{sender}","origin_server_ts":1762732800000,
               "content":{{"msgtype":"{msgtype}","body":"hello","format":"org.matrix.custom.html","formatted_body":"<b>hello</b>"}}{unsigned}}}"#
        )
    }

    #[test]
    fn text_emote_and_notice_become_messages_with_the_plain_body() {
        for (msgtype, kind) in [
            ("m.text", MessageKind::Text),
            ("m.emote", MessageKind::Emote),
            ("m.notice", MessageKind::Notice),
        ] {
            let reply = convert(
                "!r:h",
                &message("@bob:h", msgtype, ""),
                Encryption::Plain,
                ME,
            );
            assert_eq!(
                reply,
                Some(Reply::Message {
                    room: "!r:h".into(),
                    event_id: "$e".into(),
                    sender: "@bob:h".into(),
                    sender_name: None,
                    time: 1_762_732_800_000,
                    body: "hello".into(),
                    kind,
                    encrypted: false,
                    verified: false,
                })
            );
        }
    }

    #[test]
    fn other_events_are_skipped() {
        assert_eq!(
            convert(
                "!r:h",
                &message("@bob:h", "m.image", ""),
                Encryption::Plain,
                ME
            ),
            None
        );
        let member = r#"{"type":"m.room.member","event_id":"$m","sender":"@bob:h","state_key":"@bob:h","content":{"membership":"join"}}"#;
        assert_eq!(convert("!r:h", member, Encryption::Plain, ME), None);
        let redacted =
            r#"{"type":"m.room.message","event_id":"$x","sender":"@bob:h","content":{}}"#;
        assert_eq!(convert("!r:h", redacted, Encryption::Plain, ME), None);
        assert_eq!(convert("!r:h", "not json", Encryption::Plain, ME), None);
    }

    #[test]
    fn our_own_echoes_are_skipped_but_other_devices_of_ours_are_not() {
        let echo = message("@me:h", "m.text", r#","unsigned":{"transaction_id":"t1"}"#);
        assert_eq!(convert("!r:h", &echo, Encryption::Plain, ME), None);
        let decrypted_here = Encryption::Decrypted {
            sender_device: Some("MYDEV"),
            cross_signed: true,
        };
        assert_eq!(
            convert("!r:h", &message("@me:h", "m.text", ""), decrypted_here, ME),
            None
        );
        let other_device = Encryption::Decrypted {
            sender_device: Some("PHONE"),
            cross_signed: true,
        };
        assert!(convert("!r:h", &message("@me:h", "m.text", ""), other_device, ME).is_some());
    }

    #[test]
    fn encryption_and_verification_are_reported() {
        let enc = Encryption::Decrypted {
            sender_device: Some("BOBDEV"),
            cross_signed: true,
        };
        let Some(Reply::Message {
            encrypted,
            verified,
            ..
        }) = convert("!r:h", &message("@bob:h", "m.text", ""), enc, ME)
        else {
            panic!("expected a message");
        };
        assert!(encrypted && verified);
        let unverified = Encryption::Decrypted {
            sender_device: Some("BOBDEV"),
            cross_signed: false,
        };
        let Some(Reply::Message { verified, .. }) =
            convert("!r:h", &message("@bob:h", "m.text", ""), unverified, ME)
        else {
            panic!("expected a message");
        };
        assert!(!verified);
    }

    #[test]
    fn an_undecryptable_event_is_reported() {
        let raw = r#"{"type":"m.room.encrypted","event_id":"$u","sender":"@bob:h","origin_server_ts":5,"content":{"algorithm":"m.megolm.v1.aes-sha2","ciphertext":"x"}}"#;
        assert_eq!(
            convert("!r:h", raw, Encryption::Undecryptable, ME),
            Some(Reply::Undecryptable {
                room: "!r:h".into(),
                event_id: "$u".into(),
                sender: "@bob:h".into(),
                time: 5,
            })
        );
    }
}
