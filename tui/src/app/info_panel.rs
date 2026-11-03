//! The read-only info and picker panels as one enum (T611 step 2, second slice).
//!
//! `App::info` holds at most one [`InfoPanel`], so two of these panels can
//! never be open together. Each variant still has its own row in
//! [`super::overlays::OVERLAYS`]. See `spec/overlay-priority/index.md`.

use super::{AsciiPanel, FileInfoPanel, NerdPalette, SystemInfoPanel, TextInfoPanel};
use crate::app::lsp_status::LspStatusPanel;
use crate::media_type::Panel as MediaTypePanel;

/// The open info panel, one variant per panel.
pub(crate) enum InfoPanel {
    /// The Nerd Font glyph palette.
    NerdPalette(NerdPalette),
    /// The ASCII character table.
    AsciiPanel(AsciiPanel),
    /// The media-type (MIME) panel.
    MediaTypePanel(MediaTypePanel),
    /// The system information panel.
    SystemInfo(SystemInfoPanel),
    /// The LSP server status panel.
    LspStatus(LspStatusPanel),
    /// The file information panel.
    FileInfo(FileInfoPanel),
    /// The text statistics panel.
    TextInfo(TextInfoPanel),
}

/// Declares [`InfoSlot`] and its impl for `Option<InfoPanel>` from one list of
/// `variant, type, getter, getter_mut, opener, closer` rows.
macro_rules! info_slot {
    ($($var:ident, $ty:ty, $get:ident, $get_mut:ident, $open:ident, $close:ident;)*) => {
        /// Typed access to the slot that holds the open [`InfoPanel`], so a call
        /// site borrows only the `info` field.
        pub(crate) trait InfoSlot {
            $(
                /// The panel, if that is the open one.
                fn $get(&self) -> Option<&$ty>;
                /// The panel, mutably, if that is the open one.
                fn $get_mut(&mut self) -> Option<&mut $ty>;
                /// Opens the panel, replacing any other open info panel.
                fn $open(&mut self, p: $ty);
                /// Closes the panel if it is the open one; any other stays.
                fn $close(&mut self);
            )*
        }

        impl InfoSlot for Option<InfoPanel> {
            $(
                fn $get(&self) -> Option<&$ty> {
                    match self {
                        Some(InfoPanel::$var(p)) => Some(p),
                        _ => None,
                    }
                }
                fn $get_mut(&mut self) -> Option<&mut $ty> {
                    match self {
                        Some(InfoPanel::$var(p)) => Some(p),
                        _ => None,
                    }
                }
                fn $open(&mut self, p: $ty) {
                    *self = Some(InfoPanel::$var(p));
                }
                fn $close(&mut self) {
                    if matches!(self, Some(InfoPanel::$var(_))) {
                        *self = None;
                    }
                }
            )*
        }
    };
}

info_slot! {
    NerdPalette, NerdPalette, nerd_palette, nerd_palette_mut, open_nerd_palette, close_nerd_palette;
    AsciiPanel, AsciiPanel, ascii_panel, ascii_panel_mut, open_ascii_panel, close_ascii_panel;
    MediaTypePanel, MediaTypePanel, media_type_panel, media_type_panel_mut, open_media_type_panel, close_media_type_panel;
    SystemInfo, SystemInfoPanel, system_info, system_info_mut, open_system_info, close_system_info;
    LspStatus, LspStatusPanel, lsp_status, lsp_status_mut, open_lsp_status, close_lsp_status;
    FileInfo, FileInfoPanel, file_info, file_info_mut, open_file_info, close_file_info;
    TextInfo, TextInfoPanel, text_info, text_info_mut, open_text_info, close_text_info;
}
