//! The request loop (`spec/index.md`, "Requests"): read request lines,
//! answer them, and end with [`Reply::Closed`].

use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;

use tracing::warn;
use vix_matrix_protocol::{
    DecodeError, ErrorKey, PROTOCOL_VERSION, Reply, Request, decode_request,
};

use crate::lines::{self, Input, Out};
use crate::secrets::SecretStore;
use crate::session::{self, LoginParams, Session};

/// What the loop runs against: the keyring and the data directory. `main`
/// passes the OS keyring and the platform directory; tests pass an
/// in-memory keyring and a scratch directory.
pub struct Env {
    /// Where saved sessions live.
    pub secrets: Arc<dyn SecretStore>,
    /// Vix's data directory (`None`: it could not be found, so no store
    /// can be opened).
    pub data_dir: Option<PathBuf>,
}

/// Whether the loop goes on after a request.
enum Flow {
    /// Read the next request.
    Continue,
    /// Close: send [`Reply::Closed`] and return.
    Stop,
}

/// Serve requests from `input`, writing replies to `output`, until
/// [`Request::Quit`], [`Request::Logout`] or end of input. The first line
/// written is always [`Reply::Ready`] and the last always
/// [`Reply::Closed`]; `output` is returned once that line is flushed.
pub async fn serve<R, W>(input: R, output: W, env: Env) -> W
where
    R: BufRead + Send + 'static,
    W: Write + Send + 'static,
{
    let (out, writer) = lines::spawn_writer(output);
    out.send(Reply::Ready {
        helper: env!("CARGO_PKG_VERSION").to_string(),
        protocol: PROTOCOL_VERSION,
    });
    let mut requests = lines::spawn_reader(input);
    let mut session: Option<Session> = None;
    while let Some(input) = requests.recv().await {
        let request = match input {
            Input::Line(line) if line.trim().is_empty() => continue,
            Input::Line(line) => match decode_request(&line) {
                Ok(request) => request,
                Err(e) => {
                    // Only the kind of failure: serde's message can quote the
                    // line, and the line can hold a password.
                    let why = decode_failure(&e);
                    warn!(why, "bad request line");
                    out.send(Reply::error_with(ErrorKey::BadRequest, why));
                    continue;
                }
            },
            Input::TooLong => {
                out.send(Reply::error_with(ErrorKey::BadRequest, "too_long"));
                continue;
            }
            Input::NotUtf8 => {
                out.send(Reply::error_with(ErrorKey::BadRequest, "not_utf8"));
                continue;
            }
        };
        if let Flow::Stop = handle(request, &mut session, &env, &out).await {
            break;
        }
    }
    if let Some(session) = session.take() {
        session.stop().await;
    }
    out.send(Reply::Closed);
    drop(out);
    match tokio::task::spawn_blocking(move || writer.join()).await {
        Ok(Ok(output)) => output,
        // The writer thread cannot panic short of an allocation failure;
        // there is no output left to hand back then.
        _ => std::process::exit(1),
    }
}

/// A short, secret-free name for why a line did not decode.
fn decode_failure(e: &DecodeError) -> &'static str {
    match e {
        DecodeError::TooLong => "too_long",
        DecodeError::NotAnObject => "not_an_object",
        DecodeError::Version(_) => "version",
        _ => "invalid",
    }
}

/// Answer one request.
async fn handle(request: Request, session: &mut Option<Session>, env: &Env, out: &Out) -> Flow {
    match request {
        Request::Login {
            account,
            homeserver,
            user,
            password,
        } => {
            if session.is_some() {
                out.send(Reply::error_with(ErrorKey::BadRequest, "already logged in"));
                return Flow::Continue;
            }
            if account.is_empty() {
                out.send(Reply::error_with(ErrorKey::BadRequest, "empty account"));
                return Flow::Continue;
            }
            let params = LoginParams {
                account: &account,
                homeserver: &homeserver,
                user: &user,
                password: password.as_ref().map(vix_matrix_protocol::Password::expose),
            };
            match session::login(params, &env.secrets, env.data_dir.as_deref(), out).await {
                Ok(started) => *session = Some(started),
                Err(reply) => out.send(*reply),
            }
            Flow::Continue
        }
        Request::Quit => Flow::Stop,
        Request::Logout => match session.take() {
            Some(active) => {
                active.logout(&env.secrets, out).await;
                Flow::Stop
            }
            None => not_logged_in(out),
        },
        request => {
            let Some(active) = session.as_ref() else {
                return not_logged_in(out);
            };
            match request {
                Request::Send { room, body, kind } => {
                    if let Some(error) = active.send(&room, &body, kind).await {
                        out.send(error);
                    }
                }
                Request::Join { room } => active.join(&room, out).await,
                Request::Leave { room } => active.leave(&room, out).await,
                Request::Rooms => active.rooms(out).await,
                _ => out.send(Reply::error_with(ErrorKey::BadRequest, "unsupported")),
            }
            Flow::Continue
        }
    }
}

/// Answer [`ErrorKey::NotLoggedIn`].
fn not_logged_in(out: &Out) -> Flow {
    out.send(Reply::error(ErrorKey::NotLoggedIn));
    Flow::Continue
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use vix_matrix_protocol::{MessageKind, Password, decode_reply, encode_request};

    use super::*;
    use crate::secrets::memory::{BrokenKeyring, MemoryKeyring};
    use crate::store::SavedSession;

    /// A scratch data directory, unique per test, removed by the caller.
    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("vix-matrix-test-{}-{name}", std::process::id()))
    }

    fn env(secrets: Arc<dyn SecretStore>, dir: &std::path::Path) -> Env {
        Env {
            secrets,
            data_dir: Some(dir.to_path_buf()),
        }
    }

    /// Run the loop over `lines` (each followed by a line break) and return
    /// every stdout line decoded, asserting that each one is a valid reply.
    async fn run(lines: &[String], env: Env) -> Vec<Reply> {
        let mut input = String::new();
        for line in lines {
            input.push_str(line);
            input.push('\n');
        }
        let output = serve(Cursor::new(input.into_bytes()), Vec::new(), env).await;
        let text = String::from_utf8(output).expect("stdout is UTF-8");
        assert!(
            text.ends_with('\n'),
            "every line ends with a line break: {text:?}"
        );
        text.lines()
            .map(|line| {
                decode_reply(line).unwrap_or_else(|e| panic!("not a reply line ({e}): {line:?}"))
            })
            .collect()
    }

    fn ready() -> Reply {
        Reply::Ready {
            helper: env!("CARGO_PKG_VERSION").to_string(),
            protocol: 1,
        }
    }

    fn login(password: Option<&str>) -> String {
        encode_request(&Request::Login {
            account: "home".into(),
            homeserver: "https://matrix.invalid".into(),
            user: "@me:matrix.invalid".into(),
            password: password.map(|p| Password::new(p.to_string())),
        })
    }

    #[tokio::test]
    async fn ready_is_first_and_closed_last_at_end_of_input() {
        let dir = scratch("eof");
        let replies = run(&[], env(Arc::new(MemoryKeyring::default()), &dir)).await;
        assert_eq!(replies, vec![ready(), Reply::Closed]);
    }

    #[tokio::test]
    async fn quit_closes_and_nothing_after_it_is_read() {
        let dir = scratch("quit");
        let replies = run(
            &[
                encode_request(&Request::Quit),
                encode_request(&Request::Rooms),
            ],
            env(Arc::new(MemoryKeyring::default()), &dir),
        )
        .await;
        assert_eq!(replies, vec![ready(), Reply::Closed]);
    }

    #[tokio::test]
    async fn bad_lines_are_answered_and_the_loop_goes_on() {
        let dir = scratch("bad");
        let replies = run(
            &[
                "hello".into(),
                r#"{"v":2,"type":"quit"}"#.into(),
                r#"{"v":1,"type":"explode","password":"hunter2"}"#.into(),
                String::new(),
                encode_request(&Request::Quit),
            ],
            env(Arc::new(MemoryKeyring::default()), &dir),
        )
        .await;
        assert_eq!(
            replies,
            vec![
                ready(),
                Reply::error_with(ErrorKey::BadRequest, "not_an_object"),
                Reply::error_with(ErrorKey::BadRequest, "version"),
                Reply::error_with(ErrorKey::BadRequest, "invalid"),
                Reply::Closed,
            ]
        );
    }

    #[tokio::test]
    async fn requests_before_login_are_not_logged_in() {
        let dir = scratch("nologin");
        let requests = [
            Request::Send {
                room: "!r:h".into(),
                body: "hi".into(),
                kind: MessageKind::Text,
            },
            Request::Join {
                room: "#vix:h".into(),
            },
            Request::Leave {
                room: "!r:h".into(),
            },
            Request::Rooms,
            Request::Logout,
        ];
        let lines: Vec<String> = requests.iter().map(encode_request).collect();
        let replies = run(&lines, env(Arc::new(MemoryKeyring::default()), &dir)).await;
        let mut expected = vec![ready()];
        expected.extend(std::iter::repeat_n(
            Reply::error(ErrorKey::NotLoggedIn),
            requests.len(),
        ));
        expected.push(Reply::Closed);
        assert_eq!(replies, expected);
    }

    #[tokio::test]
    async fn login_without_a_saved_session_or_password_is_no_session() {
        let dir = scratch("nosession");
        let replies = run(
            &[login(None)],
            env(Arc::new(MemoryKeyring::default()), &dir),
        )
        .await;
        assert_eq!(
            replies,
            vec![ready(), Reply::error(ErrorKey::NoSession), Reply::Closed]
        );
        assert!(!dir.exists(), "no store is created without a login");
    }

    #[tokio::test]
    async fn a_saved_session_for_another_user_is_not_resumed() {
        let dir = scratch("otheruser");
        let keyring = Arc::new(MemoryKeyring::default());
        let saved = SavedSession {
            configured: "https://matrix.invalid".into(),
            homeserver: "https://matrix.invalid/".into(),
            user_id: "@someone-else:matrix.invalid".into(),
            device_id: "DEV".into(),
            access_token: "t".into(),
            passphrase: "p".into(),
        };
        keyring.set("home", &saved.to_json()).unwrap();
        let replies = run(&[login(None)], env(keyring, &dir)).await;
        assert_eq!(
            replies,
            vec![ready(), Reply::error(ErrorKey::NoSession), Reply::Closed]
        );
    }

    #[tokio::test]
    async fn a_saved_session_without_its_store_is_a_store_error() {
        let dir = scratch("nostore");
        let keyring = Arc::new(MemoryKeyring::default());
        let saved = SavedSession {
            configured: "https://matrix.invalid".into(),
            homeserver: "https://matrix.invalid/".into(),
            user_id: "@me:matrix.invalid".into(),
            device_id: "DEV".into(),
            access_token: "t".into(),
            passphrase: "p".into(),
        };
        keyring.set("home", &saved.to_json()).unwrap();
        let replies = run(&[login(None)], env(keyring, &dir)).await;
        assert_eq!(
            replies,
            vec![
                ready(),
                Reply::error_with(ErrorKey::Store, "store missing"),
                Reply::Closed
            ]
        );
    }

    #[tokio::test]
    async fn a_broken_keyring_is_reported() {
        let dir = scratch("broken");
        let replies = run(&[login(Some("pw"))], env(Arc::new(BrokenKeyring), &dir)).await;
        assert_eq!(
            replies,
            vec![ready(), Reply::error(ErrorKey::Keyring), Reply::Closed]
        );
    }

    #[tokio::test]
    async fn no_data_directory_is_a_store_error() {
        let env = Env {
            secrets: Arc::new(MemoryKeyring::default()),
            data_dir: None,
        };
        let replies = run(&[login(Some("pw"))], env).await;
        assert_eq!(
            replies,
            vec![
                ready(),
                Reply::error_with(ErrorKey::Store, "no data directory"),
                Reply::Closed
            ]
        );
    }

    #[tokio::test]
    async fn an_empty_account_is_a_bad_request() {
        let dir = scratch("empty");
        let line = encode_request(&Request::Login {
            account: String::new(),
            homeserver: "https://matrix.invalid".into(),
            user: "me".into(),
            password: None,
        });
        let replies = run(&[line], env(Arc::new(MemoryKeyring::default()), &dir)).await;
        assert_eq!(
            replies,
            vec![
                ready(),
                Reply::error_with(ErrorKey::BadRequest, "empty account"),
                Reply::Closed
            ]
        );
    }

    #[tokio::test]
    async fn an_overlong_line_is_a_bad_request() {
        let dir = scratch("long");
        let replies = run(
            &["x".repeat(vix_matrix_protocol::MAX_LINE + 1)],
            env(Arc::new(MemoryKeyring::default()), &dir),
        )
        .await;
        assert_eq!(
            replies,
            vec![
                ready(),
                Reply::error_with(ErrorKey::BadRequest, "too_long"),
                Reply::Closed
            ]
        );
    }
}
