//! The messages Vix and the `vix-matrix` helper exchange (`spec/index.md`,
//! "Protocol"). Each is one JSON object on one line with `"v": 1` and a
//! `"type"` naming the variant in `snake_case`; [`crate::codec`] reads and
//! writes them.

use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::Zeroize as _;

/// The protocol version every message carries as `"v"`. A message with any
/// other version is refused ([`crate::codec::DecodeError::Version`]).
pub const PROTOCOL_VERSION: u32 = 1;

/// An account password, sent once in [`Request::Login`].
///
/// Its `Debug` prints `<redacted>`, and its bytes are wiped when it is
/// dropped. The helper uses it for one login and never stores it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Password(String);

impl Password {
    /// Wrap a password supplied by the caller (Vix reads it from
    /// `VIX_IM_PASSWORD_<NAME>`).
    #[must_use]
    pub fn new(secret: String) -> Password {
        Password(secret)
    }

    /// The secret, for the helper's one login call.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Password {
    /// Never prints the secret, so logs, panics and test failures show only `Password(<redacted>)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(<redacted>)")
    }
}

impl Drop for Password {
    /// Wipe the bytes on drop so the secret does not linger in freed memory.
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// How a message is sent or was received: Matrix's `msgtype`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MessageKind {
    /// `m.text`, an ordinary message.
    #[default]
    Text,
    /// `m.emote`, a `/me` action.
    Emote,
    /// `m.notice`, a bot's or a server's note.
    Notice,
}

/// A request from Vix to the helper (one line on the helper's stdin).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Request {
    /// Sign in as `user` on `homeserver` and start syncing. The helper
    /// first resumes the session it saved for `account` (its store and the
    /// keyring entry). Only when there is none does it log in with
    /// `password`; without one it answers
    /// [`ErrorKey::NoSession`].
    Login {
        /// The `[[im.accounts]]` name; names the store directory and the
        /// keyring entries.
        account: String,
        /// The homeserver URL, such as `https://matrix.org`.
        homeserver: String,
        /// The Matrix user id (`@alice:example.org`) or its local part.
        user: String,
        /// The password, when Vix has one; unused when a session resumes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        password: Option<Password>,
    },
    /// Send a message to a joined room.
    Send {
        /// The room id (`!abc:example.org`).
        room: String,
        /// The plain-text body.
        body: String,
        /// `m.text`, `m.emote` or `m.notice`.
        #[serde(default)]
        kind: MessageKind,
    },
    /// Join a room by id (`!…`) or alias (`#…`).
    Join {
        /// The room id or alias.
        room: String,
    },
    /// Leave a joined room.
    Leave {
        /// The room id.
        room: String,
    },
    /// List the joined rooms again, one [`Reply::Room`] each.
    Rooms,
    /// Sign this device out on the server, delete its store and keyring
    /// entries, then end like [`Request::Quit`].
    Logout,
    /// Stop syncing, send [`Reply::Closed`] and exit. The session stays
    /// saved for the next [`Request::Login`].
    Quit,
}

/// Why something failed, as a key the App turns into translated text. The
/// helper never sends a pre-formatted English sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ErrorKey {
    /// The homeserver refused the user id or password.
    LoginFailed,
    /// No saved session for the account and no password to log in with.
    NoSession,
    /// A request that needs a session came before a successful login.
    NotLoggedIn,
    /// The homeserver could not be reached, or sync failed.
    Network,
    /// The message could not be sent.
    SendFailed,
    /// The room could not be joined.
    JoinFailed,
    /// The room could not be left.
    LeaveFailed,
    /// No joined room has that id.
    UnknownRoom,
    /// The encrypted store could not be opened, created or deleted.
    Store,
    /// The OS keyring refused to save or read the session.
    Keyring,
    /// A line Vix sent was not a valid request.
    BadRequest,
    /// A line the helper sent was not a valid reply (made by Vix's side).
    Protocol,
    /// The helper program was not found (made by Vix's side).
    HelperMissing,
    /// The helper could not be started, or it ended without saying
    /// [`Reply::Closed`] (made by Vix's side).
    HelperExited,
}

impl ErrorKey {
    /// The key's `snake_case` name, as it is written on the wire and used
    /// in the App's i18n keys (`msg.im_matrix_error_<name>`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorKey::LoginFailed => "login_failed",
            ErrorKey::NoSession => "no_session",
            ErrorKey::NotLoggedIn => "not_logged_in",
            ErrorKey::Network => "network",
            ErrorKey::SendFailed => "send_failed",
            ErrorKey::JoinFailed => "join_failed",
            ErrorKey::LeaveFailed => "leave_failed",
            ErrorKey::UnknownRoom => "unknown_room",
            ErrorKey::Store => "store",
            ErrorKey::Keyring => "keyring",
            ErrorKey::BadRequest => "bad_request",
            ErrorKey::Protocol => "protocol",
            ErrorKey::HelperMissing => "helper_missing",
            ErrorKey::HelperExited => "helper_exited",
        }
    }
}

/// A reply or event from the helper (one line on its stdout).
///
/// Every string in it came from the homeserver or other people and is
/// untrusted: the App sanitizes it before storing or drawing it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Reply {
    /// The helper started; always its first line.
    Ready {
        /// The helper's own version (its `Cargo.toml` version).
        helper: String,
        /// The protocol version it speaks.
        protocol: u32,
    },
    /// Signed in, by a resumed session or a new login; sync is running.
    LoggedIn {
        /// The full user id.
        user_id: String,
        /// This device's id.
        device_id: String,
        /// Whether a saved session was resumed (`false`: a new login).
        restored: bool,
    },
    /// A joined room: sent for every room after login, after
    /// [`Request::Rooms`], after a join, and when a name changes.
    Room {
        /// The room id.
        id: String,
        /// The room's display name as Matrix computes it.
        name: String,
        /// Whether messages in it are end-to-end encrypted.
        encrypted: bool,
        /// Whether it is a direct-message room.
        direct: bool,
    },
    /// We left the room, or were kicked or banned from it.
    Left {
        /// The room id.
        id: String,
    },
    /// A message someone else sent (the helper does not echo the ones Vix
    /// sent from this device).
    Message {
        /// The room id.
        room: String,
        /// The event id.
        event_id: String,
        /// The sender's user id.
        sender: String,
        /// The sender's display name in that room, when they have one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sender_name: Option<String>,
        /// The server's origin time, milliseconds since the Unix epoch, UTC.
        time: i64,
        /// The plain-text body (a formatted body is dropped for it).
        body: String,
        /// Text, emote or notice.
        #[serde(default)]
        kind: MessageKind,
        /// Whether it arrived end-to-end encrypted.
        encrypted: bool,
        /// Whether the sending device is cross-signed by its owner. Always
        /// `false` for an unencrypted message.
        verified: bool,
    },
    /// An encrypted event this device has no key for.
    Undecryptable {
        /// The room id.
        room: String,
        /// The event id.
        event_id: String,
        /// The sender's user id.
        sender: String,
        /// The server's origin time, milliseconds since the Unix epoch, UTC.
        time: i64,
    },
    /// Something failed. The helper keeps running unless a
    /// [`Reply::Closed`] follows.
    Error {
        /// What failed.
        key: ErrorKey,
        /// Extra detail, such as the server's error code; untrusted text.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    /// The helper is exiting; always its last line.
    Closed,
}

impl Reply {
    /// An [`Reply::Error`] with no detail.
    #[must_use]
    pub fn error(key: ErrorKey) -> Reply {
        Reply::Error { key, detail: None }
    }

    /// An [`Reply::Error`] with detail text.
    #[must_use]
    pub fn error_with(key: ErrorKey, detail: impl Into<String>) -> Reply {
        Reply::Error {
            key,
            detail: Some(detail.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_never_prints() {
        let p = Password::new("hunter2".to_string());
        assert_eq!(format!("{p:?}"), "Password(<redacted>)");
        let login = Request::Login {
            account: "a".into(),
            homeserver: "https://h".into(),
            user: "@a:h".into(),
            password: Some(p),
        };
        assert!(!format!("{login:?}").contains("hunter2"));
    }

    #[test]
    fn error_key_names_match_the_wire() {
        for key in [
            ErrorKey::LoginFailed,
            ErrorKey::NoSession,
            ErrorKey::NotLoggedIn,
            ErrorKey::Network,
            ErrorKey::SendFailed,
            ErrorKey::JoinFailed,
            ErrorKey::LeaveFailed,
            ErrorKey::UnknownRoom,
            ErrorKey::Store,
            ErrorKey::Keyring,
            ErrorKey::BadRequest,
            ErrorKey::Protocol,
            ErrorKey::HelperMissing,
            ErrorKey::HelperExited,
        ] {
            let wire = serde_json::to_string(&key).unwrap_or_default();
            assert_eq!(wire, format!("\"{}\"", key.as_str()));
        }
    }
}
