//! Pointer hit-testing for the open menu bar.
//!
//! A click and a hover over the menus both begin by asking the same
//! question: what is the pointer over? The answer is one of the bar, the
//! third-level submenu, the submenu, the top-level dropdown, or nothing --
//! tried in that order, because the deeper dropdowns are drawn over the
//! shallower ones. `menu_mouse` and `menu_hover` each used to carry their own
//! copy of the three level-by-level checks; [`App::menu_target`] is the one
//! copy, and [`dropdown_item_at`] is its pure core, testable without an
//! [`App`].

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
use ratatui::layout::Rect;

use super::{App, rect_contains};
use crate::menu::Item;

/// What the pointer is over while a menu is open. The `Option<usize>` of a
/// dropdown is the index of the *selectable* item under the pointer: `None`
/// for the border, a gap past the last item, or a separator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Target {
    /// The menu bar itself.
    Bar,
    /// The third-level submenu.
    SubSub(Option<usize>),
    /// The submenu.
    Sub(Option<usize>),
    /// The top-level dropdown.
    Top(Option<usize>),
    /// Outside the bar and every open dropdown.
    Outside,
}

/// The index of the selectable item in `items` that row `row` shows in a
/// dropdown drawn in `rect`, whose highlighted row is `highlighted` (a long
/// list scrolls to keep it visible). The dropdown's border takes the top row,
/// so items start one row below `rect.y`. `None` for the border, a row past
/// the last item, or a separator.
pub(super) fn dropdown_item_at(
    rect: Rect,
    highlighted: Option<usize>,
    items: &[Item],
    row: u16,
) -> Option<usize> {
    let top = rect.y.plus(1);
    if row < top {
        return None;
    }
    let offset =
        crate::ui::dropdown_scroll(highlighted, usize::from(rect.height.minus(2)), items.len());
    let idx = offset.plus(usize::from(row.minus(top)));
    items
        .get(idx)
        .filter(|item| !item.is_separator())
        .map(|_| idx)
}

impl App {
    /// What the pointer at `(col, row)` is over; see [`Target`].
    pub(super) fn menu_target(&self, col: u16, row: u16) -> Target {
        if rect_contains(self.layout.menu, col, row) {
            return Target::Bar;
        }
        if self.menu.subsubmenu_open() {
            let rect = self.layout.subsubmenu_dropdown;
            if rect_contains(rect, col, row) {
                let item = self
                    .menu
                    .subsubmenu_items()
                    .and_then(|items| dropdown_item_at(rect, self.menu.subsub, items, row));
                return Target::SubSub(item);
            }
        }
        if self.menu.submenu_open() {
            let rect = self.layout.submenu_dropdown;
            if rect_contains(rect, col, row) {
                let item = self
                    .menu
                    .submenu_items()
                    .and_then(|items| dropdown_item_at(rect, self.menu.sub, items, row));
                return Target::Sub(item);
            }
        }
        let rect = self.layout.menu_dropdown;
        if rect_contains(rect, col, row) {
            let item = self.menu.open.and_then(|mi| {
                let def = self.menu.defs().get(mi)?;
                dropdown_item_at(rect, self.menu.item, def.items, row)
            });
            return Target::Top(item);
        }
        Target::Outside
    }

    /// The items of the dropdown a [`Target`] names, if it names one.
    fn menu_level_items(&self, target: Target) -> Option<&'static [Item]> {
        match target {
            Target::SubSub(_) => self.menu.subsubmenu_items(),
            Target::Sub(_) => self.menu.submenu_items(),
            Target::Top(_) => self
                .menu
                .open
                .and_then(|mi| self.menu.defs().get(mi).map(|def| def.items)),
            Target::Bar | Target::Outside => None,
        }
    }

    /// The item under the pointer at `target`, with its dropdown's items.
    fn menu_item_under(&self, target: Target) -> Option<(usize, &'static Item)> {
        let (Target::SubSub(Some(idx)) | Target::Sub(Some(idx)) | Target::Top(Some(idx))) = target
        else {
            return None;
        };
        self.menu_level_items(target)
            .and_then(|items| items.get(idx).map(|item| (idx, item)))
    }

    /// Handle a left click while the menu is open: the bar switches or
    /// toggles menus, an item runs (or opens its submenu), and a click
    /// outside everything dismisses the menu.
    pub(super) fn menu_mouse(&mut self, col: u16, row: u16) {
        let target = self.menu_target(col, row);
        match target {
            Target::Bar => self.menu_click(col),
            Target::Outside => self.close_menu(),
            Target::SubSub(_) | Target::Sub(_) | Target::Top(_) => {
                let Some((idx, item)) = self.menu_item_under(target) else {
                    return;
                };
                if item.has_submenu() {
                    match target {
                        Target::Sub(_) => self.menu.highlight_sub(idx),
                        Target::Top(_) => self.menu.highlight_item(idx),
                        _ => {}
                    }
                    self.menu.right(); // opens the next level (nothing highlighted)
                } else {
                    self.run_action(item.action);
                    self.close_menu();
                }
            }
        }
    }

    /// Move the open-menu highlight to follow the pointer: hovering another
    /// top-level name switches to that menu; hovering a dropdown row
    /// highlights that item (and previews a theme). Never commits an action
    /// or closes the menu.
    pub(super) fn menu_hover(&mut self, col: u16, row: u16) {
        let target = self.menu_target(col, row);
        let item = self.menu_item_under(target);
        let mut previewed = false;
        match (target, item) {
            (Target::Bar, _) => {
                if let Some(i) = self.top_menu_index_at(col)
                    && self.menu.open != Some(i)
                {
                    self.menu.open_index(i);
                }
            }
            (Target::SubSub(_), Some((idx, _))) => self.menu.subsub = Some(idx),
            (Target::Sub(_), Some((idx, item))) => {
                self.menu.highlight_sub(idx);
                if item.has_submenu() {
                    self.menu.right(); // reveal the third level on hover
                }
                self.preview_menu_theme(item.action);
                previewed = true;
            }
            (Target::Top(_), Some((idx, item))) => {
                self.menu.highlight_item(idx);
                if item.has_submenu() {
                    self.menu.right(); // reveal the submenu on hover
                }
            }
            _ => {}
        }
        // Any pointer position other than a hovered theme item drops a live
        // theme preview.
        if !previewed {
            self.revert_theme_preview();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A menu's items together with the index of one separator in them.
    fn items_with_a_separator() -> (&'static [Item], usize) {
        // `menus()` caches its first answer for the whole test binary, so the
        // build's capability filter must be installed before it is first
        // asked, exactly as `App::new` does; otherwise a test run without
        // default features would see menu entries this build cannot run.
        crate::menu::set_action_filter(crate::capability::is_available);
        crate::menu::menus()
            .iter()
            .find_map(|m| {
                let sep = m.items.iter().position(Item::is_separator)?;
                (sep > 0 && sep + 1 < m.items.len()).then_some((m.items, sep))
            })
            .expect("some menu has an interior separator")
    }

    fn rect_for(visible: usize) -> Rect {
        // Border rows above and below the visible items.
        Rect::new(4, 1, 30, u16::try_from(visible + 2).unwrap())
    }

    #[test]
    fn rows_map_to_items_below_the_top_border() {
        let (items, sep) = items_with_a_separator();
        let rect = rect_for(items.len());
        assert_eq!(
            dropdown_item_at(rect, None, items, rect.y),
            None,
            "the border"
        );
        assert_eq!(dropdown_item_at(rect, None, items, rect.y + 1), Some(0));
        let on_sep = rect.y + 1 + u16::try_from(sep).unwrap();
        assert_eq!(
            dropdown_item_at(rect, None, items, on_sep),
            None,
            "a separator"
        );
        assert_eq!(
            dropdown_item_at(rect, None, items, on_sep + 1),
            Some(sep + 1)
        );
        let past = rect.y + 1 + u16::try_from(items.len()).unwrap();
        assert_eq!(
            dropdown_item_at(rect, None, items, past),
            None,
            "past the end"
        );
        assert_eq!(
            dropdown_item_at(rect, None, items, 0),
            None,
            "above the dropdown"
        );
    }

    #[test]
    fn a_scrolled_dropdown_maps_rows_to_the_items_it_shows() {
        let (items, _) = items_with_a_separator();
        assert!(items.len() > 4);
        let rect = rect_for(3);
        // Highlighting the last item scrolls the window to the end.
        let last = items.len() - 1;
        let offset = crate::ui::dropdown_scroll(Some(last), 3, items.len());
        assert!(offset > 0);
        let hit = dropdown_item_at(rect, Some(last), items, rect.y + 1);
        let expected = (!items[offset].is_separator()).then_some(offset);
        assert_eq!(hit, expected, "the first visible row shows item {offset}");
    }
}
