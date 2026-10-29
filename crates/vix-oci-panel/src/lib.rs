//! The OC menu's list panel: one overlay that shows containers, images,
//! volumes, networks or (on Podman) pods, with per-row actions.
//!
//! The panel is pure state. [`Panel::key`] turns a key press into a
//! [`Request`] for the app to carry out (list again, inspect, stream logs, open
//! a shell, run a command); the panel never starts a process itself. Anything
//! that changes the engine's state goes through a [`Pending`], a fully built
//! argument vector plus the exact command line to show. Destructive and
//! long-running ones sit in [`Mode::Confirm`] first, so the user always sees
//! what will run. In read-only mode every mutating key is refused with
//! [`Request::ReadOnly`] before a command exists.

use vix_input::{KeyCode, KeyEvent, KeyModifiers};
use vix_oci::argv::{self, ComposeOp, ContainerOp, Kind, RunOptions};
use vix_oci::{Container, Engine, Image, Network, OciError, Pod, Volume};

/// A command ready to run, with the line to show the user first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    /// The arguments after the engine binary.
    pub args: Vec<String>,
    /// The command as it would be typed, for display only.
    pub rendered: String,
    /// Stream the output into the bottom dock (builds, pulls, runs, Compose)
    /// instead of running to completion and reporting in the status line.
    pub stream: bool,
}

impl Pending {
    /// Wrap `args` for `binary`.
    #[must_use]
    pub fn new(binary: &str, args: Vec<String>, stream: bool) -> Pending {
        Pending {
            rendered: argv::render(binary, &args),
            args,
            stream,
        }
    }
}

/// What a text-input prompt inside the panel is collecting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Purpose {
    /// `IMAGE [name=NAME] [port=HOST:CTR]...` for `run`.
    Run,
    /// The new reference for `tag`.
    Tag,
    /// The image reference to `pull`.
    Pull,
    /// `DOCKERFILE [tag=REF] [context=DIR]` for `build`.
    Build,
}

/// What the panel is doing with keys.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Mode {
    /// Moving through the list.
    Browse,
    /// Showing the exact command and waiting for yes or no.
    Confirm(Pending),
    /// Collecting a line of text.
    Input {
        /// What the text is for.
        purpose: Purpose,
        /// The text so far.
        text: String,
        /// For [`Purpose::Tag`], the image being tagged.
        subject: String,
    },
}

/// What the app should do after a key.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Request {
    /// Nothing.
    None,
    /// Close the panel.
    Close,
    /// Fetch the current kind's list again.
    Reload,
    /// Show `inspect` output for the object.
    Inspect(Kind, String),
    /// Stream a container's logs to the dock.
    Logs(String),
    /// Open an interactive shell in the running container.
    Shell(String),
    /// Open the container's first published port in a browser.
    OpenPort(u16),
    /// Run this command (already confirmed where it needed to be).
    Exec(Pending),
    /// The key would change something but read-only mode is on.
    ReadOnly,
    /// An argument was refused before any command was built.
    Invalid(String),
}

/// The overlay's state.
#[derive(Debug, Clone)]
pub struct Panel {
    /// Which list is showing.
    pub kind: Kind,
    /// The engine being driven.
    pub engine: Engine,
    /// Listed containers (all, or only running when [`Panel::show_all`] is off).
    pub containers: Vec<Container>,
    /// Listed images.
    pub images: Vec<Image>,
    /// Listed volumes.
    pub volumes: Vec<Volume>,
    /// Listed networks.
    pub networks: Vec<Network>,
    /// Listed pods (Podman only).
    pub pods: Vec<Pod>,
    /// Include stopped containers.
    pub show_all: bool,
    /// Refuse every mutating key.
    pub read_only: bool,
    /// The highlighted row.
    pub selected: usize,
    /// The first visible row.
    pub scroll: usize,
    /// What keys do right now.
    pub mode: Mode,
}

impl Panel {
    /// An empty panel for `engine`, showing `kind`.
    #[must_use]
    pub fn new(engine: Engine, kind: Kind, read_only: bool) -> Panel {
        Panel {
            kind,
            engine,
            containers: Vec::new(),
            images: Vec::new(),
            volumes: Vec::new(),
            networks: Vec::new(),
            pods: Vec::new(),
            show_all: true,
            read_only,
            selected: 0,
            scroll: 0,
            mode: Mode::Browse,
        }
    }

    /// The kinds this engine can show, in tab order.
    #[must_use]
    pub fn kinds(&self) -> Vec<Kind> {
        let mut k = vec![Kind::Container, Kind::Image, Kind::Volume, Kind::Network];
        if self.engine.has_pods() {
            k.push(Kind::Pod);
        }
        k
    }

    /// Number of rows in the current list.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.kind {
            Kind::Container => self.containers.len(),
            Kind::Image => self.images.len(),
            Kind::Volume => self.volumes.len(),
            Kind::Network => self.networks.len(),
            Kind::Pod => self.pods.len(),
            _ => 0,
        }
    }

    /// Whether the current list has no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// One row as display text, columns separated by two spaces.
    #[must_use]
    pub fn row(&self, i: usize) -> String {
        match self.kind {
            Kind::Container => self.containers.get(i).map(|c| {
                format!(
                    "{}  {}  {}  {}  {}",
                    c.name,
                    c.image,
                    if c.status.is_empty() {
                        &c.state
                    } else {
                        &c.status
                    },
                    c.ports,
                    c.short_id()
                )
            }),
            Kind::Image => self.images.get(i).map(|m| {
                format!(
                    "{}:{}  {}  {}  {}",
                    m.repository,
                    m.tag,
                    m.size,
                    m.created,
                    &m.id[..m.id.len().min(19)]
                )
            }),
            Kind::Volume => self
                .volumes
                .get(i)
                .map(|v| format!("{}  {}  {}", v.name, v.driver, v.mountpoint)),
            Kind::Network => self
                .networks
                .get(i)
                .map(|n| format!("{}  {}  {}  {}", n.name, n.driver, n.scope, n.id)),
            Kind::Pod => self
                .pods
                .get(i)
                .map(|p| format!("{}  {}  {}", p.name, p.status, p.containers)),
            _ => None,
        }
        .unwrap_or_default()
    }

    /// The name or reference an engine command should use for the highlighted
    /// row.
    #[must_use]
    pub fn selected_ref(&self) -> Option<String> {
        let i = self.selected;
        match self.kind {
            Kind::Container => self.containers.get(i).map(|c| c.id.clone()),
            Kind::Image => self.images.get(i).map(Image::reference),
            Kind::Volume => self.volumes.get(i).map(|v| v.name.clone()),
            Kind::Network => self.networks.get(i).map(|n| n.name.clone()),
            Kind::Pod => self.pods.get(i).map(|p| p.id.clone()),
            _ => None,
        }
    }

    /// The highlighted container.
    #[must_use]
    pub fn selected_container(&self) -> Option<&Container> {
        (self.kind == Kind::Container)
            .then(|| self.containers.get(self.selected))
            .flatten()
    }

    /// Replace the current kind's rows with freshly parsed output, keeping the
    /// highlight on the same index (clamped).
    pub fn set_rows(&mut self, rows: Rows) {
        match rows {
            Rows::Containers(v) => self.containers = v,
            Rows::Images(v) => self.images = v,
            Rows::Volumes(v) => self.volumes = v,
            Rows::Networks(v) => self.networks = v,
            Rows::Pods(v) => self.pods = v,
        }
        self.selected = self.selected.min(self.len().saturating_sub(1));
    }

    /// Switch to `kind`, resetting the highlight.
    pub fn set_kind(&mut self, kind: Kind) {
        self.kind = kind;
        self.selected = 0;
        self.scroll = 0;
        self.mode = Mode::Browse;
    }

    /// Step to the next (or previous) kind this engine supports.
    pub fn cycle_kind(&mut self, forward: bool) {
        let kinds = self.kinds();
        let at = kinds.iter().position(|k| *k == self.kind).unwrap_or(0);
        let next = if forward {
            (at + 1) % kinds.len()
        } else {
            (at + kinds.len() - 1) % kinds.len()
        };
        self.set_kind(kinds[next]);
    }

    /// Move the highlight up one row.
    pub fn up(&mut self) {
        self.selected = vix_list_state::up(self.selected);
    }

    /// Move the highlight down one row.
    pub fn down(&mut self) {
        self.selected = vix_list_state::down(self.selected, self.len());
    }

    /// Move the highlight up a page.
    pub fn page_up(&mut self, page: usize) {
        self.selected = vix_list_state::page_up(self.selected, page);
    }

    /// Move the highlight down a page.
    pub fn page_down(&mut self, page: usize) {
        self.selected = vix_list_state::page_down(self.selected, page, self.len());
    }

    /// Highlight row `idx` (a click); `true` if it is a real row.
    pub fn select_index(&mut self, idx: usize) -> bool {
        match vix_list_state::select_index(idx, self.len()) {
            Some(i) => {
                self.selected = i;
                true
            }
            None => false,
        }
    }

    /// Keep the highlight inside a `height`-row viewport.
    pub fn ensure_visible(&mut self, height: usize) {
        self.scroll =
            vix_list_state::ensure_visible(self.selected, self.scroll, height, self.len());
    }

    /// Open a text prompt. `prefill` seeds the text.
    pub fn start_input(&mut self, purpose: Purpose, prefill: &str, subject: &str) {
        self.mode = Mode::Input {
            purpose,
            text: prefill.to_string(),
            subject: subject.to_string(),
        };
    }

    fn binary(&self) -> &'static str {
        self.engine.binary()
    }

    fn confirm(&mut self, args: Result<Vec<String>, OciError>, stream: bool) -> Request {
        match args {
            Ok(args) => {
                self.mode = Mode::Confirm(Pending::new(self.binary(), args, stream));
                Request::None
            }
            Err(e) => Request::Invalid(e.to_string()),
        }
    }

    fn now(&self, args: Result<Vec<String>, OciError>) -> Request {
        match args {
            Ok(args) => Request::Exec(Pending::new(self.binary(), args, false)),
            Err(e) => Request::Invalid(e.to_string()),
        }
    }

    /// Handle one key press.
    pub fn key(&mut self, key: KeyEvent, page: usize) -> Request {
        match self.mode.clone() {
            Mode::Browse => self.browse_key(key, page),
            Mode::Confirm(pending) => {
                self.mode = Mode::Browse;
                if matches!(key.code, KeyCode::Char('y' | 'Y') | KeyCode::Enter) {
                    Request::Exec(pending)
                } else {
                    Request::None
                }
            }
            Mode::Input {
                purpose,
                text,
                subject,
            } => self.input_key(key, purpose, text, &subject),
        }
    }

    fn browse_key(&mut self, key: KeyEvent, page: usize) -> Request {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return Request::None;
        }
        match key.code {
            KeyCode::Up => self.up(),
            KeyCode::Down => self.down(),
            KeyCode::PageUp => self.page_up(page),
            KeyCode::PageDown => self.page_down(page),
            KeyCode::Tab => {
                self.cycle_kind(true);
                return Request::Reload;
            }
            KeyCode::BackTab => {
                self.cycle_kind(false);
                return Request::Reload;
            }
            KeyCode::Esc => return Request::Close,
            KeyCode::Enter => return self.inspect(),
            KeyCode::Delete => return self.remove(),
            KeyCode::Char(c) => return self.char_key(c),
            _ => {}
        }
        Request::None
    }

    fn inspect(&self) -> Request {
        self.selected_ref()
            .map_or(Request::None, |id| Request::Inspect(self.kind, id))
    }

    fn guard(&self) -> Option<Request> {
        self.read_only.then_some(Request::ReadOnly)
    }

    fn remove(&mut self) -> Request {
        if let Some(r) = self.guard() {
            return r;
        }
        let Some(id) = self.selected_ref() else {
            return Request::None;
        };
        // A running container needs `-f`; everything else is removed plainly,
        // so an object that is still in use is refused by the engine.
        let force = self.selected_container().is_some_and(Container::is_running);
        let args = match self.kind {
            Kind::Container => argv::container_op(ContainerOp::Remove { force }, &id),
            other => argv::remove(other, &id, force),
        };
        self.confirm(args, false)
    }

    fn char_key(&mut self, c: char) -> Request {
        match c {
            'r' => return Request::Reload,
            'q' => return Request::Close,
            'i' => return self.inspect(),
            'd' => return self.remove(),
            _ => {}
        }
        match self.kind {
            Kind::Container => self.container_key(c),
            Kind::Image => self.image_key(c),
            _ => Request::None,
        }
    }

    fn container_key(&mut self, c: char) -> Request {
        let Some(ctr) = self.selected_container().cloned() else {
            return Request::None;
        };
        let op = match c {
            'a' => {
                self.show_all = !self.show_all;
                return Request::Reload;
            }
            'l' => return Request::Logs(ctr.id),
            'o' => {
                return ctr
                    .host_ports
                    .first()
                    .map_or(Request::None, |p| Request::OpenPort(*p));
            }
            'e' => {
                return if ctr.is_running() {
                    Request::Shell(ctr.id)
                } else {
                    Request::None
                };
            }
            's' => ContainerOp::Start,
            'x' => ContainerOp::Stop,
            't' => ContainerOp::Restart,
            'p' if ctr.is_paused() => ContainerOp::Unpause,
            'p' => ContainerOp::Pause,
            _ => return Request::None,
        };
        if let Some(r) = self.guard() {
            return r;
        }
        self.now(argv::container_op(op, &ctr.id))
    }

    fn image_key(&mut self, c: char) -> Request {
        let Some(image) = self.images.get(self.selected).cloned() else {
            return if c == 'p' {
                self.pull_prompt("")
            } else {
                Request::None
            };
        };
        let reference = image.reference();
        match c {
            'u' => {
                if let Some(r) = self.guard() {
                    return r;
                }
                self.start_input(Purpose::Run, &reference, "");
                Request::None
            }
            'g' => {
                if let Some(r) = self.guard() {
                    return r;
                }
                self.start_input(Purpose::Tag, "", &reference);
                Request::None
            }
            'p' => self.pull_prompt(&reference),
            'P' => {
                if let Some(r) = self.guard() {
                    return r;
                }
                self.confirm(argv::push(&reference), true)
            }
            _ => Request::None,
        }
    }

    fn pull_prompt(&mut self, prefill: &str) -> Request {
        if let Some(r) = self.guard() {
            return r;
        }
        self.start_input(Purpose::Pull, prefill, "");
        Request::None
    }

    fn input_key(
        &mut self,
        key: KeyEvent,
        purpose: Purpose,
        mut text: String,
        subject: &str,
    ) -> Request {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Browse;
                Request::None
            }
            KeyCode::Enter => {
                self.mode = Mode::Browse;
                self.submit(purpose, text.trim(), subject)
            }
            KeyCode::Backspace => {
                text.pop();
                self.mode = Mode::Input {
                    purpose,
                    text,
                    subject: subject.to_string(),
                };
                Request::None
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                text.push(c);
                self.mode = Mode::Input {
                    purpose,
                    text,
                    subject: subject.to_string(),
                };
                Request::None
            }
            _ => Request::None,
        }
    }

    fn submit(&mut self, purpose: Purpose, text: &str, subject: &str) -> Request {
        if self.read_only {
            return Request::ReadOnly;
        }
        match purpose {
            Purpose::Tag => self.now(argv::tag(subject, text)),
            Purpose::Pull => self.confirm(argv::pull(text), true),
            Purpose::Run => self.confirm(parse_run(text).and_then(|o| argv::run(&o)), true),
            Purpose::Build => self.confirm(parse_build(text), true),
        }
    }
}

/// Freshly parsed rows for one kind.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Rows {
    /// From `ps`.
    Containers(Vec<Container>),
    /// From `images`.
    Images(Vec<Image>),
    /// From `volume ls`.
    Volumes(Vec<Volume>),
    /// From `network ls`.
    Networks(Vec<Network>),
    /// From `pod ps`.
    Pods(Vec<Pod>),
}

/// Parse the Run prompt: `IMAGE [name=NAME] [port=HOST:CTR]...`. Runs are
/// detached, because the output streams to the dock rather than a terminal.
fn parse_run(text: &str) -> Result<RunOptions, OciError> {
    let mut opts = RunOptions {
        detach: true,
        ..RunOptions::default()
    };
    for (i, word) in text.split_whitespace().enumerate() {
        if let Some(name) = word.strip_prefix("name=") {
            opts.name = Some(name.to_string());
        } else if let Some(port) = word.strip_prefix("port=") {
            opts.ports.push(port.to_string());
        } else if i == 0 {
            opts.image = word.to_string();
        } else {
            return Err(OciError::InvalidArgument(word.to_string()));
        }
    }
    Ok(opts)
}

/// Parse the Build prompt: `DOCKERFILE [tag=REF] [context=DIR]`.
fn parse_build(text: &str) -> Result<Vec<String>, OciError> {
    let mut file = None;
    let mut tag = None;
    let mut context = ".";
    for word in text.split_whitespace() {
        if let Some(t) = word.strip_prefix("tag=") {
            tag = Some(t);
        } else if let Some(c) = word.strip_prefix("context=") {
            context = c;
        } else if file.is_none() {
            file = Some(word);
        } else {
            return Err(OciError::InvalidArgument(word.to_string()));
        }
    }
    argv::build(file.unwrap_or(""), tag, context)
}

/// The Compose command for `op`, ready to stream, as a [`Pending`]. `file` is
/// the detected compose file, if any.
///
/// # Errors
/// [`OciError::InvalidArgument`] for a bad path or service name.
pub fn compose_pending(
    engine: Engine,
    op: &ComposeOp,
    file: Option<&str>,
) -> Result<Pending, OciError> {
    Ok(Pending::new(
        engine.binary(),
        argv::compose(op, file)?,
        true,
    ))
}

/// The prune command for `kind` as a confirm-first [`Pending`].
///
/// # Errors
/// [`OciError::InvalidArgument`] for [`Kind::Pod`].
pub fn prune_pending(engine: Engine, kind: Kind) -> Result<Pending, OciError> {
    Ok(Pending::new(engine.binary(), argv::prune(kind)?, false))
}

/// File names Compose looks for, in its own precedence order.
pub const COMPOSE_FILES: [&str; 4] = [
    "compose.yaml",
    "compose.yml",
    "docker-compose.yaml",
    "docker-compose.yml",
];

/// File names the Build prompt is seeded from, first match wins.
pub const BUILD_FILES: [&str; 2] = ["Containerfile", "Dockerfile"];

#[cfg(test)]
mod tests {
    use super::*;

    fn press(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    fn ctr(id: &str, state: &str) -> Container {
        Container {
            id: id.into(),
            name: format!("n-{id}"),
            state: state.into(),
            host_ports: vec![8080],
            ..Container::default()
        }
    }

    fn panel(read_only: bool) -> Panel {
        let mut p = Panel::new(Engine::Docker, Kind::Container, read_only);
        p.set_rows(Rows::Containers(vec![
            ctr("aaa", "running"),
            ctr("bbb", "exited"),
            ctr("ccc", "paused"),
        ]));
        p
    }

    fn exec_args(r: Request) -> Vec<String> {
        match r {
            Request::Exec(p) => p.args,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn quick_actions_run_without_a_prompt() {
        let mut p = panel(false);
        assert_eq!(
            exec_args(p.key(press(KeyCode::Char('x')), 5)),
            ["stop", "aaa"]
        );
        p.down();
        assert_eq!(
            exec_args(p.key(press(KeyCode::Char('s')), 5)),
            ["start", "bbb"]
        );
        p.down();
        assert_eq!(
            exec_args(p.key(press(KeyCode::Char('p')), 5)),
            ["unpause", "ccc"]
        );
    }

    #[test]
    fn remove_shows_the_exact_command_and_needs_yes() {
        let mut p = panel(false);
        assert_eq!(p.key(press(KeyCode::Char('d')), 5), Request::None);
        let Mode::Confirm(pending) = p.mode.clone() else {
            panic!("expected a confirm");
        };
        assert_eq!(pending.rendered, "docker rm -f aaa");
        assert_eq!(p.key(press(KeyCode::Char('n')), 5), Request::None);
        assert_eq!(p.mode, Mode::Browse);
        p.key(press(KeyCode::Char('d')), 5);
        assert_eq!(
            exec_args(p.key(press(KeyCode::Char('y')), 5)),
            ["rm", "-f", "aaa"]
        );
        p.down();
        p.key(press(KeyCode::Delete), 5);
        let Mode::Confirm(pending) = p.mode.clone() else {
            panic!("expected a confirm");
        };
        assert_eq!(pending.rendered, "docker rm bbb");
    }

    #[test]
    fn read_only_refuses_every_mutating_key_but_not_viewing() {
        let mut p = panel(true);
        for c in ['s', 'x', 't', 'p', 'd'] {
            assert_eq!(p.key(press(KeyCode::Char(c)), 5), Request::ReadOnly, "{c}");
        }
        assert_eq!(p.mode, Mode::Browse);
        assert_eq!(
            p.key(press(KeyCode::Char('l')), 5),
            Request::Logs("aaa".into())
        );
        assert_eq!(
            p.key(press(KeyCode::Enter), 5),
            Request::Inspect(Kind::Container, "aaa".into())
        );
        assert_eq!(
            p.key(press(KeyCode::Char('e')), 5),
            Request::Shell("aaa".into())
        );
    }

    #[test]
    fn shell_needs_a_running_container_and_open_needs_a_port() {
        let mut p = panel(false);
        p.down();
        assert_eq!(p.key(press(KeyCode::Char('e')), 5), Request::None);
        assert_eq!(p.key(press(KeyCode::Char('o')), 5), Request::OpenPort(8080));
    }

    #[test]
    fn tab_cycles_kinds_and_pods_only_on_podman() {
        let mut p = panel(false);
        assert_eq!(p.kinds().len(), 4);
        assert_eq!(p.key(press(KeyCode::Tab), 5), Request::Reload);
        assert_eq!(p.kind, Kind::Image);
        p.key(press(KeyCode::BackTab), 5);
        p.key(press(KeyCode::BackTab), 5);
        assert_eq!(p.kind, Kind::Network);
        let podman = Panel::new(Engine::Podman, Kind::Container, false);
        assert_eq!(podman.kinds().last(), Some(&Kind::Pod));
    }

    #[test]
    fn run_prompt_builds_a_confirmed_detached_command() {
        let mut p = Panel::new(Engine::Docker, Kind::Image, false);
        p.set_rows(Rows::Images(vec![Image {
            id: "sha256:1".into(),
            repository: "nginx".into(),
            tag: "1.27".into(),
            ..Image::default()
        }]));
        p.key(press(KeyCode::Char('u')), 5);
        for c in " name=web port=8080:80".chars() {
            p.key(press(KeyCode::Char(c)), 5);
        }
        assert_eq!(p.key(press(KeyCode::Enter), 5), Request::None);
        let Mode::Confirm(pending) = p.mode.clone() else {
            panic!("expected a confirm");
        };
        assert_eq!(
            pending.rendered,
            "docker run -d --name web -p 8080:80 nginx:1.27"
        );
        assert!(pending.stream);
    }

    #[test]
    fn hostile_input_is_refused_not_run() {
        let mut p = Panel::new(Engine::Docker, Kind::Image, false);
        p.start_input(Purpose::Pull, "--privileged", "");
        assert!(matches!(
            p.key(press(KeyCode::Enter), 5),
            Request::Invalid(_)
        ));
        p.start_input(Purpose::Run, "alpine; rm -rf /", "");
        assert!(matches!(
            p.key(press(KeyCode::Enter), 5),
            Request::Invalid(_)
        ));
        assert_eq!(p.mode, Mode::Browse);
    }

    #[test]
    fn tag_runs_directly_and_push_confirms() {
        let mut p = Panel::new(Engine::Docker, Kind::Image, false);
        p.set_rows(Rows::Images(vec![Image {
            id: "sha256:1".into(),
            repository: "app".into(),
            tag: "dev".into(),
            ..Image::default()
        }]));
        p.key(press(KeyCode::Char('g')), 5);
        for c in "reg.example/app:1".chars() {
            p.key(press(KeyCode::Char(c)), 5);
        }
        assert_eq!(
            exec_args(p.key(press(KeyCode::Enter), 5)),
            ["tag", "app:dev", "reg.example/app:1"]
        );
        p.key(press(KeyCode::Char('P')), 5);
        assert!(matches!(&p.mode, Mode::Confirm(c) if c.rendered == "docker push app:dev"));
    }

    #[test]
    fn build_prompt_parses_words() {
        let args = parse_build("Containerfile tag=app:dev context=src").unwrap_or_default();
        assert_eq!(
            args,
            ["build", "-f", "Containerfile", "-t", "app:dev", "src"]
        );
        assert!(parse_build("").is_err());
        assert!(parse_build("a b").is_err());
    }

    #[test]
    fn pending_helpers_render() {
        let c = compose_pending(Engine::Podman, &ComposeOp::Up, Some("compose.yaml"))
            .unwrap_or_else(|_| panic!());
        assert_eq!(c.rendered, "podman compose -f compose.yaml up -d");
        let r = prune_pending(Engine::Docker, Kind::Image).unwrap_or_else(|_| panic!());
        assert_eq!(r.rendered, "docker image prune -f");
        assert!(!r.stream);
    }

    #[test]
    fn navigation_clamps() {
        let mut p = panel(false);
        for _ in 0..10 {
            p.down();
        }
        assert_eq!(p.selected, 2);
        p.set_rows(Rows::Containers(vec![ctr("zzz", "running")]));
        assert_eq!(p.selected, 0);
        assert!(p.select_index(0) && !p.select_index(1));
    }
}
