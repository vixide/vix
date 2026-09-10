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
