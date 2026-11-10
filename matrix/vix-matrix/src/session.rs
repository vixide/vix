//! A signed-in `matrix-sdk` client (`spec/index.md`, "Login", "Sync",
//! "Requests"): logging in or resuming, the background sync, and the
//! requests that need a session.
//!
//! Nothing here logs a message body, a token, a key or a password.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use matrix_sdk::authentication::matrix::MatrixSession;
use matrix_sdk::config::SyncSettings;
use matrix_sdk::deserialized_responses::{TimelineEventKind, VerificationLevel, VerificationState};
use matrix_sdk::ruma::api::error::ErrorKind;
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use matrix_sdk::ruma::{OwnedDeviceId, RoomId, RoomOrAliasId, UserId};
use matrix_sdk::sync::SyncResponse;
use matrix_sdk::{Client, ClientBuildError, Room, RoomState, SessionMeta, SessionTokens};
use tokio::task::JoinHandle;
use tracing::warn;
use vix_matrix_protocol::{ErrorKey, MessageKind, Reply};
use zeroize::Zeroize as _;

use crate::events::{self, Encryption, Me};
use crate::lines::Out;
use crate::secrets::SecretStore;
use crate::store::{self, SavedSession};

/// The device display name a new login registers.
const DEVICE_NAME: &str = "Vix";

/// How long one sync long-poll may wait on the server.
const SYNC_TIMEOUT: Duration = Duration::from_secs(30);

/// The longest wait between sync retries while the server is unreachable.
const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// What was last announced for a room with [`Reply::Room`].
#[derive(Debug, Clone, PartialEq, Eq)]
struct Summary {
    /// The display name.
    name: String,
    /// Whether it is end-to-end encrypted.
    encrypted: bool,
    /// Whether it is a direct-message room.
    direct: bool,
}

/// The joined rooms announced so far, by room id; shared by the request
/// loop and the sync task so a room is announced and left exactly once.
type Rooms = Arc<Mutex<HashMap<String, Summary>>>;

/// What a Login request asks for.
#[derive(Debug, Clone, Copy)]
pub struct LoginParams<'a> {
    /// The `[[im.accounts]]` name.
    pub account: &'a str,
    /// The homeserver URL or server name.
    pub homeserver: &'a str,
    /// The user id or its local part.
    pub user: &'a str,
    /// The password, when Vix has one.
    pub password: Option<&'a str>,
}

/// A signed-in client with its sync running.
#[derive(Debug)]
pub struct Session {
    /// The SDK client.
    client: Client,
    /// The account name (the keyring entry's key).
    account: String,
    /// The account's store directory.
    store_dir: PathBuf,
    /// The rooms announced so far.
    rooms: Rooms,
    /// The background sync task.
    sync: JoinHandle<()>,
}

/// Sign in: resume `account`'s saved session, else log in with the
/// password (`Reply::Error { NoSession }` without one). On success
/// [`Reply::LoggedIn`] and a [`Reply::Room`] per joined room have been sent
/// and sync is running.
///
/// # Errors
///
/// The [`Reply::Error`] to send when no session could be started (boxed:
/// a [`Reply`] is large).
pub async fn login(
    params: LoginParams<'_>,
    secrets: &Arc<dyn SecretStore>,
    data_dir: Option<&Path>,
    out: &Out,
) -> Result<Session, Box<Reply>> {
    let LoginParams {
        account,
        homeserver,
        user,
        password,
    } = params;
    let Some(base) = data_dir else {
        return Err(Box::new(Reply::error_with(
            ErrorKey::Store,
            "no data directory",
        )));
    };
    let Some(dir) = store::store_dir(base, account) else {
        return Err(Box::new(Reply::error_with(
            ErrorKey::BadRequest,
            "empty account",
        )));
    };
    let saved = match secrets.get(account) {
        Ok(Some(mut json)) => {
            let saved = SavedSession::from_json(&json);
            json.zeroize();
            saved
        }
        Ok(None) => None,
        Err(_) => return Err(Box::new(Reply::error(ErrorKey::Keyring))),
    };
    let saved = saved.filter(|s| s.matches(homeserver, user));
    if let Some(saved) = saved {
        match resume(&saved, &dir).await {
            Ok(client) => return Ok(start(client, account, dir, true, secrets, out).await),
            // A store that no longer opens is useless without a new login.
            Err(e) if password.is_none() => return Err(e),
            Err(_) => warn!("the saved session did not resume; logging in again"),
        }
    }
    let Some(password) = password else {
        return Err(Box::new(Reply::error(ErrorKey::NoSession)));
    };
    let (client, passphrase) = fresh_login(homeserver, user, password, &dir).await?;
    let saved = saved_session(&client, homeserver, passphrase)?;
    let mut json = saved.to_json();
    if secrets.set(account, &json).is_err() {
        // Signed in, but the next start cannot resume: say so and go on.
        out.send(Reply::error(ErrorKey::Keyring));
    }
    json.zeroize();
    Ok(start(client, account, dir, false, secrets, out).await)
}

/// Build a client over the existing store in `dir` and restore `saved`.
async fn resume(saved: &SavedSession, dir: &Path) -> Result<Client, Box<Reply>> {
    // A missing store would be created empty, giving an existing device id
    // a new identity that nobody trusts and that decrypts nothing.
    if !dir.is_dir() {
        return Err(Box::new(Reply::error_with(
            ErrorKey::Store,
            "store missing",
        )));
    }
    let client = Client::builder()
        .homeserver_url(&saved.homeserver)
        .sqlite_store(dir, Some(&saved.passphrase))
        .build()
        .await
        .map_err(|e| build_error(&e))?;
    let user_id = UserId::parse(&saved.user_id)
        .map_err(|_| Box::new(Reply::error_with(ErrorKey::Store, "bad user id")))?;
    let session = MatrixSession {
        meta: SessionMeta {
            user_id,
            device_id: OwnedDeviceId::from(saved.device_id.as_str()),
        },
        tokens: SessionTokens {
            access_token: saved.access_token.clone(),
            refresh_token: None,
        },
    };
    client
        .restore_session(session)
        .await
        .map_err(|_| Box::new(Reply::error_with(ErrorKey::Store, "restore failed")))?;
    Ok(client)
}

/// Log in with a password into a new, empty store in `dir`; the client
/// and the passphrase its store was created with.
async fn fresh_login(
    homeserver: &str,
    user: &str,
    password: &str,
    dir: &Path,
) -> Result<(Client, String), Box<Reply>> {
    // Whatever is there belonged to an earlier device of this account.
    if dir.exists() {
        std::fs::remove_dir_all(dir)
            .map_err(|_| Box::new(Reply::error_with(ErrorKey::Store, "cannot clear store")))?;
    }
    create_private_dir(dir)
        .map_err(|_| Box::new(Reply::error_with(ErrorKey::Store, "cannot create store")))?;
    let Some(passphrase) = store::new_passphrase() else {
        return Err(Box::new(Reply::error_with(
            ErrorKey::Store,
            "no randomness",
        )));
    };
    let client = Client::builder()
        .server_name_or_homeserver_url(homeserver)
        .sqlite_store(dir, Some(&passphrase))
        .build()
        .await
        .map_err(|e| build_error(&e))?;
    client
        .matrix_auth()
        .login_username(user, password)
        .initial_device_display_name(DEVICE_NAME)
        .send()
        .await
        .map_err(|e| {
            Box::new(match errcode(&e) {
                Some(code) => Reply::error_with(ErrorKey::LoginFailed, code),
                None => Reply::error(ErrorKey::Network),
            })
        })?;
    Ok((client, passphrase))
}

/// The session to save for a client that just logged in.
fn saved_session(
    client: &Client,
    configured: &str,
    passphrase: String,
) -> Result<SavedSession, Box<Reply>> {
    let (Some(meta), Some(tokens)) = (client.session_meta(), client.session_tokens()) else {
        return Err(Box::new(Reply::error_with(
            ErrorKey::LoginFailed,
            "no session",
        )));
    };
    Ok(SavedSession {
        configured: configured.to_string(),
        homeserver: client.homeserver().to_string(),
        user_id: meta.user_id.to_string(),
        device_id: meta.device_id.to_string(),
        access_token: tokens.access_token,
        passphrase,
    })
}

/// Create `dir` (and its parents), readable by this user only on Unix.
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// The reply for a client that could not be built.
fn build_error(e: &ClientBuildError) -> Box<Reply> {
    Box::new(match e {
        ClientBuildError::SqliteStore(_) => Reply::error_with(ErrorKey::Store, "cannot open store"),
        ClientBuildError::Url(_)
        | ClientBuildError::InvalidServerName
        | ClientBuildError::MissingHomeserver => {
            Reply::error_with(ErrorKey::Network, "invalid homeserver")
        }
        _ => Reply::error(ErrorKey::Network),
    })
}

/// The homeserver's error code (`M_FORBIDDEN`, …) for an SDK error, if
/// the server sent one.
fn errcode(e: &matrix_sdk::Error) -> Option<String> {
    e.client_api_error_kind()
        .map(|kind| kind.errcode().to_string())
}

/// Whether a decrypted event's sending device is cross-signed by its
/// owner: [`VerificationState::Verified`], or a device signed by an
/// identity we have not verified ourselves. A device whose owner's identity
/// changed since we verified it (a verification violation) counts as not
/// cross-signed, so Vix marks it `(unverified)`.
fn cross_signed(state: &VerificationState) -> bool {
    matches!(
        state,
        VerificationState::Verified
            | VerificationState::Unverified(VerificationLevel::UnverifiedIdentity)
    )
}

/// Announce `LoggedIn` and the joined rooms, and start the sync task.
async fn start(
    client: Client,
    account: &str,
    store_dir: PathBuf,
    restored: bool,
    secrets: &Arc<dyn SecretStore>,
    out: &Out,
) -> Session {
    let (user_id, device_id) = client
        .session_meta()
        .map(|m| (m.user_id.to_string(), m.device_id.to_string()))
        .unwrap_or_default();
    out.send(Reply::LoggedIn {
        user_id: user_id.clone(),
        device_id: device_id.clone(),
        restored,
    });
    let rooms: Rooms = Arc::default();
    for room in client.joined_rooms() {
        announce(&room, &rooms, out, true).await;
    }
    let sync = tokio::spawn(sync_loop(SyncTask {
        client: client.clone(),
        out: out.clone(),
        rooms: Arc::clone(&rooms),
        secrets: Arc::clone(secrets),
        account: account.to_string(),
        user_id,
        device_id,
        // A new login's first sync carries room history from before this
        // device existed; a resumed session's first sync carries what
        // arrived while the helper was not running.
        deliver_first: restored,
    }));
    Session {
        client,
        account: account.to_string(),
        store_dir,
        rooms,
        sync,
    }
}

/// Send [`Reply::Room`] for `room` when it is new or changed since the last
/// announcement, or always when `force` is set.
async fn announce(room: &Room, rooms: &Rooms, out: &Out, force: bool) {
    let name = match room.display_name().await {
        Ok(name) => name.to_string(),
        Err(_) => room.room_id().to_string(),
    };
    let summary = Summary {
        name,
        encrypted: room.encryption_state().is_encrypted(),
        direct: room.is_direct().await.unwrap_or(false),
    };
    let id = room.room_id().to_string();
    let changed = {
        let mut map = rooms.lock().unwrap_or_else(PoisonError::into_inner);
        let changed = map.get(&id) != Some(&summary);
        map.insert(id.clone(), summary.clone());
        changed
    };
    if force || changed {
        out.send(Reply::Room {
            id,
            name: summary.name,
            encrypted: summary.encrypted,
            direct: summary.direct,
        });
    }
}

/// Everything the background sync needs.
struct SyncTask {
    /// The SDK client.
    client: Client,
    /// Where replies go.
    out: Out,
    /// The rooms announced so far.
    rooms: Rooms,
    /// The keyring, to forget a session the server revoked.
    secrets: Arc<dyn SecretStore>,
    /// The account name.
    account: String,
    /// Our user id.
    user_id: String,
    /// This device's id.
    device_id: String,
    /// Whether the first sync's messages are sent to Vix.
    deliver_first: bool,
}

/// Sync until cancelled. A failure is reported once per outage
/// ([`ErrorKey::Network`]) and retried with backoff; a revoked access token
/// ends the sync with [`ErrorKey::LoginFailed`] and forgets the session, so
/// the next Login uses the password.
async fn sync_loop(task: SyncTask) {
    let mut deliver = task.deliver_first;
    let mut failing = false;
    let mut backoff = Duration::from_secs(1);
    loop {
        match task
            .client
            .sync_once(SyncSettings::default().timeout(SYNC_TIMEOUT))
            .await
        {
            Ok(response) => {
                failing = false;
                backoff = Duration::from_secs(1);
                handle_sync(&task, &response, deliver).await;
                deliver = true;
            }
            Err(e) => {
                if matches!(e.client_api_error_kind(), Some(ErrorKind::UnknownToken(_))) {
                    let _ = task.secrets.delete(&task.account);
                    task.out
                        .send(Reply::error_with(ErrorKey::LoginFailed, "M_UNKNOWN_TOKEN"));
                    return;
                }
                if !failing {
                    warn!("sync failed; retrying");
                    task.out.send(match errcode(&e) {
                        Some(code) => Reply::error_with(ErrorKey::Network, code),
                        None => Reply::error(ErrorKey::Network),
                    });
                    failing = true;
                }
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
        }
    }
}

/// Turn one sync response into replies: new or renamed rooms, messages and
/// undecryptable events (when `deliver`), and rooms we left.
async fn handle_sync(task: &SyncTask, response: &SyncResponse, deliver: bool) {
    let me = Me {
        user_id: &task.user_id,
        device_id: &task.device_id,
    };
    for (room_id, update) in &response.rooms.joined {
        let Some(room) = task.client.get_room(room_id) else {
            continue;
        };
        announce(&room, &task.rooms, &task.out, false).await;
        if !deliver {
            continue;
        }
        for event in &update.timeline.events {
            let encryption = match (&event.kind, event.encryption_info()) {
                (TimelineEventKind::UnableToDecrypt { .. }, _) => Encryption::Undecryptable,
                (_, Some(info)) => Encryption::Decrypted {
                    sender_device: info
                        .sender_device
                        .as_deref()
                        .map(matrix_sdk::ruma::DeviceId::as_str),
                    cross_signed: cross_signed(&info.verification_state),
                },
                (_, None) => Encryption::Plain,
            };
            let Some(mut reply) =
                events::convert(room_id.as_str(), event.raw().json().get(), encryption, me)
            else {
                continue;
            };
            if let Reply::Message {
                sender,
                sender_name,
                ..
            } = &mut reply
            {
                *sender_name = member_name(&room, sender).await;
            }
            task.out.send(reply);
        }
    }
    for room_id in response.rooms.left.keys() {
        let known = task
            .rooms
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(room_id.as_str())
            .is_some();
        if known {
            task.out.send(Reply::Left {
                id: room_id.to_string(),
            });
        }
    }
    // `m.direct` is global account data: a room can become direct without
    // an update of its own.
    if !response.account_data.is_empty() {
        for room in task.client.joined_rooms() {
            announce(&room, &task.rooms, &task.out, false).await;
        }
    }
}

/// `sender`'s display name in `room`, from the store.
async fn member_name(room: &Room, sender: &str) -> Option<String> {
    let user_id = UserId::parse(sender).ok()?;
    let member = room.get_member_no_sync(&user_id).await.ok()??;
    member.display_name().map(str::to_string)
}

impl Session {
    /// The joined room with id `room`, else [`ErrorKey::UnknownRoom`].
    fn joined(&self, room: &str) -> Result<Room, Box<Reply>> {
        let unknown = || Box::new(Reply::error(ErrorKey::UnknownRoom));
        let id = RoomId::parse(room).map_err(|_| unknown())?;
        let room = self.client.get_room(&id).ok_or_else(unknown)?;
        if room.state() == RoomState::Joined {
            Ok(room)
        } else {
            Err(unknown())
        }
    }

    /// Send a message; `Some` error reply when it failed.
    pub async fn send(&self, room: &str, body: &str, kind: MessageKind) -> Option<Reply> {
        let room = match self.joined(room) {
            Ok(room) => room,
            Err(e) => return Some(*e),
        };
        let content = match kind {
            MessageKind::Emote => RoomMessageEventContent::emote_plain(body),
            MessageKind::Notice => RoomMessageEventContent::notice_plain(body),
            _ => RoomMessageEventContent::text_plain(body),
        };
        match room.send(content).await {
            Ok(_) => None,
            Err(e) => Some(match errcode(&e) {
                Some(code) => Reply::error_with(ErrorKey::SendFailed, code),
                None => Reply::error(ErrorKey::SendFailed),
            }),
        }
    }

    /// Join a room by id or alias, then announce it.
    pub async fn join(&self, room: &str, out: &Out) {
        let Ok(target) = RoomOrAliasId::parse(room) else {
            out.send(Reply::error_with(
                ErrorKey::JoinFailed,
                "invalid room id or alias",
            ));
            return;
        };
        match self.client.join_room_by_id_or_alias(&target, &[]).await {
            Ok(room) => announce(&room, &self.rooms, out, true).await,
            Err(e) => out.send(match errcode(&e) {
                Some(code) => Reply::error_with(ErrorKey::JoinFailed, code),
                None => Reply::error(ErrorKey::JoinFailed),
            }),
        }
    }

    /// Leave a joined room, then say [`Reply::Left`].
    pub async fn leave(&self, room: &str, out: &Out) {
        let joined = match self.joined(room) {
            Ok(joined) => joined,
            Err(e) => {
                out.send(*e);
                return;
            }
        };
        match joined.leave().await {
            Ok(()) => {
                let id = joined.room_id().to_string();
                self.rooms
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(&id);
                out.send(Reply::Left { id });
            }
            Err(e) => out.send(match errcode(&e) {
                Some(code) => Reply::error_with(ErrorKey::LeaveFailed, code),
                None => Reply::error(ErrorKey::LeaveFailed),
            }),
        }
    }

    /// Announce every joined room again.
    pub async fn rooms(&self, out: &Out) {
        for room in self.client.joined_rooms() {
            announce(&room, &self.rooms, out, true).await;
        }
    }

    /// Stop the sync task and wait until it is gone. The session stays saved.
    pub async fn stop(self) {
        self.sync.abort();
        let _ = self.sync.await;
    }

    /// Sign this device out on the server, then delete the store and the
    /// keyring entry. Each failure is sent as an error, and the local
    /// clean-up runs even when the server could not be reached.
    pub async fn logout(self, secrets: &Arc<dyn SecretStore>, out: &Out) {
        self.sync.abort();
        let _ = self.sync.await;
        if let Err(e) = self.client.logout().await {
            out.send(match errcode(&e) {
                Some(code) => Reply::error_with(ErrorKey::Network, code),
                None => Reply::error(ErrorKey::Network),
            });
        }
        // Close the store before deleting its files (Windows refuses to
        // delete open files).
        drop(self.client);
        if self.store_dir.exists() && std::fs::remove_dir_all(&self.store_dir).is_err() {
            out.send(Reply::error_with(ErrorKey::Store, "cannot delete store"));
        }
        if secrets.delete(&self.account).is_err() {
            out.send(Reply::error(ErrorKey::Keyring));
        }
    }
}

#[cfg(test)]
mod tests {
    use matrix_sdk::deserialized_responses::DeviceLinkProblem;

    use super::*;

    #[test]
    fn only_cross_signed_devices_count_as_verified() {
        assert!(cross_signed(&VerificationState::Verified));
        assert!(cross_signed(&VerificationState::Unverified(
            VerificationLevel::UnverifiedIdentity
        )));
        for level in [
            VerificationLevel::VerificationViolation,
            VerificationLevel::UnsignedDevice,
            VerificationLevel::None(DeviceLinkProblem::MissingDevice),
            VerificationLevel::MismatchedSender,
        ] {
            assert!(!cross_signed(&VerificationState::Unverified(level)));
        }
    }
}
