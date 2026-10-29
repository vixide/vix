//! The connection thread against an in-process fake IRC server on
//! 127.0.0.1 (plaintext, `tls = false`). No real network is used.
#![cfg(feature = "net")]

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use vix_im_irc::net::{ConnectOptions, Handle, connect};
use vix_im_irc::{Command, Config, Event, Password, Sasl};

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

fn options(port: u16, tls: bool, cfg: Config) -> ConnectOptions {
    let mut o = ConnectOptions::new("127.0.0.1", port, tls, cfg);
    o.connect_timeout = Duration::from_secs(2);
    o.poll_interval = Duration::from_millis(20);
    o
}

/// Wait (bounded) for an event matching `pred`, returning it.
fn wait_for(handle: &Handle, pred: impl Fn(&Event) -> bool) -> Event {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(e) = handle.try_recv() {
            if pred(&e) {
                return e;
            }
            continue;
        }
        assert!(Instant::now() < deadline, "timed out waiting for event");
        thread::sleep(Duration::from_millis(5));
    }
}

struct Server {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
}

impl Server {
    fn accept(listener: &TcpListener) -> Server {
        let (stream, _) = listener
            .accept()
            .unwrap_or_else(|e| panic!("fake server: {e}"));
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap_or_else(|e| panic!("fake server: {e}"));
        Server {
            reader: BufReader::new(
                stream
                    .try_clone()
                    .unwrap_or_else(|e| panic!("fake server: {e}")),
            ),
            writer: stream,
        }
    }

    fn expect(&mut self, want: &str) {
        let mut line = String::new();
        self.reader
            .read_line(&mut line)
            .unwrap_or_else(|e| panic!("fake server: {e}"));
        assert!(line.ends_with("\r\n"), "{line:?}");
        assert_eq!(line.trim_end(), want);
    }

    fn say(&mut self, line: &str) {
        self.writer
            .write_all(line.as_bytes())
            .unwrap_or_else(|e| panic!("fake server: {e}"));
        self.writer
            .write_all(b"\r\n")
            .unwrap_or_else(|e| panic!("fake server: {e}"));
    }
}

#[test]
fn register_sasl_join_message_ping_quit() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let mut s = Server::accept(&listener);
        s.expect("CAP LS 302");
        s.expect("NICK ana");
        s.expect("USER ana 0 * Ana");
        s.say(":irc.test CAP * LS :server-time message-tags sasl=PLAIN account-tag");
        s.expect("CAP REQ :server-time message-tags sasl account-tag");
        s.say(":irc.test CAP ana ACK :server-time message-tags sasl account-tag");
        s.expect("AUTHENTICATE PLAIN");
        s.say("AUTHENTICATE +");
        s.expect("AUTHENTICATE AGFuYQBodW50ZXIy");
        s.say(":irc.test 903 ana :SASL authentication successful");
        s.expect("CAP END");
        s.say(":irc.test 433 * ana :Nickname is already in use");
        s.expect("NICK ana_");
        s.say(":irc.test 001 ana_ :Welcome to the test network");
        s.say(":irc.test 005 ana_ CASEMAPPING=ascii CHANTYPES=# :are supported");
        s.expect("JOIN #vix");
        s.say(":ana_!ana@host JOIN #vix");
        s.say(":irc.test 353 ana_ = #vix :ana_ @bob");
        s.say(":irc.test 366 ana_ #vix :End of /NAMES list.");
        s.say("PING :irc.test");
        s.expect("PONG irc.test");
        // The App's message reaches the server ...
        s.expect("PRIVMSG #vix :hello from vix");
        // ... and a reply with tags comes back as an event.
        s.say("@time=2026-10-29T12:00:00.000Z;account=bob :bob!b@h PRIVMSG #vix :hi ana_");
        s.say(":bob!b@h PRIVMSG #vix :\u{1}ACTION waves\u{1}");
        s.expect("QUIT bye");
    });

    let handle = connect(options(port, false, config(true)));
    let connected = wait_for(&handle, |e| matches!(e, Event::Connected { .. }));
    assert_eq!(
        connected,
        Event::Connected {
            nick: "ana_".into()
        }
    );
    assert_eq!(
        wait_for(&handle, |e| matches!(e, Event::Joined { .. })),
        Event::Joined {
            channel: "#vix".into()
        }
    );
    assert_eq!(
        wait_for(&handle, |e| matches!(e, Event::Names { .. })),
        Event::Names {
            channel: "#vix".into(),
            nicks: vec!["ana_".into(), "bob".into()]
        }
    );
    assert!(handle.send(Command::Privmsg {
        target: "#vix".into(),
        text: "hello from vix".into(),
    }));
    let Event::Message {
        from,
        from_account,
        text,
        is_action,
        time,
        ..
    } = wait_for(&handle, |e| matches!(e, Event::Message { .. }))
    else {
        unreachable!()
    };
    assert_eq!(from, "bob");
    assert_eq!(from_account.as_deref(), Some("bob"));
    assert_eq!(text, "hi ana_");
    assert!(!is_action);
    assert_eq!(time, Some(1_793_275_200_000));
    let Event::Message {
        text, is_action, ..
    } = wait_for(&handle, |e| matches!(e, Event::Message { .. }))
    else {
        unreachable!()
    };
    assert_eq!((text.as_str(), is_action), ("waves", true));

    assert!(handle.send(Command::Quit(Some("bye".into()))));
    let gone = wait_for(&handle, |e| matches!(e, Event::Disconnected { .. }));
    assert_eq!(
        gone,
        Event::Disconnected {
            reason: "quit".into()
        }
    );
    server.join().unwrap();
}

#[test]
fn close_returns_promptly_and_sends_quit() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let mut s = Server::accept(&listener);
        s.expect("CAP LS 302");
        s.expect("NICK ana");
        s.expect("USER ana 0 * Ana");
        // A server without CAP support: straight to the welcome.
        s.say(":irc.test 001 ana :Welcome");
        s.expect("JOIN #vix");
        // Then silence: the client must still close quickly.
        s.expect("QUIT");
    });
    let handle = connect(options(port, false, config(false)));
    wait_for(&handle, |e| matches!(e, Event::Connected { .. }));
    let started = Instant::now();
    let rest = handle.close();
    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(rest.iter().any(|e| matches!(e, Event::Disconnected { .. })));
    server.join().unwrap();
}

#[test]
fn server_closing_reports_disconnect() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let mut s = Server::accept(&listener);
        // Read everything the client sent first: closing with unread data
        // would reset the connection before ERROR is delivered.
        s.expect("CAP LS 302");
        s.expect("NICK ana");
        s.expect("USER ana 0 * Ana");
        s.say("ERROR :Closing link (banned)");
    });
    let handle = connect(options(port, false, config(false)));
    let err = wait_for(&handle, |e| matches!(e, Event::Error { .. }));
    assert!(matches!(err, Event::Error { detail, .. } if detail.contains("banned")));
    server.join().unwrap();
    wait_for(&handle, |e| matches!(e, Event::Disconnected { .. }));
}

#[test]
fn tls_to_a_plaintext_server_fails_cleanly() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        // Not a TLS server: answer the ClientHello with an IRC line.
        let _ = stream.write_all(b":irc.test NOTICE * :plaintext only\r\n");
    });
    let handle = connect(options(port, true, config(false)));
    let gone = wait_for(&handle, |e| matches!(e, Event::Disconnected { .. }));
    assert!(matches!(gone, Event::Disconnected { reason } if !reason.is_empty()));
    assert!(
        !std::iter::from_fn(|| handle.try_recv()).any(|e| matches!(e, Event::Connected { .. }))
    );
    server.join().unwrap();
}

#[test]
fn refused_connection_reports_disconnect() {
    // Bind then drop to get a port with nothing listening.
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let handle = connect(options(port, false, config(false)));
    wait_for(&handle, |e| matches!(e, Event::Disconnected { .. }));
    // The thread ends right after its final event; closing must not hang.
    let _ = handle.close();
}
