//! The full-screen edit surfaces as one enum (T611 step 2, third slice).
//!
//! `App::edit` holds at most one [`EditSurface`], so two of these editors can
//! never be open together. Each variant still has its own row in
//! [`super::overlays::OVERLAYS`]. See `spec/overlay-priority/index.md`.

use crate::column_view::ColumnView;
use crate::edit_bytes::Hex;
use crate::edit_outline::Tree as OutlineTree;
use crate::edit_sql::Editor as SqlEditor;
use crate::edit_table::Grid;
use crate::edit_value::Tree as ValueTree;

/// The open edit surface, one variant per editor.
pub(crate) enum EditSurface {
    /// The CSV/TSV table editor.
    EditTable(Grid),
    /// The interactive Org Column View.
    ColumnView(ColumnView),
    /// The prose outline editor.
    EditOutline(OutlineTree),
    /// The JSON/YAML value editor.
    EditValue(ValueTree),
    /// The byte (hex) editor.
    EditBytes(Hex),
    /// The SQL statement editor.
    EditSql(SqlEditor),
}

/// Declares [`EditSlot`] and its impl for `Option<EditSurface>` from one list
/// of `variant, type, getter, getter_mut, opener, closer` rows.
macro_rules! edit_slot {
    ($($var:ident, $ty:ty, $get:ident, $get_mut:ident, $open:ident, $close:ident;)*) => {
        /// Typed access to the slot that holds the open [`EditSurface`], so a
        /// call site borrows only the `edit` field.
        pub(crate) trait EditSlot {
            $(
                /// The editor, if that is the open one.
                fn $get(&self) -> Option<&$ty>;
                /// The editor, mutably, if that is the open one.
                fn $get_mut(&mut self) -> Option<&mut $ty>;
                /// Opens the editor, replacing any other open edit surface.
                fn $open(&mut self, e: $ty);
                /// Closes the editor if it is the open one; any other stays.
                fn $close(&mut self);
            )*
        }

        impl EditSlot for Option<EditSurface> {
            $(
                fn $get(&self) -> Option<&$ty> {
                    match self {
                        Some(EditSurface::$var(e)) => Some(e),
                        _ => None,
                    }
                }
                fn $get_mut(&mut self) -> Option<&mut $ty> {
                    match self {
                        Some(EditSurface::$var(e)) => Some(e),
                        _ => None,
                    }
                }
                fn $open(&mut self, e: $ty) {
                    *self = Some(EditSurface::$var(e));
                }
                fn $close(&mut self) {
                    if matches!(self, Some(EditSurface::$var(_))) {
                        *self = None;
                    }
                }
            )*
        }
    };
}

edit_slot! {
    EditTable, Grid, edit_table, edit_table_mut, open_edit_table, close_edit_table;
    ColumnView, ColumnView, column_view, column_view_mut, open_column_view, close_column_view;
    EditOutline, OutlineTree, edit_outline, edit_outline_mut, open_edit_outline, close_edit_outline;
    EditValue, ValueTree, edit_value, edit_value_mut, open_edit_value, close_edit_value;
    EditBytes, Hex, edit_bytes, edit_bytes_mut, open_edit_bytes, close_edit_bytes;
    EditSql, SqlEditor, edit_sql, edit_sql_mut, open_edit_sql, close_edit_sql;
}
