//! The helper pump against a real child process: this test binary copies
//! itself to a file named `vix-matrix-fake` and starts that copy, which then
//! acts as a fake `vix-matrix` speaking the protocol. No Matrix server is
//! involved.
//!
//! `harness = false` (see `Cargo.toml`): libtest would print to stdout,
//! which in the child is the protocol stream.

use std::io::{BufRead as _, Write as _};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use vix_im_matrix::net::{Handle, SpawnOptions};
use vix_im_matrix::{
    ErrorKey, MessageKind, PROTOCOL_VERSION, Reply, Request, decode_request, encode_reply,
};

/// The file stem that turns this binary into the fake helper.
const FAKE: &str = "vix-matrix-fake";

/// One test: `Err` holds why it failed.
type Test = fn() -> Result<(), String>;

fn main() -> ExitCode {
    let me = std::env::current_exe().ok();
    if me.as_deref().and_then(std::path::Path::file_stem) == Some(FAKE.as_ref()) {
        return fake_helper();
    }
    let tests: [(&str, Test); 6] = [
        ("ready_login_send_quit", ready_login_send_quit),
        ("no_password_no_session", no_password_no_session),
        (
            "bad_output_is_a_protocol_error",
            bad_output_is_a_protocol_error,
        ),
        (
            "a_crash_ends_with_helper_exited",
            a_crash_ends_with_helper_exited,
        ),
        ("close_quits_the_helper", close_quits_the_helper),
        (
            "a_hung_helper_is_killed_on_close",
            a_hung_helper_is_killed_on_close,
        ),
    ];
    let mut failed = 0;
    for (name, test) in tests {
        match test() {
            Ok(()) => println!("test {name} ... ok"),
            Err(why) => {
                println!("test {name} ... FAILED: {why}");
                failed += 1;
            }
        }
    }
    println!("{} passed; {failed} failed", tests.len() - failed);
    if let Ok(path) = fake_path()
        && let Some(dir) = path.parent()
    {
        let _ = std::fs::remove_dir_all(dir);
    }
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Write one reply line.
fn say(reply: &Reply) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", encode_reply(reply));
    let _ = out.flush();
}

/// The fake: a room `!r:fake`; `send` echoes the body back from `@echo:fake`;
/// joining `#garbage` prints a non-protocol line, `#crash` exits without
/// `closed`, `#hang` stops reading.
fn fake_helper() -> ExitCode {
    // The password must never reach argv or the environment.
    if std::env::args().count() > 1 || std::env::vars().any(|(_, v)| v.contains("s3cret")) {
        return ExitCode::from(9);
    }
    say(&Reply::Ready {
        helper: "fake".into(),
        protocol: PROTOCOL_VERSION,
    });
    let mut logged_in = false;
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        match decode_request(&line) {
            Err(e) => say(&Reply::error_with(ErrorKey::BadRequest, e.to_string())),
            Ok(Request::Login { password, .. }) => {
                if password.as_ref().map(vix_im_matrix::Password::expose) == Some("s3cret") {
                    logged_in = true;
                    say(&Reply::LoggedIn {
                        user_id: "@me:fake".into(),
                        device_id: "DEV".into(),
                        restored: false,
                    });
                    say(&Reply::Room {
                        id: "!r:fake".into(),
                        name: "Fake Room".into(),
                        encrypted: true,
                        direct: false,
                    });
                } else if password.is_some() {
                    say(&Reply::error(ErrorKey::LoginFailed));
                } else {
                    say(&Reply::error(ErrorKey::NoSession));
                }
            }
            Ok(Request::Quit) => break,
            Ok(_) if !logged_in => say(&Reply::error(ErrorKey::NotLoggedIn)),
            Ok(Request::Send { room, body, kind }) => say(&Reply::Message {
                room,
                event_id: "$1".into(),
                sender: "@echo:fake".into(),
                sender_name: Some("Echo".into()),
                time: 1_000,
                body,
                kind,
                encrypted: true,
                verified: false,
            }),
            Ok(Request::Join { room }) if room == "#garbage" => {
                println!("this is not json");
            }
            Ok(Request::Join { room }) if room == "#crash" => return ExitCode::from(3),
            Ok(Request::Join { room }) if room == "#hang" => loop {
                std::thread::sleep(Duration::from_secs(60));
            },
            Ok(Request::Logout) => break,
            Ok(_) => {}
        }
    }
    say(&Reply::Closed);
    ExitCode::SUCCESS
}

/// Start the fake helper.
fn start() -> Result<Handle, String> {
    Ok(Handle::spawn(&SpawnOptions::new(Some(fake_path()?))))
}

/// The fake helper's program: a copy of this binary named [`FAKE`], made
/// once per run.
fn fake_path() -> Result<PathBuf, String> {
    static PATH: std::sync::OnceLock<Result<PathBuf, String>> = std::sync::OnceLock::new();
    PATH.get_or_init(|| {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let dir = std::env::temp_dir().join(format!("vix-im-matrix-fake-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let copy = dir.join(format!("{FAKE}{}", std::env::consts::EXE_SUFFIX));
        std::fs::copy(&exe, &copy).map_err(|e| e.to_string())?;
        Ok(copy)
    })
    .clone()
}

/// Wait up to two seconds for `n` replies.
fn take(handle: &Handle, n: usize) -> Vec<Reply> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut got = Vec::new();
    while got.len() < n && Instant::now() < deadline {
        match handle.try_recv() {
            Some(r) => got.push(r),
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    got
}

/// Fail with `what` unless `ok`.
fn check(ok: bool, what: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(what.to_string()) }
}

/// A login request for the fake.
fn login(password: Option<&str>) -> Request {
    Request::Login {
        account: "fake".into(),
        homeserver: "https://fake.invalid".into(),
        user: "@me:fake".into(),
        password: password.map(|p| vix_im_matrix::Password::new(p.to_string())),
    }
}

fn ready_login_send_quit() -> Result<(), String> {
    let h = start()?;
    check(
        take(&h, 1)
            == [Reply::Ready {
                helper: "fake".into(),
                protocol: 1,
            }],
        "ready first",
    )?;
    check(h.send(Request::Rooms), "send rooms")?;
    check(
        take(&h, 1) == [Reply::error(ErrorKey::NotLoggedIn)],
        "rooms before login",
    )?;
    check(h.send(login(Some("s3cret"))), "send login")?;
    let got = take(&h, 2);
    check(
        matches!(got.as_slice(), [Reply::LoggedIn { .. }, Reply::Room { .. }]),
        "logged in and a room",
    )?;
    let body = "one\ntwo {\"v\":1,\"type\":\"quit\"}";
    check(
        h.send(Request::Send {
            room: "!r:fake".into(),
            body: body.into(),
            kind: MessageKind::Emote,
        }),
        "send message",
    )?;
    let got = take(&h, 1);
    check(
        matches!(got.as_slice(), [Reply::Message { body: b, kind: MessageKind::Emote, .. }] if b == body),
        "the body comes back whole",
    )?;
    check(h.close() == [Reply::Closed], "closed last")
}

fn no_password_no_session() -> Result<(), String> {
    let h = start()?;
    let _ = take(&h, 1);
    check(h.send(login(None)), "send login")?;
    check(
        take(&h, 1) == [Reply::error(ErrorKey::NoSession)],
        "no session",
    )?;
    check(h.send(login(Some("wrong"))), "send login")?;
    check(
        take(&h, 1) == [Reply::error(ErrorKey::LoginFailed)],
        "login failed",
    )?;
    let _ = h.close();
    Ok(())
}

/// Log in and join `room`.
fn logged_in_join(room: &str) -> Result<Handle, String> {
    let h = start()?;
    let _ = take(&h, 1);
    check(h.send(login(Some("s3cret"))), "send login")?;
    let _ = take(&h, 2);
    check(h.send(Request::Join { room: room.into() }), "send join")?;
    Ok(h)
}

fn bad_output_is_a_protocol_error() -> Result<(), String> {
    let h = logged_in_join("#garbage")?;
    let got = take(&h, 1);
    check(
        matches!(
            got.as_slice(),
            [Reply::Error {
                key: ErrorKey::Protocol,
                ..
            }]
        ),
        "protocol error",
    )?;
    check(!h.is_finished(), "still running")?;
    check(h.close() == [Reply::Closed], "closed")
}

fn a_crash_ends_with_helper_exited() -> Result<(), String> {
    let h = logged_in_join("#crash")?;
    let got = take(&h, 2);
    check(
        got == [Reply::error(ErrorKey::HelperExited), Reply::Closed],
        "helper exited then closed",
    )?;
    check(h.is_finished(), "finished")?;
    check(!h.send(Request::Rooms), "nothing more to send")
}

fn close_quits_the_helper() -> Result<(), String> {
    let h = start()?;
    let _ = take(&h, 1);
    let started = Instant::now();
    let rest = h.close();
    check(rest == [Reply::Closed], "closed")?;
    check(started.elapsed() < Duration::from_secs(2), "quick")
}

fn a_hung_helper_is_killed_on_close() -> Result<(), String> {
    let h = logged_in_join("#hang")?;
    let started = Instant::now();
    let rest = h.close();
    check(
        rest == [Reply::error(ErrorKey::HelperExited), Reply::Closed],
        "killed: helper exited then closed",
    )?;
    check(
        started.elapsed() < Duration::from_secs(5),
        "within the grace",
    )
}
