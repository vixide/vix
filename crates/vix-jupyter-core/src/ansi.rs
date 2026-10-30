//! ANSI escape stripping for kernel tracebacks.
//!
//! IPython colors its tracebacks with SGR sequences (`ESC [ 0;31m`). Vix
//! shows plain text, so [`strip_ansi`] removes every escape sequence: CSI
//! (`ESC [` … final byte), OSC (`ESC ]` … BEL or `ESC \`), and two-byte
//! `ESC x` sequences. It also drops the other C0 control characters except
//! newline and tab, and the 8-bit CSI (`U+009B`), so a hostile kernel cannot
//! move the cursor or retitle the terminal through Vix.

/// `text` with every ANSI escape sequence and stray control character
/// removed.
#[must_use]
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => match chars.next() {
                // CSI: parameter and intermediate bytes, then one final byte.
                Some('[') => {
                    for c in chars.by_ref() {
                        if ('\u{40}'..='\u{7e}').contains(&c) {
                            break;
                        }
                    }
                }
                // OSC (and DCS/PM/APC): up to BEL or ST (`ESC \`).
                Some(']' | 'P' | '^' | '_') => {
                    while let Some(c) = chars.next() {
                        if c == '\u{07}' {
                            break;
                        }
                        if c == '\u{1b}' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                // A two-byte sequence, or a lone ESC at the end.
                _ => {}
            },
            '\n' | '\t' => out.push(c),
            // Every other control character goes, CR included: a CRLF keeps
            // its LF, and a bare CR (a progress-bar redraw) is dropped.
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sgr_removed() {
        assert_eq!(
            strip_ansi("\u{1b}[0;31mZeroDivisionError\u{1b}[0m: x"),
            "ZeroDivisionError: x"
        );
    }

    #[test]
    fn osc_title_removed() {
        assert_eq!(strip_ansi("a\u{1b}]0;pwned\u{07}b"), "ab");
        assert_eq!(
            strip_ansi("a\u{1b}]8;;http://x\u{1b}\\link\u{1b}]8;;\u{1b}\\b"),
            "alinkb"
        );
    }

    #[test]
    fn controls_and_cr() {
        assert_eq!(strip_ansi("a\r\nb\rc\u{8}\u{9b}d\te"), "a\nbcd\te");
    }

    #[test]
    fn truncated_sequences_do_not_panic() {
        assert_eq!(strip_ansi("x\u{1b}"), "x");
        assert_eq!(strip_ansi("x\u{1b}[31"), "x");
        assert_eq!(strip_ansi("x\u{1b}]title"), "x");
    }

    #[test]
    fn plain_text_unchanged() {
        assert_eq!(strip_ansi("héllo — world"), "héllo — world");
    }
}
