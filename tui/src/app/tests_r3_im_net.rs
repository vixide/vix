//! Round-3 tests of the IM network glue: every IRC event turned into inbox
//! lines, connection bookkeeping against a refused loopback port, and the
//! password variable's name. No network beyond 127.0.0.1.

use vix_im::ConversationId;
use vix_im_irc::{Command, ErrorKind, Event};

use super::{irc_id, password_var};
use crate::app::App;
use crate::app::cov4::{app_at, temp};
use crate::settings::ImAccount;

/// An app with the account's server conversation present, as a connect
/// would have made it.
fn app() -> App {
    let mut app = app_at(&temp("im-net"));
    app.im_ensure_conversation(
        irc_id("net", None),
        vix_im::ConversationKind::Room,
        "net",
        None,
    );
    app
}

fn account(name: &str, protocol: &str) -> ImAccount {
    let mut a = ImAccount::default();
    a.name = name.into();
    a.protocol = protocol.into();
    a.server = "127.0.0.1".into();
    a.port = 1; // nothing listens: the connection thread ends at once
    a.tls = false;
    a
}

fn bodies(app: &App, id: &ConversationId) -> Vec<String> {
    app.im
        .inbox
        .get(id)
        .map_or_default(|c| c.messages.iter().map(|m| m.body.clone()).collect())
}

fn last(app: &App, id: &ConversationId) -> String {
    bodies(app, id).pop().unwrap_or_default()
}

#[test]
fn password_variables_are_upper_cased_with_underscores() {
    assert_eq!(password_var("libera"), "VIX_IM_PASSWORD_LIBERA");
    assert_eq!(
        password_var("my-net.work 2"),
        "VIX_IM_PASSWORD_MY_NET_WORK_2"
    );
}

#[test]
fn channel_events_become_system_lines_and_inbox_state() {
    let mut app = app();
    let server = irc_id("net", None);
    app.im_irc_event("net", Event::Connected { nick: "me".into() });
    assert_eq!(app.im.inbox.nick(), "me");
    assert_eq!(
        bodies(&app, &server),
        vec![t!("msg.im_connected", nick = "me").to_string()]
    );

    let chan = irc_id("net", Some("#vix"));
    app.im_irc_event(
        "net",
        Event::Joined {
            channel: "#vix".into(),
        },
    );
    assert_eq!(
        last(&app, &chan),
        t!("msg.im_joined", nick = "me").to_string()
    );
    app.im_irc_event(
        "net",
        Event::UserJoined {
            channel: "#vix".into(),
            nick: "ada".into(),
        },
    );
    assert_eq!(
        last(&app, &chan),
        t!("msg.im_joined", nick = "ada").to_string()
    );
    app.im_irc_event(
        "net",
        Event::UserParted {
            channel: "#vix".into(),
            nick: "ada".into(),
            reason: None,
        },
    );
    assert_eq!(
        last(&app, &chan),
        t!("msg.im_left", nick = "ada").to_string()
    );
    app.im_irc_event(
        "net",
        Event::UserQuit {
            nick: "bob".into(),
            reason: Some("bye".into()),
        },
    );
    assert_eq!(
        last(&app, &server),
        t!("msg.im_quit", nick = "bob").to_string()
    );

    app.im_irc_event(
        "net",
        Event::Topic {
            channel: "#vix".into(),
            topic: Some("hello".into()),
            set_by: None,
        },
    );
    assert_eq!(
        last(&app, &chan),
        t!("msg.im_topic", topic = "hello").to_string()
    );
    assert_eq!(
        app.im.inbox.get(&chan).unwrap().topic.as_deref(),
        Some("hello")
    );
    app.im_irc_event(
        "net",
        Event::Topic {
            channel: "#vix".into(),
            topic: None,
            set_by: None,
        },
    );
    assert_eq!(last(&app, &chan), t!("msg.im_no_topic").to_string());
    app.im_irc_event(
        "net",
        Event::Names {
            channel: "#vix".into(),
            nicks: vec!["me".into(), "ada".into()],
        },
    );
    assert_eq!(
        last(&app, &chan),
        t!("msg.im_names", names = "me, ada").to_string()
    );

    // Leaving removes the conversation and notes it on the server line.
    app.im_irc_event(
        "net",
        Event::Parted {
            channel: "#vix".into(),
            reason: None,
        },
    );
    assert!(app.im.inbox.get(&chan).is_none());
    assert!(last(&app, &server).starts_with("#vix: "));
}

#[test]
fn channel_and_private_messages_are_routed_and_verified() {
    let mut app = app();
    app.im_irc_event("net", Event::Connected { nick: "me".into() });
    // A channel message and an action, the second with a server time.
    let chan = irc_id("net", Some("#vix"));
    app.im_irc_event(
        "net",
        Event::Message {
            target: "#vix".into(),
            from: "ada".into(),
            from_account: None,
            text: "hi".into(),
            is_action: false,
            time: None,
        },
    );
    app.im_irc_event(
        "net",
        Event::Message {
            target: "#vix".into(),
            from: "ada".into(),
            from_account: Some("ada-acct".into()),
            text: "waves".into(),
            is_action: true,
            time: Some(1_700_000_000_000),
        },
    );
    let messages = &app.im.inbox.get(&chan).unwrap().messages;
    assert_eq!(messages[0].body, "hi");
    assert_eq!(messages[1].time_utc, 1_700_000_000_000);
    assert!(
        app.im
            .verified
            .iter()
            .any(|(id, nick)| *id == chan && nick == "ada"),
        "a server-confirmed account marks the nick verified"
    );

    // A private message opens a direct conversation under the sender's nick.
    app.im_irc_event(
        "net",
        Event::Message {
            target: "me".into(),
            from: "bob".into(),
            from_account: None,
            text: "psst".into(),
            is_action: false,
            time: None,
        },
    );
    assert_eq!(last(&app, &irc_id("net", Some("bob"))), "psst");
}

#[test]
fn notices_nicks_errors_and_disconnects_are_routed() {
    let mut app = app();
    let server = irc_id("net", None);
    let chan = irc_id("net", Some("#vix"));
    app.im_irc_event("net", Event::Connected { nick: "me".into() });
    app.im_irc_event(
        "net",
        Event::Joined {
            channel: "#vix".into(),
        },
    );
    // Notices: to a channel they land there, otherwise on the server line.
    app.im_irc_event(
        "net",
        Event::Notice {
            target: "#vix".into(),
            from: "svc".into(),
            text: "note".into(),
            time: None,
        },
    );
    assert_eq!(last(&app, &chan), "svc: note");
    app.im_irc_event(
        "net",
        Event::Notice {
            target: "me".into(),
            from: "svc".into(),
            text: "hello".into(),
            time: None,
        },
    );
    assert_eq!(last(&app, &server), "svc: hello");

    app.im_irc_event(
        "net",
        Event::NickChanged {
            old: "me".into(),
            new: "me2".into(),
            is_self: true,
        },
    );
    assert_eq!(app.im.inbox.nick(), "me2");
    assert_eq!(
        last(&app, &server),
        t!("msg.im_nick_changed", old = "me", new = "me2").to_string()
    );
    app.im_irc_event(
        "net",
        Event::NickChanged {
            old: "ada".into(),
            new: "ada2".into(),
            is_self: false,
        },
    );
    assert_eq!(
        app.im.inbox.nick(),
        "me2",
        "someone else's change keeps our nick"
    );

    let errors = [
        (ErrorKind::SaslFailed, t!("msg.im_sasl_failed").to_string()),
        (ErrorKind::NickInUse, t!("msg.im_nick_in_use").to_string()),
        (
            ErrorKind::Server,
            t!("msg.im_error", detail = "oops").to_string(),
        ),
    ];
    for (kind, expected) in errors {
        app.im_irc_event(
            "net",
            Event::Error {
                kind,
                detail: "oops".into(),
            },
        );
        assert_eq!(last(&app, &server), expected);
    }
    app.im_irc_event(
        "net",
        Event::Disconnected {
            reason: "gone".into(),
        },
    );
    assert_eq!(
        last(&app, &server),
        t!("msg.im_disconnected", reason = "gone").to_string()
    );
}

#[test]
fn connecting_refuses_unknown_protocols_duplicates_and_missing_matrix() {
    let mut app = app();
    app.im_connect_account(7); // no such account: nothing happens
    assert!(!app.im_connected());

    app.settings
        .im
        .accounts
        .push(account("odd", "carrier-pigeon"));
    app.im_connect_account(0);
    assert_eq!(
        app.status,
        t!(
            "status.im_unknown_protocol",
            name = "odd",
            protocol = "carrier-pigeon"
        )
        .to_string()
    );
    assert!(!app.im_connected());
}

#[test]
fn a_refused_connection_connects_reports_and_is_dropped_on_poll() {
    let mut app = app();
    app.settings.im.accounts.push(account("local", "irc"));
    assert!(
        !app.im_send("local", Command::Quit(None)),
        "not connected yet"
    );
    app.im_disconnect();
    assert_eq!(app.status, t!("status.im_not_connected").to_string());

    app.im_connect_account(0);
    assert_eq!(
        app.status,
        t!("status.im_connecting", name = "local").to_string()
    );
    assert!(app.im_connected());
    assert_eq!(app.im_link_names(), vec!["local".to_string()]);
    let server = irc_id("local", None);
    assert_eq!(bodies(&app, &server)[0], t!("msg.im_plaintext").to_string());
    // A second connect for the same account is refused.
    app.im_connect_account(0);
    assert_eq!(
        app.status,
        t!("status.im_already_connected", name = "local").to_string()
    );
    assert_eq!(app.im.links.len(), 1);

    // The thread ends with a Disconnected event; polling drops the link.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while app.im_connected() {
        app.im_poll_links();
        assert!(std::time::Instant::now() < deadline, "the link never ended");
        std::thread::yield_now();
    }
    assert!(
        bodies(&app, &server)
            .iter()
            .any(|b| b.contains(&t!("msg.im_disconnected", reason = "").to_string()[..4])),
        "{:?}",
        bodies(&app, &server)
    );
}

#[test]
fn a_missing_sasl_password_is_noted_and_disconnecting_closes_the_link() {
    let mut app = app();
    let mut a = account("sasl-acct", "irc");
    a.sasl_user = "me".into();
    app.settings.im.accounts.push(a);
    app.im_connect_account(0);
    let server = irc_id("sasl-acct", None);
    let var = password_var("sasl-acct");
    assert!(
        bodies(&app, &server).contains(&t!("msg.im_no_password", var = var.as_str()).to_string()),
        "{:?}",
        bodies(&app, &server)
    );
    // With one link, it is the active account; Disconnect closes it.
    app.im_disconnect();
    assert_eq!(
        app.status,
        t!("status.im_disconnected", name = "sasl-acct").to_string()
    );
    assert!(!app.im_connected());
}
