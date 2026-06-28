//! Themes generated from [base16](https://github.com/chriskempson/base16)
//! palettes.
//!
//! A base16 scheme is 16 colors (`base00`–`base0F`): `base00`–`base07` run dark
//! to light (backgrounds → foregrounds), `base08`–`base0F` are accents. From that
//! compact data a whole [`CustomTheme`] falls out, so this module ships a dozen
//! well-known schemes from a small table rather than a JSON file each. The themes
//! are merged into the bundled set, so they appear in View → Theme automatically.
//!
//! Names are prefixed `Base16 …` to avoid colliding with the hand-authored JSON
//! themes in `themes/`.

#![warn(clippy::pedantic)]

use crate::theme_model::{parse_theme, CustomTheme};

/// One palette: a display name and the 16 `base00`–`base0F` colors as RGB hex
/// (no leading `#`), in order.
struct Palette {
    name: &'static str,
    colors: [&'static str; 16],
}

/// The bundled base16 palettes. Hex values are the canonical scheme definitions.
const PALETTES: &[Palette] = &[
    Palette {
        name: "Base16 Tomorrow Night",
        colors: [
            "1d1f21", "282a2e", "373b41", "969896", "b4b7b4", "c5c8c6", "e0e0e0", "ffffff",
            "cc6666", "de935f", "f0c674", "b5bd68", "8abeb7", "81a2be", "b294bb", "a3685a",
        ],
    },
    Palette {
        name: "Base16 Tomorrow",
        colors: [
            "ffffff", "e0e0e0", "d6d6d6", "8e908c", "969896", "4d4d4c", "282a2e", "1d1f21",
            "c82829", "f5871f", "eab700", "718c00", "3e999f", "4271ae", "8959a8", "a3685a",
        ],
    },
    Palette {
        name: "Base16 Solarized Dark",
        colors: [
            "002b36", "073642", "586e75", "657b83", "839496", "93a1a1", "eee8d5", "fdf6e3",
            "dc322f", "cb4b16", "b58900", "859900", "2aa198", "268bd2", "6c71c4", "d33682",
        ],
    },
    Palette {
        name: "Base16 Solarized Light",
        colors: [
            "fdf6e3", "eee8d5", "93a1a1", "839496", "657b83", "586e75", "073642", "002b36",
            "dc322f", "cb4b16", "b58900", "859900", "2aa198", "268bd2", "6c71c4", "d33682",
        ],
    },
    Palette {
        name: "Base16 Gruvbox Dark Hard",
        colors: [
            "1d2021", "3c3836", "504945", "665c54", "bdae93", "d5c4a1", "ebdbb2", "fbf1c7",
            "fb4934", "fe8019", "fabd2f", "b8bb26", "8ec07c", "83a598", "d3869b", "d65d0e",
        ],
    },
    Palette {
        name: "Base16 Nord",
        colors: [
            "2e3440", "3b4252", "434c5e", "4c566a", "d8dee9", "e5e9f0", "eceff4", "8fbcbb",
            "bf616a", "d08770", "ebcb8b", "a3be8c", "88c0d0", "81a1c1", "b48ead", "5e81ac",
        ],
    },
    Palette {
        name: "Base16 Ocean",
        colors: [
            "2b303b", "343d46", "4f5b66", "65737e", "a7adba", "c0c5ce", "dfe1e8", "eff1f5",
            "bf616a", "d08770", "ebcb8b", "a3be8c", "96b5b4", "8fa1b3", "b48ead", "ab7967",
        ],
    },
    Palette {
        name: "Base16 Monokai",
        colors: [
            "272822", "383830", "49483e", "75715e", "a59f85", "f8f8f2", "f5f4f1", "f9f8f5",
            "f92672", "fd971f", "f4bf75", "a6e22e", "a1efe4", "66d9ef", "ae81ff", "cc6633",
        ],
    },
    Palette {
        name: "Base16 Material",
        colors: [
            "263238", "2e3c43", "314549", "546e7a", "b2ccd6", "eeffff", "eeffff", "ffffff",
            "f07178", "f78c6c", "ffcb6b", "c3e88d", "89ddff", "82aaff", "c792ea", "ff5370",
        ],
    },
    Palette {
        name: "Base16 Dracula",
        colors: [
            "282936", "3a3c4e", "4d4f68", "626483", "62d6e8", "e9e9f4", "f1f2f8", "f7f7fb",
            "ea51b2", "b45bcf", "00f769", "ebff87", "a1efe4", "62d6e8", "b45bcf", "00f769",
        ],
    },
];

/// Convert a 6-digit hex string to a JSON `[r, g, b]` array. Falls back to black
/// for malformed input (the palettes above are all well-formed).
fn rgb(hex: &str) -> String {
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    format!("[{}, {}, {}]", byte(0), byte(2), byte(4))
}

/// Render one palette to the bundled theme JSON schema (see `themes/*.json`),
/// mapping base16 slots to Vix regions: `base00` background, `base05` foreground,
/// `base01` chrome background, `base03` comments, `base0B` strings, `base0D`
/// cursor, `base0E` keywords.
fn to_json(p: &Palette) -> String {
    let c = |i: usize| rgb(p.colors[i]);
    let region = |fg: usize, bg: usize| {
        format!(
            "{{\"foreground\": {}, \"background\": {}}}",
            c(fg),
            c(bg)
        )
    };
    format!(
        "{{\"name\": {name:?}, \"menu-bar\": {chrome}, \"status-bar\": {chrome}, \
         \"left-dock\": {chrome}, \"right-dock\": {chrome}, \
         \"editor\": {{\"foreground\": {fg}, \"background\": {bg}, \"cursor\": {cursor}}}, \
         \"syntax\": {{\"keyword\": {kw}, \"string\": {st}, \"comment\": {cm}}}}}",
        name = p.name,
        chrome = region(5, 1),
        fg = c(5),
        bg = c(0),
        cursor = c(13),
        kw = c(14),
        st = c(11),
        cm = c(3),
    )
}

/// All base16-derived themes, ready to merge into the bundled theme set.
#[must_use]
pub fn themes() -> Vec<CustomTheme> {
    PALETTES.iter().map(to_json).filter_map(|j| parse_theme(&j)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_palette_parses_into_a_theme() {
        let themes = themes();
        assert_eq!(themes.len(), PALETTES.len(), "all palettes parse");
        // Names are namespaced and unique.
        let mut names: Vec<&str> = themes.iter().map(|t| t.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), PALETTES.len(), "names are unique");
        assert!(themes.iter().all(|t| t.name.starts_with("Base16 ")));
    }

    #[test]
    fn colors_map_from_the_palette() {
        // Tomorrow Night: base00 background = 1d1f21, base05 foreground = c5c8c6.
        let t = themes().into_iter().find(|t| t.name == "Base16 Tomorrow Night").unwrap();
        assert_eq!(t.editor.background, Some([0x1d, 0x1f, 0x21]));
        assert_eq!(t.editor.foreground, Some([0xc5, 0xc8, 0xc6]));
    }

    #[test]
    fn rgb_parses_hex() {
        assert_eq!(rgb("ff8000"), "[255, 128, 0]");
    }
}
