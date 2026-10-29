//! End-to-end tests over real loopback TCP sockets. Nothing leaves the machine.

use std::error::Error;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use vix_cowork::wire::{ErrorCode, Message};
use vix_cowork::{
    DEFAULT_CAP_BYTES, GuestEvent, GuestSession, HostSession, PeerId, Permission, SharedPath,
};
use vix_cowork_net::{Invite, Knock, LinkEvent, Listener, NetError, join};

type Res = Result<(), Box<dyn Error>>;

const T: Duration = Duration::from_secs(5);

fn loopback() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 0))
}

fn path(s: &str) -> Result<SharedPath, Box<dyn Error>> {
    Ok(SharedPath::parse(s)?)
}

fn wait_knock(l: &Listener) -> Result<Knock, Box<dyn Error>> {
    let end = Instant::now() + T;
    while Instant::now() < end {
        if let Some(k) = l.try_knock() {
            return Ok(k);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err("no knock arrived".into())
}

fn invite_for(l: &Listener) -> Result<Invite, Box<dyn Error>> {
    Ok(l.new_invite(&l.local_addr().to_string(), Duration::from_secs(60))?)
}

/// A TCP relay that records every byte crossing it in both directions.
type Capture = Arc<Mutex<Vec<u8>>>;

fn spy(target: SocketAddr) -> Result<(SocketAddr, Capture), Box<dyn Error>> {
    let listener = TcpListener::bind(loopback())?;
    let addr = listener.local_addr()?;
    let log = Arc::new(Mutex::new(Vec::new()));
    let log2 = Arc::clone(&log);
    std::thread::spawn(move || {
        let Ok((client, _)) = listener.accept() else {
            return;
        };
        let Ok(server) = TcpStream::connect(target) else {
            return;
        };
        let pairs = [
            (client.try_clone(), server.try_clone()),
            (server.try_clone(), client.try_clone()),
        ];
        for (from, to) in pairs {
            let (Ok(mut from), Ok(mut to)) = (from, to) else {
                return;
            };
            let log = Arc::clone(&log2);
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                while let Ok(n) = from.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    if let Ok(mut l) = log.lock() {
                        l.extend_from_slice(&buf[..n]);
                    }
                    if to.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
                let _ = to.shutdown(std::net::Shutdown::Both);
            });
        }
    });
    Ok((addr, log))
}

#[test]
fn a_full_session_runs_over_encrypted_loopback() -> Res {
    let listener = Listener::bind(loopback())?;
    let invite = invite_for(&listener)?;
    let guest_thread = std::thread::spawn(move || join(&invite, "Ada", T, Duration::from_secs(10)));

    let knock = wait_knock(&listener)?;
    assert_eq!(knock.name, "Ada");
    assert!(knock.peer_addr.is_some());

    let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
    let big: String = "line of text for chunking\n".repeat(20_000); // ~520 KB
    host.share(&path("src/a.txt")?, &big)?;
    let (peer, out) = host.admit(&knock.name, Permission::Edit)?;
    let (link, reader) = knock.approve();
    for o in out {
        assert_eq!(o.to, peer.id);
        link.send(&o.msg)?;
    }
    let (htx, hrx) = channel();
    let _h = reader.spawn(peer.id, htx);

    let joined = guest_thread.join().map_err(|_| "guest thread panicked")??;
    let mut guest = GuestSession::from_welcome(&joined.welcome, DEFAULT_CAP_BYTES)?;
    let (gtx, grx) = channel();
    let _g = joined.reader.spawn((), gtx);

    // Guest receives the roster and the (multi-frame) snapshot.
    let mut opened = None;
    while opened.is_none() {
        let ((), ev) = grx.recv_timeout(T)?;
        let LinkEvent::Message(m) = ev else {
            return Err("closed early".into());
        };
        for e in guest.handle(m)? {
            if let GuestEvent::Opened(p, text) = e {
                opened = Some((p, text));
            }
        }
    }
    let (p, text) = opened.ok_or("never opened")?;
    assert_eq!(p, path("src/a.txt")?);
    assert_eq!(text, big);

    // Guest edits; host applies and sees it.
    let msg = guest
        .edit(&p, 0, 0, "// edited by Ada\n")?
        .ok_or("no update")?;
    joined.link.send(&msg)?;
    let (who, ev) = hrx.recv_timeout(T)?;
    let LinkEvent::Message(m) = ev else {
        return Err("host link closed".into());
    };
    assert_eq!(who, peer.id);
    let handled = host.handle(who, m)?;
    assert_eq!(handled.edits.len(), 1);
    let host_doc = host.doc(&p).ok_or("no doc")?;
    assert!(host_doc.text().starts_with("// edited by Ada\n"));

    // Host edits; guest sees it.
    for o in host.local_edit(&p, 0, 0, "// host\n")? {
        link.send(&o.msg)?;
    }
    let mut edited = false;
    while !edited {
        let ((), ev) = grx.recv_timeout(T)?;
        let LinkEvent::Message(m) = ev else {
            return Err("closed".into());
        };
        edited = guest
            .handle(m)?
            .iter()
            .any(|e| matches!(e, GuestEvent::Edits(..)));
    }
    assert_eq!(
        guest.doc(&p).ok_or("no guest doc")?.text(),
        host.doc(&p).ok_or("no doc")?.text()
    );

    // Leaving ends the session cleanly.
    joined.link.send(&Message::Leave)?;
    let (_, ev) = hrx.recv_timeout(T)?;
    assert!(matches!(ev, LinkEvent::Message(Message::Leave)));
    Ok(())
}

#[test]
fn the_host_can_decline_a_guest() -> Res {
    let listener = Listener::bind(loopback())?;
    let invite = invite_for(&listener)?;
    let t = std::thread::spawn(move || join(&invite, "Mallory", T, T));
    wait_knock(&listener)?.decline(ErrorCode::Declined);
    let err = t.join().map_err(|_| "panic")?.unwrap_err();
    assert_eq!(err, NetError::Refused(ErrorCode::Declined));
    Ok(())
}

#[test]
fn an_unapproved_guest_receives_nothing() -> Res {
    let listener = Listener::bind(loopback())?;
    let invite = invite_for(&listener)?;
    let t = std::thread::spawn(move || join(&invite, "Eve", T, Duration::from_millis(300)));
    let knock = wait_knock(&listener)?;
    // The host sits on the decision; the guest times out having heard nothing.
    let err = t.join().map_err(|_| "panic")?.unwrap_err();
    assert_eq!(err, NetError::Timeout);
    drop(knock);
    Ok(())
}

#[test]
fn a_wrong_secret_is_rejected_and_the_invite_still_works() -> Res {
    let listener = Listener::bind(loopback())?;
    let good = invite_for(&listener)?;
    let mut bad = good.clone();
    bad.secret[0] ^= 0xff;
    let err = join(&bad, "Eve", T, T).unwrap_err();
    assert!(
        matches!(err, NetError::Closed | NetError::Crypto),
        "{err:?}"
    );
    assert!(listener.try_knock().is_none());
    // One failure does not burn the invite.
    let t = std::thread::spawn(move || join(&good, "Ada", T, T));
    let k = wait_knock(&listener)?;
    assert_eq!(k.name, "Ada");
    k.decline(ErrorCode::Declined);
    let _ = t.join();
    Ok(())
}

#[test]
fn repeated_wrong_guesses_revoke_the_invite() -> Res {
    let listener = Listener::bind(loopback())?;
    let good = invite_for(&listener)?;
    let mut bad = good.clone();
    bad.secret[0] ^= 0xff;
    for _ in 0..3 {
        assert!(join(&bad, "Eve", T, T).is_err());
    }
    let err = join(&good, "Ada", T, T).unwrap_err();
    assert!(
        matches!(err, NetError::Closed | NetError::Crypto),
        "{err:?}"
    );
    Ok(())
}

#[test]
fn an_invite_works_exactly_once() -> Res {
    let listener = Listener::bind(loopback())?;
    let invite = invite_for(&listener)?;
    let again = invite.clone();
    let t = std::thread::spawn(move || join(&invite, "Ada", T, T));
    let k = wait_knock(&listener)?;
    let (_link, _reader) = k.approve();
    // Second use of the same invite fails even though the first is still up.
    let err = join(&again, "Eve", T, T).unwrap_err();
    assert!(
        matches!(err, NetError::Closed | NetError::Crypto),
        "{err:?}"
    );
    drop(t);
    Ok(())
}

#[test]
fn an_expired_invite_is_rejected() -> Res {
    let listener = Listener::bind(loopback())?;
    let invite =
        listener.new_invite(&listener.local_addr().to_string(), Duration::from_millis(0))?;
    std::thread::sleep(Duration::from_millis(10));
    let err = join(&invite, "Ada", T, T).unwrap_err();
    assert!(
        matches!(err, NetError::Closed | NetError::Crypto),
        "{err:?}"
    );
    Ok(())
}

#[test]
fn a_guest_pins_the_hosts_key() -> Res {
    // An impostor listener that somehow learned the token id and secret still
    // cannot pass as the host: the guest's pinned key is different.
    let real = Listener::bind(loopback())?;
    let impostor = Listener::bind(loopback())?;
    let mut invite = invite_for(&real)?;
    // Same token on the impostor's side is impossible to forge without the
    // secret table, so point the invite at the impostor and swap in its key.
    let impostor_invite = invite_for(&impostor)?;
    invite.addr = impostor.local_addr().to_string();
    invite.id = impostor_invite.id;
    invite.secret = impostor_invite.secret;
    // host_key is still the REAL host's key: the impostor cannot answer for it.
    let err = join(&invite, "Ada", T, T).unwrap_err();
    assert!(
        matches!(err, NetError::Closed | NetError::Crypto),
        "{err:?}"
    );
    assert!(impostor.try_knock().is_none());
    Ok(())
}

#[test]
fn garbage_and_plaintext_connections_never_become_guests() -> Res {
    let listener = Listener::bind(loopback())?;
    for payload in [
        &b"GET / HTTP/1.1\r\n\r\n"[..],
        &[0u8; 13][..],
        b"VXCW\x01\x00\x00\x00\x00\x00\x00\x00\x00",
        b"VXCW\x01abcdefgh\x00\x30AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        &[0xff; 4096][..],
    ] {
        let mut s = TcpStream::connect(listener.local_addr())?;
        s.write_all(payload)?;
        let _ = s.shutdown(std::net::Shutdown::Write);
        let mut sink = Vec::new();
        let _ = s.read_to_end(&mut sink);
        assert!(sink.is_empty(), "the host answered a stranger: {sink:?}");
    }
    assert!(listener.try_knock().is_none());
    // And the listener is still healthy.
    let invite = invite_for(&listener)?;
    let t = std::thread::spawn(move || join(&invite, "Ada", T, T));
    wait_knock(&listener)?.decline(ErrorCode::Declined);
    let _ = t.join();
    Ok(())
}

#[test]
fn nothing_readable_crosses_the_wire() -> Res {
    let listener = Listener::bind(loopback())?;
    let mut invite = invite_for(&listener)?;
    let (proxy, log) = spy(listener.local_addr())?;
    invite.addr = proxy.to_string();
    let t = std::thread::spawn(move || join(&invite, "ZebraGuestName", T, T));

    let knock = wait_knock(&listener)?;
    let mut host = HostSession::new("HostNameQuokka", DEFAULT_CAP_BYTES);
    host.share(&path("secret.txt")?, "password = hunter2-pangolin")?;
    let (peer, out) = host.admit(&knock.name, Permission::View)?;
    let (link, _reader) = knock.approve();
    for o in out {
        assert_eq!(o.to, peer.id);
        link.send(&o.msg)?;
    }
    let joined = t.join().map_err(|_| "panic")??;
    let mut guest = GuestSession::from_welcome(&joined.welcome, DEFAULT_CAP_BYTES)?;
    let mut reader = joined.reader;
    reader.set_timeout(Some(T))?;
    let mut seen_text = false;
    for _ in 0..8 {
        let m = reader.recv()?;
        let _ = guest.handle(m.clone());
        if matches!(m, Message::Open { .. }) {
            seen_text = true;
            break;
        }
    }
    assert!(seen_text);
    std::thread::sleep(Duration::from_millis(50));
    let wire = log.lock().map_err(|_| "poisoned")?.clone();
    assert!(wire.len() > 100, "the spy saw the session");
    for needle in [
        &b"ZebraGuestName"[..],
        b"HostNameQuokka",
        b"hunter2-pangolin",
        b"secret.txt",
    ] {
        assert!(
            !wire.windows(needle.len()).any(|w| w == needle),
            "{} leaked onto the wire",
            String::from_utf8_lossy(needle)
        );
    }
    assert_eq!(&wire[..4], b"VXCW", "only the preamble is plaintext");
    Ok(())
}

#[test]
fn peer_ids_come_from_the_host_not_the_guest() -> Res {
    let listener = Listener::bind(loopback())?;
    let invite = invite_for(&listener)?;
    let t = std::thread::spawn(move || join(&invite, "\u{1b}[31mEve", T, T));
    let knock = wait_knock(&listener)?;
    assert_eq!(knock.name, "[31mEve", "escape sequences are stripped");
    let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
    let (peer, out) = host.admit(&knock.name, Permission::View)?;
    assert_eq!(peer.id, PeerId(2));
    let (link, _r) = knock.approve();
    for o in out {
        link.send(&o.msg)?;
    }
    let joined = t.join().map_err(|_| "panic")??;
    assert!(
        matches!(joined.welcome, Message::Welcome { peer, permission, .. }
        if peer == PeerId(2) && permission == Permission::View)
    );
    Ok(())
}

/// The chat seam (T756): a message in the reserved chat tag range is skipped
/// by a receiver that does not understand it, and the session carries on.
#[test]
fn a_reserved_chat_message_is_skipped_not_fatal() -> Res {
    let listener = Listener::bind(loopback())?;
    let invite = invite_for(&listener)?;
    let guest_thread = std::thread::spawn(move || join(&invite, "Ada", T, T));
    let knock = wait_knock(&listener)?;
    let mut host = HostSession::new("Host", DEFAULT_CAP_BYTES);
    let (_peer, out) = host.admit(&knock.name, Permission::View)?;
    let (link, _reader) = knock.approve();
    for o in out {
        link.send(&o.msg)?;
    }
    let joined = guest_thread.join().map_err(|_| "guest thread panicked")??;
    let first = *vix_cowork::wire::RESERVED_CHAT_TAGS.start();
    link.send_encoded(&[first, 0, 1, 2, 3])?;
    link.send(&Message::Leave)?;
    let (gtx, grx) = channel();
    let _g = joined.reader.spawn((), gtx);
    loop {
        let ((), ev) = grx.recv_timeout(T)?;
        match ev {
            LinkEvent::Message(Message::Leave) => return Ok(()),
            LinkEvent::Message(_) => {}
            LinkEvent::Closed(e) => {
                return Err(format!("closed by the reserved message: {e}").into());
            }
        }
    }
}
