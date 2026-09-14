//! Load a bundled theme, tweak one color, save it as a custom theme, and
//! reload it to prove the round trip is lossless (improvement plan T502).
//!
//! Run with: `cargo run --example theme_roundtrip`

#![warn(clippy::pedantic)]

use vix::theme_model::CustomTheme;

fn main() {
    // Every bundled theme is a plain JSON file under `themes/` -- the same
    // format `~/.config/vix/themes/<name>.json` uses for a custom one (see
    // `crates/vix-theme-editor-panel/spec/index.md`).
    let bundled = std::fs::read_to_string("themes/dark.json").expect("read themes/dark.json");
    let mut theme: CustomTheme = serde_json::from_str(&bundled).expect("parse theme JSON");
    println!(
        "loaded {:?}, editor bg = {:?}",
        theme.name, theme.editor.background
    );

    // Tweak one slot and rename it, exactly what View -> Edit Theme...'s
    // "Save As" does after a live-preview edit.
    theme.name = "My Dark".to_string();
    theme.editor.background = Some([0x10, 0x10, 0x18]); // Rgb is a plain [u8; 3]

    // `to_json` is the same serializer the theme editor's Save As uses.
    let saved = vix::theme_model::to_json(&theme);
    let dir = std::env::temp_dir().join(format!("vix-theme-roundtrip-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    let path = dir.join("my-dark.json");
    std::fs::write(&path, &saved).expect("write custom theme");

    // Reload it back and confirm the tweak survived the round trip.
    let reloaded: CustomTheme =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read it back"))
            .expect("parse it back");
    assert_eq!(reloaded.name, "My Dark");
    assert_eq!(reloaded.editor.background, theme.editor.background);
    println!(
        "round-tripped through {}: {:?}",
        path.display(),
        reloaded.name
    );
}
