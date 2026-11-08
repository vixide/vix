//! What the user sees of an agent's terminals (the `terminal/*` methods).
//!
//! Pure: no processes, no threads. The host feeds each terminal's captured
//! output (already capped by [`crate::terminal`]) into [`TerminalViews`] on its
//! tick and draws the views read-only. [`sanitize`] runs on everything an agent
//! command printed before it is stored for display, so agent output can never
//! put escape sequences into Vix's own terminal.
//!
//! Owning spec: `crates/vix-acp/spec/index.md`.

use crate::client::TerminalExit;

/// The most terminal views kept; the oldest released one goes first.
pub const MAX_VIEWS: usize = 16;

/// Spaces a tab becomes, so the display never depends on tab stops.
const TAB: &str = "    ";

/// Make agent command output safe to draw.
///
/// Every escape sequence is removed whole: CSI (`ESC [` ... final byte, or
/// the one-byte `U+009B`), OSC and the other string sequences (`ESC ]`,
/// `ESC P`, `ESC X`, `ESC ^`, `ESC _`, ended by BEL or `ESC \`), and two-byte
/// escapes. Every other control character is dropped except newline; a tab
/// becomes four spaces, `\r\n` becomes `\n`, and a lone `\r` starts the
/// current line over (so a progress bar shows its last state).
#[must_use]
pub fn sanitize(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut line_start = 0;
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\n' => {
                out.push('\n');
                line_start = out.len();
            }
            '\t' => out.push_str(TAB),
            '\r' => {
                if chars.peek() != Some(&'\n') {
                    out.truncate(line_start);
                }
            }
            '\u{1b}' => match chars.next() {
                Some('[') => skip_csi(&mut chars),
                Some(']' | 'P' | 'X' | '^' | '_') => skip_string(&mut chars),
                // ESC, intermediates (0x20-0x2F), then one final byte.
                Some(' '..='/') => {
                    while chars.next_if(|c| matches!(c, ' '..='/')).is_some() {}
                    chars.next();
                }
                _ => {}
            },
            '\u{9b}' => skip_csi(&mut chars),
            '\u{9d}' | '\u{90}' | '\u{98}' | '\u{9e}' | '\u{9f}' => skip_string(&mut chars),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Skip a CSI body: parameter and intermediate bytes (0x20-0x3F), then the
/// final byte (0x40-0x7E). A malformed body ends at the first other character,
/// which is left for the caller.
fn skip_csi(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while chars.next_if(|c| matches!(c, ' '..='?')).is_some() {}
    chars.next_if(|c| matches!(c, '@'..='~'));
}

/// Skip a string sequence up to and including its terminator (BEL, `ESC \`
/// or the one-byte ST `U+009C`), or to the end of the text.
fn skip_string(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while let Some(c) = chars.next() {
        match c {
            '\u{7}' | '\u{9c}' => return,
            '\u{1b}' => {
                chars.next_if_eq(&'\\');
                return;
            }
            _ => {}
        }
    }
}

/// One agent terminal as the user sees it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalView {
    /// The terminal id the host gave the agent.
    pub id: String,
    /// The approved command line, sanitized and on one line.
    pub command: String,
    /// The captured output so far, sanitized.
    pub output: String,
    /// Whether older output was dropped to stay under the byte limit.
    pub truncated: bool,
    /// How the command ended, once known.
    pub exit: Option<TerminalExit>,
    /// Whether the agent released the terminal (or it was closed); a released
    /// view keeps its last output and never changes again.
    pub released: bool,
}

/// The agent's terminals, oldest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalViews {
    /// The views, in the order the terminals were created.
    views: Vec<TerminalView>,
}

impl TerminalViews {
    /// Start showing terminal `id`, which runs `argv`. Beyond [`MAX_VIEWS`]
    /// the oldest released view is forgotten.
    pub fn open(&mut self, id: &str, argv: &[String]) {
        if self.views.len() >= MAX_VIEWS
            && let Some(i) = self.views.iter().position(|v| v.released)
        {
            self.views.remove(i);
        }
        let command = sanitize(&argv.join(" ")).replace('\n', " ");
        self.views.push(TerminalView {
            id: id.to_string(),
            command,
            ..TerminalView::default()
        });
    }

    /// Record terminal `id`'s latest captured output `raw` (sanitized here)
    /// and exit status. Returns whether the view changed; an unknown or
    /// released terminal never changes.
    pub fn update(
        &mut self,
        id: &str,
        raw: &str,
        truncated: bool,
        exit: Option<TerminalExit>,
    ) -> bool {
        let Some(v) = self.views.iter_mut().find(|v| v.id == id && !v.released) else {
            return false;
        };
        let output = sanitize(raw);
        let changed = v.output != output || v.truncated != truncated || v.exit != exit;
        v.output = output;
        v.truncated = truncated;
        v.exit = exit;
        changed
    }

    /// Freeze terminal `id`: it keeps its output and stops updating. Returns
    /// whether a live view was released.
    pub fn release(&mut self, id: &str) -> bool {
        match self.views.iter_mut().find(|v| v.id == id && !v.released) {
            Some(v) => {
                v.released = true;
                true
            }
            None => false,
        }
    }

    /// The views, oldest first.
    #[must_use]
    pub fn views(&self) -> &[TerminalView] {
        &self.views
    }

    /// Whether no terminal has been shown yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_and_newlines_pass_through() {
        assert_eq!(sanitize("one\ntwo\n"), "one\ntwo\n");
        assert_eq!(sanitize("a\r\nb"), "a\nb");
        assert_eq!(sanitize("é ü 漢"), "é ü 漢");
    }

    #[test]
    fn color_and_cursor_sequences_are_removed() {
        assert_eq!(sanitize("\u{1b}[31mred\u{1b}[0m ok"), "red ok");
        assert_eq!(sanitize("\u{1b}[2J\u{1b}[1;1Hhome"), "home");
        assert_eq!(sanitize("\u{1b}[?1049hx"), "x");
        assert_eq!(sanitize("\u{9b}31mc1"), "c1");
    }

    #[test]
    fn osc_and_string_sequences_are_removed_whole() {
        // A title change (BEL-terminated) and a hyperlink (ST-terminated).
        assert_eq!(sanitize("\u{1b}]0;pwned\u{7}after"), "after");
        assert_eq!(
            sanitize("\u{1b}]8;;http://x\u{1b}\\link\u{1b}]8;;\u{1b}\\"),
            "link"
        );
        assert_eq!(sanitize("\u{1b}Pq#0;1\u{1b}\\dcs"), "dcs");
        // An unterminated string swallows the rest rather than leaking it.
        assert_eq!(sanitize("ok\u{1b}]52;c;Zm9v"), "ok");
    }

    #[test]
    fn other_controls_are_dropped_or_neutralized() {
        assert_eq!(sanitize("a\u{7}b\u{8}c\u{0}d\u{7f}"), "abcd");
        assert_eq!(sanitize("\u{1b}(Bx\u{1b}7y"), "xy");
        assert_eq!(sanitize("a\tb"), "a    b");
        assert_eq!(sanitize("10%\r50%\r100%\ndone"), "100%\ndone");
        assert!(!sanitize("\u{1b}\u{1b}[m\u{1b}").contains('\u{1b}'));
    }

    fn argv(s: &str) -> Vec<String> {
        s.split(' ').map(ToString::to_string).collect()
    }

    #[test]
    fn updates_are_sanitized_and_report_changes() {
        let mut v = TerminalViews::default();
        v.open("term-1", &argv("make \u{1b}[31mall"));
        assert_eq!(v.views()[0].command, "make all");
        assert!(v.update("term-1", "\u{1b}[1mone\u{1b}[0m\n", false, None));
        assert!(!v.update("term-1", "\u{1b}[1mone\u{1b}[0m\n", false, None));
        let exit = TerminalExit {
            code: Some(0),
            signal: None,
        };
        assert!(v.update("term-1", "one\ntwo\n", true, Some(exit.clone())));
        let view = &v.views()[0];
        assert_eq!(view.output, "one\ntwo\n");
        assert!(view.truncated);
        assert_eq!(view.exit, Some(exit));
        assert!(!v.update("term-9", "x", false, None), "unknown terminal");
    }

    #[test]
    fn a_released_view_stops_updating() {
        let mut v = TerminalViews::default();
        v.open("term-1", &argv("sleep 30"));
        v.update("term-1", "before", false, None);
        assert!(v.release("term-1"));
        assert!(!v.release("term-1"), "already released");
        assert!(!v.update("term-1", "after", false, None));
        assert_eq!(v.views()[0].output, "before");
        assert!(v.views()[0].released);
    }

    #[test]
    fn views_keep_creation_order_and_a_bounded_count() {
        let mut v = TerminalViews::default();
        for i in 0..MAX_VIEWS {
            v.open(&format!("term-{i}"), &argv("true"));
        }
        // All live: nothing can be forgotten, so the list may grow.
        v.release("term-3");
        v.open("term-new", &argv("true"));
        assert_eq!(v.views().len(), MAX_VIEWS);
        assert!(v.views().iter().all(|x| x.id != "term-3"));
        assert_eq!(v.views()[0].id, "term-0");
        assert_eq!(v.views().last().unwrap().id, "term-new");
    }
}
