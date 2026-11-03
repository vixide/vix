//! Container names (T718): `docker://`, `podman://`, `k8s://` and `wsl://`,
//! parsed into a [`ContainerTarget`].
//!
//! Pure string handling, always compiled (no `ssh` feature needed). The
//! parser is deliberately strict: every name, user and path is validated so
//! that nothing option-shaped or shell-significant can reach an engine
//! command line later. The SSH counterpart is [`crate::ssh::SshTarget`];
//! [`crate::remote::RemoteTarget`] unifies the two. Owning spec:
//! `crates/vix-authority/spec/container/index.md`.

use crate::ssh::TargetError;
use crate::ssh::target::{check_path, percent_decode, percent_encode};

/// How a [`ContainerTarget`] is entered: the program whose `exec` runs the
/// scripts there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ExecKind {
    /// A running Docker container: `docker exec`.
    Docker,
    /// A running Podman container: `podman exec`.
    Podman,
    /// A container in a Kubernetes pod: `kubectl exec`.
    Kubernetes,
    /// A Windows Subsystem for Linux distribution: `wsl.exe --exec`.
    Wsl,
}

impl ExecKind {
    /// Every kind, in the order the names are listed.
    pub const ALL: [ExecKind; 4] = [
        ExecKind::Docker,
        ExecKind::Podman,
        ExecKind::Kubernetes,
        ExecKind::Wsl,
    ];

    /// The URL scheme, without `://`.
    #[must_use]
    pub fn scheme(self) -> &'static str {
        match self {
            ExecKind::Docker => "docker",
            ExecKind::Podman => "podman",
            ExecKind::Kubernetes => "k8s",
            ExecKind::Wsl => "wsl",
        }
    }

    /// The program run when no other is configured (looked up on `PATH`).
    #[must_use]
    pub fn program(self) -> &'static str {
        match self {
            ExecKind::Docker => "docker",
            ExecKind::Podman => "podman",
            ExecKind::Kubernetes => "kubectl",
            ExecKind::Wsl => "wsl.exe",
        }
    }

    /// A short, stable, untranslated name (the authority's `name()`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            ExecKind::Docker => "docker",
            ExecKind::Podman => "podman",
            ExecKind::Kubernetes => "kubernetes",
            ExecKind::Wsl => "wsl",
        }
    }
}

/// Where a container path lives: which container (pod, distribution), as
/// which user, and the path inside it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ContainerTarget {
    /// How it is entered.
    pub kind: ExecKind,
    /// The container's name or id, the pod's name (Kubernetes), or the
    /// distribution's name (WSL).
    pub name: String,
    /// The namespace (Kubernetes only).
    pub namespace: Option<String>,
    /// The container in the pod (Kubernetes only).
    pub container: Option<String>,
    /// The user to run as (`exec -u`; `wsl.exe -u`), when the name gives
    /// one. Not for Kubernetes, whose `exec` has no user.
    pub user: Option<String>,
    /// The path inside: absolute, or `~` / `~/…` for the home directory of
    /// the user the commands run as.
    pub path: String,
}

impl ContainerTarget {
    /// A Docker (`kind`) or Podman container, or a WSL distribution, by
    /// `name`, at its home directory.
    ///
    /// # Errors
    /// [`TargetError::BadName`] for a bad name, or a Kubernetes `kind` (use
    /// [`ContainerTarget::kubernetes`]).
    pub fn new(kind: ExecKind, name: &str) -> Result<Self, TargetError> {
        if kind == ExecKind::Kubernetes {
            return Err(TargetError::BadName);
        }
        check_name(name, kind)?;
        Ok(Self {
            kind,
            name: name.to_string(),
            namespace: None,
            container: None,
            user: None,
            path: "~".into(),
        })
    }

    /// Container `container` of pod `pod` in `namespace`.
    ///
    /// # Errors
    /// [`TargetError::BadName`] when one of them is not a Kubernetes name.
    pub fn kubernetes(namespace: &str, pod: &str, container: &str) -> Result<Self, TargetError> {
        for part in [namespace, pod, container] {
            check_name(part, ExecKind::Kubernetes)?;
        }
        Ok(Self {
            kind: ExecKind::Kubernetes,
            name: pod.to_string(),
            namespace: Some(namespace.to_string()),
            container: Some(container.to_string()),
            user: None,
            path: "~".into(),
        })
    }

    /// This target running as `user` (a name, a uid, or `uid:gid`).
    ///
    /// # Errors
    /// [`TargetError::BadUser`], also for Kubernetes.
    pub fn with_user(mut self, user: &str) -> Result<Self, TargetError> {
        if self.kind == ExecKind::Kubernetes || !is_user(user) {
            return Err(TargetError::BadUser);
        }
        self.user = Some(user.to_string());
        Ok(self)
    }

    /// This target at `path` (absolute, `~` or `~/…`).
    ///
    /// # Errors
    /// [`TargetError::BadPath`].
    pub fn with_path(mut self, path: &str) -> Result<Self, TargetError> {
        self.path = check_path(path)?;
        Ok(self)
    }

    /// Whether `name` uses one of the container forms (it may still fail to
    /// parse).
    #[must_use]
    pub fn is_container(name: &str) -> bool {
        ExecKind::ALL
            .iter()
            .any(|k| name.starts_with(&format!("{}://", k.scheme())))
    }

    /// Parse `docker://[user@]container[/path]`,
    /// `podman://[user@]container[/path]`, `k8s://namespace/pod/container[/path]`
    /// or `wsl://[user@]distribution[/path]`. The path is `%`-decoded; no path
    /// (or `/~`) is the home directory.
    ///
    /// # Errors
    /// A [`TargetError`] naming the bad part.
    pub fn parse(text: &str) -> Result<Self, TargetError> {
        let (kind, rest) = ExecKind::ALL
            .iter()
            .find_map(|k| {
                text.strip_prefix(k.scheme())
                    .and_then(|r| r.strip_prefix("://"))
                    .map(|r| (*k, r))
            })
            .ok_or(TargetError::NotRemote)?;
        let (target, path) = if kind == ExecKind::Kubernetes {
            let mut parts = rest.splitn(4, '/');
            let ns = parts.next().unwrap_or_default();
            let pod = parts.next().unwrap_or_default();
            let ctr = parts.next().unwrap_or_default();
            let path = parts.next().map(|p| format!("/{p}")).unwrap_or_default();
            (Self::kubernetes(ns, pod, ctr)?, path)
        } else {
            let (login, path) = match rest.find('/') {
                Some(i) => (&rest[..i], &rest[i..]),
                None => (rest, ""),
            };
            let (user, name) = match login.rfind('@') {
                Some(i) => (Some(&login[..i]), &login[i + 1..]),
                None => (None, login),
            };
            let mut target = Self::new(kind, name)?;
            if let Some(user) = user {
                let user = percent_decode(user).map_err(|_| TargetError::BadUser)?;
                target = target.with_user(&user)?;
            }
            (target, path.to_string())
        };
        let decoded = percent_decode(&path)?;
        let path = match decoded.strip_prefix('/') {
            Some(p) if p == "~" || p.starts_with("~/") => p.to_string(),
            _ => decoded,
        };
        target.with_path(&path)
    }

    /// The canonical name: `docker://user@name/path`, `k8s://ns/pod/ctr/path`,
    /// … (the path `%`-encoded where it must be). Round-trips through
    /// [`ContainerTarget::parse`].
    #[must_use]
    pub fn to_url(&self) -> String {
        let mut out = format!("{}://", self.kind.scheme());
        if let Some(user) = &self.user {
            out.push_str(&percent_encode(user));
            out.push('@');
        }
        out.push_str(&self.place());
        if !self.path.starts_with('/') {
            out.push('/');
        }
        out.push_str(&percent_encode(&self.path));
        out
    }

    /// A short label for the status bar and error messages:
    /// `docker:user@name`, `k8s:ns/pod/ctr`, `wsl:distro`.
    #[must_use]
    pub fn label(&self) -> String {
        let mut out = format!("{}:", self.kind.scheme());
        if let Some(user) = &self.user {
            out.push_str(user);
            out.push('@');
        }
        out.push_str(&self.place());
        out
    }
}

impl ContainerTarget {
    /// The container part of the name: `name`, or `ns/pod/ctr`.
    fn place(&self) -> String {
        match (&self.namespace, &self.container) {
            (Some(ns), Some(ctr)) => [ns.as_str(), &self.name, ctr].join("/"),
            _ => self.name.clone(),
        }
    }
}

impl std::fmt::Display for ContainerTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_url())
    }
}

/// Whether `name` is a valid name for `kind`'s containers. Docker and
/// Podman: `[A-Za-z0-9][A-Za-z0-9_.-]*` (ids are hex); Kubernetes:
/// lowercase DNS names `[a-z0-9][a-z0-9.-]*`; WSL:
/// `[A-Za-z0-9][A-Za-z0-9_.-]*`. So nothing option-shaped, and nothing a
/// shell or argv parser could misread, ever reaches a command line.
fn check_name(name: &str, kind: ExecKind) -> Result<(), TargetError> {
    let mut chars = name.chars();
    let first_ok = chars.next().is_some_and(|c| {
        if kind == ExecKind::Kubernetes {
            c.is_ascii_lowercase() || c.is_ascii_digit()
        } else {
            c.is_ascii_alphanumeric()
        }
    });
    let rest_ok = chars.all(|c| {
        if kind == ExecKind::Kubernetes {
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.'
        } else {
            c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')
        }
    });
    if first_ok && rest_ok && name.len() <= 253 {
        Ok(())
    } else {
        Err(TargetError::BadName)
    }
}

/// A user name, uid, or `user:group` / `uid:gid`: `[A-Za-z0-9_]` first,
/// then `[A-Za-z0-9_.:-]`.
fn is_user(user: &str) -> bool {
    let mut chars = user.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'))
        && user.len() <= 64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_names_parse() {
        let t = ContainerTarget::parse("docker://web/srv/app").unwrap();
        assert_eq!(
            (t.kind, t.name.as_str(), t.path.as_str()),
            (ExecKind::Docker, "web", "/srv/app")
        );
        assert_eq!(t.user, None);
        let t = ContainerTarget::parse("podman://node@api").unwrap();
        assert_eq!(
            (t.kind, t.user.as_deref(), t.path.as_str()),
            (ExecKind::Podman, Some("node"), "~")
        );
        let t = ContainerTarget::parse("docker://1000:1000@abc123/~/src").unwrap();
        assert_eq!(
            (t.user.as_deref(), t.path.as_str()),
            (Some("1000:1000"), "~/src")
        );
        let t = ContainerTarget::parse("k8s://dev/web-7f9/app/srv/a%20b").unwrap();
        assert_eq!(t.kind, ExecKind::Kubernetes);
        assert_eq!(t.namespace.as_deref(), Some("dev"));
        assert_eq!(t.name, "web-7f9");
        assert_eq!(t.container.as_deref(), Some("app"));
        assert_eq!(t.path, "/srv/a b");
        let t = ContainerTarget::parse("k8s://dev/web/app").unwrap();
        assert_eq!(t.path, "~");
        let t = ContainerTarget::parse("wsl://me@Ubuntu-24.04/home/me").unwrap();
        assert_eq!(
            (t.kind, t.name.as_str(), t.user.as_deref()),
            (ExecKind::Wsl, "Ubuntu-24.04", Some("me"))
        );
    }

    #[test]
    fn names_round_trip() {
        for name in [
            "docker://web/srv/app",
            "podman://node@api/~",
            "docker://web/a%20b/c%25d",
            "k8s://dev/web/app/srv",
            "wsl://Ubuntu/home/me",
        ] {
            let t = ContainerTarget::parse(name).unwrap();
            assert_eq!(ContainerTarget::parse(&t.to_url()).unwrap(), t, "{name}");
        }
        assert_eq!(
            ContainerTarget::parse("docker://web").unwrap().to_url(),
            "docker://web/~"
        );
    }

    #[test]
    fn hostile_names_are_refused() {
        use TargetError as E;
        let cases = [
            ("docker://", E::BadName),
            ("docker://-it/x", E::BadName),
            ("docker://a b/x", E::BadName),
            ("docker://a;b/x", E::BadName),
            ("docker://-u@web/x", E::BadUser),
            ("docker://a b@web/x", E::BadUser),
            ("docker://web/rel%ZZ", E::BadPath),
            ("k8s://dev/web", E::BadName),
            ("k8s://Dev/web/app", E::BadName),
            ("k8s://dev/-web/app", E::BadName),
            ("k8s://me@dev/web/app", E::BadName),
            ("wsl://-d/x", E::BadName),
            ("ftp://x/y", E::NotRemote),
        ];
        for (name, want) in cases {
            assert_eq!(ContainerTarget::parse(name), Err(want), "{name}");
        }
        assert!(
            ContainerTarget::new(ExecKind::Docker, "web")
                .unwrap()
                .with_user("x y")
                .is_err()
        );
    }

    #[test]
    fn labels_name_the_container() {
        let t = ContainerTarget::parse("docker://me@web/x").unwrap();
        assert_eq!(t.label(), "docker:me@web");
        let t = ContainerTarget::parse("k8s://dev/web/app/x").unwrap();
        assert_eq!(t.label(), "k8s:dev/web/app");
        assert!(ContainerTarget::is_container("wsl://Ubuntu"));
        assert!(!ContainerTarget::is_container("ssh://h/x"));
    }
}
