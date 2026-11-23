//! The Open Remote pickers (T718c): **File → Open Remote…** offers
//! "Kubernetes container…" when `kubectl` is on `PATH` and "WSL
//! distribution…" when `wsl.exe` is. Choosing one runs the program's list
//! command (`kubectl get pods`, `wsl.exe --list --quiet`), shows what it
//! found, and fills in the Open Remote prompt with the chosen name and `~`,
//! so the user only finishes the path.
//!
//! The listing runs on the UI thread with a deadline ([`LIST_DEADLINE`]);
//! `kubectl` also gets its own request timeout. Parsing and the strict name
//! checks are `vix_authority::container::pick`. Owning spec:
//! `crates/vix-authority/spec/container/index.md` ("Pickers").

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

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use vix_authority::container::pick;
use vix_authority::container::{ContainerTarget, ExecKind};

use super::ide_parity::{ChoiceList, ChoicePurpose};
use super::{App, Prompt, PromptKind};

/// How long a listing may run before it is killed.
const LIST_DEADLINE: Duration = Duration::from_secs(8);

/// The picker programs, when tests or callers choose them; `None` leaves
/// that picker out.
#[derive(Debug, Clone, Default)]
pub(crate) struct PickerPrograms {
    /// `kubectl`.
    pub(crate) kubectl: Option<PathBuf>,
    /// `wsl.exe`.
    pub(crate) wsl: Option<PathBuf>,
}

impl App {
    /// Each picker the dialog offers, with the program it runs.
    pub(super) fn remote_picker_programs(&self) -> Vec<(ExecKind, PathBuf)> {
        let programs = self
            .remote
            .picker_programs
            .clone()
            .unwrap_or_else(|| PickerPrograms {
                kubectl: find_program(ExecKind::Kubernetes.program()),
                wsl: find_program(ExecKind::Wsl.program()),
            });
        let mut out = Vec::new();
        if let Some(path) = programs.kubectl {
            out.push((ExecKind::Kubernetes, path));
        }
        if let Some(path) = programs.wsl {
            out.push((ExecKind::Wsl, path));
        }
        out
    }

    /// Run the `kind` picker's listing and show what it found.
    pub(super) fn run_remote_picker(&mut self, kind: ExecKind) {
        let Some((_, program)) = self
            .remote_picker_programs()
            .into_iter()
            .find(|(k, _)| *k == kind)
        else {
            return;
        };
        let args = if kind == ExecKind::Kubernetes {
            pick::kubectl_pods_args()
        } else {
            pick::wsl_list_args()
        };
        let output = match run_listing(&program, &args) {
            Ok(output) => output,
            Err(error) => {
                let name = kind.program();
                self.messages
                    .error(t!("msg.remote_pick_failed", program = name, error = error).to_string());
                self.status = t!("status.remote_pick_failed", program = name).to_string();
                return;
            }
        };
        let targets = if kind == ExecKind::Kubernetes {
            pick::parse_kubectl_pods(&String::from_utf8_lossy(&output))
        } else {
            pick::parse_wsl_list(&output)
        };
        if targets.is_empty() {
            self.status = if kind == ExecKind::Kubernetes {
                t!("status.remote_pick_no_pods")
            } else {
                t!("status.remote_pick_no_distros")
            }
            .to_string();
            return;
        }
        let rows = targets.iter().map(ContainerTarget::label).collect();
        self.choice_list = Some(ChoiceList {
            panel: vix_choice_list::Panel::new(
                t!("ui.remote_open_title").to_string(),
                t!("ui.remote_pick_hint").to_string(),
                rows,
            ),
            purpose: ChoicePurpose::RemotePick { targets },
        });
    }

    /// A picked container or distribution: the Open Remote prompt, filled in
    /// with its name at the home directory, for the user to finish the path.
    pub(super) fn remote_pick_chosen(&mut self, target: &ContainerTarget) {
        self.prompt = Some(
            Prompt::new(PromptKind::OpenRemote, t!("prompt.open_remote").to_string())
                .with_input(target.to_url()),
        );
    }
}

/// `name` on `PATH`; for a `.exe` name also its stem, since `PATHEXT` adds
/// the extension on Windows (and inside WSL the file keeps its `.exe`).
fn find_program(name: &str) -> Option<PathBuf> {
    crate::fileops::which_on_path(name).or_else(|| {
        name.strip_suffix(".exe")
            .and_then(crate::fileops::which_on_path)
    })
}

/// Run `program args…` with no stdin and return its stdout, or why not: it
/// could not start, ran past [`LIST_DEADLINE`] (it is killed), or failed
/// (the first line of its stderr, or its status).
fn run_listing(program: &Path, args: &[String]) -> Result<Vec<u8>, String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    // Read both pipes on their own threads so a chatty child cannot block.
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let out = std::thread::spawn(move || drain(stdout.as_mut()));
    let err = std::thread::spawn(move || drain(stderr.as_mut()));
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() < LIST_DEADLINE => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(
                    t!("msg.remote_pick_timeout", seconds = LIST_DEADLINE.as_secs()).to_string(),
                );
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    let stdout = out.join().unwrap_or_default();
    let stderr = err.join().unwrap_or_default();
    if status.success() {
        return Ok(stdout);
    }
    let text = String::from_utf8_lossy(&stderr);
    Err(text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map_or_else(|| status.to_string(), str::to_string))
}

/// Everything left in `pipe`.
fn drain(pipe: Option<&mut impl Read>) -> Vec<u8> {
    let mut buf = Vec::new();
    if let Some(pipe) = pipe {
        let _ = pipe.read_to_end(&mut buf);
    }
    buf
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use vix_input::{KeyCode, KeyEvent};

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("vix-remote-pick-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    /// A fake program that logs its arguments and runs `body`.
    fn fake(dir: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        let log = dir.join(format!("{name}-argv"));
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> '{}'; done\n{body}\n",
                log.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn app(dir: &Path, kubectl: Option<PathBuf>, wsl: Option<PathBuf>) -> App {
        App::new(dir.to_path_buf(), Settings::default())
            .with_session_path(dir.join("session.toml"))
            .with_remote_pickers(kubectl, wsl)
    }

    fn rows(app: &App) -> Vec<String> {
        app.choice_list
            .as_ref()
            .map_or_default(|c| c.panel.rows.clone())
    }

    fn enter(app: &mut App) {
        app.on_key(KeyEvent::from(KeyCode::Enter));
    }

    #[test]
    fn no_pickers_and_no_recents_go_straight_to_the_prompt() {
        let dir = scratch("none");
        let mut app = app(&dir, None, None);
        app.run_action("file.open_remote");
        assert!(app.choice_list.is_none());
        assert!(matches!(
            app.prompt.as_ref().map(|p| p.kind),
            Some(PromptKind::OpenRemote)
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_kubernetes_picker_fills_in_the_prompt() {
        let dir = scratch("k8s");
        let kubectl = fake(
            &dir,
            "kubectl",
            "printf 'dev\\tweb\\tRunning\\tapp sidecar \\ndev\\tjob\\tSucceeded\\tmain \\n'",
        );
        let mut app = app(&dir, Some(kubectl), None);
        app.run_action("file.open_remote");
        assert_eq!(rows(&app), ["New connection…", "Kubernetes container…"]);
        app.on_key(KeyEvent::from(KeyCode::Down));
        enter(&mut app);
        let argv = std::fs::read_to_string(dir.join("kubectl-argv")).unwrap();
        assert!(
            argv.starts_with("get\npods\n--request-timeout=5s\n-o\njsonpath="),
            "{argv}"
        );
        assert_eq!(rows(&app), ["k8s:dev/web/app", "k8s:dev/web/sidecar"]);
        app.on_key(KeyEvent::from(KeyCode::Down));
        enter(&mut app);
        assert!(app.choice_list.is_none());
        let prompt = app.prompt.as_ref().expect("the Open Remote prompt");
        assert!(matches!(prompt.kind, PromptKind::OpenRemote));
        assert_eq!(prompt.input, "k8s://dev/web/sidecar/~");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_wsl_picker_reads_utf16_and_a_failure_is_reported() {
        let dir = scratch("wsl");
        // "Ubuntu\r\n" in UTF-16LE with a byte-order mark, as wsl.exe writes.
        let wsl = fake(
            &dir,
            "wsl.exe",
            "printf '\\377\\376U\\000b\\000u\\000n\\000t\\000u\\000\\r\\000\\n\\000'",
        );
        let mut app = app(&dir, None, Some(wsl));
        app.run_action("file.open_remote");
        assert_eq!(rows(&app), ["New connection…", "WSL distribution…"]);
        app.on_key(KeyEvent::from(KeyCode::Down));
        enter(&mut app);
        let argv = std::fs::read_to_string(dir.join("wsl.exe-argv")).unwrap();
        assert_eq!(argv, "--list\n--quiet\n");
        assert_eq!(rows(&app), ["wsl:Ubuntu"]);
        enter(&mut app);
        assert_eq!(
            app.prompt.as_ref().map(|p| p.input.as_str()),
            Some("wsl://Ubuntu/~")
        );

        // A failing kubectl: its stderr is the message, no list opens.
        let kubectl = fake(
            &dir,
            "kubectl",
            "echo 'The connection to the server localhost:8080 was refused' >&2; exit 1",
        );
        let mut app = super::tests::app(&dir, Some(kubectl), None);
        app.run_action("file.open_remote");
        app.on_key(KeyEvent::from(KeyCode::Down));
        enter(&mut app);
        assert!(app.choice_list.is_none());
        assert!(app.prompt.is_none());
        assert!(app.status.contains("kubectl"), "{}", app.status);
        // No running pods: said on the status line.
        let kubectl = fake(&dir, "kubectl", "printf 'dev\\tjob\\tSucceeded\\tmain \\n'");
        let mut app = super::tests::app(&dir, Some(kubectl), None);
        app.run_action("file.open_remote");
        app.on_key(KeyEvent::from(KeyCode::Down));
        enter(&mut app);
        assert!(app.choice_list.is_none());
        assert_ne!(app.status, "");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
