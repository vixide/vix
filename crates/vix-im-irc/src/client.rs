//! The client state machine: registration (`CAP LS 302`, SASL PLAIN, `NICK`,
//! `USER`), channel joins, `PING`/`PONG`, and turning server lines into
//! [`Event`]s and App [`Command`]s into lines. Pure: no sockets, no clock.
//!
//! Feed it with [`Client::start`], then [`Client::handle_line`] for every
//! line received and [`Client::command`] for every App request; each returns
//! a [`Step`] holding the lines to send (without CRLF) and the events to
//! report.

use std::fmt;

use base64::Engine as _;
use zeroize::Zeroize as _;

use crate::codec::{Line, server_time_millis};
use crate::ctcp;
use crate::isupport::Isupport;
use crate::split::{self, MAX_LINE, TextError};

/// IRCv3 capabilities this client asks for when the server offers them.
pub const WANTED_CAPS: [&str; 4] = ["server-time", "message-tags", "sasl", "account-tag"];

/// A secret that never shows in `Debug` output and is wiped from memory when
/// dropped.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Password(String);

impl Password {
    /// Wrap a password supplied by the caller (for example from an
    /// environment variable). It is used once, for SASL, then wiped.
    #[must_use]
    pub fn new(secret: String) -> Password {
        Password(secret)
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(<redacted>)")
    }
}

impl Drop for Password {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// SASL PLAIN credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sasl {
    /// The account name (authentication identity).
    pub user: String,
    /// The account password; its `Debug` output is redacted.
    pub password: Password,
}

/// Who to register as and what to join.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The nick to ask for; `_` is appended while the server says it is in use.
    pub nick: String,
    /// The user (ident) name sent in `USER`.
    pub user: String,
    /// The real name sent in `USER`.
    pub realname: String,
    /// Channels to join once registered.
    pub channels: Vec<String>,
    /// SASL PLAIN credentials, used only when the server offers `sasl`.
    pub sasl: Option<Sasl>,
}

/// A request from the App.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Command {
    /// Join a channel (queued until registration completes).
    Join(String),
    /// Leave a channel, with an optional reason.
    Part {
        /// The channel.
        channel: String,
        /// The part message.
        reason: Option<String>,
    },
    /// Send a message to a channel or nick. Line breaks start new messages;
    /// long text is split to fit the line limit.
    Privmsg {
        /// The channel or nick.
        target: String,
        /// The message text.
        text: String,
    },
    /// Send a CTCP `ACTION` (`/me`).
    Action {
        /// The channel or nick.
        target: String,
        /// The action text.
        text: String,
    },
    /// Change nick.
    Nick(String),
    /// Query (`None`) or set a channel's topic.
    Topic {
        /// The channel.
        channel: String,
        /// The new topic, or `None` to ask for the current one.
        topic: Option<String>,
    },
    /// Leave the server, with an optional reason.
    Quit(Option<String>),
}

/// What kind of problem an [`Event::Error`] reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The nick asked for after registration is taken (`433`).
    NickInUse,
    /// SASL authentication failed or was aborted (`902`, `904`-`908`).
    SaslFailed,
    /// The server sent an error numeric (`400`-`599`).
    Numeric(u16),
    /// The server sent `ERROR` (it is about to close the connection).
    Server,
    /// An App command was refused before sending (bad name, empty text,
    /// not registered yet).
    InvalidCommand,
}

/// Something the App should show or act on.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Event {
    /// Registration finished (`001`); `nick` is the nick the server gave us.
    Connected {
        /// Our nick.
        nick: String,
    },
    /// We joined `channel`.
    Joined {
        /// The channel.
        channel: String,
    },
    /// We left `channel` (by `PART` or `KICK`).
    Parted {
        /// The channel.
        channel: String,
        /// The part or kick reason, if any.
        reason: Option<String>,
    },
    /// Someone else joined a channel we are in.
    UserJoined {
        /// The channel.
        channel: String,
        /// Their nick.
        nick: String,
    },
    /// Someone else left a channel (by `PART` or `KICK`).
    UserParted {
        /// The channel.
        channel: String,
        /// Their nick.
        nick: String,
        /// The reason, if any.
        reason: Option<String>,
    },
    /// Someone else quit the server.
    UserQuit {
        /// Their nick.
        nick: String,
        /// The quit reason, if any.
        reason: Option<String>,
    },
    /// A `PRIVMSG` (or CTCP `ACTION`). The text is raw: the App sanitizes it
    /// before storing or drawing it.
    Message {
        /// The channel, or our own nick for a direct message.
        target: String,
        /// The sender's nick (unverified unless `from_account` is set).
        from: String,
        /// The sender's account as confirmed by the server's `account` tag.
        from_account: Option<String>,
        /// The message text (without CTCP framing).
        text: String,
        /// Whether this was a `/me` action.
        is_action: bool,
        /// The server's `server-time`, in UTC milliseconds since the epoch;
        /// `None` when the server sent none (the App uses its own clock).
        time: Option<i64>,
    },
    /// A `NOTICE`.
    Notice {
        /// The channel, our nick, or `*` before registration.
        target: String,
        /// The sender's nick or the server name.
        from: String,
        /// The notice text (raw; the App sanitizes it).
        text: String,
        /// The server's `server-time` in UTC milliseconds, if sent.
        time: Option<i64>,
    },
    /// A channel topic (on join, on query, or when someone changes it).
    Topic {
        /// The channel.
        channel: String,
        /// The topic, or `None` when the channel has none.
        topic: Option<String>,
        /// Who changed it, when this is a live change.
        set_by: Option<String>,
    },
    /// The member list of a channel (all `353` lines up to `366`), with
    /// status prefixes (`@`, `+`, ...) removed.
    Names {
        /// The channel.
        channel: String,
        /// The nicks.
        nicks: Vec<String>,
    },
    /// A nick changed; `is_self` when it was ours.
    NickChanged {
        /// The old nick.
        old: String,
        /// The new nick.
        new: String,
        /// Whether it was our own nick.
        is_self: bool,
    },
    /// Something went wrong; the connection may still be usable.
    Error {
        /// The kind of problem.
        kind: ErrorKind,
        /// The server's text or a short description (never a credential).
        detail: String,
    },
    /// The connection ended.
    Disconnected {
        /// Why (the server's `ERROR` text, an I/O error, or a quit).
        reason: String,
    },
}

/// The result of one input: lines to send (without CRLF) and events.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Step {
    /// Lines to write to the server, in order, each without its CRLF.
    pub send: Vec<String>,
    /// Events for the App, in order.
    pub events: Vec<Event>,
}

impl Step {
    fn line(&mut self, command: &str, params: &[&str]) {
        // Every name in an outgoing line was checked or came from the
        // server's own lines, so this only fails on a bug; drop the line
        // rather than send a broken one.
        if let Ok(wire) = Line::new(command, params).to_wire() {
            self.send.push(wire);
        }
    }

    fn event(&mut self, event: Event) {
        self.events.push(event);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Before [`Client::start`].
    Idle,
    /// Collecting `CAP LS` replies.
    CapLs,
    /// Waiting for `CAP ACK`/`NAK`.
    CapReq,
    /// Running SASL.
    Sasl,
    /// `CAP END` sent (or no CAP support); waiting for `001`.
    Registering,
    /// Registered.
    Registered,
    /// Quit sent or `ERROR` received.
    Closed,
}

/// The IRC client state machine. See the module docs.
#[derive(Debug)]
pub struct Client {
    config: Config,
    phase: Phase,
    nick: String,
    isupport: Isupport,
    offered: Vec<(String, Option<String>)>,
    enabled: Vec<String>,
    pending_joins: Vec<String>,
    names: Vec<(String, Vec<String>)>,
    nick_retries: u8,
}

/// How many times registration retries a taken nick before giving up and
/// reporting [`ErrorKind::NickInUse`].
const MAX_NICK_RETRIES: u8 = 5;

/// Bytes kept free on outgoing lines for the `:nick!user@host ` prefix the
/// server adds when it relays our message, so others receive it whole.
fn relay_reserve(nick: &str, user: &str) -> usize {
    // ':' nick '!' '~' user '@' host(<=63) ' '
    nick.len() + user.len() + 63 + 5
}

impl Client {
    /// A client that will register as `config` says.
    #[must_use]
    pub fn new(config: Config) -> Client {
        let nick = config.nick.clone();
        let pending_joins = config.channels.clone();
        Client {
            config,
            phase: Phase::Idle,
            nick,
            isupport: Isupport::default(),
            offered: Vec::new(),
            enabled: Vec::new(),
            pending_joins,
            names: Vec::new(),
            nick_retries: 0,
        }
    }

    /// Our current nick.
    #[must_use]
    pub fn nick(&self) -> &str {
        &self.nick
    }

    /// The server's declared `CASEMAPPING`, `NICKLEN` and `CHANTYPES`.
    #[must_use]
    pub fn isupport(&self) -> &Isupport {
        &self.isupport
    }

    /// The capabilities the server acknowledged.
    #[must_use]
    pub fn enabled_caps(&self) -> &[String] {
        &self.enabled
    }

    /// Whether registration has finished (`001` received).
    #[must_use]
    pub fn is_registered(&self) -> bool {
        self.phase == Phase::Registered
    }

    /// Whether `name` is our own nick under the server's case mapping.
    #[must_use]
    pub fn is_me(&self, name: &str) -> bool {
        self.isupport.casemapping.same(name, &self.nick)
    }

    /// The opening lines: `CAP LS 302`, `NICK`, `USER`. The server holds
    /// registration until `CAP END`, which this client sends once capability
    /// negotiation (and SASL) is done.
    ///
    /// # Errors
    /// [`TextError::BadName`] when the configured nick or user name is
    /// unusable (empty, spaces, control characters), so nothing is sent.
    pub fn start(&mut self) -> Result<Step, TextError> {
        split::check_name(&self.config.nick)?;
        split::check_name(&self.config.user)?;
        let mut step = Step::default();
        step.line("CAP", &["LS", "302"]);
        step.line("NICK", &[&self.config.nick]);
        let realname = split::clean_single_line(&self.config.realname);
        let realname = if realname.is_empty() {
            self.config.nick.clone()
        } else {
            realname
        };
        step.line("USER", &[&self.config.user, "0", "*", &realname]);
        self.phase = Phase::CapLs;
        Ok(step)
    }

    /// Handle one line received from the server. Unparseable lines are
    /// ignored.
    #[must_use]
    pub fn handle_line(&mut self, raw: &str) -> Step {
        let mut step = Step::default();
        let Ok(line) = Line::parse(raw) else {
            return step;
        };
        match line.command.as_str() {
            "PING" => step.line(
                "PONG",
                &line.params.iter().map(String::as_str).collect::<Vec<_>>(),
            ),
            "CAP" => self.on_cap(&line, &mut step),
            "AUTHENTICATE" => self.on_authenticate(&line, &mut step),
            "903" => self.end_cap(&mut step),
            "902" | "904" | "905" | "906" | "907" | "908" => {
                if self.phase == Phase::Sasl {
                    step.event(Event::Error {
                        kind: ErrorKind::SaslFailed,
                        detail: line.params.last().cloned().unwrap_or_default(),
                    });
                    self.end_cap(&mut step);
                }
            }
            "001" => self.on_welcome(&line, &mut step),
            "005" => {
                let tokens = line.params.get(1..).unwrap_or_default();
                self.isupport.apply(tokens);
            }
            "433" if self.phase != Phase::Registered && self.nick_retries < MAX_NICK_RETRIES => {
                self.retry_nick(&mut step);
            }
            "332" => step.event(Event::Topic {
                channel: line.param(1).to_string(),
                topic: Some(line.param(2).to_string()),
                set_by: None,
            }),
            "331" => step.event(Event::Topic {
                channel: line.param(1).to_string(),
                topic: None,
                set_by: None,
            }),
            "353" => self.on_names(&line),
            "366" => self.on_end_names(&line, &mut step),
            "JOIN" => self.on_join(&line, &mut step),
            "PART" => self.on_part(&line, &mut step),
            "KICK" => self.on_kick(&line, &mut step),
            "QUIT" => step.event(Event::UserQuit {
                nick: line.source_nick().to_string(),
                reason: line.params.first().cloned().filter(|r| !r.is_empty()),
            }),
            "NICK" => self.on_nick(&line, &mut step),
            "TOPIC" => step.event(Event::Topic {
                channel: line.param(0).to_string(),
                topic: Some(line.param(1).to_string()).filter(|t| !t.is_empty()),
                set_by: Some(line.source_nick().to_string()),
            }),
            "PRIVMSG" => self.on_privmsg(&line, &mut step),
            "NOTICE" => step.event(Event::Notice {
                target: line.param(0).to_string(),
                from: line.source_nick().to_string(),
                text: line.param(1).to_string(),
                time: Self::time_of(&line),
            }),
            "ERROR" => {
                self.phase = Phase::Closed;
                step.event(Event::Error {
                    kind: ErrorKind::Server,
                    detail: line.param(0).to_string(),
                });
            }
            numeric => Self::on_other(numeric, &line, &mut step),
        }
        step
    }

    /// Turn an App command into lines. A refused command yields an
    /// [`Event::Error`] of kind [`ErrorKind::InvalidCommand`] and sends
    /// nothing. Our own messages are not echoed back as events: the App
    /// records what it sent.
    #[must_use]
    pub fn command(&mut self, command: Command) -> Step {
        let mut step = Step::default();
        if let Err(e) = self.try_command(command, &mut step) {
            step.send.clear();
            step.event(Event::Error {
                kind: ErrorKind::InvalidCommand,
                detail: e.to_string(),
            });
        }
        step
    }

    fn try_command(&mut self, command: Command, step: &mut Step) -> Result<(), TextError> {
        let registered = self.phase == Phase::Registered;
        match command {
            Command::Join(channel) => {
                split::check_name(&channel)?;
                if registered {
                    step.line("JOIN", &[&channel]);
                } else {
                    self.pending_joins.push(channel);
                }
            }
            Command::Part { channel, reason } => {
                split::check_name(&channel)?;
                Self::need_registered(registered)?;
                match reason.map(|r| split::clean_single_line(&r)) {
                    Some(r) if !r.is_empty() => step.line("PART", &[&channel, &r]),
                    _ => step.line("PART", &[&channel]),
                }
            }
            Command::Privmsg { target, text } => {
                Self::need_registered(registered)?;
                step.send.extend(split::privmsg_lines(
                    &target,
                    &text,
                    false,
                    self.max_line(),
                )?);
            }
            Command::Action { target, text } => {
                Self::need_registered(registered)?;
                step.send
                    .extend(split::privmsg_lines(&target, &text, true, self.max_line())?);
            }
            Command::Nick(nick) => {
                split::check_name(&nick)?;
                if !registered {
                    nick.clone_into(&mut self.nick);
                }
                step.line("NICK", &[&nick]);
            }
            Command::Topic { channel, topic } => {
                split::check_name(&channel)?;
                Self::need_registered(registered)?;
                match topic {
                    // An empty string clears the topic, so it is sent as `:`.
                    Some(t) => step.line("TOPIC", &[&channel, &split::clean_single_line(&t)]),
                    None => step.line("TOPIC", &[&channel]),
                }
            }
            Command::Quit(reason) => {
                match reason.map(|r| split::clean_single_line(&r)) {
                    Some(r) if !r.is_empty() => step.line("QUIT", &[&r]),
                    _ => step.line("QUIT", &[]),
                }
                self.phase = Phase::Closed;
            }
        }
        Ok(())
    }

    fn need_registered(registered: bool) -> Result<(), TextError> {
        if registered {
            Ok(())
        } else {
            Err(TextError::NotRegistered)
        }
    }

    fn max_line(&self) -> usize {
        MAX_LINE.saturating_sub(relay_reserve(&self.nick, &self.config.user))
    }

    fn time_of(line: &Line) -> Option<i64> {
        line.tag("time").and_then(server_time_millis)
    }

    fn on_cap(&mut self, line: &Line, step: &mut Step) {
        // CAP <target> <sub> [*] :<caps>
        let sub = line.param(1).to_ascii_uppercase();
        let more = line.params.len() > 3 && line.param(2) == "*";
        let caps = if line.params.len() >= 3 {
            line.params.last().map_or("", String::as_str)
        } else {
            ""
        };
        match sub.as_str() {
            "LS" if self.phase == Phase::CapLs => {
                for cap in caps.split(' ').filter(|c| !c.is_empty()) {
                    let (name, value) = match cap.split_once('=') {
                        Some((n, v)) => (n.to_string(), Some(v.to_string())),
                        None => (cap.to_string(), None),
                    };
                    self.offered.push((name, value));
                }
                if !more {
                    self.request_caps(step);
                }
            }
            "ACK" => {
                for cap in caps.split(' ').filter(|c| !c.is_empty()) {
                    // `-name` acknowledges that a capability was disabled.
                    if let Some(gone) = cap.strip_prefix('-') {
                        self.enabled.retain(|c| c != gone);
                    } else if !self.enabled.iter().any(|c| c == cap) {
                        self.enabled.push(cap.to_string());
                    }
                }
                if self.phase == Phase::CapReq {
                    if self.enabled.iter().any(|c| c == "sasl") && self.config.sasl.is_some() {
                        self.phase = Phase::Sasl;
                        step.line("AUTHENTICATE", &["PLAIN"]);
                    } else {
                        self.end_cap(step);
                    }
                }
            }
            "NAK" if self.phase == Phase::CapReq => self.end_cap(step),
            _ => {}
        }
    }

    fn sasl_plain_offered(&self) -> bool {
        self.offered.iter().any(|(name, value)| {
            name == "sasl"
                && value
                    .as_deref()
                    .is_none_or(|v| v.split(',').any(|m| m.eq_ignore_ascii_case("PLAIN")))
        })
    }

    fn request_caps(&mut self, step: &mut Step) {
        let wanted: Vec<&str> = WANTED_CAPS
            .iter()
            .copied()
            .filter(|w| self.offered.iter().any(|(name, _)| name == w))
            .filter(|w| *w != "sasl" || (self.config.sasl.is_some() && self.sasl_plain_offered()))
            .collect();
        if wanted.is_empty() {
            self.end_cap(step);
        } else {
            self.phase = Phase::CapReq;
            step.line("CAP", &["REQ", &wanted.join(" ")]);
        }
    }

    fn on_authenticate(&mut self, line: &Line, step: &mut Step) {
        if self.phase != Phase::Sasl || line.param(0) != "+" {
            return;
        }
        let Some(sasl) = self.config.sasl.take() else {
            step.line("AUTHENTICATE", &["*"]);
            return;
        };
        // authzid NUL authcid NUL passwd; authzid empty (same as authcid).
        let mut plain = Vec::with_capacity(sasl.user.len() * 2 + sasl.password.0.len() + 2);
        plain.push(0);
        plain.extend_from_slice(sasl.user.as_bytes());
        plain.push(0);
        plain.extend_from_slice(sasl.password.0.as_bytes());
        let mut encoded = base64::engine::general_purpose::STANDARD.encode(&plain);
        plain.zeroize();
        // `sasl` (and its Password) drops here and is wiped.
        drop(sasl);
        for chunk in encoded.as_bytes().chunks(400) {
            // Base64 is ASCII, so every chunk is valid UTF-8.
            step.line("AUTHENTICATE", &[std::str::from_utf8(chunk).unwrap_or("*")]);
        }
        if encoded.len() % 400 == 0 {
            step.line("AUTHENTICATE", &["+"]);
        }
        encoded.zeroize();
    }

    fn end_cap(&mut self, step: &mut Step) {
        // The password is needed for one handshake only.
        self.config.sasl = None;
        if matches!(self.phase, Phase::CapLs | Phase::CapReq | Phase::Sasl) {
            step.line("CAP", &["END"]);
            self.phase = Phase::Registering;
        }
    }

    fn retry_nick(&mut self, step: &mut Step) {
        self.nick_retries += 1;
        let mut next = format!("{}_", self.nick);
        if let Some(max) = self.isupport.nicklen
            && next.len() > max
        {
            // Out of room: replace the last character instead.
            next = self.nick.chars().take(max.saturating_sub(1)).collect();
            next.push('_');
            if next == self.nick {
                next.pop();
                next.push('-');
            }
        }
        step.line("NICK", &[&next]);
        self.nick = next;
    }

    fn on_welcome(&mut self, line: &Line, step: &mut Step) {
        // A server without CAP support goes straight to 001.
        self.config.sasl = None;
        self.phase = Phase::Registered;
        if !line.param(0).is_empty() {
            self.nick = line.param(0).to_string();
        }
        step.event(Event::Connected {
            nick: self.nick.clone(),
        });
        for channel in std::mem::take(&mut self.pending_joins) {
            if split::check_name(&channel).is_ok() {
                step.line("JOIN", &[&channel]);
            }
        }
    }

    fn on_names(&mut self, line: &Line) {
        // 353 me <type> <channel> :nicks
        let channel = line.param(2).to_string();
        let nicks = line
            .param(3)
            .split(' ')
            .filter(|n| !n.is_empty())
            .map(|n| n.trim_start_matches(['~', '&', '@', '%', '+']).to_string())
            .filter(|n| !n.is_empty());
        if let Some((_, list)) = self.names.iter_mut().find(|(c, _)| *c == channel) {
            list.extend(nicks);
        } else {
            self.names.push((channel, nicks.collect()));
        }
    }

    fn on_end_names(&mut self, line: &Line, step: &mut Step) {
        let channel = line.param(1);
        let at = self.names.iter().position(|(c, _)| c == channel);
        let nicks = at.map(|i| self.names.swap_remove(i).1).unwrap_or_default();
        step.event(Event::Names {
            channel: channel.to_string(),
            nicks,
        });
    }

    fn on_join(&self, line: &Line, step: &mut Step) {
        let channel = line.param(0).to_string();
        let who = line.source_nick();
        if self.is_me(who) {
            step.event(Event::Joined { channel });
        } else {
            step.event(Event::UserJoined {
                channel,
                nick: who.to_string(),
            });
        }
    }

    fn on_part(&self, line: &Line, step: &mut Step) {
        let channel = line.param(0).to_string();
        let reason = line.params.get(1).cloned().filter(|r| !r.is_empty());
        let who = line.source_nick();
        if self.is_me(who) {
            step.event(Event::Parted { channel, reason });
        } else {
            step.event(Event::UserParted {
                channel,
                nick: who.to_string(),
                reason,
            });
        }
    }

    fn on_kick(&self, line: &Line, step: &mut Step) {
        // KICK <channel> <nick> [:reason]
        let channel = line.param(0).to_string();
        let victim = line.param(1);
        let reason = line.params.get(2).cloned().filter(|r| !r.is_empty());
        if self.is_me(victim) {
            step.event(Event::Parted { channel, reason });
        } else {
            step.event(Event::UserParted {
                channel,
                nick: victim.to_string(),
                reason,
            });
        }
    }

    fn on_nick(&mut self, line: &Line, step: &mut Step) {
        let old = line.source_nick().to_string();
        let new = line.param(0).to_string();
        let is_self = self.is_me(&old);
        if is_self {
            self.nick.clone_from(&new);
        }
        step.event(Event::NickChanged { old, new, is_self });
    }

    fn on_privmsg(&self, line: &Line, step: &mut Step) {
        let body = line.param(1);
        let (text, is_action) = match ctcp::decode_action(body) {
            Some(text) => (text, true),
            // Other CTCP requests are ignored and never answered.
            None if ctcp::is_ctcp(body) => return,
            None => (body, false),
        };
        let from_account = if self.enabled.iter().any(|c| c == "account-tag") {
            line.tag("account").map(str::to_string)
        } else {
            None
        };
        step.event(Event::Message {
            target: line.param(0).to_string(),
            from: line.source_nick().to_string(),
            from_account,
            text: text.to_string(),
            is_action,
            time: Self::time_of(line),
        });
    }

    fn on_other(command: &str, line: &Line, step: &mut Step) {
        let Ok(code) = command.parse::<u16>() else {
            return;
        };
        if command.len() != 3 || !(400..600).contains(&code) {
            return;
        }
        let kind = if code == 433 {
            ErrorKind::NickInUse
        } else {
            ErrorKind::Numeric(code)
        };
        // Skip our own nick (param 0); the rest names the subject and reason.
        let detail = line.params.get(1..).unwrap_or_default().join(" ");
        step.event(Event::Error { kind, detail });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_LINES: [String; 0] = [];

    fn config(sasl: bool) -> Config {
        Config {
            nick: "ana".into(),
            user: "ana".into(),
            realname: "Ana".into(),
            channels: vec!["#vix".into()],
            sasl: sasl.then(|| Sasl {
                user: "ana".into(),
                password: Password::new("hunter2".into()),
            }),
        }
    }

    #[test]
    fn password_is_redacted() {
        let c = config(true);
        let shown = format!("{c:?}");
        assert!(!shown.contains("hunter2"), "{shown}");
        assert!(shown.contains("<redacted>"));
    }

    #[test]
    fn registration_with_sasl() {
        let mut c = Client::new(config(true));
        let s = c.start().unwrap();
        assert_eq!(s.send, vec!["CAP LS 302", "NICK ana", "USER ana 0 * Ana"]);
        let s = c.handle_line(":srv CAP * LS * :multi-prefix sasl=PLAIN,EXTERNAL");
        assert_eq!(s.send, NO_LINES);
        let s = c.handle_line(":srv CAP * LS :server-time account-tag away-notify");
        assert_eq!(s.send, vec!["CAP REQ :server-time sasl account-tag"]);
        let s = c.handle_line(":srv CAP ana ACK :server-time sasl account-tag");
        assert_eq!(s.send, vec!["AUTHENTICATE PLAIN"]);
        let s = c.handle_line("AUTHENTICATE +");
        // base64("\0ana\0hunter2")
        assert_eq!(s.send, vec!["AUTHENTICATE AGFuYQBodW50ZXIy"]);
        assert!(format!("{c:?}").find("hunter2").is_none());
        let s = c.handle_line(":srv 903 ana :SASL authentication successful");
        assert_eq!(s.send, vec!["CAP END"]);
        let s = c.handle_line(":srv 001 ana :Welcome");
        assert_eq!(s.events, vec![Event::Connected { nick: "ana".into() }]);
        assert_eq!(s.send, vec!["JOIN #vix"]);
        assert!(c.is_registered());
    }

    #[test]
    fn sasl_not_requested_without_credentials_or_plain() {
        let mut c = Client::new(config(false));
        let _ = c.start().unwrap();
        let s = c.handle_line("CAP * LS :sasl message-tags");
        assert_eq!(s.send, vec!["CAP REQ message-tags"]);
        let mut c = Client::new(config(true));
        let _ = c.start().unwrap();
        let s = c.handle_line("CAP * LS :sasl=EXTERNAL");
        assert_eq!(s.send, vec!["CAP END"]);
    }

    #[test]
    fn sasl_failure_reports_and_ends_cap() {
        let mut c = Client::new(config(true));
        let _ = c.start().unwrap();
        let _ = c.handle_line("CAP * LS :sasl");
        let _ = c.handle_line("CAP * ACK :sasl");
        let _ = c.handle_line("AUTHENTICATE +");
        let s = c.handle_line(":srv 904 ana :SASL authentication failed");
        assert_eq!(s.send, vec!["CAP END"]);
        assert!(matches!(
            s.events[0],
            Event::Error {
                kind: ErrorKind::SaslFailed,
                ..
            }
        ));
    }

    #[test]
    fn nak_and_no_cap_server() {
        let mut c = Client::new(config(false));
        let _ = c.start().unwrap();
        let _ = c.handle_line("CAP * LS :server-time");
        assert_eq!(
            c.handle_line("CAP * NAK :server-time").send,
            vec!["CAP END"]
        );
        // A server that ignores CAP and welcomes directly.
        let mut c = Client::new(config(false));
        let _ = c.start().unwrap();
        let s = c.handle_line(":srv 001 ana :hi");
        assert!(c.is_registered());
        assert_eq!(s.send, vec!["JOIN #vix"]);
    }

    #[test]
    fn nick_in_use_appends_underscore() {
        let mut c = Client::new(config(false));
        let _ = c.start().unwrap();
        let s = c.handle_line(":srv 433 * ana :Nickname is already in use");
        assert_eq!(s.send, vec!["NICK ana_"]);
        let s = c.handle_line(":srv 433 * ana_ :Nickname is already in use");
        assert_eq!(s.send, vec!["NICK ana__"]);
        let s = c.handle_line(":srv 001 ana__ :Welcome");
        assert_eq!(c.nick(), "ana__");
        assert!(matches!(s.events[0], Event::Connected { .. }));
        // After registration, 433 is an error, not a retry.
        let s = c.handle_line(":srv 433 ana__ bob :Nickname is already in use");
        assert_eq!(s.send, NO_LINES);
        assert!(matches!(
            s.events[0],
            Event::Error {
                kind: ErrorKind::NickInUse,
                ..
            }
        ));
    }

    #[test]
    fn nicklen_bounds_retry() {
        let mut c = Client::new(config(false));
        let _ = c.start().unwrap();
        let _ = c.handle_line(":srv 005 * NICKLEN=3 :are supported");
        let s = c.handle_line(":srv 433 * ana :in use");
        assert_eq!(s.send, vec!["NICK an_"]);
        let s = c.handle_line(":srv 433 * an_ :in use");
        assert_eq!(s.send, vec!["NICK an-"]);
    }

    fn registered() -> Client {
        let mut c = Client::new(config(false));
        let _ = c.start().unwrap();
        let _ = c.handle_line("CAP * LS :server-time account-tag message-tags");
        let _ = c.handle_line("CAP * ACK :server-time account-tag message-tags");
        let _ = c.handle_line(":srv 001 ana :Welcome");
        c
    }

    #[test]
    fn cap_edge_cases() {
        let mut c = Client::new(config(false));
        let _ = c.start().unwrap();
        // An empty LS (no caps parameter) requests nothing.
        assert_eq!(c.handle_line("CAP * LS").send, vec!["CAP END"]);
        let mut c = registered();
        let _ = c.handle_line("CAP ana ACK :-account-tag");
        assert!(!c.enabled_caps().iter().any(|x| x == "account-tag"));
        // Retries stop after MAX_NICK_RETRIES; then 433 is an error.
        let mut c = Client::new(config(false));
        let _ = c.start().unwrap();
        for _ in 0..MAX_NICK_RETRIES {
            assert_eq!(c.handle_line(":srv 433 * x :in use").send.len(), 1);
        }
        let s = c.handle_line(":srv 433 * x :in use");
        assert_eq!(s.send, NO_LINES);
        assert!(matches!(
            s.events[0],
            Event::Error {
                kind: ErrorKind::NickInUse,
                ..
            }
        ));
    }

    #[test]
    fn ping_pong() {
        let mut c = registered();
        assert_eq!(c.handle_line("PING :abc def").send, vec!["PONG :abc def"]);
        assert_eq!(c.handle_line("PING x").send, vec!["PONG x"]);
    }

    #[test]
    fn messages_and_actions() {
        let mut c = registered();
        let s = c.handle_line(
            "@time=2024-01-02T03:04:05.000Z;account=bobacct :bob!b@h PRIVMSG #vix :hello",
        );
        assert_eq!(
            s.events,
            vec![Event::Message {
                target: "#vix".into(),
                from: "bob".into(),
                from_account: Some("bobacct".into()),
                text: "hello".into(),
                is_action: false,
                time: server_time_millis("2024-01-02T03:04:05.000Z"),
            }]
        );
        let s = c.handle_line(":bob!b@h PRIVMSG ana :\u{1}ACTION waves\u{1}");
        let Event::Message {
            is_action,
            text,
            time,
            from_account,
            ..
        } = &s.events[0]
        else {
            panic!("{s:?}")
        };
        assert!(*is_action);
        assert_eq!(text, "waves");
        assert_eq!(*time, None);
        assert_eq!(*from_account, None);
        // CTCP VERSION is ignored and never answered.
        let s = c.handle_line(":bob!b@h PRIVMSG ana :\u{1}VERSION\u{1}");
        assert_eq!(s, Step::default());
    }

    #[test]
    fn account_tag_ignored_unless_acked() {
        let mut c = Client::new(config(false));
        let _ = c.start().unwrap();
        let _ = c.handle_line(":srv 001 ana :Welcome");
        let s = c.handle_line("@account=fake :bob!b@h PRIVMSG #vix :hi");
        assert!(matches!(
            &s.events[0],
            Event::Message {
                from_account: None,
                ..
            }
        ));
    }

    #[test]
    fn membership_topic_names_nick() {
        let mut c = registered();
        assert_eq!(
            c.handle_line(":Ana!a@h JOIN #vix").events,
            vec![Event::Joined {
                channel: "#vix".into()
            }]
        );
        assert_eq!(
            c.handle_line(":bob!b@h JOIN :#vix").events,
            vec![Event::UserJoined {
                channel: "#vix".into(),
                nick: "bob".into()
            }]
        );
        assert_eq!(
            c.handle_line(":srv 332 ana #vix :The topic").events,
            vec![Event::Topic {
                channel: "#vix".into(),
                topic: Some("The topic".into()),
                set_by: None
            }]
        );
        assert_eq!(
            c.handle_line(":srv 353 ana = #vix :@ana +bob").events,
            Vec::<Event>::new()
        );
        let _ = c.handle_line(":srv 353 ana = #vix :carol");
        assert_eq!(
            c.handle_line(":srv 366 ana #vix :End of /NAMES list")
                .events,
            vec![Event::Names {
                channel: "#vix".into(),
                nicks: vec!["ana".into(), "bob".into(), "carol".into()]
            }]
        );
        assert_eq!(
            c.handle_line(":bob!b@h PART #vix :bye").events,
            vec![Event::UserParted {
                channel: "#vix".into(),
                nick: "bob".into(),
                reason: Some("bye".into())
            }]
        );
        assert_eq!(
            c.handle_line(":op!o@h KICK #vix ana :out").events,
            vec![Event::Parted {
                channel: "#vix".into(),
                reason: Some("out".into())
            }]
        );
        let s = c.handle_line(":ana!a@h NICK :anna");
        assert_eq!(
            s.events,
            vec![Event::NickChanged {
                old: "ana".into(),
                new: "anna".into(),
                is_self: true
            }]
        );
        assert_eq!(c.nick(), "anna");
        assert!(matches!(
            c.handle_line(":bob!b@h QUIT :gone").events[0],
            Event::UserQuit { .. }
        ));
        let s = c.handle_line("ERROR :Closing link");
        assert!(matches!(
            s.events[0],
            Event::Error {
                kind: ErrorKind::Server,
                ..
            }
        ));
    }

    #[test]
    fn commands() {
        let mut c = Client::new(config(false));
        let _ = c.start().unwrap();
        // Before registration: joins queue, messages are refused.
        assert_eq!(c.command(Command::Join("#rust".into())).send, NO_LINES);
        let s = c.command(Command::Privmsg {
            target: "#vix".into(),
            text: "hi".into(),
        });
        assert_eq!(s.send, NO_LINES);
        assert!(matches!(
            s.events[0],
            Event::Error {
                kind: ErrorKind::InvalidCommand,
                ..
            }
        ));
        let s = c.handle_line(":srv 001 ana :Welcome");
        assert_eq!(s.send, vec!["JOIN #vix", "JOIN #rust"]);

        let s = c.command(Command::Privmsg {
            target: "#vix".into(),
            text: "hello\r\nQUIT :pwned".into(),
        });
        assert_eq!(
            s.send,
            vec!["PRIVMSG #vix :hello", "PRIVMSG #vix :QUIT :pwned"]
        );
        let s = c.command(Command::Action {
            target: "#vix".into(),
            text: "waves".into(),
        });
        assert_eq!(s.send, vec!["PRIVMSG #vix :\u{1}ACTION waves\u{1}"]);
        let s = c.command(Command::Join("#a\r\nQUIT".into()));
        assert_eq!(s.send, NO_LINES);
        assert_eq!(s.events.len(), 1);
        let s = c.command(Command::Nick("bad nick".into()));
        assert_eq!(s.send, NO_LINES);
        let s = c.command(Command::Topic {
            channel: "#vix".into(),
            topic: Some("new\ntopic".into()),
        });
        assert_eq!(s.send, vec!["TOPIC #vix :new topic"]);
        let s = c.command(Command::Topic {
            channel: "#vix".into(),
            topic: None,
        });
        assert_eq!(s.send, vec!["TOPIC #vix"]);
        let s = c.command(Command::Part {
            channel: "#vix".into(),
            reason: Some("bye".into()),
        });
        assert_eq!(s.send, vec!["PART #vix bye"]);
        assert_eq!(
            c.command(Command::Nick("anna".into())).send,
            vec!["NICK anna"]
        );
        assert_eq!(c.command(Command::Quit(None)).send, vec!["QUIT"]);
    }

    #[test]
    fn long_messages_leave_room_for_the_relay_prefix() {
        let mut c = registered();
        let text = "é".repeat(1000);
        let s = c.command(Command::Privmsg {
            target: "#vix".into(),
            text: text.clone(),
        });
        assert!(s.send.len() > 2);
        let mut joined = String::new();
        for l in &s.send {
            // Fits 512 even after the server prepends ":ana!~ana@<63-byte host> ".
            assert!(l.len() + 2 + relay_reserve("ana", "ana") <= MAX_LINE);
            joined.push_str(&l["PRIVMSG #vix :".len()..]);
        }
        assert_eq!(joined, text);
    }

    #[test]
    fn start_refuses_bad_config() {
        let mut cfg = config(false);
        cfg.nick = "a\r\nb".into();
        assert_eq!(Client::new(cfg).start(), Err(TextError::BadName));
    }

    #[test]
    fn casemapped_self_detection() {
        let mut c = registered();
        let _ = c.handle_line(":srv 005 ana CASEMAPPING=rfc1459 :are supported");
        let mut cfg = config(false);
        cfg.nick = "a[b]".into();
        let mut c2 = Client::new(cfg);
        let _ = c2.start().unwrap();
        let _ = c2.handle_line(":srv 001 a[b] :hi");
        assert_eq!(
            c2.handle_line(":A{B}!x@h JOIN #vix").events,
            vec![Event::Joined {
                channel: "#vix".into()
            }]
        );
        assert!(c.is_me("ANA"));
    }
}
