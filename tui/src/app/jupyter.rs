//! The `jupyter` capability (T734c–T734g, `spec/jupyter`): Jupyter kernels
//! started from their kernelspecs, code sent to them from the editor, and
//! their output streamed into the bottom dock, an Org block's `#+RESULTS:`
//! or an image tab.
//!
//! - **Kernels.** One per language per workspace, held here as
//!   [`vix_jupyter::KernelHandle`]s. A kernel starts only on an explicit
//!   action (**Start Kernel…**, or **Run** when the language has none yet),
//!   never on open, restore or save, and never for a remote workspace: the
//!   code would run on this computer. [`App::poll_jupyter`] drains each
//!   kernel's events on the tick.
//! - **Run targets** (T734d), in order: the selection; the Org `src` block
//!   at the cursor; the `# %%` cell (the language's line comment, then
//!   `%%`); the current line.
//! - **Output.** Every output of a request goes to the bottom dock as plain
//!   text with ANSI codes removed, under a `[*] <kernel>` header, followed by
//!   a `[n] <kernel>: done/failed` line when the reply arrives. The last
//!   output's first line also shows as an end-of-line hint after the code.
//!   An image opens in an image tab (T734g, [`results`]).
//! - **Org babel** (T734e). An Org `src` block's run (also `C-c C-c` in
//!   the block) writes its `#+RESULTS:` when the reply arrives, following
//!   the block's `:results`, `:file` and `:eval` header arguments.
//! - **Trust** (T734f). An Org block asks before it runs as the
//!   `jupyter_confirm_evaluate` setting says (by default: its first run in a
//!   session, unless the workspace is trusted; always for a file outside
//!   the workspace), and the answer "always in this workspace" is
//!   persisted. Kernelspecs inside the workspace are used only when it is
//!   trusted. A selection, cell or line is something the user sends on
//!   purpose and is never prompted.

mod results;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use vix_input::{KeyCode, KeyEvent};
use vix_jupyter::core::ansi::strip_ansi;
use vix_jupyter::core::babel::{self, Eval, Handling, Header};
use vix_jupyter::core::cell;
use vix_jupyter::core::kernelspec::{self, normalize_language};
use vix_jupyter::core::message::ReplyStatus;
use vix_jupyter::core::{ConnectionInfo, KernelSpec, Output, Payload};
use vix_jupyter::{DeathReason, KernelEvent, KernelHandle, KernelState, LaunchOptions, RequestId};

use super::ide_parity::{ChoiceList, ChoicePurpose};
use super::{App, Confirm, ConfirmKind, Visible};

/// The longest end-of-line result hint, in characters.
const HINT_CHARS: usize = 60;
/// How many lines of an Org block the trust confirmation quotes.
const CONFIRM_CODE_LINES: usize = 3;
/// How many characters of the code the dock header quotes.
const HEADER_CODE_CHARS: usize = 60;
/// The most output bytes an Org run keeps for its `#+RESULTS:` (a hostile
/// or runaway kernel cannot grow memory without bound); later outputs still
/// reach the dock, which is capped on its own.
const MAX_ORG_OUTPUT_BYTES: usize = 4 * 1024 * 1024;

/// Every running kernel, and what the App needs to route their output.
#[derive(Default)]
pub(crate) struct State {
    /// Running kernels, oldest first.
    kernels: Vec<Kernel>,
    /// The kernelspec name of the kernel that ran code last.
    last: Option<String>,
    /// Discover kernelspecs only in these `kernels` directories (tests).
    kernel_dirs: Option<Vec<PathBuf>>,
    /// Kernelspec name → a running local kernel to attach to instead of
    /// launching the spec's argv (tests, with an in-process fake kernel).
    connect: BTreeMap<String, ConnectionInfo>,
    /// Org blocks confirmed this session: `(file, code)`.
    approved: HashSet<(Option<PathBuf>, String)>,
    /// The Org block run waiting for the trust confirmation.
    pending_block: Option<Run>,
    /// Whether that confirmation offers "always in this workspace".
    pending_always: bool,
    /// Where images opened in a tab are written (T734g), made on first
    /// use and removed with the App.
    #[cfg(feature = "image")]
    images: Option<vix_jupyter::PrivateDir>,
}

/// The kernel picker's rows, and the run waiting for the chosen kernel.
#[derive(Debug, Clone)]
pub(crate) struct Pick {
    /// One kernelspec per row.
    specs: Vec<KernelSpec>,
    /// The run to send once the kernel starts (`None` for Start Kernel…).
    run: Option<Run>,
}

/// Code to send, and where its result hint goes.
#[derive(Debug, Clone)]
pub(crate) struct Run {
    /// The code.
    code: String,
    /// Its language, as written (`python`, `jupyter-python`, …).
    language: String,
    /// The buffer's file, if any.
    path: Option<PathBuf>,
    /// The buffer's tab index when the run was asked for.
    tab: usize,
    /// The 0-based line the end-of-line hint goes on.
    hint_line: usize,
    /// Run and Advance: the line to move the cursor to.
    advance_to: Option<usize>,
    /// The Org `src` block the code is (trust-gated; writes `#+RESULTS:`).
    org: Option<OrgRun>,
}

/// An Org `src` block run: where the block starts and its header arguments.
#[derive(Debug, Clone)]
pub(crate) struct OrgRun {
    /// The `#+begin_src` line (0-based) when the run was asked for.
    begin: usize,
    /// Its header arguments.
    header: Header,
}

impl OrgRun {
    /// Whether the run writes `#+RESULTS:` (anything but `:results silent`).
    fn writes(&self) -> bool {
        self.header.handling != Handling::Silent
    }
}

/// One running kernel.
struct Kernel {
    /// The kernelspec it was started from.
    spec: KernelSpec,
    /// The normalized languages it serves.
    languages: Vec<String>,
    /// The transport.
    handle: KernelHandle,
    /// Its last reported state.
    state: KernelState,
    /// Its last execution count.
    execution_count: Option<u64>,
    /// Requests still waiting for their reply.
    runs: HashMap<RequestId, InFlight>,
}

/// A request in flight: where its hint or results go, and what came back.
struct InFlight {
    /// The buffer's file, if any.
    path: Option<PathBuf>,
    /// The buffer's tab index at send time.
    tab: usize,
    /// The hint's line.
    line: usize,
    /// The last output's first line.
    hint: Option<String>,
    /// The code sent (to find an Org block again when the reply arrives).
    code: String,
    /// The Org block, when the run is one.
    org: Option<OrgRun>,
    /// The outputs so far, kept only for an Org run that writes results.
    outputs: Vec<Output>,
    /// The bytes of text and data in `outputs`.
    kept: usize,
}

/// Kernel controls that act on the current kernel.
#[derive(Clone, Copy)]
enum Control {
    /// `jupyter.interrupt`.
    Interrupt,
    /// `jupyter.restart`.
    Restart,
    /// `jupyter.shutdown`.
    Shutdown,
}

/// The code at the cursor and where it sits.
struct Target {
    /// The code.
    code: String,
    /// Its language.
    language: String,
    /// The last line of the code (or the `#+end_src` line).
    last: usize,
    /// The Org `src` block it is, if any.
    org: Option<OrgRun>,
}

impl App {
    /// Dispatch the `jupyter.*` actions. Returns `true` if handled.
    pub(super) fn run_jupyter_action(&mut self, action: &str) -> bool {
        match action {
            "jupyter.start" => self.jupyter_start_picker(),
            "jupyter.run" => self.jupyter_run(false),
            "jupyter.run_advance" => self.jupyter_run(true),
            "jupyter.interrupt" => self.jupyter_control(Control::Interrupt),
            "jupyter.restart" => self.jupyter_control(Control::Restart),
            "jupyter.shutdown" => self.jupyter_control(Control::Shutdown),
            "jupyter.clear_results" => self.jupyter_clear_results(),
            _ => return false,
        }
        true
    }

    // ----- test hooks --------------------------------------------------------

    /// Look for kernelspecs only in `dirs` (each a Jupyter `kernels`
    /// directory) instead of Jupyter's standard data directories. Tests use
    /// this with a temporary directory, so the environment is never edited.
    #[must_use]
    pub fn with_jupyter_kernel_dirs(mut self, dirs: Vec<PathBuf>) -> Self {
        self.jupyter.kernel_dirs = Some(dirs);
        self
    }

    /// Starting the kernelspec named `spec` attaches to the running local
    /// kernel `info` instead of launching the spec's argv. Tests use this
    /// with an in-process fake kernel; it starts nothing by itself.
    #[must_use]
    pub fn with_jupyter_connection(mut self, spec: &str, info: ConnectionInfo) -> Self {
        self.jupyter.connect.insert(spec.to_owned(), info);
        self
    }

    /// Each running kernel's kernelspec name and last reported state.
    #[must_use]
    pub fn jupyter_kernels(&self) -> Vec<(String, KernelState)> {
        self.jupyter
            .kernels
            .iter()
            .map(|k| (k.spec.name.clone(), k.state))
            .collect()
    }

    /// How many kernelspecs the open kernel picker lists, or `None` when it
    /// is not open.
    #[must_use]
    pub fn jupyter_picker_rows(&self) -> Option<usize> {
        match &self.choice_list.as_ref()?.purpose {
            ChoicePurpose::Jupyter(pick) => Some(pick.specs.len()),
            _ => None,
        }
    }

    /// Whether the Org-block trust confirmation is open.
    #[must_use]
    pub fn jupyter_confirm_open(&self) -> bool {
        self.confirm.as_ref().map(|c| c.kind) == Some(ConfirmKind::JupyterRunBlock)
    }

    // ----- shared entry points -------------------------------------------------

    /// Drain every kernel's events: outputs to the dock, state to the
    /// status bar, and a finished kernel out of the list.
    pub fn poll_jupyter(&mut self) {
        let mut events = Vec::new();
        for (i, kernel) in self.jupyter.kernels.iter().enumerate() {
            while let Ok(event) = kernel.handle.events().try_recv() {
                events.push((i, event));
            }
        }
        let mut dead = Vec::new();
        for (i, event) in events {
            if self.jupyter_event(i, event) {
                dead.push(i);
            }
        }
        for i in dead.into_iter().rev() {
            self.jupyter.kernels.remove(i);
        }
    }

    /// Whether a kernel is starting or running code (poll faster).
    #[must_use]
    pub fn jupyter_busy(&self) -> bool {
        self.jupyter.kernels.iter().any(|k| {
            !k.runs.is_empty()
                || matches!(
                    k.state,
                    KernelState::Starting | KernelState::Busy | KernelState::Restarting
                )
        })
    }

    /// The status-bar segment for the current kernel: its kernelspec name,
    /// state and execution count, or nothing when none runs.
    #[must_use]
    pub fn jupyter_segment(&self) -> String {
        let Some(kernel) = self.jupyter_current().map(|i| &self.jupyter.kernels[i]) else {
            return String::new();
        };
        let state = match kernel.state {
            KernelState::Starting => t!("status.jupyter_state_starting"),
            KernelState::Busy => t!("status.jupyter_state_busy"),
            KernelState::Restarting => t!("status.jupyter_state_restarting"),
            KernelState::Dead => t!("status.jupyter_state_dead"),
            _ => t!("status.jupyter_state_idle"),
        };
        match kernel.execution_count {
            Some(n) => format!("{} {state} [{n}]   ", kernel.spec.name),
            None => format!("{} {state}   ", kernel.spec.name),
        }
    }

    /// Keys for the Org-block trust confirmation. Returns whether it was open.
    pub(super) fn jupyter_confirm_key(&mut self, key: KeyEvent) -> bool {
        if self.confirm.as_ref().map(|c| c.kind) != Some(ConfirmKind::JupyterRunBlock) {
            return false;
        }
        match key.code {
            KeyCode::Char('y' | 'Y') => self.jupyter_confirmed(false),
            KeyCode::Char('a' | 'A') if self.jupyter.pending_always => {
                self.jupyter_confirmed(true);
            }
            KeyCode::Char('n' | 'N') | KeyCode::Esc => {
                self.confirm = None;
                self.jupyter.pending_block = None;
                self.status = t!("status.jupyter_run_cancelled").to_string();
            }
            _ => {}
        }
        true
    }

    /// Org `C-c C-c` (T734e): in an Org `src` block, evaluate the block
    /// (even with a selection) and return `true`; elsewhere `false`, so
    /// `C-c C-c` does what it does outside a block.
    pub(super) fn jupyter_org_ctrl_c(&mut self) -> bool {
        let Some(target) = self.jupyter_block_target() else {
            return false;
        };
        if self.jupyter_remote_refused() {
            return true;
        }
        if target.code.trim().is_empty() {
            self.status = t!("status.jupyter_nothing_to_run").to_string();
        } else {
            self.jupyter_run_target(target, false);
        }
        true
    }

    /// The picker's row `row` was chosen: start that kernel, then send the
    /// waiting run, if any.
    pub(super) fn jupyter_picked(&mut self, pick: Pick, row: usize) {
        let Some(spec) = pick.specs.into_iter().nth(row) else {
            return;
        };
        let alias = pick.run.as_ref().map(|r| normalize_language(&r.language));
        let Some(i) = self.jupyter_start(spec, alias.as_deref()) else {
            return;
        };
        if let Some(run) = pick.run {
            self.jupyter_execute(i, run);
        }
    }

    // ----- actions -----------------------------------------------------------

    /// Refuse when the workspace is remote: a kernel runs on this computer.
    fn jupyter_remote_refused(&mut self) -> bool {
        if crate::authority::current().is_local() {
            return false;
        }
        self.messages.warn(t!("msg.jupyter_remote").to_string());
        true
    }

    /// **Start Kernel…**: the picker, with nothing to run afterwards.
    fn jupyter_start_picker(&mut self) {
        if !self.jupyter_remote_refused() {
            self.jupyter_open_picker(None);
        }
    }

    /// **Run** / **Run and Advance**: send the code at the cursor to its
    /// language's kernel, starting one if needed.
    fn jupyter_run(&mut self, advance: bool) {
        if self.jupyter_remote_refused() {
            return;
        }
        let Some(target) = self.jupyter_target() else {
            self.status = t!("status.jupyter_nothing_to_run").to_string();
            return;
        };
        self.jupyter_run_target(target, advance);
    }

    /// Run `target`: refuse an Org block with `:eval no`/`never`, ask first
    /// when the trust rules say so, else send it.
    fn jupyter_run_target(&mut self, target: Target, advance: bool) {
        if let Some(org) = &target.org
            && let Eval::Never(value) = &org.header.eval
        {
            self.status = t!("status.jupyter_eval_never", value = value).to_string();
            return;
        }
        let run = Run {
            path: self.editor.active_tab().and_then(|t| t.path.clone()),
            tab: self.editor.active,
            hint_line: target.last,
            advance_to: advance.then_some(target.last + 1),
            org: target.org,
            code: target.code,
            language: target.language,
        };
        match self.jupyter_ask(&run) {
            Some(offer_always) => self.jupyter_confirm_block(run, offer_always),
            None => self.jupyter_dispatch(run),
        }
    }

    /// Whether `run` asks before it runs (T734f), and if so whether the
    /// question offers "always in this workspace". `None` runs at once.
    /// Only an Org block ever asks: `:eval query` always; otherwise as
    /// `jupyter_confirm_evaluate` says (`untrusted`, the default: the first
    /// run of each block in a session unless the workspace is trusted, and
    /// always for a file outside the workspace; `always`; `never`).
    fn jupyter_ask(&self, run: &Run) -> Option<bool> {
        let org = run.org.as_ref()?;
        if org.header.eval == Eval::Query {
            return Some(false);
        }
        match self.settings.jupyter_confirm_evaluate.as_str() {
            "never" => None,
            "always" => Some(false),
            _ => {
                let approved = self
                    .jupyter
                    .approved
                    .contains(&(run.path.clone(), run.code.clone()));
                if approved {
                    None
                } else if !self.jupyter_in_workspace(run.path.as_deref()) {
                    Some(false)
                } else {
                    (!self.jupyter_trusted()).then_some(true)
                }
            }
        }
    }

    /// Whether `path` (a buffer's file; `None` for an unsaved buffer) is
    /// inside the workspace root.
    fn jupyter_in_workspace(&self, path: Option<&Path>) -> bool {
        let Some(path) = path else {
            return true;
        };
        let authority = crate::authority::current();
        let root = authority
            .canonicalize(&self.root)
            .unwrap_or_else(|_| self.root.clone());
        let path = authority
            .canonicalize(path)
            .unwrap_or_else(|_| path.to_path_buf());
        path.starts_with(&root)
    }

    /// Whether the workspace is trusted for Jupyter: the persisted T132
    /// script-trust decision, or an earlier "always in this workspace"
    /// answer to the run confirmation.
    fn jupyter_trusted(&self) -> bool {
        self.workspace_trusted()
            || (!self.remote_untrusted()
                && self
                    .load_session()
                    .workspace(&self.session_key())
                    .is_some_and(|w| w.jupyter_trusted == Some(true)))
    }

    /// Ask before running an Org block.
    fn jupyter_confirm_block(&mut self, run: Run, offer_always: bool) {
        let quoted: Vec<&str> = run.code.lines().take(CONFIRM_CODE_LINES).collect();
        let code = quoted.join(" ⏎ ");
        let message = if offer_always {
            t!(
                "confirm.jupyter_block_always",
                language = run.language,
                code = code
            )
        } else {
            t!(
                "confirm.jupyter_block",
                language = run.language,
                code = code
            )
        };
        self.confirm = Some(Confirm {
            message: message.to_string(),
            paths: Vec::new(),
            kind: ConfirmKind::JupyterRunBlock,
        });
        self.jupyter.pending_block = Some(run);
        self.jupyter.pending_always = offer_always;
    }

    /// The confirmation was answered yes (`always`: for every block in this
    /// workspace, persisted): run the waiting block.
    fn jupyter_confirmed(&mut self, always: bool) {
        self.confirm = None;
        if always {
            let key = self.session_key();
            let mut session = self.load_session();
            session.set_jupyter_trusted(&key, Some(true));
            self.store_session_or_warn(&session);
            self.messages
                .info(t!("msg.jupyter_trusted_always").to_string());
        }
        if let Some(run) = self.jupyter.pending_block.take() {
            self.jupyter
                .approved
                .insert((run.path.clone(), run.code.clone()));
            self.jupyter_dispatch(run);
        }
    }

    /// Send `run` to its language's kernel: the running one, else the
    /// configured or first matching kernelspec, else the picker.
    fn jupyter_dispatch(&mut self, run: Run) {
        if let Some(line) = run.advance_to
            && let Some(tab) = self.editor.tabs.get_mut(run.tab)
        {
            tab.editor.set_cursor_line(line);
        }
        let language = normalize_language(&run.language);
        if let Some(i) = self.jupyter_kernel_for(&language) {
            self.jupyter_execute(i, run);
            return;
        }
        let specs = self.jupyter_specs();
        let configured = self
            .settings
            .jupyter_kernels
            .iter()
            .find(|(lang, _)| normalize_language(lang) == language)
            .and_then(|(_, name)| specs.iter().find(|s| &s.name == name));
        let chosen = configured
            .or_else(|| specs.iter().find(|s| s.runs(&language)))
            .cloned();
        match chosen {
            Some(spec) => {
                if let Some(i) = self.jupyter_start(spec, Some(&language)) {
                    self.jupyter_execute(i, run);
                }
            }
            None => self.jupyter_open_picker(Some(run)),
        }
    }

    /// Interrupt, restart or shut down the current kernel.
    fn jupyter_control(&mut self, control: Control) {
        if matches!(control, Control::Restart) && self.jupyter_remote_refused() {
            return;
        }
        let Some(i) = self.jupyter_current() else {
            self.status = t!("status.jupyter_no_kernel").to_string();
            return;
        };
        let kernel = &self.jupyter.kernels[i];
        let name = kernel.spec.display_name.clone();
        self.status = match control {
            Control::Interrupt => {
                kernel.handle.interrupt();
                t!("status.jupyter_interrupted", name = name)
            }
            Control::Restart => {
                kernel.handle.restart();
                t!("status.jupyter_restarting", name = name)
            }
            Control::Shutdown => {
                kernel.handle.shutdown();
                t!("status.jupyter_shutting_down", name = name)
            }
        }
        .to_string();
    }

    /// **Clear Results**: remove the Org block's `#+RESULTS:` at the cursor
    /// (one undo step) and the end-of-line result hint from the active
    /// buffer.
    fn jupyter_clear_results(&mut self) {
        self.jupyter_clear_org_results();
        if let Some(tab) = self.editor.active_tab_mut() {
            tab.editor.set_inlay_hints(Vec::new());
        }
        self.status = t!("status.jupyter_results_cleared").to_string();
    }

    // ----- kernels -----------------------------------------------------------

    /// The installed kernelspecs. A kernelspec directory inside the
    /// workspace (such as an activated `.venv`) counts only when the
    /// workspace is trusted, which also adds its `.venv` and `venv`
    /// kernelspecs (T734f).
    fn jupyter_specs(&self) -> Vec<KernelSpec> {
        let dirs = match &self.jupyter.kernel_dirs {
            Some(dirs) => dirs.clone(),
            None => kernelspec::kernel_dirs(&vix_jupyter::discover::path_env()),
        };
        let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let dirs = dirs.iter().map(|d| canonical(d)).collect();
        let root = canonical(&self.root);
        vix_jupyter::discover::kernelspecs_in(&kernelspec::gate_project_dirs(
            dirs,
            &root,
            self.jupyter_trusted(),
        ))
    }

    /// Open the kernel picker, or say how to install a kernel when there is
    /// none. `run` is sent once the chosen kernel starts.
    fn jupyter_open_picker(&mut self, run: Option<Run>) {
        let specs = self.jupyter_specs();
        if specs.is_empty() {
            self.messages.warn(t!("msg.jupyter_no_kernels").to_string());
            return;
        }
        let rows = specs
            .iter()
            .map(|s| format!("{} — {} — {}", s.display_name, s.language, s.argv.join(" ")))
            .collect();
        self.choice_list = Some(ChoiceList {
            panel: vix_choice_list::Panel::new(
                t!("ui.jupyter_pick_title").to_string(),
                t!("ui.jupyter_pick_hint").to_string(),
                rows,
            ),
            purpose: ChoicePurpose::Jupyter(Pick { specs, run }),
        });
    }

    /// The running kernel serving normalized `language`.
    fn jupyter_kernel_for(&self, language: &str) -> Option<usize> {
        self.jupyter
            .kernels
            .iter()
            .position(|k| k.languages.iter().any(|l| l == language))
    }

    /// The kernel the controls and the status bar act on: the one for the
    /// language at the cursor, else the one that ran code last, else the
    /// first.
    fn jupyter_current(&self) -> Option<usize> {
        if self.jupyter.kernels.is_empty() {
            return None;
        }
        // The status bar asks every frame: only an Org file's text is read
        // (to find the block's language), never a large buffer's on each draw.
        let language = self.editor.active_tab().map(|tab| {
            let is_org = tab
                .path
                .as_ref()
                .and_then(|p| p.extension())
                .is_some_and(|e| e.eq_ignore_ascii_case("org"));
            is_org
                .then(|| {
                    let text = tab.editor.get_content();
                    crate::org::src_block_at(&text, tab.editor.cursor_line())
                })
                .flatten()
                .map_or_else(|| tab.editor.language().to_owned(), |(_, _, lang)| lang)
        });
        language
            .and_then(|l| self.jupyter_kernel_for(&normalize_language(&l)))
            .or_else(|| {
                let last = self.jupyter.last.as_ref()?;
                self.jupyter
                    .kernels
                    .iter()
                    .position(|k| &k.spec.name == last)
            })
            .or(Some(0))
    }

    /// Start `spec` (or attach to its test connection), serving its own
    /// language and `alias`. A kernel already running from `spec` is reused.
    fn jupyter_start(&mut self, spec: KernelSpec, alias: Option<&str>) -> Option<usize> {
        let mut languages = vec![normalize_language(&spec.language)];
        languages.extend(alias.map(str::to_owned));
        if let Some(i) = self
            .jupyter
            .kernels
            .iter()
            .position(|k| k.spec.name == spec.name)
        {
            let kernel = &mut self.jupyter.kernels[i];
            for language in languages {
                if !kernel.languages.contains(&language) {
                    kernel.languages.push(language);
                }
            }
            if alias.is_none() {
                self.status =
                    t!("status.jupyter_already_running", name = spec.display_name).to_string();
            }
            return Some(i);
        }
        let options = LaunchOptions {
            cwd: Some(self.root.clone()),
            ..LaunchOptions::default()
        };
        let started = match self.jupyter.connect.get(&spec.name) {
            Some(info) => KernelHandle::connect(info.clone(), &options),
            None => KernelHandle::launch(&spec, &options),
        };
        match started {
            Ok(handle) => {
                self.status = t!("status.jupyter_starting", name = spec.display_name).to_string();
                languages.dedup();
                self.jupyter.kernels.push(Kernel {
                    spec,
                    languages,
                    handle,
                    state: KernelState::Starting,
                    execution_count: None,
                    runs: HashMap::new(),
                });
                Some(self.jupyter.kernels.len() - 1)
            }
            Err(e) => {
                self.messages.error(
                    t!(
                        "msg.jupyter_start_failed",
                        name = spec.display_name,
                        error = e
                    )
                    .to_string(),
                );
                None
            }
        }
    }

    /// Send `run` to kernel `i` and open its dock section.
    fn jupyter_execute(&mut self, i: usize, run: Run) {
        let kernel = &mut self.jupyter.kernels[i];
        let request = kernel.handle.execute(&run.code);
        let name = kernel.spec.display_name.clone();
        kernel.runs.insert(
            request,
            InFlight {
                path: run.path,
                tab: run.tab,
                line: run.hint_line,
                hint: None,
                code: run.code.clone(),
                org: run.org,
                outputs: Vec::new(),
                kept: 0,
            },
        );
        self.jupyter.last = Some(kernel.spec.name.clone());
        self.visible.set(Visible::BOTTOM_DOCK, true);
        self.settings.panels.show_bottom_dock = true;
        self.bottom_dock.push(format!("[*] {name}"));
        let first = run
            .code
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("");
        self.bottom_dock
            .push(format!("  » {}", clip(first.trim_end(), HEADER_CODE_CHARS)));
    }

    // ----- events --------------------------------------------------------------

    /// Apply one event from kernel `i`. Returns `true` when the kernel is
    /// gone and should be dropped.
    fn jupyter_event(&mut self, i: usize, event: KernelEvent) -> bool {
        let name = self.jupyter.kernels[i].spec.display_name.clone();
        match event {
            KernelEvent::State(state) => self.jupyter.kernels[i].state = state,
            KernelEvent::Ready(_) => {
                // Ready accepts requests; IOPub's own `status` may not follow.
                let kernel = &mut self.jupyter.kernels[i];
                if matches!(
                    kernel.state,
                    KernelState::Starting | KernelState::Restarting
                ) {
                    kernel.state = KernelState::Idle;
                }
                self.status = t!("status.jupyter_ready", name = name).to_string();
            }
            KernelEvent::Output { request, output } => self.jupyter_output(i, &request, output),
            KernelEvent::Reply {
                request,
                status,
                execution_count,
            } => self.jupyter_reply(i, &request, status, execution_count),
            KernelEvent::Restarted => {
                let kernel = &mut self.jupyter.kernels[i];
                kernel.execution_count = None;
                kernel.runs.clear();
                self.bottom_dock
                    .push(t!("ui.jupyter_restarted", name = name).to_string());
            }
            KernelEvent::Dead(reason) => {
                self.jupyter_died(&name, reason);
                return true;
            }
            // A message that failed its signature or size check: the
            // transport already dropped it.
            _ => {}
        }
        false
    }

    /// One output of kernel `i`: its dock lines, an image shown (or kept
    /// for an Org block's results), and the hint so far.
    fn jupyter_output(&mut self, i: usize, request: &RequestId, output: Output) {
        let mut lines = output_lines(&output);
        let (ours, writes_org) = self.jupyter.kernels[i]
            .runs
            .get(request)
            .map_or((false, false), |r| {
                (true, r.org.as_ref().is_some_and(OrgRun::writes))
            });
        let hint = lines
            .iter()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_owned());
        if let Some((mime, b64)) = babel::first_image(std::slice::from_ref(&output)) {
            // An Org block's image is saved with its results; a kernel's
            // image nobody asked for is not opened.
            if !writes_org {
                let note = if ours {
                    self.jupyter_show_image(mime, &b64)
                } else {
                    t!("ui.jupyter_image_placeholder", mime = mime).to_string()
                };
                lines.insert(0, note);
            }
        }
        if let Some(run) = self.jupyter.kernels[i].runs.get_mut(request) {
            if hint.is_some() {
                run.hint = hint;
            }
            let weight = output_weight(&output);
            if writes_org && run.kept.saturating_add(weight) <= MAX_ORG_OUTPUT_BYTES {
                run.kept += weight;
                run.outputs.push(output);
            }
        }
        for line in lines {
            self.bottom_dock.push(line);
        }
    }

    /// A request finished: its dock footer, and its Org `#+RESULTS:` or
    /// end-of-line hint.
    fn jupyter_reply(
        &mut self,
        i: usize,
        request: &RequestId,
        status: ReplyStatus,
        execution_count: Option<u64>,
    ) {
        let kernel = &mut self.jupyter.kernels[i];
        if execution_count.is_some() {
            kernel.execution_count = execution_count;
        }
        let Some(run) = kernel.runs.remove(request) else {
            return;
        };
        let name = kernel.spec.display_name.clone();
        let n = execution_count.map_or_else(|| "?".to_owned(), |n| n.to_string());
        let footer = match status {
            ReplyStatus::Ok => t!("ui.jupyter_reply_ok", n = n, name = name),
            ReplyStatus::Aborted => t!("ui.jupyter_reply_aborted", n = n, name = name),
            ReplyStatus::Error | ReplyStatus::Unknown => {
                t!("ui.jupyter_reply_error", n = n, name = name)
            }
        };
        self.bottom_dock.push(footer.to_string());
        if run.org.as_ref().is_some_and(OrgRun::writes) {
            self.jupyter_write_results(&run, status);
        } else if let Some(hint) = &run.hint {
            self.jupyter_set_hint(&run, hint);
        }
    }

    /// The tab of a run's buffer, if still open: by file, or (an unsaved
    /// buffer) the same tab index if it still has no file.
    fn jupyter_tab_index(&self, path: Option<&PathBuf>, tab: usize) -> Option<usize> {
        match path {
            Some(path) => self
                .editor
                .tabs
                .iter()
                .position(|t| t.path.as_ref() == Some(path)),
            None => self
                .editor
                .tabs
                .get(tab)
                .is_some_and(|t| t.path.is_none())
                .then_some(tab),
        }
    }

    /// Show `hint` at the end of the run's line, in its buffer if still open.
    fn jupyter_set_hint(&mut self, run: &InFlight, hint: &str) {
        let index = self.jupyter_tab_index(run.path.as_ref(), run.tab);
        let Some(tab) = index.and_then(|i| self.editor.tabs.get_mut(i)) else {
            return;
        };
        let text = tab.editor.get_content();
        let Some(col) = text.split('\n').nth(run.line).map(|l| l.chars().count()) else {
            return;
        };
        tab.editor.set_inlay_hints(vec![(
            run.line,
            col,
            format!("  ⇒ {}", clip(hint, HINT_CHARS)),
        )]);
    }

    /// Report a kernel that is gone.
    fn jupyter_died(&mut self, name: &str, reason: DeathReason) {
        let why = match reason {
            DeathReason::Shutdown => {
                self.status = t!("status.jupyter_shut_down", name = name).to_string();
                return;
            }
            DeathReason::Exited { code, stderr } => {
                for line in stderr.lines().filter(|l| !l.trim().is_empty()) {
                    self.bottom_dock.push(line.to_owned());
                }
                match code {
                    Some(code) => t!("msg.jupyter_exit_code", code = code).to_string(),
                    None => t!("msg.jupyter_killed").to_string(),
                }
            }
            DeathReason::StartTimeout => t!("msg.jupyter_start_timeout").to_string(),
            DeathReason::Transport(text) => text,
            _ => String::new(),
        };
        self.messages
            .error(t!("msg.jupyter_kernel_died", name = name, reason = why).to_string());
    }

    // ----- run targets ---------------------------------------------------------

    /// The code at the cursor (T734d): the selection, the Org `src` block,
    /// the `# %%` cell, then the current line. `None` when it is blank.
    fn jupyter_target(&self) -> Option<Target> {
        let tab = self.editor.active_tab().filter(|t| !t.is_image())?;
        let editor = &tab.editor;
        let text = editor.get_content();
        let language = editor.language().to_owned();
        if let Some((start, end)) = editor.selection_span() {
            let code: String = text.chars().skip(start).take(end - start).collect();
            let first = line_of(&text, start);
            let mut last = line_of(&text, end);
            if last > first && code.ends_with('\n') {
                last -= 1;
            }
            return target(code, language, last, None);
        }
        let line = editor.cursor_line();
        if let Some(block) = block_target(&text, line, &language) {
            return (!block.code.trim().is_empty()).then_some(block);
        }
        let lines: Vec<&str> = text.split('\n').collect();
        if let Some(cell) = cell::cell_at(&text, line, &editor.comment_prefix()) {
            let last = cell.body.end.saturating_sub(1).max(cell.body.start);
            let code = lines
                .get(cell.body)
                .map(|l| l.join("\n").trim_end().to_owned())
                .unwrap_or_default();
            return target(code, language, last, None);
        }
        target(
            lines.get(line).copied().unwrap_or_default().to_owned(),
            language,
            line,
            None,
        )
    }

    /// The Org `src` block at the cursor as a run target (its code may be
    /// blank), or `None` outside any block.
    fn jupyter_block_target(&self) -> Option<Target> {
        let tab = self.editor.active_tab().filter(|t| !t.is_image())?;
        let text = tab.editor.get_content();
        block_target(&text, tab.editor.cursor_line(), tab.editor.language())
    }
}

/// The Org `src` block around `line` as a run target (an empty block
/// language means the buffer's `language`; the code may be blank), or
/// `None` outside any block.
fn block_target(text: &str, line: usize, language: &str) -> Option<Target> {
    let (begin, end, block_language) = crate::org::src_block_at(text, line)?;
    let lines: Vec<&str> = text.split('\n').collect();
    let code = lines[begin + 1..end].join("\n");
    let language = if block_language.is_empty() {
        language.to_owned()
    } else {
        block_language
    };
    let org = OrgRun {
        begin,
        header: Header::parse(lines[begin]),
    };
    Some(Target {
        code,
        language,
        last: end,
        org: Some(org),
    })
}

/// A [`Target`], or `None` for blank code.
fn target(code: String, language: String, last: usize, org: Option<OrgRun>) -> Option<Target> {
    (!code.trim().is_empty()).then_some(Target {
        code,
        language,
        last,
        org,
    })
}

/// The 0-based line of char offset `offset` in `text`.
fn line_of(text: &str, offset: usize) -> usize {
    text.chars().take(offset).filter(|&c| c == '\n').count()
}

/// `text` cut to `max` characters, with `…` when cut.
fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

/// One output as dock lines: plain text with ANSI codes removed, an image's
/// `text/plain` form (the image itself is shown by the caller), and a note
/// when a richer form (such as HTML) was dropped.
fn output_lines(output: &Output) -> Vec<String> {
    let text = match output {
        Output::Result { data, .. } | Output::Display { data, .. } => {
            let Some(choice) = data.choose() else {
                return Vec::new();
            };
            match choice.payload {
                Payload::Base64(_) => {
                    return data
                        .text("text/plain")
                        .map(|plain| text_lines(&plain))
                        .unwrap_or_default();
                }
                Payload::Text(text) => {
                    let mut lines = text_lines(&text);
                    if choice.dropped_richer {
                        lines.push(t!("ui.jupyter_richer_dropped").to_string());
                    }
                    return lines;
                }
            }
        }
        other => other.plain_text(),
    };
    text_lines(&text)
}

/// About how many bytes `output` holds: its text, or its MIME data.
fn output_weight(output: &Output) -> usize {
    match output {
        Output::Stream { text, .. } => text.len(),
        Output::Result { data, .. } | Output::Display { data, .. } => data
            .0
            .values()
            .map(|v| v.as_str().map_or_else(|| v.to_string().len(), str::len))
            .sum(),
        Output::Error {
            ename,
            evalue,
            traceback,
        } => ename.len() + evalue.len() + traceback.iter().map(String::len).sum::<usize>(),
        _ => 0,
    }
}

/// `text` without ANSI codes, split into lines, without a trailing empty one.
fn text_lines(text: &str) -> Vec<String> {
    let clean = strip_ansi(text);
    clean
        .trim_end_matches('\n')
        .split('\n')
        .map(|l| l.trim_end_matches('\r').to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use vix_jupyter::core::{MimeBundle, StreamName};

    fn bundle(pairs: &[(&str, &str)]) -> MimeBundle {
        MimeBundle(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), serde_json::Value::String((*v).to_owned())))
                .collect(),
        )
    }

    #[test]
    fn streams_lose_ansi_and_the_trailing_newline() {
        let out = Output::Stream {
            name: StreamName::Stdout,
            text: "\u{1b}[31mred\u{1b}[0m\nplain\n".into(),
        };
        assert_eq!(output_lines(&out), ["red", "plain"]);
    }

    #[test]
    fn errors_show_the_name_value_and_traceback() {
        let out = Output::Error {
            ename: "ZeroDivisionError".into(),
            evalue: "division by zero".into(),
            traceback: vec!["\u{1b}[31mTraceback\u{1b}[0m".into()],
        };
        assert_eq!(
            output_lines(&out),
            ["ZeroDivisionError: division by zero", "Traceback"]
        );
    }

    #[test]
    fn an_image_leaves_its_plain_text() {
        let out = Output::Display {
            data: bundle(&[("image/png", "iVBORw0KGgo="), ("text/plain", "<Image>")]),
            display_id: None,
            update: false,
        };
        assert_eq!(output_lines(&out), ["<Image>"]);
    }

    #[test]
    fn a_block_target_reads_its_header() {
        let text = "#+begin_src python :results output\nprint(1)\n#+end_src\n";
        let Some(t) = block_target(text, 1, "org") else {
            panic!("no block target");
        };
        assert_eq!(t.code, "print(1)");
        assert_eq!(t.last, 2);
        let org = t.org.unwrap_or_else(|| unreachable!());
        assert_eq!(org.begin, 0);
        assert_eq!(org.header.collect, babel::Collect::Output);
        let blank = block_target("#+begin_src python\n\n#+end_src", 0, "org");
        assert!(blank.is_some_and(|t| t.code.trim().is_empty()));
        assert!(block_target("plain", 0, "org").is_none());
    }

    #[test]
    fn html_falls_back_to_plain_text_with_a_note() {
        let out = Output::Result {
            execution_count: Some(1),
            data: bundle(&[("text/html", "<b>2</b>"), ("text/plain", "2")]),
        };
        let lines = output_lines(&out);
        assert_eq!(lines[0], "2");
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn clip_and_line_of() {
        assert_eq!(clip("abc", 5), "abc");
        assert_eq!(clip("abcdef", 3), "abc…");
        assert_eq!(line_of("a\nb\nc", 4), 2);
        assert_eq!(line_of("a\nb", 0), 0);
    }
}
