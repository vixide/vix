//! Ghost-text suggestion state (T618): the dimmed text shown after the cursor.
//!
//! Pure rules kept out of the host so they are unit-tested without an editor:
//! when a suggestion still applies, how much of it one "accept word" takes, and
//! what is left afterwards. The host (`tui/src/app/ghost.rs`) owns the request
//! timing and the insertion.

/// The `inline_completion` setting: where suggestions come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// No suggestions (the default).
    Off,
    /// `textDocument/inlineCompletion` from a language server.
    Lsp,
    /// A fill-in-the-middle prompt to the configured AI provider.
    Ai,
}

impl Source {
    /// Parse the setting's value: `"lsp"` or `"ai"`. Any other value is
    /// [`Source::Off`].
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value {
            "lsp" => Source::Lsp,
            "ai" => Source::Ai,
            _ => Source::Off,
        }
    }
}

/// One suggestion, valid for exactly the buffer revision and cursor it was
/// computed for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ghost {
    text: String,
    revision: u64,
    cursor: usize,
}

impl Ghost {
    /// A suggestion `text` for the buffer at `revision` with the cursor at char
    /// offset `cursor`; `None` when `text` is empty.
    #[must_use]
    pub fn new(text: impl Into<String>, revision: u64, cursor: usize) -> Option<Self> {
        let text = text.into();
        (!text.is_empty()).then_some(Ghost {
            text,
            revision,
            cursor,
        })
    }

    /// The whole suggestion.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The buffer revision it was computed for.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The char offset it is anchored at.
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Whether the suggestion still applies: the buffer is at the revision it
    /// was computed for and the cursor has not moved.
    #[must_use]
    pub fn is_current(&self, revision: u64, cursor: usize) -> bool {
        self.revision == revision && self.cursor == cursor
    }

    /// The first line of the suggestion, which is what is drawn.
    #[must_use]
    pub fn first_line(&self) -> &str {
        self.text.lines().next().unwrap_or("")
    }

    /// Whether the suggestion continues past its first line.
    #[must_use]
    pub fn has_more_lines(&self) -> bool {
        self.text.trim_end_matches(['\n', '\r']).contains('\n')
    }

    /// The prefix one "accept word" takes: leading whitespace, then either a
    /// run of word characters (letters, digits, `_`) or a run of other
    /// non-blank characters. A suggestion of only whitespace is taken whole.
    #[must_use]
    pub fn next_word(&self) -> &str {
        let rest = self.text.trim_start();
        let lead = self.text.len() - rest.len();
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let Some(first) = rest.chars().next() else {
            return &self.text;
        };
        let word = is_word(first);
        let len = rest
            .char_indices()
            .find(|&(_, c)| c.is_whitespace() || is_word(c) != word)
            .map_or(rest.len(), |(i, _)| i);
        &self.text[..lead + len]
    }

    /// What remains after `taken` (a prefix of the suggestion) was inserted,
    /// anchored at the new `revision` and `cursor`; `None` when nothing remains.
    #[must_use]
    pub fn remainder(&self, taken: &str, revision: u64, cursor: usize) -> Option<Ghost> {
        let rest = self.text.strip_prefix(taken)?;
        Ghost::new(rest, revision, cursor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ghost(text: &str) -> Ghost {
        Ghost::new(text, 4, 10).unwrap()
    }

    #[test]
    fn source_parses_with_off_as_the_fallback() {
        assert_eq!(Source::parse("lsp"), Source::Lsp);
        assert_eq!(Source::parse("ai"), Source::Ai);
        assert_eq!(Source::parse("off"), Source::Off);
        assert_eq!(Source::parse("copilot"), Source::Off);
    }

    #[test]
    fn empty_text_is_no_suggestion() {
        assert!(Ghost::new("", 0, 0).is_none());
    }

    #[test]
    fn it_applies_only_at_its_revision_and_cursor() {
        let g = ghost("x");
        assert!(g.is_current(4, 10));
        assert!(!g.is_current(5, 10));
        assert!(!g.is_current(4, 11));
    }

    #[test]
    fn first_line_and_continuation() {
        let g = ghost("foo(\n    bar)\n");
        assert_eq!(g.first_line(), "foo(");
        assert!(g.has_more_lines());
        assert!(!ghost("foo()\n").has_more_lines());
        assert!(!ghost("foo()").has_more_lines());
    }

    #[test]
    fn next_word_takes_blanks_then_one_run() {
        assert_eq!(ghost("hello world").next_word(), "hello");
        assert_eq!(ghost("  hello world").next_word(), "  hello");
        assert_eq!(ghost("foo_bar(x)").next_word(), "foo_bar");
        assert_eq!(ghost("(x)").next_word(), "(");
        assert_eq!(ghost("::new()").next_word(), "::");
        assert_eq!(ghost("\n    let").next_word(), "\n    let");
        assert_eq!(ghost("   ").next_word(), "   ");
        assert_eq!(ghost("héllo wörld").next_word(), "héllo");
    }

    #[test]
    fn remainder_follows_what_was_taken() {
        let g = ghost("hello world");
        let rest = g.remainder("hello", 7, 15).unwrap();
        assert_eq!(rest.text(), " world");
        assert!(rest.is_current(7, 15));
        assert!(g.remainder("hello world", 7, 21).is_none());
        assert!(g.remainder("nope", 7, 15).is_none());
    }
}
