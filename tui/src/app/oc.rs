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
//! Container workspaces (T718: **Open Workspace in Container…**, **Reopen in
//! Dev Container**) are in the `workspace` submodule.
//!
//! Long or chatty commands (logs, builds, pulls, pushes, runs, Compose) stream
//! into the bottom dock through the same no-shell runner Git and Jujutsu use;
//! short ones run to completion and report in the status line.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

use crate::geo::Geo;
use vix_input::{KeyEvent, MouseEvent};

use crate::oci::argv::{self, ComposeOp, Kind};
use crate::oci::{Engine, OciError, parse};
use crate::oci_panel::{BUILD_FILES, COMPOSE_FILES, Mode, Panel, Pending, Purpose, Request, Rows};

use super::App;

mod workspace;

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
            "oc.open_workspace" => self.oc_open_workspace(),
            "oc.devcontainer_reopen" => self.oc_devcontainer_reopen(),
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
        let Some(next) = choices
            .get(vix_list_state::wrap_down(at, choices.len()))
            .cloned()
        else {
            return;
        };
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
        let (rows, cols) = self.terminal_start_size();
        match crate::terminal::Terminal::open_command(
            &program,
            &args,
            &self.root,
            rows,
            cols,
            self.settings.terminal_scrollback,
        ) {
            Ok(term) => {
                self.oc = None;
                // A new tab beside any shells already open (T631).
                self.terminals.push(term);
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
        let page = (usize::from(self.layout.oc.height)).max(1);
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
            Request::OpenWorkspace(name) => self.oc_workspace_prompt(&name),
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
        let r = self.layout.oc;
        let Some(row) = crate::app::hit::left_click_row(r, mouse) else {
            return;
        };
        let Some(p) = self.oc.as_mut() else {
            return;
        };
        let idx = p.scroll.plus(row);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::cov4::*;
    use crate::oci::parse::Container;
    use ratatui::layout::Rect;
    use vix_input::KeyCode;

    fn container(id: &str) -> Container {
        Container {
            id: id.to_string(),
            name: format!("name-{id}"),
            image: "img".into(),
            state: "running".into(),
            status: "Up".into(),
            ports: String::new(),
            host_ports: Vec::new(),
            created: String::new(),
        }
    }

    /// An app with an OC panel already open on two containers, so no engine
    /// has to be found or run.
    fn with_panel(tag: &str, read_only: bool) -> App {
        let mut app = app_at(&temp(tag));
        let mut p = Panel::new(Engine::Docker, Kind::Container, read_only);
        p.set_rows(Rows::Containers(vec![container("aaa"), container("bbb")]));
        app.oc = Some(p);
        app
    }

    #[test]
    fn engine_errors_carry_the_install_hint_only_when_nothing_is_installed() {
        assert_eq!(
            error_text(&OciError::NotInstalled),
            t!("msg.oc_not_installed").to_string()
        );
        let other = OciError::Parse("bad json".into());
        assert_eq!(error_text(&other), other.to_string());
    }

    #[test]
    fn unknown_actions_are_not_handled_and_known_ones_are() {
        let mut app = app_at(&temp("oc-actions"));
        assert!(!app.run_oc_action("oc.no_such_thing"));
        assert!(app.run_oc_action("oc.engine_detect"));
    }

    #[test]
    fn detect_reports_the_engine_found_or_the_install_hint() {
        let mut app = app_at(&temp("oc-detect"));
        app.settings.oc_engine = "auto".into();
        let found = Engine::ALL
            .into_iter()
            .find_map(|e| crate::fileops::which_on_path(e.binary()).map(|p| (e, p)));
        app.run_oc_action("oc.engine_detect");
        match found {
            Some((engine, path)) => assert_eq!(
                app.status,
                t!(
                    "status.oc_engine",
                    name = engine.binary(),
                    path = path.display().to_string()
                )
                .to_string()
            ),
            None => assert_eq!(
                last_message(&app),
                Some(t!("msg.oc_not_installed").to_string())
            ),
        }
    }

    #[test]
    fn choosing_an_engine_steps_the_setting_through_auto_and_the_installed_ones() {
        let mut app = app_at(&temp("oc-choose"));
        let mut choices = vec!["auto".to_string()];
        choices.extend(
            Engine::ALL
                .into_iter()
                .filter(|e| crate::fileops::which_on_path(e.binary()).is_some())
                .map(|e| e.binary().to_string()),
        );
        app.settings.oc_engine = "auto".into();
        for step in 1..=choices.len() {
            app.run_oc_action("oc.engine_choose");
            let expected = choices.get(step % choices.len()).unwrap().clone();
            assert_eq!(app.settings.oc_engine, expected);
            assert_eq!(
                app.status,
                t!("status.oc_engine_set", name = expected.as_str()).to_string()
            );
        }
        // An engine that is not installed starts again from `auto`.
        app.settings.oc_engine = "not-an-engine".into();
        app.run_oc_action("oc.engine_choose");
        assert_eq!(
            app.settings.oc_engine,
            choices.get(1 % choices.len()).unwrap().as_str()
        );
    }

    #[test]
    fn only_the_full_mode_allows_changes() {
        let mut app = app_at(&temp("oc-mode"));
        app.settings.oc_mode = "full".into();
        assert!(!app.oc_read_only());
        for mode in ["read-only", "ful", ""] {
            app.settings.oc_mode = mode.into();
            assert!(app.oc_read_only(), "{mode:?}");
        }
    }

    #[test]
    fn the_panel_closes_on_escape_and_ignores_keys_when_absent() {
        let mut app = with_panel("oc-keys", true);
        app.oc_key(code(KeyCode::Down));
        assert_eq!(app.oc.as_ref().unwrap().selected, 1);
        app.oc_key(code(KeyCode::Esc));
        assert!(app.oc.is_none());
        app.oc_key(code(KeyCode::Esc)); // no panel: nothing
    }

    #[test]
    fn requests_close_report_read_only_and_refused_arguments() {
        let mut app = with_panel("oc-requests", true);
        app.oc_request(Request::None);
        assert!(app.oc.is_some());
        app.oc_request(Request::ReadOnly);
        assert_eq!(app.status, t!("status.oc_read_only").to_string());
        app.oc_request(Request::Invalid("bad id".into()));
        assert_eq!(
            app.status,
            t!("msg.oc_invalid", error = "bad id").to_string()
        );
        app.oc_request(Request::Close);
        assert!(app.oc.is_none());
    }

    #[test]
    fn a_port_request_copies_a_localhost_url() {
        let _clip = CLIP
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut app = with_panel("oc-port", true);
        app.oc_request(Request::PortUrl(8080));
        assert_eq!(
            app.status,
            t!("status.oc_url_copied", url = "http://localhost:8080").to_string()
        );
    }

    #[test]
    fn refused_ids_are_reported_before_any_engine_is_run() {
        let mut app = with_panel("oc-invalid", true);
        app.oc_request(Request::Inspect(Kind::Container, "-bad".into()));
        let m = last_message(&app).unwrap();
        assert!(
            m.starts_with(t!("msg.oc_failed", error = "").to_string().trim_end()),
            "{m}"
        );
        assert!(app.oc.is_some(), "a failed inspect keeps the panel open");
        app.oc_request(Request::Logs("-bad".into()));
        assert!(
            app.status
                .starts_with(t!("msg.oc_invalid", error = "").to_string().trim_end())
        );
        assert!(app.oc.is_some());
        // With no panel these requests do nothing.
        app.oc = None;
        app.status.clear();
        app.oc_request(Request::Logs("abc".into()));
        app.oc_request(Request::Inspect(Kind::Container, "abc".into()));
        app.oc_request(Request::Shell("abc".into()));
        app.oc_request(Request::Exec(Pending::new(
            "docker",
            vec!["ps".into()],
            false,
        )));
        assert_eq!(app.status, "");
    }

    #[test]
    fn clicking_a_row_selects_it_and_clicks_outside_the_panel_do_nothing() {
        let mut app = with_panel("oc-mouse", true);
        app.layout.oc = Rect::new(0, 0, 40, 10);
        app.oc_mouse(click(1, 1));
        assert_eq!(app.oc.as_ref().unwrap().selected, 1);
        app.oc_mouse(click(1, 9)); // past the last row: selection unchanged
        assert_eq!(app.oc.as_ref().unwrap().selected, 1);
        app.oc_mouse(click(60, 40)); // outside the panel
        assert_eq!(app.oc.as_ref().unwrap().selected, 1);
        app.oc = None;
        app.oc_mouse(click(1, 1));
    }
}
