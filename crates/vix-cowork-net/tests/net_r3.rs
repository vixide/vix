//! Round-3 tests over 127.0.0.1 ephemeral ports only: invite parsing edges,
//! the table's bookkeeping, revoking and closing a listener, protocol-version
//! refusal, and oversized sends.

use std::error::Error;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use vix_cowork::wire::{ErrorCode, Message};
use vix_cowork_net::{
    Invite, InviteTable, Knock, Listener, MAX_OUTSTANDING, NetError, connect, join,
};

type Res = Result<(), Box<dyn Error>>;

const T: Duration = Duration::from_secs(5);

fn loopback() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 0))
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

fn sample_invite() -> Invite {
    Invite {
        addr: "127.0.0.1:9".into(),
        host_key: [1; 32],
        id: [2; 8],
        secret: [3; 32],
    }
}

#[test]
fn invite_strings_round_trip_and_hide_the_secret() -> Res {
    let invite = sample_invite();
    let text = invite.to_string();
    assert!(text.starts_with("vix-cowork:1:"));
    assert!(text.ends_with("@127.0.0.1:9"));
    assert_eq!(text.parse::<Invite>()?, invite);
    assert_eq!(
        format!("  {text}\n").parse::<Invite>()?,
        invite,
        "surrounding whitespace is trimmed"
    );
    let debug = format!("{invite:?}");
    assert!(debug.contains("127.0.0.1:9"));
    assert!(
        !debug.contains("secret"),
        "the secret is never printed: {debug}"
    );
    Ok(())
}

#[test]
fn malformed_invites_are_rejected() {
    let good = sample_invite().to_string();
    let (head, addr) = good.split_once('@').unwrap();
    let bad = [
        String::new(),
        "not an invite".to_string(),
        "vix-cowork:2:AAAA@host:1".to_string(),
        head.to_string(),
        format!("{head}@"),
        format!("{head}@host\u{7}:1"),
        format!("{head}@{}", "h".repeat(256)),
        format!("vix-cowork:1:!!!notbase64!!!@{addr}"),
        format!("vix-cowork:1:AAAA@{addr}"),
    ];
    for text in bad {
        assert_eq!(text.parse::<Invite>(), Err(NetError::BadInvite), "{text:?}");
    }
}

#[test]
fn the_invite_table_expires_counts_caps_and_revokes() {
    let table = InviteTable::default();
    assert!(format!("{table:?}").starts_with("InviteTable"));
    assert_eq!(table.outstanding(), 0);
    let (id, secret) = table.mint(Duration::from_secs(60)).unwrap();
    assert_eq!(table.lookup(&id), Some(secret));
    assert_eq!(table.outstanding(), 1);
    // Three failures revoke the token; before that it is still live.
    table.record_failure(&id);
    table.record_failure(&id);
    assert_eq!(table.lookup(&id), Some(secret));
    table.record_failure(&id);
    assert_eq!(table.lookup(&id), None);
    assert!(!table.consume(&id), "a revoked token cannot be consumed");
    table.record_failure(&id); // unknown id: no-op

    let (id2, _) = table.mint(Duration::from_secs(60)).unwrap();
    assert!(table.consume(&id2));
    assert!(!table.consume(&id2), "single use");

    // A zero lifetime is already expired; lookup drops it.
    let (gone, _) = table.mint(Duration::ZERO).unwrap();
    assert_eq!(table.lookup(&gone), None);
    assert_eq!(table.outstanding(), 0);

    // A lifetime too long to be a deadline is refused.
    assert_eq!(table.mint(Duration::MAX), Err(NetError::TooLarge));

    for _ in 0..MAX_OUTSTANDING {
        table.mint(Duration::from_secs(60)).unwrap();
    }
    assert_eq!(table.mint(Duration::from_secs(60)), Err(NetError::TooLarge));
    table.revoke_all();
    assert_eq!(table.outstanding(), 0);
    assert!(
        table.mint(Duration::from_secs(60)).is_ok(),
        "room again after revoking"
    );
}

#[test]
fn revoking_invites_makes_an_outstanding_invite_unusable() -> Res {
    let listener = Listener::bind(loopback())?;
    assert!(format!("{listener:?}").starts_with("Listener"));
    let invite =
        listener.new_invite(&listener.local_addr().to_string(), Duration::from_secs(60))?;
    assert_eq!(invite.addr, listener.local_addr().to_string());
    listener.revoke_invites();
    let err = join(&invite, "Late", T, Duration::from_millis(500)).unwrap_err();
    assert_ne!(err, NetError::Refused(ErrorCode::Declined));
    assert!(
        listener.try_knock().is_none(),
        "a revoked invite never reaches the human"
    );
    listener.close();
    Ok(())
}

#[test]
fn a_closed_listener_stops_accepting() -> Res {
    let listener = Listener::bind(loopback())?;
    let invite =
        listener.new_invite(&listener.local_addr().to_string(), Duration::from_secs(60))?;
    listener.close();
    assert!(connect(&invite, Duration::from_millis(500)).is_err());
    Ok(())
}

#[test]
fn a_guest_speaking_another_protocol_version_is_refused() -> Res {
    let listener = Listener::bind(loopback())?;
    let invite =
        listener.new_invite(&listener.local_addr().to_string(), Duration::from_secs(60))?;
    let (link, mut reader) = connect(&invite, T)?;
    link.send(&Message::Hello {
        version: 0xFE,
        name: "Old".into(),
    })?;
    reader.set_timeout(Some(T))?;
    assert_eq!(
        reader.recv()?,
        Message::Error {
            code: ErrorCode::Version
        }
    );
    assert!(listener.try_knock().is_none());
    Ok(())
}

#[test]
fn a_guest_that_does_not_say_hello_first_is_dropped() -> Res {
    let listener = Listener::bind(loopback())?;
    let invite =
        listener.new_invite(&listener.local_addr().to_string(), Duration::from_secs(60))?;
    let (link, mut reader) = connect(&invite, T)?;
    // Anything but Hello first violates the protocol: the host just hangs up.
    link.send(&Message::Error {
        code: ErrorCode::Removed,
    })?;
    reader.set_timeout(Some(T))?;
    assert!(reader.recv().is_err());
    Ok(())
}

#[test]
fn oversized_messages_are_refused_before_hitting_the_wire() -> Res {
    let listener = Listener::bind(loopback())?;
    let invite =
        listener.new_invite(&listener.local_addr().to_string(), Duration::from_secs(60))?;
    let guest = std::thread::spawn(move || join(&invite, "Ada", T, T));
    let knock = wait_knock(&listener)?;
    let (link, _reader) = knock.approve();
    let too_big = vec![0u8; 64 * 1024 * 1024 + 1];
    assert_eq!(link.send_encoded(&too_big), Err(NetError::TooLarge));
    assert!(format!("{link:?}").starts_with("Link"));
    link.close();
    // The guest sees the connection end without a Welcome.
    let outcome = guest.join().map_err(|_| "guest panicked")?;
    assert!(outcome.is_err());
    Ok(())
}

#[test]
fn declining_tells_the_guest_why() -> Res {
    let listener = Listener::bind(loopback())?;
    let invite =
        listener.new_invite(&listener.local_addr().to_string(), Duration::from_secs(60))?;
    let guest = std::thread::spawn(move || join(&invite, "Eve", T, T));
    let knock = wait_knock(&listener)?;
    knock.decline(ErrorCode::Declined);
    let err = guest.join().map_err(|_| "guest panicked")?.unwrap_err();
    assert_eq!(err, NetError::Refused(ErrorCode::Declined));
    Ok(())
}
