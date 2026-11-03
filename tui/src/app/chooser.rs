//! The list choosers as one enum (T611 step 2, first slice).
//!
//! `App::chooser` holds at most one [`Chooser`], so two of these pickers can
//! never be open together. Each variant still has its own row in
//! [`super::overlays::OVERLAYS`]. See `spec/overlay-priority/index.md`.

use super::{
    BranchChooser, CaptureChooser, ClipboardChooser, EncodingChooser, HttpEnvChooser,
    LocationChooser, MacroChooser, PluginChooser, RecentChooser, RefileChooser, ScriptChooser,
    StashChooser, TaskChooser, WorkspaceChooser, WorktreeChooser,
};

/// The open list chooser, one variant per picker.
pub(crate) enum Chooser {
    /// The Branch chooser.
    #[cfg_attr(
        not(feature = "git"),
        allow(dead_code, reason = "this chooser is only opened by the `git` feature")
    )]
    Branch(BranchChooser),
    /// The Stash chooser.
    #[cfg_attr(
        not(feature = "git"),
        allow(dead_code, reason = "this chooser is only opened by the `git` feature")
    )]
    Stash(StashChooser),
    /// The Worktree chooser.
    #[cfg_attr(
        not(feature = "git"),
        allow(dead_code, reason = "this chooser is only opened by the `git` feature")
    )]
    Worktree(WorktreeChooser),
    /// The Task chooser.
    Task(TaskChooser),
    /// The Script chooser.
    #[cfg_attr(
        not(feature = "script"),
        allow(
            dead_code,
            reason = "this chooser is only opened by the `script` feature"
        )
    )]
    Script(ScriptChooser),
    /// The Plugin chooser.
    #[cfg_attr(
        not(feature = "script"),
        allow(
            dead_code,
            reason = "this chooser is only opened by the `script` feature"
        )
    )]
    Plugin(PluginChooser),
    /// The Location chooser.
    Location(LocationChooser),
    /// The Capture chooser.
    Capture(CaptureChooser),
    /// The Refile chooser.
    Refile(RefileChooser),
    /// The Recent chooser.
    Recent(RecentChooser),
    /// The Encoding chooser.
    Encoding(EncodingChooser),
    /// The Macro chooser.
    Macro(MacroChooser),
    /// The Clipboard chooser.
    Clipboard(ClipboardChooser),
    /// The Workspace chooser.
    Workspace(WorkspaceChooser),
    /// The `HttpEnv` chooser.
    #[cfg_attr(
        not(feature = "http"),
        allow(
            dead_code,
            reason = "this chooser is only opened by the `http` feature"
        )
    )]
    HttpEnv(HttpEnvChooser),
}

/// Typed access to the slot that holds the open [`Chooser`], so a call site
/// borrows only the `chooser` field.
pub(crate) trait ChooserSlot {
    /// The Branch chooser, if that is the open one.
    fn branch(&self) -> Option<&BranchChooser>;
    /// The Branch chooser, mutably, if that is the open one.
    #[cfg_attr(
        not(feature = "git"),
        allow(dead_code, reason = "this method is only opened by the `git` feature")
    )]
    fn branch_mut(&mut self) -> Option<&mut BranchChooser>;
    /// Closes the Branch chooser and returns it; any other open chooser stays.
    #[cfg_attr(
        not(feature = "git"),
        allow(dead_code, reason = "this method is only opened by the `git` feature")
    )]
    fn take_branch(&mut self) -> Option<BranchChooser>;
    /// Opens the Branch chooser, replacing any other open chooser.
    #[cfg_attr(
        not(feature = "git"),
        allow(dead_code, reason = "this method is only opened by the `git` feature")
    )]
    fn open_branch(&mut self, c: BranchChooser);
    /// The Stash chooser, if that is the open one.
    fn stash(&self) -> Option<&StashChooser>;
    /// The Stash chooser, mutably, if that is the open one.
    #[cfg_attr(
        not(feature = "git"),
        allow(dead_code, reason = "this method is only opened by the `git` feature")
    )]
    fn stash_mut(&mut self) -> Option<&mut StashChooser>;
    /// Opens the Stash chooser, replacing any other open chooser.
    #[cfg_attr(
        not(feature = "git"),
        allow(dead_code, reason = "this method is only opened by the `git` feature")
    )]
    fn open_stash(&mut self, c: StashChooser);
    /// The Worktree chooser, if that is the open one.
    fn worktree(&self) -> Option<&WorktreeChooser>;
    /// The Worktree chooser, mutably, if that is the open one.
    #[cfg_attr(
        not(feature = "git"),
        allow(dead_code, reason = "this method is only opened by the `git` feature")
    )]
    fn worktree_mut(&mut self) -> Option<&mut WorktreeChooser>;
    /// Opens the Worktree chooser, replacing any other open chooser.
    #[cfg_attr(
        not(feature = "git"),
        allow(dead_code, reason = "this method is only opened by the `git` feature")
    )]
    fn open_worktree(&mut self, c: WorktreeChooser);
    /// The Task chooser, if that is the open one.
    fn task(&self) -> Option<&TaskChooser>;
    /// The Task chooser, mutably, if that is the open one.
    fn task_mut(&mut self) -> Option<&mut TaskChooser>;
    /// Closes the Task chooser and returns it; any other open chooser stays.
    fn take_task(&mut self) -> Option<TaskChooser>;
    /// Opens the Task chooser, replacing any other open chooser.
    fn open_task(&mut self, c: TaskChooser);
    /// The Script chooser, if that is the open one.
    fn script(&self) -> Option<&ScriptChooser>;
    /// The Script chooser, mutably, if that is the open one.
    #[cfg_attr(
        not(feature = "script"),
        allow(
            dead_code,
            reason = "this method is only opened by the `script` feature"
        )
    )]
    fn script_mut(&mut self) -> Option<&mut ScriptChooser>;
    /// Closes the Script chooser and returns it; any other open chooser stays.
    #[cfg_attr(
        not(feature = "script"),
        allow(
            dead_code,
            reason = "this method is only opened by the `script` feature"
        )
    )]
    fn take_script(&mut self) -> Option<ScriptChooser>;
    /// Opens the Script chooser, replacing any other open chooser.
    #[cfg_attr(
        not(feature = "script"),
        allow(
            dead_code,
            reason = "this method is only opened by the `script` feature"
        )
    )]
    fn open_script(&mut self, c: ScriptChooser);
    /// The Plugin chooser, if that is the open one.
    fn plugin(&self) -> Option<&PluginChooser>;
    /// The Plugin chooser, mutably, if that is the open one.
    #[cfg_attr(
        not(feature = "script"),
        allow(
            dead_code,
            reason = "this method is only opened by the `script` feature"
        )
    )]
    fn plugin_mut(&mut self) -> Option<&mut PluginChooser>;
    /// Opens the Plugin chooser, replacing any other open chooser.
    #[cfg_attr(
        not(feature = "script"),
        allow(
            dead_code,
            reason = "this method is only opened by the `script` feature"
        )
    )]
    fn open_plugin(&mut self, c: PluginChooser);
    /// The Location chooser, if that is the open one.
    fn location(&self) -> Option<&LocationChooser>;
    /// The Location chooser, mutably, if that is the open one.
    fn location_mut(&mut self) -> Option<&mut LocationChooser>;
    /// Closes the Location chooser and returns it; any other open chooser stays.
    fn take_location(&mut self) -> Option<LocationChooser>;
    /// Opens the Location chooser, replacing any other open chooser.
    fn open_location(&mut self, c: LocationChooser);
    /// The Capture chooser, if that is the open one.
    fn capture(&self) -> Option<&CaptureChooser>;
    /// The Capture chooser, mutably, if that is the open one.
    fn capture_mut(&mut self) -> Option<&mut CaptureChooser>;
    /// Closes the Capture chooser and returns it; any other open chooser stays.
    fn take_capture(&mut self) -> Option<CaptureChooser>;
    /// Opens the Capture chooser, replacing any other open chooser.
    fn open_capture(&mut self, c: CaptureChooser);
    /// The Refile chooser, if that is the open one.
    fn refile(&self) -> Option<&RefileChooser>;
    /// The Refile chooser, mutably, if that is the open one.
    fn refile_mut(&mut self) -> Option<&mut RefileChooser>;
    /// Closes the Refile chooser and returns it; any other open chooser stays.
    fn take_refile(&mut self) -> Option<RefileChooser>;
    /// Opens the Refile chooser, replacing any other open chooser.
    fn open_refile(&mut self, c: RefileChooser);
    /// The Recent chooser, if that is the open one.
    fn recent(&self) -> Option<&RecentChooser>;
    /// The Recent chooser, mutably, if that is the open one.
    fn recent_mut(&mut self) -> Option<&mut RecentChooser>;
    /// Closes the Recent chooser and returns it; any other open chooser stays.
    fn take_recent(&mut self) -> Option<RecentChooser>;
    /// Opens the Recent chooser, replacing any other open chooser.
    fn open_recent(&mut self, c: RecentChooser);
    /// The Encoding chooser, if that is the open one.
    fn encoding(&self) -> Option<&EncodingChooser>;
    /// The Encoding chooser, mutably, if that is the open one.
    fn encoding_mut(&mut self) -> Option<&mut EncodingChooser>;
    /// Closes the Encoding chooser and returns it; any other open chooser stays.
    fn take_encoding(&mut self) -> Option<EncodingChooser>;
    /// Opens the Encoding chooser, replacing any other open chooser.
    fn open_encoding(&mut self, c: EncodingChooser);
    /// The Macro chooser, if that is the open one.
    fn macros(&self) -> Option<&MacroChooser>;
    /// The Macro chooser, mutably, if that is the open one.
    fn macros_mut(&mut self) -> Option<&mut MacroChooser>;
    /// Closes the Macro chooser and returns it; any other open chooser stays.
    fn take_macros(&mut self) -> Option<MacroChooser>;
    /// Opens the Macro chooser, replacing any other open chooser.
    fn open_macros(&mut self, c: MacroChooser);
    /// The Clipboard chooser, if that is the open one.
    fn clipboard(&self) -> Option<&ClipboardChooser>;
    /// The Clipboard chooser, mutably, if that is the open one.
    fn clipboard_mut(&mut self) -> Option<&mut ClipboardChooser>;
    /// Closes the Clipboard chooser and returns it; any other open chooser stays.
    fn take_clipboard(&mut self) -> Option<ClipboardChooser>;
    /// Opens the Clipboard chooser, replacing any other open chooser.
    fn open_clipboard(&mut self, c: ClipboardChooser);
    /// The Workspace chooser, if that is the open one.
    fn workspace(&self) -> Option<&WorkspaceChooser>;
    /// The Workspace chooser, mutably, if that is the open one.
    fn workspace_mut(&mut self) -> Option<&mut WorkspaceChooser>;
    /// Closes the Workspace chooser and returns it; any other open chooser stays.
    fn take_workspace(&mut self) -> Option<WorkspaceChooser>;
    /// Opens the Workspace chooser, replacing any other open chooser.
    fn open_workspace(&mut self, c: WorkspaceChooser);
    /// The `HttpEnv` chooser, if that is the open one.
    fn http_env(&self) -> Option<&HttpEnvChooser>;
    /// The `HttpEnv` chooser, mutably, if that is the open one.
    #[cfg_attr(
        not(feature = "http"),
        allow(dead_code, reason = "this method is only opened by the `http` feature")
    )]
    fn http_env_mut(&mut self) -> Option<&mut HttpEnvChooser>;
    /// Closes the `HttpEnv` chooser and returns it; any other open chooser stays.
    #[cfg_attr(
        not(feature = "http"),
        allow(dead_code, reason = "this method is only opened by the `http` feature")
    )]
    fn take_http_env(&mut self) -> Option<HttpEnvChooser>;
    /// Opens the `HttpEnv` chooser, replacing any other open chooser.
    #[cfg_attr(
        not(feature = "http"),
        allow(dead_code, reason = "this method is only opened by the `http` feature")
    )]
    fn open_http_env(&mut self, c: HttpEnvChooser);
}

impl ChooserSlot for Option<Chooser> {
    fn branch(&self) -> Option<&BranchChooser> {
        match self {
            Some(Chooser::Branch(c)) => Some(c),
            _ => None,
        }
    }
    fn branch_mut(&mut self) -> Option<&mut BranchChooser> {
        match self {
            Some(Chooser::Branch(c)) => Some(c),
            _ => None,
        }
    }
    fn take_branch(&mut self) -> Option<BranchChooser> {
        match self.take() {
            Some(Chooser::Branch(c)) => Some(c),
            other => {
                *self = other;
                None
            }
        }
    }
    fn open_branch(&mut self, c: BranchChooser) {
        *self = Some(Chooser::Branch(c));
    }
    fn stash(&self) -> Option<&StashChooser> {
        match self {
            Some(Chooser::Stash(c)) => Some(c),
            _ => None,
        }
    }
    fn stash_mut(&mut self) -> Option<&mut StashChooser> {
        match self {
            Some(Chooser::Stash(c)) => Some(c),
            _ => None,
        }
    }
    fn open_stash(&mut self, c: StashChooser) {
        *self = Some(Chooser::Stash(c));
    }
    fn worktree(&self) -> Option<&WorktreeChooser> {
        match self {
            Some(Chooser::Worktree(c)) => Some(c),
            _ => None,
        }
    }
    fn worktree_mut(&mut self) -> Option<&mut WorktreeChooser> {
        match self {
            Some(Chooser::Worktree(c)) => Some(c),
            _ => None,
        }
    }
    fn open_worktree(&mut self, c: WorktreeChooser) {
        *self = Some(Chooser::Worktree(c));
    }
    fn task(&self) -> Option<&TaskChooser> {
        match self {
            Some(Chooser::Task(c)) => Some(c),
            _ => None,
        }
    }
    fn task_mut(&mut self) -> Option<&mut TaskChooser> {
        match self {
            Some(Chooser::Task(c)) => Some(c),
            _ => None,
        }
    }
    fn take_task(&mut self) -> Option<TaskChooser> {
        match self.take() {
            Some(Chooser::Task(c)) => Some(c),
            other => {
                *self = other;
                None
            }
        }
    }
    fn open_task(&mut self, c: TaskChooser) {
        *self = Some(Chooser::Task(c));
    }
    fn script(&self) -> Option<&ScriptChooser> {
        match self {
            Some(Chooser::Script(c)) => Some(c),
            _ => None,
        }
    }
    fn script_mut(&mut self) -> Option<&mut ScriptChooser> {
        match self {
            Some(Chooser::Script(c)) => Some(c),
            _ => None,
        }
    }
    fn take_script(&mut self) -> Option<ScriptChooser> {
        match self.take() {
            Some(Chooser::Script(c)) => Some(c),
            other => {
                *self = other;
                None
            }
        }
    }
    fn open_script(&mut self, c: ScriptChooser) {
        *self = Some(Chooser::Script(c));
    }
    fn plugin(&self) -> Option<&PluginChooser> {
        match self {
            Some(Chooser::Plugin(c)) => Some(c),
            _ => None,
        }
    }
    fn plugin_mut(&mut self) -> Option<&mut PluginChooser> {
        match self {
            Some(Chooser::Plugin(c)) => Some(c),
            _ => None,
        }
    }
    fn open_plugin(&mut self, c: PluginChooser) {
        *self = Some(Chooser::Plugin(c));
    }
    fn location(&self) -> Option<&LocationChooser> {
        match self {
            Some(Chooser::Location(c)) => Some(c),
            _ => None,
        }
    }
    fn location_mut(&mut self) -> Option<&mut LocationChooser> {
        match self {
            Some(Chooser::Location(c)) => Some(c),
            _ => None,
        }
    }
    fn take_location(&mut self) -> Option<LocationChooser> {
        match self.take() {
            Some(Chooser::Location(c)) => Some(c),
            other => {
                *self = other;
                None
            }
        }
    }
    fn open_location(&mut self, c: LocationChooser) {
        *self = Some(Chooser::Location(c));
    }
    fn capture(&self) -> Option<&CaptureChooser> {
        match self {
            Some(Chooser::Capture(c)) => Some(c),
            _ => None,
        }
    }
    fn capture_mut(&mut self) -> Option<&mut CaptureChooser> {
        match self {
            Some(Chooser::Capture(c)) => Some(c),
            _ => None,
        }
    }
    fn take_capture(&mut self) -> Option<CaptureChooser> {
        match self.take() {
            Some(Chooser::Capture(c)) => Some(c),
            other => {
                *self = other;
                None
            }
        }
    }
    fn open_capture(&mut self, c: CaptureChooser) {
        *self = Some(Chooser::Capture(c));
    }
    fn refile(&self) -> Option<&RefileChooser> {
        match self {
            Some(Chooser::Refile(c)) => Some(c),
            _ => None,
        }
    }
    fn refile_mut(&mut self) -> Option<&mut RefileChooser> {
        match self {
            Some(Chooser::Refile(c)) => Some(c),
            _ => None,
        }
    }
    fn take_refile(&mut self) -> Option<RefileChooser> {
        match self.take() {
            Some(Chooser::Refile(c)) => Some(c),
            other => {
                *self = other;
                None
            }
        }
    }
    fn open_refile(&mut self, c: RefileChooser) {
        *self = Some(Chooser::Refile(c));
    }
    fn recent(&self) -> Option<&RecentChooser> {
        match self {
            Some(Chooser::Recent(c)) => Some(c),
            _ => None,
        }
    }
    fn recent_mut(&mut self) -> Option<&mut RecentChooser> {
        match self {
            Some(Chooser::Recent(c)) => Some(c),
            _ => None,
        }
    }
    fn take_recent(&mut self) -> Option<RecentChooser> {
        match self.take() {
            Some(Chooser::Recent(c)) => Some(c),
            other => {
                *self = other;
                None
            }
        }
    }
    fn open_recent(&mut self, c: RecentChooser) {
        *self = Some(Chooser::Recent(c));
    }
    fn encoding(&self) -> Option<&EncodingChooser> {
        match self {
            Some(Chooser::Encoding(c)) => Some(c),
            _ => None,
        }
    }
    fn encoding_mut(&mut self) -> Option<&mut EncodingChooser> {
        match self {
            Some(Chooser::Encoding(c)) => Some(c),
            _ => None,
        }
    }
    fn take_encoding(&mut self) -> Option<EncodingChooser> {
        match self.take() {
            Some(Chooser::Encoding(c)) => Some(c),
            other => {
                *self = other;
                None
            }
        }
    }
    fn open_encoding(&mut self, c: EncodingChooser) {
        *self = Some(Chooser::Encoding(c));
    }
    fn macros(&self) -> Option<&MacroChooser> {
        match self {
            Some(Chooser::Macro(c)) => Some(c),
            _ => None,
        }
    }
    fn macros_mut(&mut self) -> Option<&mut MacroChooser> {
        match self {
            Some(Chooser::Macro(c)) => Some(c),
            _ => None,
        }
    }
    fn take_macros(&mut self) -> Option<MacroChooser> {
        match self.take() {
            Some(Chooser::Macro(c)) => Some(c),
            other => {
                *self = other;
                None
            }
        }
    }
    fn open_macros(&mut self, c: MacroChooser) {
        *self = Some(Chooser::Macro(c));
    }
    fn clipboard(&self) -> Option<&ClipboardChooser> {
        match self {
            Some(Chooser::Clipboard(c)) => Some(c),
            _ => None,
        }
    }
    fn clipboard_mut(&mut self) -> Option<&mut ClipboardChooser> {
        match self {
            Some(Chooser::Clipboard(c)) => Some(c),
            _ => None,
        }
    }
    fn take_clipboard(&mut self) -> Option<ClipboardChooser> {
        match self.take() {
            Some(Chooser::Clipboard(c)) => Some(c),
            other => {
                *self = other;
                None
            }
        }
    }
    fn open_clipboard(&mut self, c: ClipboardChooser) {
        *self = Some(Chooser::Clipboard(c));
    }
    fn workspace(&self) -> Option<&WorkspaceChooser> {
        match self {
            Some(Chooser::Workspace(c)) => Some(c),
            _ => None,
        }
    }
    fn workspace_mut(&mut self) -> Option<&mut WorkspaceChooser> {
        match self {
            Some(Chooser::Workspace(c)) => Some(c),
            _ => None,
        }
    }
    fn take_workspace(&mut self) -> Option<WorkspaceChooser> {
        match self.take() {
            Some(Chooser::Workspace(c)) => Some(c),
            other => {
                *self = other;
                None
            }
        }
    }
    fn open_workspace(&mut self, c: WorkspaceChooser) {
        *self = Some(Chooser::Workspace(c));
    }
    fn http_env(&self) -> Option<&HttpEnvChooser> {
        match self {
            Some(Chooser::HttpEnv(c)) => Some(c),
            _ => None,
        }
    }
    fn http_env_mut(&mut self) -> Option<&mut HttpEnvChooser> {
        match self {
            Some(Chooser::HttpEnv(c)) => Some(c),
            _ => None,
        }
    }
    fn take_http_env(&mut self) -> Option<HttpEnvChooser> {
        match self.take() {
            Some(Chooser::HttpEnv(c)) => Some(c),
            other => {
                *self = other;
                None
            }
        }
    }
    fn open_http_env(&mut self, c: HttpEnvChooser) {
        *self = Some(Chooser::HttpEnv(c));
    }
}

impl super::App {
    /// Routes a mouse event to the open chooser's handler. Returns `true`
    /// when a chooser is open (it swallows the event, as every overlay does).
    pub(super) fn chooser_mouse(&mut self, mouse: vix_input::MouseEvent) -> bool {
        match self.chooser {
            None => return false,
            Some(Chooser::Recent(_)) => self.recent_mouse(mouse),
            Some(Chooser::Location(_)) => self.location_mouse(mouse),
            Some(Chooser::Capture(_)) => self.capture_chooser_mouse(mouse),
            Some(Chooser::Refile(_)) => self.refile_chooser_mouse(mouse),
            Some(Chooser::Branch(_)) => self.branch_mouse(mouse),
            Some(Chooser::Stash(_)) => self.stash_chooser_mouse(mouse),
            Some(Chooser::Worktree(_)) => self.worktree_chooser_mouse(mouse),
            Some(Chooser::Task(_)) => self.tasks_mouse(mouse),
            Some(Chooser::Macro(_)) => self.macro_mouse(mouse),
            Some(Chooser::Encoding(_)) => self.encoding_mouse(mouse),
            Some(Chooser::HttpEnv(_)) => self.http_env_mouse(mouse),
            Some(Chooser::Script(_)) => self.script_chooser_mouse(mouse),
            Some(Chooser::Plugin(_)) => self.plugin_chooser_mouse(mouse),
            Some(Chooser::Clipboard(_)) => self.clipboard_mouse(mouse),
            Some(Chooser::Workspace(_)) => self.workspace_chooser_mouse(mouse),
        }
        true
    }
}
