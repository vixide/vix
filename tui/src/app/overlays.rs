//! The one overlay priority table (T611).
//!
//! Every modal layer that can sit over the editor -- menus, prompts, choosers,
//! tool panels, dialogs -- has exactly one row in [`OVERLAYS`], listed from the
//! bottom of the stack to the top. The frame draws the open rows in table
//! order (`crate::ui::draw`), and a key goes to the open rows in the reverse
//! order (`App::route_overlay_key`), so the layer drawn on top is by
//! construction the layer that receives keys. See
//! `spec/overlay-priority/index.md`.

use vix_input::KeyEvent;

use super::{App, ConfirmKind, Visible};

/// Which layer a row stands for. The discriminant is the row's index in
/// [`OVERLAYS`] (checked by a test), so an id also names a stacking height.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverlayId {
    /// The `Jump` layer.
    Jump,
    /// The `Calendar` layer.
    Calendar,
    /// The `Clock` layer.
    Clock,
    /// The `Menu` layer.
    Menu,
    /// The `Search` layer.
    Search,
    /// The `Palette` layer.
    Palette,
    /// The `WorkspaceSearch` layer.
    WorkspaceSearch,
    /// The `KeybindingEditor` layer.
    KeybindingEditor,
    /// The `GitRebase` layer (below `Prompt`: its reword prompt opens over it).
    GitRebase,
    /// The `Prompt` layer.
    Prompt,
    /// The `QueryReplace` layer.
    QueryReplace,
    /// The `Confirm` layer.
    Confirm,
    /// The `ScriptTrust` layer.
    ScriptTrust,
    /// The `ReplaceConfirm` layer.
    ReplaceConfirm,
    /// The `Unsaved` layer.
    Unsaved,
    /// The `SpellSuggest` layer.
    SpellSuggest,
    /// The `ContextMenu` layer.
    ContextMenu,
    /// The `GitPanel` layer.
    GitPanel,
    /// The `BranchChooser` layer.
    BranchChooser,
    /// The `TaskChooser` layer.
    TaskChooser,
    /// The `StashChooser` layer.
    StashChooser,
    /// The `WorktreeChooser` layer.
    WorktreeChooser,
    /// The `ScriptChooser` layer.
    ScriptChooser,
    /// The `PluginChooser` layer.
    PluginChooser,
    /// The `DiffView` layer.
    DiffView,
    /// The `LocationChooser` layer.
    LocationChooser,
    /// The `CaptureChooser` layer.
    CaptureChooser,
    /// The `RefileChooser` layer.
    RefileChooser,
    /// The `RecentChooser` layer.
    RecentChooser,
    /// The `FileBrowser` layer.
    FileBrowser,
    /// The `NerdPalette` layer.
    NerdPalette,
    /// The `AsciiPanel` layer.
    AsciiPanel,
    /// The `EditTable` layer.
    EditTable,
    /// The `ColumnView` layer.
    ColumnView,
    /// The `EditOutline` layer.
    EditOutline,
    /// The `EditValue` layer.
    EditValue,
    /// The `EditBytes` layer.
    EditBytes,
    /// The `LargeFile` layer.
    LargeFile,
    /// The `Qrcode` layer.
    Qrcode,
    /// The `AiPanel` layer.
    AiPanel,
    /// The `Terminal` layer.
    Terminal,
    /// The `CoworkTerminal` layer.
    CoworkTerminal,
    /// The `CoworkConfirm` layer.
    CoworkConfirm,
    /// The `AiDiff` layer.
    AiDiff,
    /// The `ThemeEditor` layer.
    ThemeEditor,
    /// The `UndoTree` layer.
    UndoTree,
    /// The `X11Panel` layer.
    X11Panel,
    /// The `HtmlPanel` layer.
    HtmlPanel,
    /// The `WhichKey` layer.
    WhichKey,
    /// The `WgrepConfirm` layer.
    WgrepConfirm,
    /// The `DirEditConfirm` layer.
    DirEditConfirm,
    /// The `GitLog` layer.
    GitLog,
    /// The `GitGraph` layer.
    GitGraph,
    /// The `Oc` layer.
    Oc,
    /// The `ImDialog` layer.
    ImDialog,
    /// The `TodoTree` layer.
    TodoTree,
    /// The `FhirBundle` layer.
    FhirBundle,
    /// The `Hl7Panel` layer.
    Hl7Panel,
    /// The `HttpEnvChooser` layer.
    HttpEnvChooser,
    /// The `ChoiceList` layer.
    ChoiceList,
    /// The `StructuralReplace` layer.
    StructuralReplace,
    /// The `EditSql` layer.
    EditSql,
    /// The `Db` layer.
    Db,
    /// The `MediaTypePanel` layer.
    MediaTypePanel,
    /// The `EncodingChooser` layer.
    EncodingChooser,
    /// The `MacroChooser` layer.
    MacroChooser,
    /// The `ClipboardChooser` layer.
    ClipboardChooser,
    /// The `WorkspaceChooser` layer.
    WorkspaceChooser,
    /// The `SystemInfo` layer.
    SystemInfo,
    /// The `LspStatus` layer.
    LspStatus,
    /// The `FileInfo` layer.
    FileInfo,
    /// The `TextInfo` layer.
    TextInfo,
    /// The `MarkdownPreview` layer.
    MarkdownPreview,
    /// The `MarkdownToc` layer.
    MarkdownToc,
    /// The `Snippets` layer.
    Snippets,
    /// The `Contacts` layer.
    Contacts,
    /// The `Vcard` layer.
    Vcard,
    /// The `Dashboard` layer.
    Dashboard,
    /// The `Outline` layer.
    Outline,
    /// The `ConflictList` layer.
    ConflictList,
    /// The `Completion` layer.
    Completion,
    /// The `Hover` layer.
    Hover,
    /// The `Paste` layer.
    Paste,
    /// The `Help` layer.
    Help,
    /// The `Dialog` layer.
    Dialog,
    /// The `ColorConverter` layer.
    ColorConverter,
    /// The `UnitConverter` layer.
    UnitConverter,
    /// The `Calculator` layer.
    Calculator,
    /// The `RegexTester` layer.
    RegexTester,
    /// The `CodeActions` layer.
    CodeActions,
    /// The `Pomodoro` layer.
    Pomodoro,
    /// The `Welcome` layer.
    Welcome,
}

/// How a layer takes keys.
#[derive(Clone, Copy)]
pub(crate) enum Key {
    /// Drawn only; never consumes a key (a popup that the next key dismisses
    /// or that is purely informational).
    None,
    /// Consumes keys, but `App::dispatch_key` hands them over before the
    /// table is consulted (jump-label mode, which falls through on a miss).
    Early,
    /// The layer's key handler; it returns `false` to let the key fall
    /// through to the layer beneath (the terminal does, for its own chords).
    Handler(fn(&mut App, KeyEvent) -> bool),
}

/// One row of [`OVERLAYS`].
#[derive(Clone, Copy)]
pub(crate) struct Overlay {
    /// The layer this row stands for.
    pub(crate) id: OverlayId,
    /// Whether the layer is currently open (drawn, and offered keys).
    pub(crate) open: fn(&App) -> bool,
    /// How the layer takes keys.
    pub(crate) key: Key,
}

impl Overlay {
    /// Whether this layer takes keystrokes while open.
    pub(crate) const fn takes_keys(&self) -> bool {
        !matches!(self.key, Key::None)
    }
}

/// A row for the common case: open while `app.$field` is `Some`, and its
/// `App::$handler` takes every key.
macro_rules! unit {
    ($id:ident, $field:ident, $handler:ident) => {
        Overlay {
            id: OverlayId::$id,
            open: |a| a.$field.is_some(),
            key: Key::Handler(|a, k| {
                a.$handler(k);
                true
            }),
        }
    };
}

/// A row for a layer that is drawn but never takes keys.
macro_rules! draw_only {
    ($id:ident, $field:ident) => {
        Overlay {
            id: OverlayId::$id,
            open: |a| a.$field.is_some(),
            key: Key::None,
        }
    };
}

/// Every overlay, from the bottom of the stack to the top. Drawing walks this
/// forwards; key routing walks it backwards.
pub(crate) const OVERLAYS: &[Overlay] = &[
    Overlay {
        id: OverlayId::Jump,
        open: |a| a.jump.is_some(),
        key: Key::Early,
    },
    Overlay {
        id: OverlayId::Calendar,
        open: |a| a.visible.contains(Visible::CALENDAR),
        key: Key::Handler(|a, k| {
            a.calendar_key(k);
            true
        }),
    },
    Overlay {
        id: OverlayId::Clock,
        open: |a| a.visible.contains(Visible::CLOCK),
        key: Key::Handler(|a, k| {
            a.clock_key(k);
            true
        }),
    },
    Overlay {
        id: OverlayId::Menu,
        open: |a| a.menu.is_open(),
        key: Key::Handler(|a, k| {
            a.menu_key(k);
            true
        }),
    },
    unit!(Search, search, search_key),
    unit!(Palette, palette, palette_key),
    unit!(WorkspaceSearch, workspace_search, workspace_search_key),
    unit!(KeybindingEditor, keybinding_editor, keybinding_editor_key),
    // Below `Prompt`: `r` opens a reword prompt over the rebase overlay, and the
    // prompt has to win the key.
    unit!(GitRebase, git_rebase, git_rebase_key),
    unit!(Prompt, prompt, prompt_key),
    unit!(QueryReplace, query_replace, qr_key),
    unit!(Confirm, confirm, confirm_key),
    unit!(ScriptTrust, script_trust, script_trust_key),
    unit!(ReplaceConfirm, replace_confirm, replace_confirm_key),
    unit!(Unsaved, unsaved, unsaved_key),
    unit!(SpellSuggest, spell_suggest, spell_suggest_key),
    unit!(ContextMenu, context_menu, context_menu_key),
    unit!(GitPanel, git_panel, git_panel_key),
    unit!(BranchChooser, branch_chooser, branch_key),
    unit!(TaskChooser, task_chooser, tasks_key),
    unit!(StashChooser, stash_chooser, stash_chooser_key),
    unit!(WorktreeChooser, worktree_chooser, worktree_chooser_key),
    unit!(ScriptChooser, script_chooser, script_chooser_key),
    unit!(PluginChooser, plugin_chooser, plugin_chooser_key),
    unit!(DiffView, diff_view, diff_view_key),
    unit!(LocationChooser, location_chooser, location_key),
    unit!(CaptureChooser, capture_chooser, capture_chooser_key),
    unit!(RefileChooser, refile_chooser, refile_chooser_key),
    unit!(RecentChooser, recent_chooser, recent_key),
    unit!(FileBrowser, file_browser, file_browser_key),
    unit!(NerdPalette, nerd_palette, nerd_key),
    unit!(AsciiPanel, ascii_panel, ascii_key),
    unit!(EditTable, edit_table, edit_table_key),
    unit!(ColumnView, column_view, column_view_key),
    unit!(EditOutline, edit_outline, edit_outline_key),
    unit!(EditValue, edit_value, edit_value_key),
    unit!(EditBytes, edit_bytes, edit_bytes_key),
    unit!(LargeFile, large_file, large_file_key),
    unit!(Qrcode, qrcode, qrcode_key),
    unit!(AiPanel, ai_panel, ai_panel_key),
    Overlay {
        id: OverlayId::Terminal,
        open: |a| a.terminal_visible(),
        key: Key::Handler(App::terminal_key),
    },
    Overlay {
        id: OverlayId::CoworkTerminal,
        open: |a| a.cowork_terminal_viewing(),
        key: Key::Handler(|a, k| {
            a.cowork_terminal_key(k);
            true
        }),
    },
    Overlay {
        id: OverlayId::CoworkConfirm,
        open: |a| {
            a.confirm.as_ref().is_some_and(|c| {
                matches!(
                    c.kind,
                    ConfirmKind::CoworkShareTerminal | ConfirmKind::CoworkTerminalInput
                )
            })
        },
        key: Key::Handler(|a, k| {
            a.confirm_key(k);
            true
        }),
    },
    Overlay {
        id: OverlayId::AiDiff,
        open: |a| a.ai_diff.is_some(),
        key: Key::Handler(|a, k| {
            a.ai_diff_key(k);
            true
        }),
    },
    unit!(ThemeEditor, theme_editor, theme_editor_key),
    unit!(UndoTree, undo_tree, undo_tree_key),
    unit!(X11Panel, x11_panel, x11_key),
    unit!(HtmlPanel, html_panel, html_key),
    Overlay {
        id: OverlayId::WhichKey,
        open: |a| a.which_key().is_some(),
        key: Key::None,
    },
    unit!(WgrepConfirm, wgrep_confirm, wgrep_confirm_key),
    unit!(DirEditConfirm, dir_edit_confirm, dir_edit_confirm_key),
    unit!(GitLog, git_log, git_log_key),
    unit!(GitGraph, git_graph, git_graph_key),
    unit!(Oc, oc, oc_key),
    unit!(ImDialog, im_dialog, im_dialog_key),
    unit!(TodoTree, todo_tree, todo_tree_key),
    unit!(FhirBundle, fhir_bundle, fhir_bundle_key),
    unit!(Hl7Panel, hl7_panel, hl7_key),
    unit!(HttpEnvChooser, http_env_chooser, http_env_key),
    unit!(ChoiceList, choice_list, choice_list_key),
    unit!(StructuralReplace, structural_replace, sr_key),
    unit!(EditSql, edit_sql, edit_sql_key),
    unit!(Db, db, db_key),
    unit!(MediaTypePanel, media_type_panel, media_type_key),
    unit!(EncodingChooser, encoding_chooser, encoding_key),
    unit!(MacroChooser, macro_chooser, macro_key),
    unit!(ClipboardChooser, clipboard_chooser, clipboard_key),
    unit!(WorkspaceChooser, workspace_chooser, workspace_chooser_key),
    unit!(SystemInfo, system_info, system_info_key),
    unit!(LspStatus, lsp_status, lsp_status_key),
    unit!(FileInfo, file_info, file_info_key),
    unit!(TextInfo, text_info, text_info_key),
    unit!(MarkdownPreview, markdown_preview, markdown_preview_key),
    unit!(MarkdownToc, markdown_toc, markdown_toc_key),
    unit!(Snippets, snippets, snippets_key),
    unit!(Contacts, contacts, contacts_key),
    unit!(Vcard, vcard, vcard_key),
    unit!(Dashboard, dashboard, dashboard_key),
    unit!(Outline, outline, outline_key),
    unit!(ConflictList, conflict_list, conflict_list_key),
    draw_only!(Completion, completion),
    draw_only!(Hover, hover),
    Overlay {
        id: OverlayId::Paste,
        open: |a| a.paste.as_ref().is_some_and(|p| p.conflict.is_some()),
        key: Key::Handler(|a, k| {
            a.paste_key(k);
            true
        }),
    },
    unit!(Help, help, help_key),
    unit!(Dialog, dialog, dialog_key),
    unit!(ColorConverter, color_converter, color_converter_key),
    unit!(UnitConverter, unit_converter, unit_converter_key),
    unit!(Calculator, calculator, calculator_key),
    unit!(RegexTester, regex_tester, regex_tester_key),
    unit!(CodeActions, code_actions, code_action_key),
    Overlay {
        id: OverlayId::Pomodoro,
        open: |a| a.visible.contains(Visible::POMODORO),
        key: Key::Handler(|a, k| {
            a.pomodoro_key(k);
            true
        }),
    },
    unit!(Welcome, welcome, welcome_key),
];

impl App {
    /// The open overlays in drawing order (bottom of the stack first).
    pub(crate) fn open_overlays(&self) -> impl DoubleEndedIterator<Item = &'static Overlay> + '_ {
        OVERLAYS.iter().filter(|o| (o.open)(self))
    }

    /// Whether a modal layer is taking keystrokes, so a keypress does not
    /// reach the editor (and a bracketed paste belongs to that input, not to
    /// the buffer behind it).
    pub(crate) fn overlay_capturing_keys(&self) -> bool {
        self.open_overlays().any(Overlay::takes_keys)
    }

    /// Offer `key` to the open overlays, topmost first. Returns `true` when a
    /// layer consumed it.
    pub(crate) fn route_overlay_key(&mut self, key: KeyEvent) -> bool {
        // The same iterator the frame draws from, walked from the top.
        let offered: Vec<Overlay> = self.open_overlays().rev().copied().collect();
        for o in offered {
            if let Key::Handler(handler) = o.key
                && handler(self, key)
            {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use vix_input::{KeyCode, KeyModifiers};

    fn app() -> App {
        App::new(std::env::temp_dir(), Settings::default())
    }

    fn esc() -> KeyEvent {
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)
    }

    #[test]
    fn a_row_sits_at_the_index_its_id_names() {
        for (i, o) in OVERLAYS.iter().enumerate() {
            assert_eq!(o.id as usize, i, "row {i} is out of step with its id");
        }
        assert_eq!(
            OVERLAYS.last().map(|o| o.id),
            Some(OverlayId::Welcome),
            "the topmost layer is the welcome panel"
        );
    }

    #[test]
    fn drawing_order_and_key_order_are_one_table_read_in_opposite_directions() {
        let mut a = app();
        a.open_palette();
        a.open_help();
        a.welcome = Some(crate::app::WelcomePanel::open(vec![String::from("hi")]));
        let drawn: Vec<OverlayId> = a.open_overlays().map(|o| o.id).collect();
        assert_eq!(
            drawn,
            [OverlayId::Palette, OverlayId::Help, OverlayId::Welcome]
        );
        // Keys are offered to the layers in exactly the reverse order.
        let offered: Vec<OverlayId> = a.open_overlays().rev().map(|o| o.id).collect();
        let mut reversed = drawn;
        reversed.reverse();
        assert_eq!(offered, reversed);
        // And the topmost drawn layer is the one that really gets the key:
        // Esc closes the welcome panel first, then help, then the palette.
        a.route_overlay_key(esc());
        assert!(a.welcome.is_none() && a.help.is_some() && a.palette.is_some());
        a.route_overlay_key(esc());
        assert!(a.help.is_none() && a.palette.is_some());
        a.route_overlay_key(esc());
        assert!(a.palette.is_none());
        assert!(!a.overlay_capturing_keys());
    }

    #[test]
    fn every_layer_that_takes_keys_is_drawn_and_every_drawn_layer_has_a_key_policy() {
        // `Key::None` is a deliberate, small set: popups the next key dismisses
        // or that only inform. A new overlay must choose a policy to compile.
        let drawn_only: Vec<OverlayId> = OVERLAYS
            .iter()
            .filter(|o| !o.takes_keys())
            .map(|o| o.id)
            .collect();
        assert_eq!(
            drawn_only,
            [OverlayId::WhichKey, OverlayId::Completion, OverlayId::Hover]
        );
    }
}
