//! Matrix accounts in the IM menu (T793, feature `matrix`,
//! `spec/instant-message/matrix.md`): start the `vix-matrix` helper for a
//! `protocol = "matrix"` account, turn the App's IRC-shaped commands into
//! helper requests, and turn the helper's replies into `vix-im` messages.
//!
//! Every received string reaches the inbox through
//! [`vix_im::Inbox::push`] or a sanitized conversation title, as for IRC. A
//! sender is shown by their Matrix user id, never their display name:
//! display names are not unique, so one could pass for someone else's. The
//! password is read from `VIX_IM_PASSWORD_<NAME>` at connect time and goes
//! to the helper once, over its stdin.

use std::path::Path;

use vix_im::{ConversationId, ConversationKind, Message};
use vix_im_irc::Command;
use vix_im_matrix::net::{Handle, SpawnOptions, find_helper};
use vix_im_matrix::{ErrorKey, MessageKind, Password, Reply, Request};

use super::App;
use super::im::Route;
use super::im_net::{Backend, Link, password_var};
use crate::settings::ImAccount;

/// A Matrix conversation's id: `matrix/<account>` for the account's own
/// conversation, `matrix/<account>/<room id>` for a room. Room ids are
/// case-sensitive, so nothing is folded.
pub(crate) fn matrix_id(account: &str, room: Option<&str>) -> ConversationId {
    match room {
        Some(r) => ConversationId::new(format!("matrix/{account}/{r}")),
        None => ConversationId::new(format!("matrix/{account}")),
    }
}

/// The helper request for an IRC-shaped command from the shared IM code;
/// `None` when Matrix has no counterpart in this slice (`/nick`, `/topic`).
pub(super) fn request_for(command: Command) -> Option<Request> {
    match command {
        Command::Privmsg { target, text } => Some(Request::Send {
            room: target,
            body: text,
            kind: MessageKind::Text,
        }),
        Command::Action { target, text } => Some(Request::Send {
            room: target,
            body: text,
            kind: MessageKind::Emote,
        }),
        Command::Join(room) => Some(Request::Join { room }),
        Command::Part { channel, .. } => Some(Request::Leave { room: channel }),
        Command::Quit(_) => Some(Request::Quit),
        _ => None,
    }
}

/// The translated text for a helper error.
fn error_text(key: ErrorKey) -> String {
    match key {
        ErrorKey::LoginFailed => t!("msg.im_matrix_error_login_failed"),
        ErrorKey::NotLoggedIn => t!("msg.im_matrix_error_not_logged_in"),
        ErrorKey::Network => t!("msg.im_matrix_error_network"),
        ErrorKey::SendFailed => t!("msg.im_matrix_error_send_failed"),
        ErrorKey::JoinFailed => t!("msg.im_matrix_error_join_failed"),
        ErrorKey::LeaveFailed => t!("msg.im_matrix_error_leave_failed"),
        ErrorKey::UnknownRoom => t!("msg.im_matrix_error_unknown_room"),
        ErrorKey::Store => t!("msg.im_matrix_error_store"),
        ErrorKey::Keyring => t!("msg.im_matrix_error_keyring"),
        ErrorKey::BadRequest => t!("msg.im_matrix_error_bad_request"),
        ErrorKey::HelperMissing => t!("msg.im_matrix_error_helper_missing"),
        ErrorKey::HelperExited => t!("msg.im_matrix_error_helper_exited"),
        // `Protocol`, and any key a newer helper adds.
        _ => t!("msg.im_matrix_error_protocol"),
    }
    .to_string()
}

impl App {
    /// **IM → Connect…** for a `protocol = "matrix"` account: start the
    /// helper and sign in.
    pub(super) fn im_matrix_connect(&mut self, account: &ImAccount) {
        if account.homeserver.trim().is_empty() || account.user.trim().is_empty() {
            self.status =
                t!("status.im_matrix_incomplete", name = account.name.as_str()).to_string();
            return;
        }
        let setting = self.settings.im.matrix_helper.trim();
        let setting = (!setting.is_empty()).then(|| Path::new(setting));
        let exe = std::env::current_exe().ok();
        let program = find_helper(setting, exe.as_deref(), std::env::var_os("PATH").as_deref());
        let mut options = SpawnOptions::new(program);
        options.stderr_log = std::env::var_os("VIX_MATRIX_LOG").map(Into::into);
        self.im_matrix_start(account, Handle::spawn(&options));
    }

    /// Sign `account` in over `handle` (a started helper, or a test's
    /// [`Handle::pair`]) and add it to the links.
    pub(crate) fn im_matrix_start(&mut self, account: &ImAccount, handle: Handle) {
        let home = self.im_ensure_conversation(
            matrix_id(&account.name, None),
            ConversationKind::Room,
            &account.name,
            Some(Route {
                account: account.name.clone(),
                target: None,
            }),
        );
        let password = std::env::var(password_var(&account.name))
            .ok()
            .filter(|p| !p.is_empty())
            .map(Password::new);
        // A failed send means the helper already ended; its replies say why.
        let _ = handle.send(Request::Login {
            account: account.name.clone(),
            homeserver: account.homeserver.trim().to_string(),
            user: account.user.trim().to_string(),
            password,
        });
        self.im.links.push(Link {
            account: account.name.clone(),
            handle: Backend::Matrix(handle),
        });
        self.im.panel.select(&mut self.im.inbox, &home);
        self.status = t!("status.im_connecting", name = account.name.as_str()).to_string();
    }

    /// A room's conversation, created (as a room, titled by its id) when a
    /// message arrives for one the helper has not announced.
    fn im_matrix_room(&mut self, account: &str, room: &str) -> ConversationId {
        self.im_ensure_conversation(
            matrix_id(account, Some(room)),
            ConversationKind::Room,
            &vix_im::sanitize::sanitize(room),
            Some(Route {
                account: account.to_string(),
                target: Some(room.to_string()),
            }),
        )
    }

    /// A room the helper announced: add it, or update its title.
    fn im_matrix_room_info(&mut self, account: &str, id: String, name: &str, direct: bool) {
        let kind = if direct {
            ConversationKind::Direct
        } else {
            ConversationKind::Room
        };
        let title = vix_im::sanitize::sanitize(name);
        let title = if title.trim().is_empty() {
            vix_im::sanitize::sanitize(&id)
        } else {
            title
        };
        let conversation = self.im_ensure_conversation(
            matrix_id(account, Some(&id)),
            kind,
            &title,
            Some(Route {
                account: account.to_string(),
                target: Some(id),
            }),
        );
        if let Some(c) = self.im.inbox.get_mut(&conversation) {
            c.title = title;
        }
    }

    /// Turn one helper reply into inbox changes.
    pub(super) fn im_matrix_reply(&mut self, account: &str, reply: Reply) {
        let home = matrix_id(account, None);
        match reply {
            Reply::LoggedIn {
                user_id, device_id, ..
            } => {
                let line = t!(
                    "msg.im_matrix_logged_in",
                    user = user_id.as_str(),
                    device = device_id.as_str()
                );
                self.im_system(&home, &line);
            }
            Reply::Room {
                id, name, direct, ..
            } => self.im_matrix_room_info(account, id, &name, direct),
            Reply::Left { id } => {
                let conversation = matrix_id(account, Some(&id));
                let title = self.im.title(&conversation);
                self.im.inbox.remove(&conversation);
                self.im.routes.remove(&conversation);
                self.im.panel.clamp(&self.im.inbox);
                let room = if title.is_empty() { id } else { title };
                self.im_system(&home, &t!("msg.im_matrix_left", room = room));
            }
            Reply::Message {
                room,
                sender,
                time,
                body,
                kind,
                verified,
                ..
            } => {
                let conversation = self.im_matrix_room(account, &room);
                let author = vix_im::sanitize::sanitize(&sender);
                let entry = (
                    conversation.clone(),
                    self.im.inbox.case_mapping().fold(&author),
                );
                // The mark follows the latest message: one from a device
                // that is not cross-signed marks the sender unverified again.
                if verified {
                    self.im.verified.insert(entry);
                } else {
                    self.im.verified.remove(&entry);
                }
                let message = match kind {
                    MessageKind::Emote => Message::action(author, time, body),
                    MessageKind::Notice => Message::system(time, format!("{author}: {body}")),
                    _ => Message::text(author, time, body),
                };
                self.im_deliver(&conversation, message);
            }
            Reply::Undecryptable { room, sender, .. } => {
                let conversation = self.im_matrix_room(account, &room);
                let line = t!("msg.im_matrix_undecryptable", sender = sender.as_str());
                self.im_system(&conversation, &line);
            }
            Reply::Error {
                key: ErrorKey::NoSession,
                ..
            } => {
                let var = password_var(account);
                self.im_system(&home, &t!("msg.im_matrix_no_session", var = var.as_str()));
            }
            Reply::Error { key, detail } => {
                let error = error_text(key);
                let line = if let Some(detail) = detail.filter(|d| !d.trim().is_empty()) {
                    t!("msg.im_matrix_error_detail", error = error, detail = detail)
                } else {
                    t!("msg.im_matrix_error", error = error)
                };
                self.im_system(&home, &line);
            }
            Reply::Closed => {
                self.im_system(&home, &t!("msg.im_matrix_closed"));
            }
            // `Ready`, and anything a newer helper adds.
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn irc_shaped_commands_become_requests() {
        assert_eq!(
            request_for(Command::Privmsg {
                target: "!r:h".into(),
                text: "hi".into()
            }),
            Some(Request::Send {
                room: "!r:h".into(),
                body: "hi".into(),
                kind: MessageKind::Text
            })
        );
        assert_eq!(
            request_for(Command::Action {
                target: "!r:h".into(),
                text: "waves".into()
            }),
            Some(Request::Send {
                room: "!r:h".into(),
                body: "waves".into(),
                kind: MessageKind::Emote
            })
        );
        assert_eq!(
            request_for(Command::Join("#vix:h".into())),
            Some(Request::Join {
                room: "#vix:h".into()
            })
        );
        assert_eq!(
            request_for(Command::Part {
                channel: "!r:h".into(),
                reason: None
            }),
            Some(Request::Leave {
                room: "!r:h".into()
            })
        );
        assert_eq!(request_for(Command::Nick("x".into())), None);
        assert_eq!(
            request_for(Command::Topic {
                channel: "!r:h".into(),
                topic: None
            }),
            None
        );
    }

    #[test]
    fn room_ids_keep_their_case() {
        assert_eq!(
            matrix_id("home", Some("!AbC:h")).as_str(),
            "matrix/home/!AbC:h"
        );
        assert_eq!(matrix_id("home", None).as_str(), "matrix/home");
    }
}
