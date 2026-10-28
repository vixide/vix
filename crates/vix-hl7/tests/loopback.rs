//! Loopback MLLP test: an in-test server thread on 127.0.0.1 echoes an ACK.
#![cfg(feature = "net")]

use std::io::{self, Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use vix_hl7::net::{Endpoint, SendError, SendOptions, send};
use vix_hl7::{Ack, AckCode, Message, mllp};

const MESSAGE: &str = "MSH|^~\\&|SEND|FAC|RECV|FAC|202401011200||ADT^A01|MSG0001|P|2.5\nPID|1||12345^^^HOSP^MR||DOE^JANE";

fn fast() -> SendOptions {
    SendOptions {
        connect_timeout: Duration::from_secs(2),
        io_timeout: Duration::from_millis(500),
        reply_timeout: Duration::from_secs(2),
        max_reply: 4096,
    }
}

type Served = (Endpoint, thread::JoinHandle<io::Result<Vec<u8>>>);

/// Serve one connection: read one frame, hand its payload to `reply`, write back.
fn serve(reply: impl FnOnce(&[u8]) -> Vec<u8> + Send + 'static) -> io::Result<Served> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let handle = thread::spawn(move || {
        let (mut conn, _) = listener.accept()?;
        let mut deframer = mllp::Deframer::new(1 << 16);
        let mut buf = [0u8; 1024];
        let payload = loop {
            let n = conn.read(&mut buf)?;
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            let frames = deframer
                .push(&buf[..n])
                .map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
            if let Some(p) = frames.into_iter().next() {
                break p;
            }
        };
        conn.write_all(&reply(&payload))?;
        Ok(payload)
    });
    let ep = Endpoint {
        host: "127.0.0.1".into(),
        port,
    };
    Ok((ep, handle))
}

#[test]
fn sends_a_message_and_reads_the_ack() {
    let (ep, server) = serve(|payload| {
        let msg = Message::parse(&String::from_utf8_lossy(payload));
        let id = msg
            .get(&vix_hl7::Path::parse("MSH-10").unwrap())
            .unwrap_or_default();
        mllp::frame(
            format!("MSH|^~\\&|RECV|FAC|SEND|FAC|2024||ACK|9|P|2.5\rMSA|AA|{id}").as_bytes(),
        )
    })
    .unwrap();
    let reply = send(&ep, &mllp::to_wire(MESSAGE), &fast()).unwrap();
    let received = server.join().unwrap().unwrap();
    assert_eq!(
        received,
        mllp::to_wire(MESSAGE),
        "segments arrive CR-separated"
    );
    let ack = Ack::from_message(&Message::parse(&String::from_utf8(reply).unwrap())).unwrap();
    assert_eq!(ack.code, AckCode::ApplicationAccept);
    assert_eq!(ack.control_id, "MSG0001");
}

#[test]
fn reports_a_nak() {
    let (ep, server) = serve(|_| mllp::frame(b"MSH|^~\\&|R\rMSA|AR|MSG0001|nope")).unwrap();
    let reply = send(&ep, b"MSH|^~\\&|S", &fast()).unwrap();
    server.join().unwrap().unwrap();
    let ack = Ack::from_message(&Message::parse(&String::from_utf8(reply).unwrap())).unwrap();
    assert!(ack.code.is_nak());
}

#[test]
fn a_closed_peer_is_an_error() {
    let (ep, server) = serve(|_| Vec::new()).unwrap();
    let err = send(&ep, b"MSH|^~\\&|S", &fast()).unwrap_err();
    server.join().unwrap().unwrap();
    assert_eq!(err, SendError::Closed);
}

#[test]
fn a_silent_peer_times_out() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let ep = Endpoint {
        host: "127.0.0.1".into(),
        port: listener.local_addr().unwrap().port(),
    };
    let holder = thread::spawn(move || {
        let (conn, _) = listener.accept().unwrap();
        thread::sleep(Duration::from_millis(1500));
        drop(conn);
    });
    let err = send(&ep, b"MSH|^~\\&|S", &fast()).unwrap_err();
    assert_eq!(err, SendError::Timeout);
    holder.join().unwrap();
}

#[test]
fn a_refused_connection_is_an_error() {
    let port = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let ep = Endpoint {
        host: "127.0.0.1".into(),
        port,
    };
    assert!(matches!(
        send(&ep, b"x", &fast()),
        Err(SendError::Connect(_) | SendError::Timeout)
    ));
}
