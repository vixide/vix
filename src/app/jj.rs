//! Jujutsu (`jj`): the `jj.*` action dispatcher and its prompt handlers, for
//! repos that use `jj` instead of (or alongside) Git.

#![warn(clippy::pedantic)]

use super::{App, Prompt, PromptKind};

impl App {
    /// Dispatch a Jujutsu (`jj.*`) version-control action, streaming command
    /// output into the bottom dock. Returns `true` if `action` was handled.
    /// Mirrors [`App::run_git_action`] for the jj VCS.
    pub(super) fn run_jj_action(&mut self, action: &str) -> bool {
        match action {
            "jj.init" => self.jj_init(),
            "jj.clone" => self.jj_begin_clone(),
            "jj.status" => self.jj_command("jj --no-pager status"),
            "jj.diff" => self.jj_command("jj --no-pager diff"),
            "jj.show" => self.jj_command("jj --no-pager show"),
            "jj.log" => self.jj_command("jj --no-pager log"),
            "jj.log_all" => self.jj_command("jj --no-pager log -r ::"),
            "jj.op_log" => self.jj_command("jj --no-pager op log"),
            "jj.new" => self.jj_command("jj new"),
            "jj.describe" => self.jj_begin_prompt(PromptKind::JjDescribe, "prompt.jj_describe"),
            "jj.commit" => self.jj_begin_prompt(PromptKind::JjCommit, "prompt.jj_commit"),
            "jj.edit" => self.jj_begin_prompt(PromptKind::JjEdit, "prompt.jj_edit"),
            "jj.squash" => self.jj_command("jj squash"),
            "jj.abandon" => self.jj_command("jj abandon"),
            "jj.restore" => self.jj_command("jj restore"),
            "jj.rebase" => self.jj_begin_prompt(PromptKind::JjRebase, "prompt.jj_rebase"),
            "jj.undo" => self.jj_command("jj undo"),
            "jj.bookmark_create" => {
                self.jj_begin_prompt(PromptKind::JjBookmarkCreate, "prompt.jj_bookmark_create");
            }
            "jj.bookmark_set" => {
                self.jj_begin_prompt(PromptKind::JjBookmarkSet, "prompt.jj_bookmark_set");
            }
            "jj.bookmark_delete" => {
                self.jj_begin_prompt(PromptKind::JjBookmarkDelete, "prompt.jj_bookmark_delete");
            }
            "jj.bookmark_list" => self.jj_command("jj --no-pager bookmark list"),
            "jj.git_push" => self.jj_command("jj git push"),
            "jj.git_fetch" => self.jj_command("jj git fetch"),
            _ => return false,
        }
        true
    }

    /// Whether the workspace root holds a Jujutsu repo (a `.jj` directory).
    pub(super) fn jj_is_repo(&self) -> bool {
        self.root.join(".jj").exists()
    }

    /// Run a `jj` command in the workspace, streaming its output into the bottom
    /// dock, but only when the workspace is a jj repo.
    pub(super) fn jj_command(&mut self, cmd: &str) {
        if !self.jj_is_repo() {
            self.status = t!("status.jj_not_repo").into();
            return;
        }
        self.run_command(cmd);
    }

    /// Open a single-line prompt (of `kind`, titled by i18n key `title`) for a jj
    /// command, but only when the workspace is a jj repo.
    pub(super) fn jj_begin_prompt(&mut self, kind: PromptKind, title: &str) {
        if !self.jj_is_repo() {
            self.status = t!("status.jj_not_repo").into();
            return;
        }
        self.prompt = Some(Prompt::new(kind, t!(title).to_string()));
    }

    /// Initialize a Jujutsu repo in the workspace, refusing if one already exists.
    /// Colocates with an existing Git repo (so both tools share the working copy)
    /// when a `.git` is present; otherwise creates a plain jj-backed repo.
    pub(super) fn jj_init(&mut self) {
        if self.jj_is_repo() {
            self.status = t!("status.jj_already_init").into();
            return;
        }
        #[cfg(feature = "git")]
        let colocate = self.root.join(".git").exists() || crate::git::is_repo(&self.root);
        #[cfg(not(feature = "git"))]
        let colocate = self.root.join(".git").exists();
        self.run_command(if colocate {
            "jj git init --colocate"
        } else {
            "jj git init"
        });
    }

    /// Begin cloning a repository with jj: prompt for its URL (works outside a
    /// repo too, so it is not guarded by [`App::jj_is_repo`]).
    pub(super) fn jj_begin_clone(&mut self) {
        self.prompt = Some(Prompt::new(
            PromptKind::JjClone,
            // Same text as `prompt.git_clone` (Run H, T524: was a separate,
            // identical-in-every-locale duplicate key).
            t!("prompt.git_clone").to_string(),
        ));
    }

    /// Clone `url` into the workspace with `jj git clone`.
    pub(super) fn jj_clone(&mut self, url: &str) {
        let url = url.trim();
        if url.is_empty() {
            // Same text as `status.git_empty_url` (Run H, T524: was a
            // separate, identical-in-every-locale duplicate key).
            self.status = t!("status.git_empty_url").into();
            return;
        }
        if !Self::vcs_arg_ok(url, false) {
            self.status = t!("status.git_bad_argument").into();
            return;
        }
        self.run_argv("jj", &["git", "clone", "--", url]);
    }

    /// Set the working-copy change description (`jj describe -m`).
    pub(super) fn jj_describe(&mut self, message: &str) {
        self.jj_message_command("describe", message);
    }

    /// Describe the working-copy change and start a new one (`jj commit -m`).
    pub(super) fn jj_commit(&mut self, message: &str) {
        self.jj_message_command("commit", message);
    }

    /// Run `jj <sub> --message=<message>`, rejecting an empty message. The
    /// `--message=` form keeps a message that starts with `-` a value, and no
    /// shell is involved (T569).
    pub(super) fn jj_message_command(&mut self, sub: &str, message: &str) {
        let message = message.trim();
        if message.is_empty() {
            self.status = t!("status.jj_empty_message").into();
            return;
        }
        self.run_argv("jj", &[sub, &format!("--message={message}")]);
    }

    /// Set the working-copy revision to `rev` (`jj edit`).
    pub(super) fn jj_edit(&mut self, rev: &str) {
        let rev = rev.trim();
        if rev.is_empty() {
            self.status = t!("status.jj_empty_revision").into();
            return;
        }
        if !Self::vcs_arg_ok(rev, false) {
            self.status = t!("status.git_bad_argument").into();
            return;
        }
        self.run_argv("jj", &["edit", "--", rev]);
    }

    /// Rebase the working-copy change onto destination `dest` (`jj rebase -d`).
    pub(super) fn jj_rebase(&mut self, dest: &str) {
        let dest = dest.trim();
        if dest.is_empty() {
            self.status = t!("status.jj_empty_revision").into();
            return;
        }
        if !Self::vcs_arg_ok(dest, false) {
            self.status = t!("status.git_bad_argument").into();
            return;
        }
        self.run_argv("jj", &["rebase", "-d", dest]);
    }

    /// Run a `jj bookmark <sub> <name>` command, rejecting an empty name.
    pub(super) fn jj_bookmark(&mut self, sub: &str, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            self.status = t!("status.jj_empty_bookmark").into();
            return;
        }
        if !Self::vcs_arg_ok(name, false) {
            self.status = t!("status.git_bad_argument").into();
            return;
        }
        self.run_argv("jj", &["bookmark", sub, "--", name]);
    }

    /// Handle a completed Jujutsu prompt: run the jj command for the entered
    /// `input`. Grouped out of [`App::accept_prompt`] to keep it within the line
    /// limit, mirroring [`App::accept_roam_prompt`].
    pub(super) fn accept_jj_prompt(&mut self, kind: PromptKind, input: &str) {
        match kind {
            PromptKind::JjClone => self.jj_clone(input),
            PromptKind::JjDescribe => self.jj_describe(input),
            PromptKind::JjCommit => self.jj_commit(input),
            PromptKind::JjEdit => self.jj_edit(input),
            PromptKind::JjRebase => self.jj_rebase(input),
            PromptKind::JjBookmarkCreate => self.jj_bookmark("create", input),
            PromptKind::JjBookmarkSet => self.jj_bookmark("set", input),
            PromptKind::JjBookmarkDelete => self.jj_bookmark("delete", input),
            _ => {}
        }
    }
}
