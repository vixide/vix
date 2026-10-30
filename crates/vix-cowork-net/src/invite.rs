//! Invite tokens: what a host hands a guest out of band.
//!
//! An invite carries everything a guest needs and nothing the network can
//! learn from watching the handshake:
//!
//! - the host's **static public key** (the guest pins it, so a man in the
//!   middle cannot impersonate the host),
//! - a public **token id** (so the host can find the matching secret),
//! - a **secret** mixed into the Noise handshake as a pre-shared key (so only
//!   someone holding the invite can complete it),
//! - the **address** to connect to.
//!
//! Invites are single-use and short-lived. The host's table keeps the secrets;
//! it expires them, caps how many are outstanding, and revokes a token after
//! repeated failed handshakes against it.

use crate::error::NetError;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Length of the public token id.
pub const ID_LEN: usize = 8;
/// Length of the secret.
pub const SECRET_LEN: usize = 32;
/// Length of the host's X25519 public key.
pub const KEY_LEN: usize = 32;
/// Invites the host may have outstanding at once.
pub const MAX_OUTSTANDING: usize = 16;
/// Failed handshakes against one token before it is revoked.
const MAX_FAILURES: u8 = 3;
/// The prefix of the invite string.
const PREFIX: &str = "vix-cowork:1:";

/// A parsed invite.
#[derive(Clone, PartialEq, Eq)]
pub struct Invite {
    /// `host:port` to connect to.
    pub addr: String,
    /// The host's static public key to pin.
    pub host_key: [u8; KEY_LEN],
    /// Public token id.
    pub id: [u8; ID_LEN],
    /// The pre-shared secret.
    pub secret: [u8; SECRET_LEN],
}

impl fmt::Debug for Invite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never print the secret, even in a debug log.
        f.debug_struct("Invite")
            .field("addr", &self.addr)
            .finish_non_exhaustive()
    }
}

impl fmt::Display for Invite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut raw = Vec::with_capacity(KEY_LEN + ID_LEN + SECRET_LEN);
        raw.extend_from_slice(&self.host_key);
        raw.extend_from_slice(&self.id);
        raw.extend_from_slice(&self.secret);
        write!(f, "{PREFIX}{}@{}", URL_SAFE_NO_PAD.encode(raw), self.addr)
    }
}

impl FromStr for Invite {
    type Err = NetError;

    fn from_str(s: &str) -> Result<Self, NetError> {
        let rest = s.trim().strip_prefix(PREFIX).ok_or(NetError::BadInvite)?;
        let (token, addr) = rest.split_once('@').ok_or(NetError::BadInvite)?;
        if addr.is_empty() || addr.len() > 255 || addr.chars().any(char::is_control) {
            return Err(NetError::BadInvite);
        }
        let raw = URL_SAFE_NO_PAD
            .decode(token)
            .map_err(|_| NetError::BadInvite)?;
        if raw.len() != KEY_LEN + ID_LEN + SECRET_LEN {
            return Err(NetError::BadInvite);
        }
        let mut host_key = [0u8; KEY_LEN];
        let mut id = [0u8; ID_LEN];
        let mut secret = [0u8; SECRET_LEN];
        host_key.copy_from_slice(&raw[..KEY_LEN]);
        id.copy_from_slice(&raw[KEY_LEN..KEY_LEN + ID_LEN]);
        secret.copy_from_slice(&raw[KEY_LEN + ID_LEN..]);
        Ok(Self {
            addr: addr.to_string(),
            host_key,
            id,
            secret,
        })
    }
}

struct Entry {
    secret: [u8; SECRET_LEN],
    expires: Instant,
    failures: u8,
}

/// The host's outstanding invites.
#[derive(Default)]
pub struct InviteTable {
    entries: Mutex<HashMap<[u8; ID_LEN], Entry>>,
}

impl fmt::Debug for InviteTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InviteTable").finish_non_exhaustive()
    }
}

/// Fill `buf` with operating-system randomness.
pub(crate) fn random(buf: &mut [u8]) -> Result<(), NetError> {
    getrandom::fill(buf).map_err(|_| NetError::Crypto)
}

impl InviteTable {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<[u8; ID_LEN], Entry>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Mint a token valid for `ttl`. Returns its id and secret.
    ///
    /// # Errors
    ///
    /// [`NetError::TooLarge`] when [`MAX_OUTSTANDING`] invites are already out;
    /// [`NetError::Crypto`] if the system has no randomness.
    pub fn mint(&self, ttl: Duration) -> Result<([u8; ID_LEN], [u8; SECRET_LEN]), NetError> {
        let mut map = self.lock();
        let now = Instant::now();
        map.retain(|_, e| e.expires > now);
        if map.len() >= MAX_OUTSTANDING {
            return Err(NetError::TooLarge);
        }
        let mut id = [0u8; ID_LEN];
        let mut secret = [0u8; SECRET_LEN];
        random(&mut id)?;
        random(&mut secret)?;
        map.insert(
            id,
            Entry {
                secret,
                expires: now + ttl,
                failures: 0,
            },
        );
        Ok((id, secret))
    }

    /// The secret for `id`, if the token is live.
    #[must_use]
    pub fn lookup(&self, id: &[u8; ID_LEN]) -> Option<[u8; SECRET_LEN]> {
        let mut map = self.lock();
        let live = map.get(id).is_some_and(|e| e.expires > Instant::now());
        if !live {
            map.remove(id);
            return None;
        }
        map.get(id).map(|e| e.secret)
    }

    /// Record a failed handshake against `id`; the third revokes it.
    pub fn record_failure(&self, id: &[u8; ID_LEN]) {
        let mut map = self.lock();
        if let Some(e) = map.get_mut(id) {
            e.failures += 1;
            if e.failures >= MAX_FAILURES {
                map.remove(id);
            }
        }
    }

    /// Use the token up. `true` exactly once per token: a second, concurrent
    /// handshake with the same invite loses.
    pub fn consume(&self, id: &[u8; ID_LEN]) -> bool {
        self.lock().remove(id).is_some()
    }

    /// Revoke every outstanding invite.
    pub fn revoke_all(&self) {
        self.lock().clear();
    }

    /// How many invites are outstanding.
    #[must_use]
    pub fn outstanding(&self) -> usize {
        self.lock().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invite() -> Invite {
        Invite {
            addr: "127.0.0.1:7878".into(),
            host_key: [7; KEY_LEN],
            id: [1; ID_LEN],
            secret: [9; SECRET_LEN],
        }
    }

    #[test]
    fn an_invite_round_trips_through_its_string_form() {
        let i = invite();
        let s = i.to_string();
        assert!(s.starts_with("vix-cowork:1:") && s.ends_with("@127.0.0.1:7878"));
        assert_eq!(s.parse::<Invite>().unwrap(), i);
        assert_eq!(format!("  {s}\n").parse::<Invite>().unwrap(), i);
    }

    #[test]
    fn malformed_invites_are_refused() {
        for bad in [
            "",
            "hello",
            "vix-cowork:1:",
            "vix-cowork:1:AAAA@x",
            "vix-cowork:1:!!!!@x:1",
            "vix-cowork:2:AAAA@x:1",
        ] {
            assert_eq!(bad.parse::<Invite>(), Err(NetError::BadInvite), "{bad:?}");
        }
        let mut s = invite().to_string();
        s.truncate(s.find('@').unwrap());
        assert_eq!(s.parse::<Invite>(), Err(NetError::BadInvite));
    }

    #[test]
    fn the_debug_form_hides_the_secret() {
        let shown = format!("{:?}", invite());
        assert!(!shown.contains("secret"));
    }

    #[test]
    fn tokens_are_single_use() {
        let t = InviteTable::default();
        let (id, secret) = t.mint(Duration::from_secs(60)).unwrap();
        assert_eq!(t.lookup(&id), Some(secret));
        assert!(t.consume(&id));
        assert!(!t.consume(&id));
        assert_eq!(t.lookup(&id), None);
    }

    #[test]
    fn tokens_expire() {
        let t = InviteTable::default();
        let (id, _) = t.mint(Duration::from_millis(0)).unwrap();
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(t.lookup(&id), None);
        assert_eq!(t.outstanding(), 0);
    }

    #[test]
    fn repeated_failures_revoke_a_token() {
        let t = InviteTable::default();
        let (id, _) = t.mint(Duration::from_secs(60)).unwrap();
        t.record_failure(&id);
        t.record_failure(&id);
        assert!(t.lookup(&id).is_some());
        t.record_failure(&id);
        assert_eq!(t.lookup(&id), None);
    }

    #[test]
    fn outstanding_invites_are_capped() {
        let t = InviteTable::default();
        for _ in 0..MAX_OUTSTANDING {
            t.mint(Duration::from_secs(60)).unwrap();
        }
        assert_eq!(t.mint(Duration::from_secs(60)), Err(NetError::TooLarge));
        t.revoke_all();
        assert!(t.mint(Duration::from_secs(60)).is_ok());
    }

    #[test]
    fn minted_tokens_differ() {
        let t = InviteTable::default();
        let a = t.mint(Duration::from_secs(60)).unwrap();
        let b = t.mint(Duration::from_secs(60)).unwrap();
        assert_ne!(a.0, b.0);
        assert_ne!(a.1, b.1);
    }
}
