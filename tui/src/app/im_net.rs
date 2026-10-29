//! The `im` capability's network half (T752): IRC connections through
//! `vix-im-irc`'s connection thread, and the event pump that turns its
//! [`Event`]s into `vix-im` messages.
//!
//! Every received string reaches the inbox through [`vix_im::Inbox::push`]
//! (or a sanitized conversation title), which strips control characters,
//! escape sequences and bidi overrides. The SASL password is read from
//! `VIX_IM_PASSWORD_<NAME>` at connect time, handed to the connection once
//! and never stored, shown or logged.

use vix_im::{ConversationId, ConversationKind, Message};
use vix_im_irc::net::{ConnectOptions, Handle, connect};
use vix_im_irc::{Command, Config, ErrorKind, Event, Password, Sasl};

use super::App;
use super::im::{Route, irc_id, is_channel, now_ms};

/// The environment variable holding an account's SASL password:
/// `VIX_IM_PASSWORD_` plus the name upper-cased, every other character `_`.
fn password_var(account: &str) -> String {
    let name: String = account
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    format!("VIX_IM_PASSWORD_{name}")
}

/// One open IRC connection.
pub(crate) struct Link {
    /// The `[[im.accounts]]` name.
    pub(crate) account: String,
    /// The connection thread.
    handle: Handle,
}

impl App {
    /// Connect the `index`th configured account.
    pub(super) fn im_connect_account(&mut self, index: usize) {
        let Some(account) = self.settings.im.accounts.get(index).cloned() else {
            return;
        };
        if self.im.links.iter().any(|l| l.account == account.name) {
            self.status =
                t!("status.im_already_connected", name = account.name.as_str()).to_string();
            return;
        }
        let server = self.im_ensure_conversation(
            irc_id(&account.name, None),
            ConversationKind::Room,
            &account.name,
            Some(Route {
                account: account.name.clone(),
                target: None,
            }),
        );
        if !account.tls {
            self.im_deliver(&server, Message::system(now_ms(), t!("msg.im_plaintext")));
        }
        let nick = self.im.inbox.nick().to_string();
        let sasl = if account.sasl_user.is_empty() {
            None
        } else {
            let var = password_var(&account.name);
            match std::env::var(&var) {
                Ok(secret) if !secret.is_empty() => Some(Sasl {
                    user: account.sasl_user.clone(),
                    password: Password::new(secret),
                }),
                _ => {
                    let note = t!("msg.im_no_password", var = var.as_str());
                    self.im_deliver(&server, Message::system(now_ms(), note));
                    None
                }
            }
        };
        let config = Config {
            user: nick.clone(),
            realname: nick.clone(),
            nick,
            channels: account.channels.clone(),
            sasl,
        };
        let handle = connect(ConnectOptions::new(
            &account.server,
            account.port,
            account.tls,
            config,
        ));
        self.im.links.push(Link {
            account: account.name.clone(),
            handle,
        });
        self.im.panel.select(&mut self.im.inbox, &server);
        self.status = t!("status.im_connecting", name = account.name.as_str()).to_string();
    }

    /// **IM → Disconnect**: close the active account's connection.
    pub(super) fn im_disconnect(&mut self) {
        let Some(account) = self.im_active_account() else {
            self.status = t!("status.im_not_connected").to_string();
            return;
        };
        let Some(at) = self.im.links.iter().position(|l| l.account == account) else {
            self.status = t!("status.im_not_connected").to_string();
            return;
        };
        let link = self.im.links.remove(at);
        for event in link.handle.close() {
            self.im_irc_event(&account, event);
        }
        self.status = t!("status.im_disconnected", name = account.as_str()).to_string();
    }

    /// The names of the connected accounts.
    pub(super) fn im_link_names(&self) -> Vec<String> {
        self.im.links.iter().map(|l| l.account.clone()).collect()
    }

    /// Send a command on `account`'s connection. `false` when it is not
    /// connected (or its thread has ended).
    pub(super) fn im_send(&mut self, account: &str, command: Command) -> bool {
        self.im
            .links
            .iter()
            .find(|l| l.account == account)
            .is_some_and(|l| l.handle.send(command))
    }

    /// Drain every connection's events; drop connections whose thread ended.
    pub(super) fn im_poll_links(&mut self) {
        let mut events = Vec::new();
        let mut ended = Vec::new();
        for (i, link) in self.im.links.iter().enumerate() {
            // Check for the end first: a thread that has finished sent its
            // last event before finishing, so the drain below gets it all.
            if link.handle.is_finished() {
                ended.push(i);
            }
            while let Some(event) = link.handle.try_recv() {
                events.push((link.account.clone(), event));
            }
        }
        for i in ended.into_iter().rev() {
            self.im.links.remove(i);
        }
        for (account, event) in events {
            self.im_irc_event(&account, event);
        }
    }

    /// Whether a connection is open (the loop polls faster then).
    #[must_use]
    pub fn im_connected(&self) -> bool {
        !self.im.links.is_empty()
    }

    /// A room's or direct conversation's id, created on first use.
    fn im_target(&mut self, account: &str, target: &str) -> ConversationId {
        let kind = if is_channel(target) {
            ConversationKind::Room
        } else {
            ConversationKind::Direct
        };
        self.im_ensure_conversation(
            irc_id(account, Some(target)),
            kind,
            target,
            Some(Route {
                account: account.to_string(),
                target: Some(target.to_string()),
            }),
        )
    }

    /// A system line in a conversation.
    fn im_system(&mut self, id: &ConversationId, text: &str) {
        self.im_deliver(id, Message::system(now_ms(), text));
    }

    /// Turn one connection event into inbox changes.
    fn im_irc_event(&mut self, account: &str, event: Event) {
        let server = irc_id(account, None);
        match event {
            Event::Connected { nick } => {
                self.im_system(&server, &t!("msg.im_connected", nick = nick.as_str()));
                self.im.inbox.set_nick(nick);
            }
            Event::Joined { channel } => {
                let id = self.im_target(account, &channel);
                let nick = self.im.inbox.nick().to_string();
                self.im_system(&id, &t!("msg.im_joined", nick = nick));
                self.im.panel.select(&mut self.im.inbox, &id);
            }
            Event::Parted { channel, .. } => {
                let id = irc_id(account, Some(&channel));
                self.im.inbox.remove(&id);
                self.im.routes.remove(&id);
                self.im.panel.clamp(&self.im.inbox);
                let nick = self.im.inbox.nick().to_string();
                let line = format!("{channel}: {}", t!("msg.im_left", nick = nick));
                self.im_system(&server, &line);
            }
            Event::UserJoined { channel, nick } => {
                let id = self.im_target(account, &channel);
                self.im_system(&id, &t!("msg.im_joined", nick = nick));
            }
            Event::UserParted { channel, nick, .. } => {
                let id = self.im_target(account, &channel);
                self.im_system(&id, &t!("msg.im_left", nick = nick));
            }
            Event::UserQuit { nick, .. } => {
                self.im_system(&server, &t!("msg.im_quit", nick = nick));
            }
            Event::Message {
                target,
                from,
                from_account,
                text,
                is_action,
                time,
            } => self.im_irc_message(
                account,
                &target,
                &from,
                from_account.is_some(),
                &text,
                is_action,
                time,
            ),
            Event::Notice {
                target, from, text, ..
            } => {
                let id = if is_channel(&target) {
                    self.im_target(account, &target)
                } else {
                    server
                };
                self.im_deliver(&id, Message::system(now_ms(), format!("{from}: {text}")));
            }
            Event::Topic { channel, topic, .. } => {
                let id = self.im_target(account, &channel);
                if let Some(c) = self.im.inbox.get_mut(&id) {
                    c.set_topic(topic.as_deref());
                }
                let line =
                    topic.map_or_else(|| t!("msg.im_no_topic"), |t| t!("msg.im_topic", topic = t));
                self.im_system(&id, &line);
            }
            Event::Names { channel, nicks } => {
                let id = self.im_target(account, &channel);
                let line = t!("msg.im_names", names = nicks.join(", "));
                self.im_system(&id, &line);
            }
            Event::NickChanged { old, new, is_self } => {
                if is_self {
                    self.im.inbox.set_nick(new.clone());
                }
                let line = t!("msg.im_nick_changed", old = old, new = new);
                self.im_system(&server, &line);
            }
            Event::Error { kind, detail } => {
                let line = match kind {
                    ErrorKind::SaslFailed => t!("msg.im_sasl_failed").to_string(),
                    ErrorKind::NickInUse => t!("msg.im_nick_in_use").to_string(),
                    _ => t!("msg.im_error", detail = detail).to_string(),
                };
                self.im_system(&server, &line);
            }
            Event::Disconnected { reason } => {
                self.im_system(&server, &t!("msg.im_disconnected", reason = reason));
            }
            _ => {}
        }
    }

    /// A `PRIVMSG`: into its room, or into the direct conversation with the
    /// sender. A server-confirmed account marks the nick verified in that
    /// conversation.
    #[allow(
        clippy::too_many_arguments,
        reason = "the fields of one Event::Message, passed through as they are"
    )]
    fn im_irc_message(
        &mut self,
        account: &str,
        target: &str,
        from: &str,
        verified: bool,
        text: &str,
        is_action: bool,
        time: Option<i64>,
    ) {
        let id = if is_channel(target) {
            self.im_target(account, target)
        } else {
            self.im_target(account, from)
        };
        if verified {
            let folded = self.im.inbox.case_mapping().fold(from);
            self.im.verified.insert((id.clone(), folded));
        }
        let time = time.unwrap_or_else(now_ms);
        let message = if is_action {
            Message::action(from, time, text)
        } else {
            Message::text(from, time, text)
        };
        self.im_deliver(&id, message);
    }
}

#[cfg(test)]
mod tests {
    use super::password_var;

    #[test]
    fn password_variable_is_upper_cased_with_underscores() {
        assert_eq!(password_var("Libera"), "VIX_IM_PASSWORD_LIBERA");
        assert_eq!(password_var("my net.2"), "VIX_IM_PASSWORD_MY_NET_2");
    }
}
