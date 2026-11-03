//! The received-text sanitizer (spec: `spec/instant-message`, "Security").
//!
//! Everything that arrives from another person — a message body, a nick, a
//! topic, a snippet's code — goes through here before it is stored or drawn.
//! The sanitizer removes exactly what can change how a terminal behaves or
//! how text *appears* to be ordered, and nothing else:
//!
//! - C0 controls (U+0000–U+001F), DEL (U+007F) and C1 controls
//!   (U+0080–U+009F). A tab becomes [`TAB_SPACES`] spaces; a line break
//!   becomes one space in [`sanitize`] and is kept in [`sanitize_multiline`].
//! - `ESC` together with the whole escape sequence it starts: CSI
//!   (`ESC [` … final byte), OSC (`ESC ]` … `BEL` or `ST`), DCS, SOS, PM and
//!   APC strings (`ESC P` / `X` / `^` / `_` … `ST`), `nF` sequences and the
//!   two-character `Fp`/`Fe`/`Fs` forms. The single-character C1 introducers
//!   (U+009B CSI, U+009D OSC, U+0090 DCS, …) start the same sequences.
//! - The bidirectional embedding, override and isolate controls U+202A–U+202E
//!   and U+2066–U+2069 ("Trojan Source").
//!
//! Every other character is kept as-is, in any script and direction. In
//! particular the zero-width joiner and non-joiner (U+200D, U+200C) stay:
//! Persian, the Indic scripts and emoji sequences need them.
//!
//! The output never contains a control character (other than `'\n'` from
//! [`sanitize_multiline`]), never contains `ESC`, never contains one of the
//! listed bidi controls, and sanitizing it again changes nothing.
//!
//! [`crate::Message::sanitize`], [`crate::Conversation::new`],
//! [`crate::Inbox::push`] and the history and thread decoders all call this
//! module, so no received string reaches the App unfiltered.
//!
//! ```
//! use vix_im::{sanitize, sanitize_multiline};
//! assert_eq!(sanitize("a\u{1b}[31mred\u{1b}[0m\tb"), "ared    b");
//! assert_eq!(sanitize_multiline("x\r\ny"), "x\ny");
//! ```

/// How many spaces a received tab becomes.
pub const TAB_SPACES: usize = 4;

/// The escape character that introduces most terminal control sequences.
const ESC: char = '\u{1b}';
/// Terminates an OSC string (the legacy alternative to `ST`).
const BEL: char = '\u{07}';
/// The C1 String Terminator.
const ST: char = '\u{9c}';

/// Whether `c` is one of the bidirectional embedding, override or isolate
/// controls the sanitizer removes (U+202A–U+202E, U+2066–U+2069).
#[must_use]
pub fn is_bidi_control(c: char) -> bool {
    matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// Sanitize one line of received text: line breaks become a space.
///
/// Use this for nicks, topics, titles and message bodies; use
/// [`sanitize_multiline`] only where line structure is meaningful.
#[must_use]
pub fn sanitize(text: &str) -> String {
    run(text, false)
}

/// Sanitize received text that may span lines (a snippet's code): `'\n'` is
/// kept, a `"\r\n"` pair becomes `'\n'`, and a lone `'\r'` becomes `'\n'`.
#[must_use]
pub fn sanitize_multiline(text: &str) -> String {
    run(text, true)
}

/// What kind of string an escape introducer opened.
#[derive(Clone, Copy)]
enum StringKind {
    /// OSC: ends at `BEL` or `ST`.
    Osc,
    /// DCS, SOS, PM, APC: end at `ST` only.
    Control,
}

/// The one-pass filter behind [`sanitize`] and [`sanitize_multiline`]. It
/// never fails: malformed or truncated escape sequences are consumed up to
/// the end of input rather than leaked.
fn run(text: &str, keep_newlines: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\t' => out.extend(std::iter::repeat_n(' ', TAB_SPACES)),
            '\r' | '\n' => {
                // Fold `\r\n` into one break, not two.
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push(if keep_newlines { '\n' } else { ' ' });
            }
            ESC => skip_escape(&mut chars),
            '\u{9b}' => skip_csi(&mut chars),
            '\u{9d}' => skip_string(&mut chars, StringKind::Osc),
            '\u{90}' | '\u{98}' | '\u{9e}' | '\u{9f}' => {
                skip_string(&mut chars, StringKind::Control);
            }
            // Drop every other control (C0/C1/DEL) and the bidi overrides.
            c if c.is_control() || is_bidi_control(c) => {}
            c => out.push(c),
        }
    }
    out
}

/// The shared, peekable character stream the escape skippers consume.
type Chars<'a> = std::iter::Peekable<std::str::Chars<'a>>;

/// Skip what follows an `ESC` (already consumed).
fn skip_escape(chars: &mut Chars<'_>) {
    let Some(&next) = chars.peek() else {
        return;
    };
    match next {
        '[' => {
            chars.next();
            skip_csi(chars);
        }
        ']' => {
            chars.next();
            skip_string(chars, StringKind::Osc);
        }
        'P' | 'X' | '^' | '_' => {
            chars.next();
            skip_string(chars, StringKind::Control);
        }
        // nF: intermediate bytes, then one final byte.
        '\u{20}'..='\u{2F}' => {
            while chars
                .peek()
                .is_some_and(|c| ('\u{20}'..='\u{2F}').contains(c))
            {
                chars.next();
            }
            if chars
                .peek()
                .is_some_and(|c| ('\u{30}'..='\u{7E}').contains(c))
            {
                chars.next();
            }
        }
        // Fp, Fe, Fs: one final byte.
        '\u{30}'..='\u{7E}' => {
            chars.next();
        }
        // Anything else (another ESC, a control, a non-ASCII character) is
        // not part of this sequence; the main loop handles it.
        _ => {}
    }
}

/// Skip a CSI's parameter and intermediate bytes and its final byte. A
/// character outside the CSI grammar ends the sequence without being eaten,
/// so a stray `ESC [` cannot swallow the rest of a message.
fn skip_csi(chars: &mut Chars<'_>) {
    while let Some(&c) = chars.peek() {
        match c {
            '\u{30}'..='\u{3F}' | '\u{20}'..='\u{2F}' => {
                chars.next();
            }
            '\u{40}'..='\u{7E}' => {
                chars.next();
                return;
            }
            _ => return,
        }
    }
}

/// Skip a control string up to and including its terminator. An
/// unterminated string runs to the end of the text, as it would in a
/// terminal.
fn skip_string(chars: &mut Chars<'_>, kind: StringKind) {
    while let Some(c) = chars.next() {
        match c {
            ST => return,
            BEL if matches!(kind, StringKind::Osc) => return,
            ESC if chars.peek() == Some(&'\\') => {
                chars.next();
                return;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_unchanged() {
        assert_eq!(sanitize("hello, world!"), "hello, world!");
    }

    #[test]
    fn tab_becomes_spaces() {
        assert_eq!(sanitize("a\tb"), "a    b");
    }

    #[test]
    fn line_breaks() {
        assert_eq!(sanitize("a\r\nb\nc\rd"), "a b c d");
        assert_eq!(sanitize_multiline("a\r\nb\nc\rd"), "a\nb\nc\nd");
    }

    #[test]
    fn csi_is_removed_whole() {
        assert_eq!(sanitize("x\u{1b}[31;1mred\u{1b}[0m"), "xred");
        assert_eq!(sanitize("\u{1b}[2J\u{1b}[Hhi"), "hi");
        assert_eq!(sanitize("\u{9b}31mred"), "red");
    }

    #[test]
    fn osc_with_either_terminator() {
        let bel = "\u{1b}]8;;https://evil.example\u{07}click\u{1b}]8;;\u{07}";
        assert_eq!(sanitize(bel), "click");
        let st = "\u{1b}]52;c;ZXZpbA==\u{1b}\\after";
        assert_eq!(sanitize(st), "after");
        assert_eq!(sanitize("\u{9d}0;title\u{9c}ok"), "ok");
    }

    #[test]
    fn dcs_and_friends() {
        assert_eq!(sanitize("\u{1b}Pq#0;2;0;0;0\u{1b}\\ok"), "ok");
        assert_eq!(sanitize("\u{1b}_apc\u{07}still\u{1b}\\ok"), "ok");
        assert_eq!(sanitize("\u{1b}^pm\u{9c}ok"), "ok");
        assert_eq!(sanitize("\u{1b}Xsos\u{1b}\\ok"), "ok");
    }

    #[test]
    fn short_escapes() {
        assert_eq!(sanitize("a\u{1b}7b\u{1b}8c"), "abc");
        assert_eq!(sanitize("a\u{1b}(Bb"), "ab");
        assert_eq!(sanitize("a\u{1b}"), "a");
        assert_eq!(sanitize("a\u{1b}\u{1b}[mb"), "ab");
    }

    #[test]
    fn broken_csi_does_not_eat_the_message() {
        assert_eq!(sanitize("\u{1b}[12é rest"), "é rest");
    }

    #[test]
    fn other_controls() {
        assert_eq!(sanitize("a\u{0}b\u{7}c\u{7f}d\u{85}e\u{8}f"), "abcdef");
    }

    #[test]
    fn bidi_controls_removed_joiners_kept() {
        let trojan = "access\u{202E}\u{2066}// admin\u{2069}\u{202C}";
        assert_eq!(sanitize(trojan), "access// admin");
        let persian = "می\u{200C}خواهم";
        assert_eq!(sanitize(persian), persian);
        let family = "👨\u{200D}👩\u{200D}👧";
        assert_eq!(sanitize(family), family);
    }
}
