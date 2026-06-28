//! Persisted keyboard macros: named recordings of editor key events.
//!
//! The host records a macro as a `Vec<KeyEvent>`; this module serializes those
//! events to compact text tokens so they can be saved to `macros.toml` and
//! replayed in a later session. Each token is the key plus modifier prefixes
//! (`C-` ctrl, `A-` alt, `S-` shift), e.g. `C-c`, `S-Tab`, `Enter`, `a`.
//!
//! ```toml
//! [[macro]]
//! name = "wrap-parens"
//! keys = ["(", "Right", "Right"]
//! ```

#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};

/// One saved macro: a display name and its tokenized key sequence.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Macro {
    /// Display name shown in the chooser.
    pub name: String,
    /// Tokenized key events (see module docs), replayed in order.
    pub keys: Vec<String>,
}

/// The `macros.toml` schema: a list of `[[macro]]` tables.
#[derive(Debug, Default, Deserialize, Serialize)]
struct MacrosFile {
    #[serde(default, rename = "macro")]
    macros: Vec<Macro>,
}

/// Encode a key event to a token (`C-`/`A-`/`S-` prefixes + key name).
#[must_use]
pub fn encode_key(key: KeyEvent) -> String {
    let mut s = String::new();
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        s.push_str("C-");
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        s.push_str("A-");
    }
    // Shift is implicit in an uppercase char; only record it for named keys.
    let is_char = matches!(key.code, KeyCode::Char(_));
    if key.modifiers.contains(KeyModifiers::SHIFT) && !is_char {
        s.push_str("S-");
    }
    s.push_str(&encode_code(key.code));
    s
}

/// Name for a key code (single chars verbatim; `Space` for `' '`).
fn encode_code(code: KeyCode) -> String {
    match code {
        KeyCode::Char(' ') => "Space".to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::BackTab => "BackTab".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Delete => "Delete".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Up => "Up".to_string(),
        KeyCode::Down => "Down".to_string(),
        KeyCode::Left => "Left".to_string(),
        KeyCode::Right => "Right".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "PageUp".to_string(),
        KeyCode::PageDown => "PageDown".to_string(),
        KeyCode::Insert => "Insert".to_string(),
        KeyCode::F(n) => format!("F{n}"),
        _ => String::new(),
    }
}

/// Decode a token back to a key event, or `None` if unrecognized/empty.
#[must_use]
pub fn decode_key(token: &str) -> Option<KeyEvent> {
    let mut rest = token;
    let mut mods = KeyModifiers::NONE;
    loop {
        if let Some(r) = rest.strip_prefix("C-") {
            mods |= KeyModifiers::CONTROL;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("A-") {
            mods |= KeyModifiers::ALT;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("S-") {
            mods |= KeyModifiers::SHIFT;
            rest = r;
        } else {
            break;
        }
    }
    let code = decode_code(rest)?;
    Some(KeyEvent::new(code, mods))
}

/// Parse a key-code name (the inverse of [`encode_code`]).
fn decode_code(name: &str) -> Option<KeyCode> {
    let code = match name {
        "" => return None,
        "Space" => KeyCode::Char(' '),
        "Enter" => KeyCode::Enter,
        "Tab" => KeyCode::Tab,
        "BackTab" => KeyCode::BackTab,
        "Backspace" => KeyCode::Backspace,
        "Delete" => KeyCode::Delete,
        "Esc" => KeyCode::Esc,
        "Up" => KeyCode::Up,
        "Down" => KeyCode::Down,
        "Left" => KeyCode::Left,
        "Right" => KeyCode::Right,
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        "PageUp" => KeyCode::PageUp,
        "PageDown" => KeyCode::PageDown,
        "Insert" => KeyCode::Insert,
        f if f.starts_with('F') && f.len() > 1 => {
            let n: u8 = f[1..].parse().ok()?;
            KeyCode::F(n)
        }
        other => {
            let mut chars = other.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None; // multi-char, not a known name
            }
            KeyCode::Char(c)
        }
    };
    Some(code)
}

/// Tokenize a recorded key sequence (dropping any unencodable events).
#[must_use]
pub fn encode(keys: &[KeyEvent]) -> Vec<String> {
    keys.iter().map(|k| encode_key(*k)).filter(|s| !s.is_empty()).collect()
}

/// Decode tokens back to key events (dropping any unrecognized tokens).
#[must_use]
pub fn decode(tokens: &[String]) -> Vec<KeyEvent> {
    tokens.iter().filter_map(|t| decode_key(t)).collect()
}

/// Load all saved macros from `path` (empty when missing or unparseable).
#[must_use]
pub fn load(path: &Path) -> Vec<Macro> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str::<MacrosFile>(&text).ok())
        .map(|f| f.macros)
        .unwrap_or_default()
}

/// Insert or replace `mac` (by name) in `path`, creating the file if needed.
///
/// # Errors
/// Returns an error if the file cannot be written or serialized.
pub fn upsert(path: &Path, mac: Macro) -> std::io::Result<()> {
    let mut macros = load(path);
    if let Some(existing) = macros.iter_mut().find(|m| m.name == mac.name) {
        *existing = mac;
    } else {
        macros.push(mac);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let body = toml::to_string(&MacrosFile { macros })
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    std::fs::write(path, body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn round_trips_chars_named_and_modifiers() {
        let keys = vec![
            k(KeyCode::Char('a'), KeyModifiers::NONE),
            k(KeyCode::Char('c'), KeyModifiers::CONTROL),
            k(KeyCode::Char(' '), KeyModifiers::NONE),
            k(KeyCode::Tab, KeyModifiers::SHIFT),
            k(KeyCode::Enter, KeyModifiers::NONE),
            k(KeyCode::Left, KeyModifiers::ALT),
            k(KeyCode::F(5), KeyModifiers::NONE),
        ];
        let tokens = encode(&keys);
        assert_eq!(tokens, vec!["a", "C-c", "Space", "S-Tab", "Enter", "A-Left", "F5"]);
        assert_eq!(decode(&tokens), keys);
    }

    #[test]
    fn upsert_writes_and_replaces_by_name() {
        let path = std::env::temp_dir().join(format!("vix-macros-{}.toml", std::process::id()));
        let _ = std::fs::remove_file(&path);
        upsert(&path, Macro { name: "m".into(), keys: vec!["a".into()] }).unwrap();
        upsert(&path, Macro { name: "n".into(), keys: vec!["b".into()] }).unwrap();
        // Re-saving "m" replaces rather than duplicates.
        upsert(&path, Macro { name: "m".into(), keys: vec!["x".into(), "y".into()] }).unwrap();
        let macros = load(&path);
        assert_eq!(macros.len(), 2);
        let m = macros.iter().find(|m| m.name == "m").unwrap();
        assert_eq!(m.keys, vec!["x".to_string(), "y".to_string()]);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn decode_skips_unknown_tokens() {
        assert!(decode_key("").is_none());
        assert!(decode_key("Nonsense").is_none());
        assert_eq!(decode(&["x".to_string(), "??".to_string()]), vec![k(KeyCode::Char('x'), KeyModifiers::NONE)]);
    }
}
