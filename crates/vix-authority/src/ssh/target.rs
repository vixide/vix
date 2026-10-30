//! Remote path names (T712): `ssh://user@host:port/path` and the TRAMP-style
//! alias `/ssh:user@host#port:/path`, parsed into an [`SshTarget`].

/// Where a remote path lives: the SSH login and the path on that machine.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SshTarget {
    /// The login name, when the name gives one (otherwise `ssh_config` or the
    /// local user name decides).
    pub user: Option<String>,
    /// The host name, address or `ssh_config` alias. An IPv6 address is held
    /// without its brackets.
    pub host: String,
    /// The port, when the name gives one (otherwise `ssh_config` or 22).
    pub port: Option<u16>,
    /// The remote path: absolute (`/srv/app`), or `~` / `~/…` for the remote
    /// home directory (an empty path in the name means `~`).
    pub path: String,
}

/// Why a name is not a usable remote path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TargetError {
    /// The name starts with none of the remote forms (`ssh://`, `/ssh:`, and
    /// the container forms of [`crate::container::ContainerTarget`]).
    NotRemote,
    /// No host was given.
    MissingHost,
    /// The host is option-shaped (`-…`) or holds whitespace, a control
    /// character, `/` or `@`.
    BadHost,
    /// The user name is option-shaped or holds whitespace, a control
    /// character or `:` (an `@` is fine: it is passed with `-l`).
    BadUser,
    /// The port is not a number from 1 to 65535.
    BadPort,
    /// The path is relative (other than `~`), holds a NUL byte, or has a bad
    /// `%` escape.
    BadPath,
    /// A container name (the container, pod, namespace, Kubernetes container
    /// or WSL distribution, T718) is missing, option-shaped, or holds a
    /// character such names never have.
    BadName,
}

impl std::fmt::Display for TargetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotRemote => "not a remote name (ssh://, docker://, podman://, k8s://, wsl://)",
            Self::MissingHost => "no host name",
            Self::BadHost => "invalid host name",
            Self::BadUser => "invalid user name",
            Self::BadPort => "invalid port",
            Self::BadPath => "invalid remote path",
            Self::BadName => "invalid or missing container, pod or distribution name",
        })
    }
}

impl std::error::Error for TargetError {}

impl SshTarget {
    /// A target for `host`, with no user or port, at the remote home.
    ///
    /// # Errors
    /// [`TargetError::BadHost`] or [`TargetError::MissingHost`].
    pub fn new(host: &str) -> Result<Self, TargetError> {
        check_host(host)?;
        Ok(Self {
            user: None,
            host: host.to_string(),
            port: None,
            path: "~".into(),
        })
    }

    /// This target with `user` as its login name.
    ///
    /// # Errors
    /// [`TargetError::BadUser`].
    pub fn with_user(mut self, user: &str) -> Result<Self, TargetError> {
        check_user(user)?;
        self.user = Some(user.to_string());
        Ok(self)
    }

    /// This target on `port`.
    #[must_use]
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = (port != 0).then_some(port);
        self
    }

    /// This target at `path` (absolute, `~` or `~/…`).
    ///
    /// # Errors
    /// [`TargetError::BadPath`].
    pub fn with_path(mut self, path: &str) -> Result<Self, TargetError> {
        self.path = check_path(path)?;
        Ok(self)
    }

    /// Whether `name` uses one of the remote forms (it may still fail to
    /// parse).
    #[must_use]
    pub fn is_remote(name: &str) -> bool {
        name.starts_with("ssh://") || name.starts_with("/ssh:")
    }

    /// Parse `ssh://[user@]host[:port][/path]` (the path is `%`-decoded; an
    /// IPv6 host is written `[::1]`) or `/ssh:[user@]host[#port]:[path]`
    /// (TRAMP's form; the path is literal).
    ///
    /// # Errors
    /// A [`TargetError`] naming the bad part.
    pub fn parse(name: &str) -> Result<Self, TargetError> {
        if let Some(rest) = name.strip_prefix("ssh://") {
            let (authority, path) = match rest.find('/') {
                Some(i) => (&rest[..i], &rest[i..]),
                None => (rest, ""),
            };
            let (user, hostport) = split_user(authority);
            let user = user
                .map(percent_decode)
                .transpose()
                .map_err(|_| TargetError::BadUser)?;
            let (host, port) = split_host_port(hostport, ':')?;
            let decoded = percent_decode(path)?;
            // `ssh://host/~/x` names a home-relative path.
            let path = match decoded.strip_prefix('/') {
                Some(p) if p == "~" || p.starts_with("~/") => p.to_string(),
                _ => decoded,
            };
            return Self::build(user.as_deref(), host, port, &path);
        }
        if let Some(rest) = name.strip_prefix("/ssh:") {
            let (login, path) = if rest.starts_with('[') || rest.contains("@[") {
                let close = rest.find(']').ok_or(TargetError::BadHost)?;
                let colon = rest[close..]
                    .find(':')
                    .ok_or(TargetError::BadPath)?
                    .saturating_add(close);
                (&rest[..colon], &rest[colon + 1..])
            } else {
                let colon = rest.find(':').ok_or(TargetError::BadPath)?;
                (&rest[..colon], &rest[colon + 1..])
            };
            let (user, hostport) = split_user(login);
            let (host, port) = split_host_port(hostport, '#')?;
            return Self::build(user, host, port, path);
        }
        Err(TargetError::NotRemote)
    }

    fn build(
        user: Option<&str>,
        host: &str,
        port: Option<u16>,
        path: &str,
    ) -> Result<Self, TargetError> {
        let mut target = Self::new(host)?;
        if let Some(user) = user.filter(|u| !u.is_empty()) {
            target = target.with_user(user)?;
        }
        target.port = port;
        target.with_path(path)
    }

    /// The canonical `ssh://` name for this target (the path `%`-encoded
    /// where it must be).
    #[must_use]
    pub fn to_url(&self) -> String {
        let mut out = String::from("ssh://");
        if let Some(user) = &self.user {
            out.push_str(&percent_encode(user));
            out.push('@');
        }
        if self.host.contains(':') {
            out.push('[');
            out.push_str(&self.host);
            out.push(']');
        } else {
            out.push_str(&self.host);
        }
        if let Some(port) = self.port {
            out.push(':');
            out.push_str(&port.to_string());
        }
        if !self.path.starts_with('/') {
            out.push('/');
        }
        out.push_str(&percent_encode(&self.path));
        out
    }
}

impl std::fmt::Display for SshTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_url())
    }
}

/// Split `user@rest` at the last `@`.
fn split_user(text: &str) -> (Option<&str>, &str) {
    match text.rfind('@') {
        Some(i) => (Some(&text[..i]), &text[i + 1..]),
        None => (None, text),
    }
}

/// Split `host[<sep>port]`, with `[v6]` brackets.
fn split_host_port(text: &str, sep: char) -> Result<(&str, Option<u16>), TargetError> {
    let (host, port) = if let Some(inner) = text.strip_prefix('[') {
        let close = inner.find(']').ok_or(TargetError::BadHost)?;
        let after = &inner[close + 1..];
        let port = match after.strip_prefix(sep) {
            Some(p) => Some(p),
            None if after.is_empty() => None,
            None => return Err(TargetError::BadHost),
        };
        (&inner[..close], port)
    } else {
        match text.split_once(sep) {
            Some((h, p)) => (h, Some(p)),
            None => (text, None),
        }
    };
    let port = match port {
        Some(p) => Some(
            p.parse::<u16>()
                .ok()
                .filter(|&n| n != 0 && p.bytes().all(|b| b.is_ascii_digit()))
                .ok_or(TargetError::BadPort)?,
        ),
        None => None,
    };
    Ok((host, port))
}

fn bad_char(c: char) -> bool {
    c.is_whitespace() || c.is_control()
}

fn check_host(host: &str) -> Result<(), TargetError> {
    if host.is_empty() {
        return Err(TargetError::MissingHost);
    }
    if host.starts_with('-') || host.chars().any(|c| bad_char(c) || c == '/' || c == '@') {
        return Err(TargetError::BadHost);
    }
    Ok(())
}

fn check_user(user: &str) -> Result<(), TargetError> {
    if user.is_empty() || user.starts_with('-') || user.chars().any(|c| bad_char(c) || c == ':') {
        return Err(TargetError::BadUser);
    }
    Ok(())
}

pub(crate) fn check_path(path: &str) -> Result<String, TargetError> {
    if path.is_empty() {
        return Ok("~".into());
    }
    if path.contains('\0') {
        return Err(TargetError::BadPath);
    }
    if path.starts_with('/') || path == "~" || path.starts_with("~/") {
        Ok(path.to_string())
    } else {
        Err(TargetError::BadPath)
    }
}

/// Decode `%XX` escapes (the result must be UTF-8).
pub(crate) fn percent_decode(text: &str) -> Result<String, TargetError> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3).ok_or(TargetError::BadPath)?;
            let hex = std::str::from_utf8(hex).map_err(|_| TargetError::BadPath)?;
            out.push(u8::from_str_radix(hex, 16).map_err(|_| TargetError::BadPath)?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| TargetError::BadPath)
}

/// Encode `%`, whitespace, control characters, `?` and `#` as `%XX`.
pub(crate) fn percent_encode(text: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c == '%' || c == '?' || c == '#' || bad_char(c) {
            let mut buf = [0; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                let _ = write!(out, "%{b:02X}");
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(user: Option<&str>, host: &str, port: Option<u16>, path: &str) -> SshTarget {
        SshTarget {
            user: user.map(str::to_string),
            host: host.into(),
            port,
            path: path.into(),
        }
    }

    #[test]
    fn ssh_urls_parse() {
        let cases = [
            ("ssh://host/srv/app", t(None, "host", None, "/srv/app")),
            (
                "ssh://me@host:2222/srv/app",
                t(Some("me"), "host", Some(2222), "/srv/app"),
            ),
            ("ssh://host", t(None, "host", None, "~")),
            ("ssh://host/", t(None, "host", None, "/")),
            ("ssh://host/~/src", t(None, "host", None, "~/src")),
            ("ssh://host/~", t(None, "host", None, "~")),
            ("ssh://[::1]:22/x", t(None, "::1", Some(22), "/x")),
            ("ssh://me@[fe80::1]/x", t(Some("me"), "fe80::1", None, "/x")),
            ("ssh://h/a%20b/%25", t(None, "h", None, "/a b/%")),
            (
                "ssh://first.last%40corp@h/x",
                t(Some("first.last@corp"), "h", None, "/x"),
            ),
        ];
        for (name, want) in cases {
            assert_eq!(SshTarget::parse(name), Ok(want), "{name}");
        }
    }

    #[test]
    fn tramp_names_parse() {
        let cases = [
            ("/ssh:host:/srv/app", t(None, "host", None, "/srv/app")),
            (
                "/ssh:me@host#2222:/srv/a b",
                t(Some("me"), "host", Some(2222), "/srv/a b"),
            ),
            ("/ssh:host:", t(None, "host", None, "~")),
            ("/ssh:host:~/x", t(None, "host", None, "~/x")),
            ("/ssh:[::1]:/x", t(None, "::1", None, "/x")),
            (
                "/ssh:me@[::1]#22:/x%20y",
                t(Some("me"), "::1", Some(22), "/x%20y"),
            ),
        ];
        for (name, want) in cases {
            assert_eq!(SshTarget::parse(name), Ok(want), "{name}");
        }
    }

    #[test]
    fn bad_names_are_refused() {
        use TargetError as E;
        let cases = [
            ("/home/x", E::NotRemote),
            ("ssh:///x", E::MissingHost),
            ("ssh://-oProxyCommand=x/y", E::BadHost),
            ("ssh://-me@h/y", E::BadUser),
            ("ssh://h:0/x", E::BadPort),
            ("ssh://h:99999/x", E::BadPort),
            ("ssh://h:+22/x", E::BadPort),
            ("ssh://h/%zz", E::BadPath),
            ("ssh://h/%00", E::BadPath),
            ("/ssh:host", E::BadPath),
            ("/ssh:host:relative", E::BadPath),
            ("/ssh:a b:/x", E::BadHost),
            ("ssh://[::1/x", E::BadHost),
        ];
        for (name, want) in cases {
            assert_eq!(SshTarget::parse(name), Err(want), "{name}");
        }
        assert!(SshTarget::is_remote("ssh://h/x") && SshTarget::is_remote("/ssh:h:/x"));
        assert!(!SshTarget::is_remote("/home/ssh:x"));
    }

    #[test]
    fn urls_round_trip() {
        for name in [
            "ssh://me@host:2222/srv/app",
            "ssh://host/~/x",
            "ssh://[::1]:22/a%20b",
            "ssh://h/100%25",
        ] {
            let parsed = SshTarget::parse(name).unwrap();
            assert_eq!(parsed.to_url(), name);
            assert_eq!(SshTarget::parse(&parsed.to_string()), Ok(parsed));
        }
        let tramp = SshTarget::parse("/ssh:me@h#2:/a").unwrap();
        assert_eq!(tramp.to_url(), "ssh://me@h:2/a");
    }
}
