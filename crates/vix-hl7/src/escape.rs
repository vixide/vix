//! HL7 escape sequences (`\F\`, `\S\`, `\T\`, `\R\`, `\E\`, `\.br\`, `\Xhh..\`).

use crate::message::Delimiters;

/// Decode the escape sequences in `raw` using the message's delimiters.
///
/// Lenient: an unterminated or unknown sequence is kept literally, `\H\` and
/// `\N\` (highlighting) are dropped, `\.br\` becomes a newline, and `\Xhh..\`
/// hex runs decode as UTF-8 (lossily).
#[must_use]
pub fn unescape(raw: &str, d: &Delimiters) -> String {
    if !raw.contains(d.escape) {
        return raw.to_string();
    }
    let chars: Vec<char> = raw.chars().collect();
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != d.escape {
            out.push(c);
            i += 1;
            continue;
        }
        let close = chars[i + 1..].iter().position(|&x| x == d.escape);
        let decoded = close.and_then(|close| {
            let body: String = chars[i + 1..i + 1 + close].iter().collect();
            decode_sequence(&body, d).map(|text| (text, close))
        });
        if let Some((text, close)) = decoded {
            out.push_str(&text);
            i += close + 2;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn decode_sequence(body: &str, d: &Delimiters) -> Option<String> {
    match body {
        "F" => Some(d.field.to_string()),
        "S" => Some(d.component.to_string()),
        "T" => Some(d.subcomponent.to_string()),
        "R" => Some(d.repetition.to_string()),
        "E" => Some(d.escape.to_string()),
        ".br" => Some("\n".to_string()),
        "H" | "N" => Some(String::new()),
        _ => decode_hex(body.strip_prefix('X')?),
    }
}

fn decode_hex(hex: &str) -> Option<String> {
    if hex.is_empty() || !hex.is_ascii() || !hex.len().is_multiple_of(2) {
        return None;
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    for pair in hex.as_bytes().chunks(2) {
        let s = std::str::from_utf8(pair).ok()?;
        bytes.push(u8::from_str_radix(s, 16).ok()?);
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Encode literal `text` so it can sit inside a field: every delimiter
/// character becomes its escape sequence, and a newline becomes `\.br\`.
#[must_use]
pub fn escape(text: &str, d: &Delimiters) -> String {
    let mut out = String::with_capacity(text.len());
    let e = d.escape;
    for c in text.chars() {
        let code = if c == d.escape {
            Some('E')
        } else if c == d.field {
            Some('F')
        } else if c == d.component {
            Some('S')
        } else if c == d.subcomponent {
            Some('T')
        } else if c == d.repetition {
            Some('R')
        } else {
            None
        };
        match (code, c) {
            (Some(k), _) => {
                out.push(e);
                out.push(k);
                out.push(e);
            }
            (None, '\n') => {
                out.push(e);
                out.push_str(".br");
                out.push(e);
            }
            (None, '\r') => {}
            (None, other) => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_standard_sequences() {
        let d = Delimiters::default();
        assert_eq!(unescape(r"a\F\b\S\c\T\d\R\e\E\f", &d), r"a|b^c&d~e\f");
        assert_eq!(unescape(r"x\.br\y", &d), "x\ny");
        assert_eq!(unescape(r"\H\bold\N\", &d), "bold");
        assert_eq!(unescape(r"\X414243\", &d), "ABC");
    }

    #[test]
    fn keeps_malformed_sequences_literally() {
        let d = Delimiters::default();
        assert_eq!(unescape(r"a\Fb", &d), r"a\Fb");
        assert_eq!(unescape(r"\Zxx\", &d), r"\Zxx\");
        assert_eq!(unescape(r"\X4\", &d), r"\X4\");
        assert_eq!(unescape(r"\XZZ\", &d), r"\XZZ\");
    }

    #[test]
    fn escape_round_trips_delimiters() {
        let d = Delimiters::default();
        let s = "a|b^c&d~e\\f\ng";
        assert_eq!(unescape(&escape(s, &d), &d), s);
    }
}
