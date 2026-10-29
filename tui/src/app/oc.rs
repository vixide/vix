//! The **OC** menu (T700-T708): containers, images, volumes, networks and pods
//! of an OCI engine (Docker, Podman or nerdctl), driven through its CLI.
//!
//! The pure pieces live in `vix-oci` (detection, argv builders, parsers) and
//! `vix-oci-panel` (the list panel's state machine). This module is the App
//! side: finding the engine, running listings, carrying out a panel
//! [`Request`], and the menu actions. Nothing here builds a command line by
//! hand -- every command is an argv vector from `vix_oci::argv`, and anything
//! that changes the engine's state has already been shown to the user (or is a
//! plain start/stop) by the panel before it reaches [`App::oc_exec`].
//!
//! Long or chatty commands (logs, builds, pulls, pushes, runs, Compose) stream
//! into the bottom dock through the same no-shell runner Git and Jujutsu use;
//! short ones run to completion and report in the status line.

use vix_input::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};

use crate::oci::argv::{self, ComposeOp, Kind};
use crate::oci::{Engine, OciError, parse};
use crate::oci_panel::{BUILD_FILES, COMPOSE_FILES, Mode, Panel, Pending, Purpose, Request, Rows};

use super::{App, rect_contains};

/// The user-facing text of an engine error, with the install hint for a
/// missing engine.
fn error_text(e: &OciError) -> String {
    match e {
        OciError::NotInstalled => t!("msg.oc_not_installed").to_string(),
        other => other.to_string(),
    }
}

impl App {
    /// Dispatch an `oc.*` action. Returns `true` if `action` was handled.
    pub(super) fn run_oc_action(&mut self, action: &str) -> bool {
        match action {
            "oc.engine_detect" => self.oc_engine_detect(),
            "oc.engine_choose" => self.oc_engine_choose(),
            "oc.containers" => self.open_oc(Kind::Container, None),
            "oc.images" => self.open_oc(Kind::Image, None),
            "oc.volumes" => self.open_oc(Kind::Volume, None),
            "oc.networks" => self.open_oc(Kind::Network, None),
            "oc.pods" => self.open_oc(Kind::Pod, None),
            "oc.run" => self.open_oc(Kind::Image, Some((Purpose::Run, String::new()))),
            "oc.pull" => self.open_oc(Kind::Image, Some((Purpose::Pull, String::new()))),
            "oc.build" => {
                let seed = BUILD_FILES
                    .iter()
                    .find(|f| crate::authority::current().is_file(&self.root.join(f)))
                    .unwrap_or(&BUILD_FILES[1]);
                self.open_oc(Kind::Image, Some((Purpose::Build, (*seed).to_string())));
            }
            "oc.prune" => self.open_oc(
                Kind::Container,
                Some((Purpose::Prune, "containers".to_string())),
            ),
            "oc.compose_up" => self.oc_compose(&ComposeOp::Up),
            "oc.compose_down" => self.oc_compose(&ComposeOp::Down),
            "oc.compose_logs" => self.oc_compose(&ComposeOp::Logs {
                service: None,
                tail: 200,
            }),
            _ => return false,
        }
        true
    }

    /// Whether the `oc_mode` setting forbids changes. Anything but `"full"`
    /// reads as read-only, the safer interpretation of a typo.
    fn oc_read_only(&self) -> bool {
        self.settings.oc_mode != "full"
    }

    /// Find the engine the settings ask for, or tell the user how to get one.
    fn oc_detect(&mut self) -> Option<crate::oci::Detected> {
        match crate::oci::detect(&self.settings.oc_engine, crate::fileops::which_on_path) {
            Ok(d) => Some(d),
            Err(e) => {
                self.messages.error(error_text(&e));
                None
            }
        }
    }

    /// Run the engine to completion and return its output.
    fn oc_run(engine: Engine, args: &[String], stdin: Option<&str>) -> Result<String, OciError> {
        let program =
            crate::fileops::which_on_path(engine.binary()).ok_or(OciError::NotInstalled)?;
        crate::oci::run(&program, args, stdin)
    }

    /// **OC → Engine → Detect**: say which engine would be used.
    fn oc_engine_detect(&mut self) {
        if let Some(d) = self.oc_detect() {
            self.status = t!(
                "status.oc_engine",
                name = d.engine.binary(),
                path = d.program.display().to_string()
            )
            .to_string();
        }
    }

    /// **OC → Engine → Choose**: step the `oc_engine` setting through `auto`
    /// and each installed engine.
    fn oc_engine_choose(&mut self) {
        let mut choices = vec!["auto".to_string()];
        choices.extend(
            Engine::ALL
                .into_iter()
                .filter(|e| crate::fileops::which_on_path(e.binary()).is_some())
                .map(|e| e.binary().to_string()),
        );
        let at = choices
            .iter()
            .position(|c| *c == self.settings.oc_engine)
            .unwrap_or(0);
        let next = choices[(at + 1) % choices.len()].clone();
        self.status = t!("status.oc_engine_set", name = next.as_str()).to_string();
        self.settings.oc_engine = next;
    }

    /// Open the panel on `kind`, optionally starting a text prompt, and load
    /// the first listing.
    fn open_oc(&mut self, kind: Kind, input: Option<(Purpose, String)>) {
        let Some(d) = self.oc_detect() else {
            return;
        };
        if kind == Kind::Pod && !d.engine.has_pods() {
            self.status = t!("status.oc_no_pods", name = d.engine.binary()).to_string();
            return;
        }
        let mut panel = Panel::new(d.engine, kind, self.oc_read_only());
        if let Some((purpose, prefill)) = input {
            if panel.read_only {
                self.status = t!("status.oc_read_only").to_string();
                return;
            }
            panel.start_input(purpose, &prefill, "");
        }
        self.oc = Some(panel);
        self.oc_reload();
    }

    /// Fetch the current kind's list again.
    fn oc_reload(&mut self) {
        let Some(p) = self.oc.as_ref() else {
            return;
        };
        let (engine, kind) = (p.engine, p.kind);
        let args = argv::list(kind, p.show_all);
        let parsed = App::oc_run(engine, &args, None).and_then(|out| match kind {
            Kind::Container => parse::containers(&out).map(Rows::Containers),
            Kind::Image => parse::images(&out).map(Rows::Images),
            Kind::Volume => parse::volumes(&out).map(Rows::Volumes),
            Kind::Network => parse::networks(&out).map(Rows::Networks),
            _ => parse::pods(&out).map(Rows::Pods),
        });
        match parsed {
            Ok(rows) => {
                if let Some(p) = self.oc.as_mut() {
                    p.set_rows(rows);
                }
            }
            Err(e) => self
                .messages
                .error(t!("msg.oc_failed", error = error_text(&e)).to_string()),
        }
    }

    /// **OC → Compose**: Up and Down open the panel on the exact command and
    /// wait for a yes; Logs streams straight to the dock. The compose file is
    /// found in the workspace root, or left to the engine's own search.
    fn oc_compose(&mut self, op: &ComposeOp) {
        let Some(d) = self.oc_detect() else {
            return;
        };
        let file = COMPOSE_FILES
            .iter()
            .find(|f| crate::authority::current().is_file(&self.root.join(f)))
            .copied();
        if file.is_none() {
            self.status = t!("status.oc_no_compose").to_string();
            return;
        }
        let pending = match crate::oci_panel::compose_pending(d.engine, op, file) {
            Ok(p) => p,
            Err(e) => {
                self.status = t!("msg.oc_invalid", error = error_text(&e)).to_string();
                return;
            }
        };
        if matches!(op, ComposeOp::Logs { .. }) {
            self.oc_stream(d.engine, &pending);
            return;
        }
        if self.oc_read_only() {
            self.status = t!("status.oc_read_only").to_string();
            return;
        }
        let mut panel = Panel::new(d.engine, Kind::Container, false);
        panel.mode = Mode::Confirm(pending);
        self.oc = Some(panel);
        self.oc_reload();
    }

    /// Stream a command's output into the bottom dock.
    fn oc_stream(&mut self, engine: Engine, pending: &Pending) {
        let args: Vec<&str> = pending.args.iter().map(String::as_str).collect();
        self.run_argv(engine.binary(), &args);
    }

    /// Carry out a confirmed command: stream it to the dock and close the
    /// panel, or run it to completion, report, and refresh the list.
    fn oc_exec(&mut self, pending: &Pending) {
        let Some(engine) = self.oc.as_ref().map(|p| p.engine) else {
            return;
        };
        if pending.stream {
            self.oc = None;
            self.oc_stream(engine, pending);
            return;
        }
        match App::oc_run(engine, &pending.args, None) {
            Ok(_) => {
                self.status = t!("status.oc_done", command = pending.rendered.as_str()).to_string();
                self.oc_reload();
            }
            Err(e) => self.messages.error(
                t!(
                    "msg.oc_failed",
                    error = format!("{}: {}", pending.rendered, error_text(&e))
                )
                .to_string(),
            ),
        }
    }

    /// Show `inspect` output for an object in a read-only tab.
    fn oc_inspect(&mut self, kind: Kind, id: &str) {
        let Some(engine) = self.oc.as_ref().map(|p| p.engine) else {
            return;
        };
        let result = argv::inspect(kind, id).and_then(|args| App::oc_run(engine, &args, None));
        match result {
            Ok(out) => {
                self.oc = None;
                self.open_readonly_text_tab(&format!("{id} inspect"), &parse::pretty(&out));
            }
            Err(e) => self
                .messages
                .error(t!("msg.oc_failed", error = error_text(&e)).to_string()),
        }
    }

    /// Stream a container's recent logs into the dock.
    fn oc_logs(&mut self, id: &str) {
        let Some(engine) = self.oc.as_ref().map(|p| p.engine) else {
            return;
        };
        match argv::logs(id, 500) {
            Ok(args) => {
                self.oc = None;
                self.oc_stream(engine, &Pending::new(engine.binary(), args, true));
            }
            Err(e) => self.status = t!("msg.oc_invalid", error = error_text(&e)).to_string(),
        }
    }

    /// Open `exec -it <container> sh` in the integrated terminal.
    #[cfg(feature = "terminal")]
    fn oc_shell(&mut self, id: &str) {
        let Some(engine) = self.oc.as_ref().map(|p| p.engine) else {
            return;
        };
        let Some(program) = crate::fileops::which_on_path(engine.binary()) else {
            self.messages.error(t!("msg.oc_not_installed").to_string());
            return;
        };
        let args = match argv::exec_shell(id) {
            Ok(a) => a,
            Err(e) => {
                self.status = t!("msg.oc_invalid", error = error_text(&e)).to_string();
                return;
            }
        };
        let area = self.layout.editor;
        match crate::terminal::Terminal::open_command(
            &program.to_string_lossy(),
            &args,
            &self.root,
            area.height.max(1),
            area.width.max(1),
        ) {
            Ok(term) => {
                self.oc = None;
                self.terminal = Some(term);
            }
            Err(e) => self
                .messages
                .error(t!("msg.command_failed", error = e).to_string()),
        }
    }

    /// Without the integrated terminal there is no shell tab.
    #[cfg(not(feature = "terminal"))]
    fn oc_shell(&mut self, _id: &str) {
        self.status = t!("msg.capability_missing", name = "terminal").to_string();
    }

    /// A key while the OC panel is open.
    pub(super) fn oc_key(&mut self, key: KeyEvent) {
        let page = (self.layout.oc.height as usize).max(1);
        let Some(request) = self.oc.as_mut().map(|p| p.key(key, page)) else {
            return;
        };
        self.oc_request(request);
    }

    /// Carry out what the panel asked for.
    fn oc_request(&mut self, request: Request) {
        match request {
            Request::Close => self.oc = None,
            Request::Reload => self.oc_reload(),
            Request::Inspect(kind, id) => self.oc_inspect(kind, &id),
            Request::Logs(id) => self.oc_logs(&id),
            Request::Shell(id) => self.oc_shell(&id),
            Request::PortUrl(port) => {
                let url = format!("http://localhost:{port}");
                if let Some(t) = self.editor.active_tab_mut() {
                    t.editor.copy_to_clipboard(&url);
                } else {
                    vix_clipboard::set(&url).ok();
                }
                self.status = t!("status.oc_url_copied", url = url).to_string();
            }
            Request::Exec(pending) => self.oc_exec(&pending),
            Request::ReadOnly => self.status = t!("status.oc_read_only").to_string(),
            Request::Invalid(why) => {
                self.status = t!("msg.oc_invalid", error = why).to_string();
            }
            _ => {}
        }
    }

    /// A click selects a row; a click on the highlighted row inspects it.
    pub(super) fn oc_mouse(&mut self, mouse: MouseEvent) {
        if !matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return;
        }
        let r = self.layout.oc;
        if !rect_contains(r, mouse.column, mouse.row) {
            return;
        }
        let row = (mouse.row - r.y) as usize;
        let Some(p) = self.oc.as_mut() else {
            return;
        };
        let idx = p.scroll + row;
        let again = p.selected == idx;
        if p.select_index(idx)
            && again
            && matches!(p.mode, Mode::Browse)
            && let Some(id) = p.selected_ref()
        {
            let kind = p.kind;
            self.oc_inspect(kind, &id);
        }
    }
}
