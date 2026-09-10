//! Regression coverage for the bundled `themes/*.json` files (T203): pinned
//! color values so a slot can't silently drift, plus a name-uniqueness check
//! (which would have caught a real bug found while adding this: two files —
//! `phosphor-amber.json` and `safelight-red.json` — both declared
//! `"name": "Phosphor Amber"`, so the picker only ever offered one of the
//! two). `tests/snapshots.rs` covers each of these with a rendered-screen
//! smoke test; this file pins the actual RGB values `insta`'s plain-text
//! screen flattening can't see (it captures only glyphs, not color/style).

#![warn(clippy::pedantic)]
// Shared fixtures/helpers live in `common.rs`; this file needs `fs` from
// there, and a glob import earns its keep over a long explicit list.
#![allow(clippy::wildcard_imports)]

use crate::common::*;

fn theme(json: &str) -> vix::theme_model::CustomTheme {
    vix::theme_model::parse_theme(json).expect("bundled theme JSON parses")
}

#[test]
fn solarized_dark_pins_its_documented_palette() {
    let t = theme(include_str!("../../themes/solarized-dark.json"));
    assert_eq!(t.name, "Solarized Dark");
    assert_eq!(t.editor.background, Some([0, 43, 54]));
    assert_eq!(t.editor.foreground, Some([131, 148, 150]));
    assert_eq!(t.syntax.keyword, Some([133, 153, 0]));
    assert_eq!(t.syntax.string, Some([42, 161, 152]));
}

#[test]
fn solarized_light_pins_its_documented_palette() {
    let t = theme(include_str!("../../themes/solarized-light.json"));
    assert_eq!(t.name, "Solarized Light");
    assert_eq!(t.editor.background, Some([253, 246, 227]));
    assert_eq!(t.editor.foreground, Some([101, 123, 131]));
}

#[test]
fn tokyo_night_pins_its_documented_palette() {
    let t = theme(include_str!("../../themes/tokyo-night.json"));
    assert_eq!(t.name, "Tokyo Night");
    assert_eq!(t.editor.background, Some([26, 27, 38]));
    assert_eq!(t.editor.foreground, Some([192, 202, 245]));
}

#[test]
fn catppuccin_mocha_pins_its_documented_palette() {
    let t = theme(include_str!("../../themes/catppuccin-mocha.json"));
    assert_eq!(t.name, "Catppuccin Mocha");
    // The published Catppuccin Mocha base/text/mauve/green/peach values.
    assert_eq!(t.editor.background, Some([30, 30, 46]));
    assert_eq!(t.editor.foreground, Some([205, 214, 244]));
    assert_eq!(t.syntax.keyword, Some([203, 166, 247]));
    assert_eq!(t.syntax.string, Some([166, 227, 161]));
    assert_eq!(t.syntax.number, Some([250, 179, 135]));
}

#[test]
fn high_contrast_meets_wcag_aa_for_every_text_color() {
    let t = theme(include_str!("../../themes/high-contrast.json"));
    assert_eq!(t.name, "High Contrast");
    let bg = t.editor.background.expect("background set");
    assert_eq!(bg, [0, 0, 0], "pure black, for maximum headroom");
    // WCAG AA requires a contrast ratio of at least 4.5:1 for normal text
    // (https://www.w3.org/TR/WCAG21/#contrast-minimum); every text color this
    // theme uses clears that with room to spare (all are 11:1 or better —
    // see the ratios noted below, computed with the standard relative-
    // luminance formula).
    let fg = t.editor.foreground.expect("foreground set");
    assert_eq!(fg, [255, 255, 255], "pure white: 21:1 against black");
    assert_eq!(
        t.syntax.keyword,
        Some([255, 255, 0]),
        "yellow: 19.6:1 against black"
    );
    assert_eq!(
        t.syntax.string,
        Some([0, 255, 0]),
        "green: 15.3:1 against black"
    );
    assert_eq!(
        t.syntax.comment,
        Some([190, 190, 190]),
        "light gray: 11.3:1 against black — comments stay readable too"
    );
    assert_eq!(
        t.syntax.number,
        Some([0, 255, 255]),
        "cyan: 16.75:1 against black"
    );
}

#[test]
fn bundled_theme_names_are_unique() {
    // Every `themes/*.json` file, parsed the same way the app embeds them
    // (crate::app::bundled_themes isn't public, so this walks the directory
    // directly — same files, same parser).
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/themes");
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("themes/ exists")
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| fs::read_to_string(e.path()).ok())
        .filter_map(|json| vix::theme_model::parse_theme(&json))
        .map(|t| t.name)
        .collect();
    names.sort();
    let mut deduped = names.clone();
    deduped.dedup();
    assert_eq!(
        names, deduped,
        "two bundled theme files declare the same name — the picker would \
         only ever offer one of them"
    );
}

// ----- Theme editor (T202) -------------------------------------------------
//
// Driven through `run_action`/`on_key` only, like every other overlay's own
// tests (see `keybinding_editor_*` in `tests/integration/keybindings.rs`) —
// `open_theme_editor`/`theme_editor_key`/`save_theme_as` are `pub(super)`,
// internal to the `app` module, not part of the public API these tests (a
// separate crate) can call directly.
//
// `save_theme_as`'s actual disk write goes through `Settings::themes_dir()`,
// which (like `Settings::keybindings_path()`, see T204's own tests) has no
// test-only override — these tests cover everything up to but not including
// a successful save, so none of them submits a non-empty name.

#[test]
fn open_theme_editor_starts_from_the_active_default_theme() {
    let mut app = app_at(Path::new("."));
    assert_eq!(app.settings.theme, "dark", "the default");
    app.run_action("view.theme_edit");
    let editor = app.theme_editor.as_ref().expect("editor opened");
    // themes/dark.json's own documented values.
    assert_eq!(editor.theme.editor.foreground, Some([215, 215, 215]));
    assert_eq!(editor.theme.editor.background, Some([40, 40, 40]));
    assert_eq!(editor.selected, 0);
    assert!(!editor.dirty);
}

#[test]
fn theme_editor_navigation_moves_the_highlight() {
    let mut app = app_at(Path::new("."));
    app.run_action("view.theme_edit");
    app.on_key(keycode(KeyCode::Down));
    assert_eq!(app.theme_editor.as_ref().unwrap().selected, 1);
    app.on_key(keycode(KeyCode::Up));
    assert_eq!(app.theme_editor.as_ref().unwrap().selected, 0);
}

#[test]
fn enter_opens_the_x11_picker_over_the_theme_editor() {
    let mut app = app_at(Path::new("."));
    app.run_action("view.theme_edit");
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.x11_panel.is_some(), "picker opened");
    assert!(
        app.theme_editor.is_some(),
        "editor stays open underneath the picker"
    );
}

#[test]
fn picking_a_color_applies_it_to_the_selected_slot_and_returns_to_the_editor() {
    let mut app = app_at(Path::new("."));
    app.run_action("view.theme_edit");
    app.on_key(keycode(KeyCode::Enter)); // open the picker
    assert!(app.x11_panel.is_some());
    // Pick whatever the picker's own default selection is - this is about
    // the theme editor's slot actually changing, not which color it is.
    let picked = {
        let c = app.x11_panel.as_ref().unwrap().selected_color();
        [c.r, c.g, c.b]
    };
    app.on_key(keycode(KeyCode::Enter)); // choose it (routes to x11_key)
    assert!(
        app.x11_panel.is_none(),
        "the picker closes once a color is chosen"
    );
    let editor = app.theme_editor.as_ref().expect("back at the editor");
    assert_eq!(
        editor.theme.menu_bar.foreground,
        Some(picked),
        "the highlighted slot (row 0: Menu Bar Foreground) now holds the picked color"
    );
    assert!(editor.dirty);
}

#[test]
fn esc_closes_the_theme_editor_and_reverts_the_live_preview() {
    let mut app = app_at(Path::new("."));
    app.run_action("view.theme_edit");
    // Edit a slot (via the same real picker flow the UI uses) so the live
    // preview genuinely differs from the committed theme, then cancel.
    app.on_key(keycode(KeyCode::Enter));
    app.on_key(keycode(KeyCode::Enter));
    assert!(app.theme_editor.as_ref().unwrap().dirty, "an edit was made");
    app.on_key(esc());
    assert!(app.theme_editor.is_none());
    assert_eq!(
        app.settings.theme, "dark",
        "cancelling never touches the committed setting"
    );
    // The live-active theme model reverted too, not just the closed panel's
    // own draft - Dark's real menu-bar foreground, [215, 215, 215].
    assert_eq!(
        vix::theme::region_fg(vix::theme::Region::MenuBar),
        ratatui::style::Color::Rgb(215, 215, 215)
    );
}

#[test]
fn ctrl_s_opens_the_save_as_prompt() {
    let mut app = app_at(Path::new("."));
    app.run_action("view.theme_edit");
    app.on_key(ctrl('s'));
    let prompt = app.prompt.as_ref().expect("save-as prompt open");
    assert!(matches!(prompt.kind, vix::app::PromptKind::ThemeSaveAs));
}

#[test]
fn save_theme_as_with_an_empty_name_is_a_no_op() {
    // Submitting the prompt empty never reaches `Settings::themes_dir()`
    // (checked first, before any disk access), so this is safe to run for
    // real rather than needing a test-only override.
    let mut app = app_at(Path::new("."));
    app.run_action("view.theme_edit");
    app.on_key(ctrl('s'));
    app.on_key(keycode(KeyCode::Enter)); // submit the empty prompt
    assert!(
        app.theme_editor.is_some(),
        "an empty name leaves the editor open, nothing saved"
    );
}
