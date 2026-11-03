//! Argument-vector builders for every engine command Vix issues.
//!
//! Each builder returns the arguments **after** the engine binary (so the same
//! vector works for `docker`, `podman` and `nerdctl`). Nothing here runs a
//! shell: a value reaches the engine as exactly one argv element. Values an
//! engine could mistake for an option (a leading `-`), and values with
//! whitespace or control characters, are refused by [`check`] before any
//! command exists. [`render`] formats a vector for the confirm prompts that
//! precede every mutating command.
//!
//! The refusal rule is the crate's injection defense: because there is no
//! shell, the only way a hostile value (an image name from a registry, a
//! container name) could change a command is by being read as an *option*, so
//! a leading `-` is rejected rather than escaped. A value that must legally
//! start with `-` is not something Vix ever needs to pass. Owning spec:
//! `crates/vix-oci/spec/index.md`.
//!
//! ```
//! use vix_oci::argv;
//! let args = argv::pull("docker.io/library/alpine:3").unwrap();
//! assert_eq!(argv::render("docker", &args), "docker pull docker.io/library/alpine:3");
//! assert!(argv::pull("--privileged").is_err());
//! ```

use crate::OciError;

/// What a list or inspect command is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Kind {
    /// Containers (`ps`).
    Container,
    /// Images (`images`).
    Image,
    /// Volumes (`volume ls`).
    Volume,
    /// Networks (`network ls`).
    Network,
    /// Podman pods (`pod ps`).
    Pod,
}

/// A per-container action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ContainerOp {
    /// `start`.
    Start,
    /// `stop`.
    Stop,
    /// `restart`.
    Restart,
    /// `pause`.
    Pause,
    /// `unpause`.
    Unpause,
    /// `rm`, with `-f` to remove a running container.
    Remove {
        /// Pass `-f`.
        force: bool,
    },
}

impl ContainerOp {
    /// Whether the operation destroys something and so needs a confirmation.
    #[must_use]
    pub fn is_destructive(self) -> bool {
        matches!(self, ContainerOp::Remove { .. })
    }
}

/// Options for `run`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunOptions {
    /// The image to run (required).
    pub image: String,
    /// `--name`, when set.
    pub name: Option<String>,
    /// `-p` mappings such as `8080:80`.
    pub ports: Vec<String>,
    /// Detach (`-d`) instead of attaching an interactive terminal (`-it`).
    pub detach: bool,
}

/// A Compose action.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ComposeOp {
    /// `up -d`.
    Up,
    /// `down`.
    Down,
    /// `logs`, optionally for one service, without following.
    Logs {
        /// The service, or every service when `None`.
        service: Option<String>,
        /// How many trailing lines per service.
        tail: usize,
    },
    /// `ps`.
    Ps,
    /// `config --services`: the service names.
    Services,
}

/// Refuse a value that is empty, starts with `-`, or contains whitespace or a
/// control character. Used for ids, names, image references and ports.
/// Returns the value unchanged so calls can be chained inline.
///
/// # Errors
/// [`OciError::InvalidArgument`] naming the refused value.
pub fn check(arg: &str) -> Result<&str, OciError> {
    if arg.is_empty()
        || arg.starts_with('-')
        || arg.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(OciError::InvalidArgument(arg.to_string()));
    }
    Ok(arg)
}

/// Like [`check`] for a filesystem path: spaces are fine, control characters
/// and a leading `-` are not.
///
/// # Errors
/// [`OciError::InvalidArgument`] naming the refused value.
pub fn check_path(arg: &str) -> Result<&str, OciError> {
    if arg.is_empty() || arg.starts_with('-') || arg.chars().any(char::is_control) {
        return Err(OciError::InvalidArgument(arg.to_string()));
    }
    Ok(arg)
}

/// A vector of owned strings from literals; keeps the builders readable.
fn v(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| (*s).to_string()).collect()
}

/// The listing command for `kind`, always `--format json`. `all` includes
/// stopped containers (`ps -a`) and intermediate images are never included.
#[must_use]
pub fn list(kind: Kind, all: bool) -> Vec<String> {
    match kind {
        Kind::Container if all => v(&["ps", "-a", "--no-trunc", "--format", "json"]),
        Kind::Container => v(&["ps", "--no-trunc", "--format", "json"]),
        Kind::Image => v(&["images", "--format", "json"]),
        Kind::Volume => v(&["volume", "ls", "--format", "json"]),
        Kind::Network => v(&["network", "ls", "--no-trunc", "--format", "json"]),
        Kind::Pod => v(&["pod", "ps", "--format", "json"]),
    }
}

/// `inspect` for one object of `kind`.
///
/// # Errors
/// [`OciError::InvalidArgument`] for a bad `id`.
pub fn inspect(kind: Kind, id: &str) -> Result<Vec<String>, OciError> {
    let noun = match kind {
        Kind::Container => "container",
        Kind::Image => "image",
        Kind::Volume => "volume",
        Kind::Network => "network",
        Kind::Pod => "pod",
    };
    Ok(vec![noun.into(), "inspect".into(), check(id)?.into()])
}

/// A container action.
///
/// # Errors
/// [`OciError::InvalidArgument`] for a bad `id`.
pub fn container_op(op: ContainerOp, id: &str) -> Result<Vec<String>, OciError> {
    let id = check(id)?;
    let mut a = match op {
        ContainerOp::Start => v(&["start"]),
        ContainerOp::Stop => v(&["stop"]),
        ContainerOp::Restart => v(&["restart"]),
        ContainerOp::Pause => v(&["pause"]),
        ContainerOp::Unpause => v(&["unpause"]),
        ContainerOp::Remove { force: true } => v(&["rm", "-f"]),
        ContainerOp::Remove { force: false } => v(&["rm"]),
    };
    a.push(id.into());
    Ok(a)
}

/// The last `tail` log lines of a container, not following.
///
/// # Errors
/// [`OciError::InvalidArgument`] for a bad `id`.
pub fn logs(id: &str, tail: usize) -> Result<Vec<String>, OciError> {
    Ok(vec![
        "logs".into(),
        "--tail".into(),
        tail.to_string(),
        check(id)?.into(),
    ])
}

/// An interactive shell in a running container (for a PTY tab).
///
/// # Errors
/// [`OciError::InvalidArgument`] for a bad `id`.
pub fn exec_shell(id: &str) -> Result<Vec<String>, OciError> {
    Ok(vec![
        "exec".into(),
        "-it".into(),
        check(id)?.into(),
        "sh".into(),
    ])
}

/// `run` for an image. Detached runs use `-d`; otherwise `-it --rm` so the
/// container does not outlive the session.
///
/// # Errors
/// [`OciError::InvalidArgument`] for a bad image, name or port mapping.
pub fn run(opts: &RunOptions) -> Result<Vec<String>, OciError> {
    let mut a = v(&["run"]);
    // Attached runs are interactive and self-cleaning (`--rm`); detached
    // ones persist until removed from the Containers panel.
    if opts.detach {
        a.push("-d".into());
    } else {
        a.extend(v(&["-it", "--rm"]));
    }
    if let Some(name) = &opts.name {
        a.push("--name".into());
        a.push(check(name)?.into());
    }
    for port in &opts.ports {
        a.push("-p".into());
        a.push(check(port)?.into());
    }
    // The image goes last: anything after it would be the container's own
    // command, which Vix never supplies.
    a.push(check(&opts.image)?.into());
    Ok(a)
}

/// `tag SOURCE TARGET`.
///
/// # Errors
/// [`OciError::InvalidArgument`] for a bad reference.
pub fn tag(source: &str, target: &str) -> Result<Vec<String>, OciError> {
    Ok(vec![
        "tag".into(),
        check(source)?.into(),
        check(target)?.into(),
    ])
}

/// `pull IMAGE`.
///
/// # Errors
/// [`OciError::InvalidArgument`] for a bad reference.
pub fn pull(image: &str) -> Result<Vec<String>, OciError> {
    Ok(vec!["pull".into(), check(image)?.into()])
}

/// `push IMAGE`.
///
/// # Errors
/// [`OciError::InvalidArgument`] for a bad reference.
pub fn push(image: &str) -> Result<Vec<String>, OciError> {
    Ok(vec!["push".into(), check(image)?.into()])
}

/// Remove one object of `kind` (`rm`, `rmi`, `volume rm`, `network rm`,
/// `pod rm`). `force` adds `-f` where the engine accepts it.
///
/// # Errors
/// [`OciError::InvalidArgument`] for a bad `id`.
pub fn remove(kind: Kind, id: &str, force: bool) -> Result<Vec<String>, OciError> {
    let mut a = match kind {
        Kind::Container => v(&["rm"]),
        Kind::Image => v(&["rmi"]),
        Kind::Volume => v(&["volume", "rm"]),
        Kind::Network => v(&["network", "rm"]),
        Kind::Pod => v(&["pod", "rm"]),
    };
    // `network rm -f` means "ignore missing" on some engines and is
    // rejected by others, so it is never emitted for networks.
    if force && kind != Kind::Network {
        a.push("-f".into());
    }
    a.push(check(id)?.into());
    Ok(a)
}

/// `build -f FILE [-t TAG] CONTEXT`.
///
/// # Errors
/// [`OciError::InvalidArgument`] for a bad path or tag.
pub fn build(file: &str, tag: Option<&str>, context: &str) -> Result<Vec<String>, OciError> {
    let mut a = vec!["build".into(), "-f".into(), check_path(file)?.into()];
    if let Some(tag) = tag {
        a.push("-t".into());
        a.push(check(tag)?.into());
    }
    a.push(check_path(context)?.into());
    Ok(a)
}

/// A `compose` command against an optional compose file. Podman and nerdctl
/// both accept `compose` as a subcommand (Podman delegates to a provider).
///
/// # Errors
/// [`OciError::InvalidArgument`] for a bad path or service name.
pub fn compose(op: &ComposeOp, file: Option<&str>) -> Result<Vec<String>, OciError> {
    let mut a = v(&["compose"]);
    if let Some(file) = file {
        a.push("-f".into());
        a.push(check_path(file)?.into());
    }
    match op {
        ComposeOp::Up => a.extend(v(&["up", "-d"])),
        ComposeOp::Down => a.push("down".into()),
        ComposeOp::Ps => a.push("ps".into()),
        ComposeOp::Services => a.extend(v(&["config", "--services"])),
        ComposeOp::Logs { service, tail } => {
            a.extend(v(&["logs", "--no-color", "--tail"]));
            a.push(tail.to_string());
            if let Some(service) = service {
                a.push(check(service)?.into());
            }
        }
    }
    Ok(a)
}

/// `login --username USER --password-stdin REGISTRY`. The password is **never**
/// an argument: the caller writes it to the engine's standard input.
///
/// # Errors
/// [`OciError::InvalidArgument`] for a bad registry or username.
pub fn login(registry: &str, username: &str) -> Result<Vec<String>, OciError> {
    Ok(vec![
        "login".into(),
        "--username".into(),
        check(username)?.into(),
        "--password-stdin".into(),
        check(registry)?.into(),
    ])
}

/// A non-interactive prune (`-f`, because the app has already asked) of
/// stopped containers, dangling images, unused volumes or unused networks.
/// Pods have no prune.
///
/// # Errors
/// [`OciError::InvalidArgument`] for [`Kind::Pod`].
pub fn prune(kind: Kind) -> Result<Vec<String>, OciError> {
    match kind {
        Kind::Container => Ok(v(&["container", "prune", "-f"])),
        Kind::Image => Ok(v(&["image", "prune", "-f"])),
        Kind::Volume => Ok(v(&["volume", "prune", "-f"])),
        Kind::Network => Ok(v(&["network", "prune", "-f"])),
        // Podman has no `pod prune` in the form the other kinds use.
        Kind::Pod => Err(OciError::InvalidArgument("pod".into())),
    }
}

/// The command line as a person would type it, for display only: arguments
/// with anything beyond `[A-Za-z0-9_@%+=:,./-]` are single-quoted. The vector
/// itself is what runs; this string is never executed.
#[must_use]
pub fn render(binary: &str, args: &[String]) -> String {
    // POSIX single-quote style: an embedded `'` closes the quote, adds an
    // escaped quote, and reopens (`'\''`).
    let mut out = String::from(binary);
    for a in args {
        out.push(' ');
        let plain = !a.is_empty()
            && a.chars()
                .all(|c| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c));
        if plain {
            out.push_str(a);
        } else {
            out.push('\'');
            out.push_str(&a.replace('\'', "'\\''"));
            out.push('\'');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_values_are_refused() {
        for bad in ["", "-rf", "a b", "a\nb", "--privileged", "x\0y"] {
            assert!(
                matches!(check(bad), Err(OciError::InvalidArgument(_))),
                "{bad:?}"
            );
        }
        assert_eq!(
            check("ghcr.io/x/y:1@sha256:ab").unwrap(),
            "ghcr.io/x/y:1@sha256:ab"
        );
        assert!(container_op(ContainerOp::Stop, "--all").is_err());
        assert!(check_path("my dir/Dockerfile").is_ok());
        assert!(check_path("-f").is_err());
    }

    #[test]
    fn container_ops_build_the_expected_argv() {
        assert_eq!(
            container_op(ContainerOp::Restart, "abc").unwrap(),
            ["restart", "abc"]
        );
        assert_eq!(
            container_op(ContainerOp::Remove { force: true }, "abc").unwrap(),
            ["rm", "-f", "abc"]
        );
        assert!(ContainerOp::Remove { force: false }.is_destructive());
        assert!(!ContainerOp::Stop.is_destructive());
    }

    #[test]
    fn listing_always_asks_for_json() {
        for kind in [
            Kind::Container,
            Kind::Image,
            Kind::Volume,
            Kind::Network,
            Kind::Pod,
        ] {
            let a = list(kind, true);
            assert_eq!(&a[a.len() - 2..], ["--format", "json"]);
        }
        assert!(list(Kind::Container, true).contains(&"-a".to_string()));
        assert!(!list(Kind::Container, false).contains(&"-a".to_string()));
    }

    #[test]
    fn run_detached_and_interactive() {
        let mut o = RunOptions {
            image: "alpine:3".into(),
            name: Some("web".into()),
            ports: vec!["8080:80".into()],
            detach: true,
        };
        assert_eq!(
            run(&o).unwrap(),
            ["run", "-d", "--name", "web", "-p", "8080:80", "alpine:3"]
        );
        o.detach = false;
        o.name = None;
        o.ports.clear();
        assert_eq!(run(&o).unwrap(), ["run", "-it", "--rm", "alpine:3"]);
        o.image = "--rm".into();
        assert!(run(&o).is_err());
    }

    #[test]
    fn the_password_is_never_an_argument() {
        let a = login("ghcr.io", "octocat").unwrap();
        assert!(a.contains(&"--password-stdin".to_string()));
        assert!(!a.iter().any(|s| s == "-p" || s.starts_with("--password=")));
    }

    #[test]
    fn compose_and_build_and_prune() {
        assert_eq!(
            compose(&ComposeOp::Up, Some("dir/compose.yaml")).unwrap(),
            ["compose", "-f", "dir/compose.yaml", "up", "-d"]
        );
        assert_eq!(
            compose(
                &ComposeOp::Logs {
                    service: Some("db".into()),
                    tail: 50
                },
                None
            )
            .unwrap(),
            ["compose", "logs", "--no-color", "--tail", "50", "db"]
        );
        assert_eq!(
            build("Containerfile", Some("app:dev"), ".").unwrap(),
            ["build", "-f", "Containerfile", "-t", "app:dev", "."]
        );
        assert_eq!(prune(Kind::Volume).unwrap(), ["volume", "prune", "-f"]);
        assert!(prune(Kind::Pod).is_err());
    }

    #[test]
    fn render_quotes_only_what_needs_it() {
        let a = vec![
            "build".to_string(),
            "-f".into(),
            "my dir/it's".into(),
            String::new(),
        ];
        assert_eq!(render("docker", &a), "docker build -f 'my dir/it'\\''s' ''");
        assert_eq!(render("podman", &["ps".to_string()]), "podman ps");
    }
}
