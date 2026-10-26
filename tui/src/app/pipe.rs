//! Pipe the selection through a shell command (T659): `edit.pipe_selection`
//! replaces the selection (or the whole buffer) with the command's output, and
//! `edit.insert_command_output` inserts that output at the cursor. See
//! `crates/vix-editor/spec/pipe-selection/index.md`.

use std::cell::RefCell;

use super::{App, Prompt, PromptKind, Visible, run_shell_filter};

/// How long a piped command may run before it is killed.
pub(super) const PIPE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

impl App {
    /// Open the command prompt for `edit.pipe_selection` (`replace`) or
    /// `edit.insert_command_output`.
    pub(super) fn begin_pipe_selection(&mut self, replace: bool) {
        let (kind, label) = if replace {
            (PromptKind::PipeSelection, t!("prompt.pipe_selection"))
        } else {
            (
                PromptKind::InsertCommandOutput,
                t!("prompt.insert_command_output"),
            )
        };
        self.prompt = Some(Prompt::new(kind, label.to_string()));
    }

    /// Run `cmd` with the selection (or the whole buffer) on stdin. On success
    /// the output replaces that text (`replace`) or is inserted at the cursor;
    /// on failure the buffer is untouched and stderr goes to the bottom dock.
    pub(super) fn pipe_selection(&mut self, cmd: &str, replace: bool) {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return;
        }
        let root = self.root.clone();
        let failure: RefCell<Option<String>> = RefCell::new(None);
        let run = |input: &str| -> Result<String, String> {
            match run_shell_filter(cmd, &root, input.as_bytes(), PIPE_TIMEOUT) {
                Ok(out) if out.success => Ok(trim_added_newline(
                    input,
                    String::from_utf8_lossy(&out.stdout).into_owned(),
                )),
                Ok(out) => {
                    *failure.borrow_mut() = Some(String::from_utf8_lossy(&out.stderr).into_owned());
                    Err(String::new())
                }
                Err(e) => {
                    *failure.borrow_mut() = Some(e.to_string());
                    Err(String::new())
                }
            }
        };
        if replace {
            self.transform_selection_or_buffer_try(run);
        } else {
            self.insert_command_output(&run);
        }
        if let Some(stderr) = failure.into_inner() {
            self.visible.set(Visible::BOTTOM_DOCK, true);
            self.settings.panels.show_bottom_dock = true;
            self.bottom_dock.push(format!("$ {cmd}"));
            for line in stderr.lines() {
                self.bottom_dock.push(line.to_string());
            }
            self.status = t!("status.pipe_failed", command = cmd).to_string();
        }
    }

    /// Feed the selection (or the whole buffer) to `run` and insert its output
    /// at the cursor, leaving the input in place.
    fn insert_command_output(&mut self, run: &dyn Fn(&str) -> Result<String, String>) {
        if self.active_read_only() {
            self.status = t!("status.read_only_blocked").into();
            return;
        }
        let Some(tab) = self.editor.active_tab_mut() else {
            return;
        };
        if tab.is_image() {
            return;
        }
        let input = match tab.editor.get_selection() {
            Some(sel) if !sel.is_empty() => tab.editor.get_content_slice(sel.start, sel.end),
            _ => tab.editor.get_content(),
        };
        match run(&input) {
            Ok(output) => {
                let area = self.editor_view();
                if let Some(tab) = self.editor.active_tab_mut() {
                    tab.editor.set_selection(None);
                }
                self.editor.insert_str(&output, area);
            }
            Err(e) => self.status = e,
        }
    }
}

/// Drop the one trailing newline a command like `sort` adds when `input` had
/// none, so piping a mid-line selection doesn't introduce a line break.
fn trim_added_newline(input: &str, mut output: String) -> String {
    if input.ends_with('\n') {
        return output;
    }
    for ending in ["\r\n", "\n"] {
        if let Some(rest) = output.strip_suffix(ending)
            && !rest.ends_with('\n')
        {
            output.truncate(rest.len());
            break;
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::trim_added_newline;

    #[test]
    fn keeps_newline_when_input_had_one() {
        assert_eq!(trim_added_newline("b\na\n", "a\nb\n".into()), "a\nb\n");
    }

    #[test]
    fn drops_one_added_newline() {
        assert_eq!(trim_added_newline("b\na", "a\nb\n".into()), "a\nb");
        assert_eq!(trim_added_newline("x", "X\r\n".into()), "X");
    }

    #[test]
    fn keeps_deliberate_blank_line() {
        assert_eq!(trim_added_newline("x", "X\n\n".into()), "X\n\n");
    }
}
