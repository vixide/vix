//! Fill-in-the-middle prompts for ghost-text completion (T618).
//!
//! Pure: [`input`] builds the text sent through [`crate::complete`] with
//! [`INSTRUCTION`], and [`clean`] turns the model's reply into the text to
//! insert. No I/O, so it is unit-tested with plain strings.
//!
//! The editor sends the text around the cursor, then shows the cleaned reply
//! as dimmed ghost text the user can accept. Bounding the context
//! ([`PREFIX_CHARS`], [`SUFFIX_CHARS`]) and the suggestion ([`MAX_LINES`],
//! [`MAX_CHARS`]) keeps requests cheap and suggestions unobtrusive. See
//! `crates/vix-ai-core/spec/index.md`.

/// How many characters before the cursor are sent.
pub const PREFIX_CHARS: usize = 2000;
/// How many characters after the cursor are sent.
pub const SUFFIX_CHARS: usize = 500;
/// The most lines a cleaned suggestion keeps.
pub const MAX_LINES: usize = 8;
/// The most characters a cleaned suggestion keeps.
pub const MAX_CHARS: usize = 400;
/// The marker in [`input`] where the model should write.
pub const CURSOR: &str = "<CURSOR>";

/// The fixed directions sent as the instruction.
pub const INSTRUCTION: &str = "You complete text at a cursor. The input holds the text of a file with the marker <CURSOR> where the cursor is. Reply with ONLY the text to insert at <CURSOR>, continuing naturally. Do not repeat the text before the cursor, do not explain, and do not use Markdown fences.";

/// The model input: the last [`PREFIX_CHARS`] characters of `before`, the
/// [`CURSOR`] marker, then the first [`SUFFIX_CHARS`] characters of `after`.
#[must_use]
pub fn input(before: &str, after: &str) -> String {
    let skip = before.chars().count().saturating_sub(PREFIX_CHARS);
    let head: String = before.chars().skip(skip).collect();
    let tail: String = after.chars().take(SUFFIX_CHARS).collect();
    format!("{head}{CURSOR}{tail}")
}

/// Turn a model reply into the text to insert after `line_prefix` (the text of
/// the cursor's line before the cursor). Strips a Markdown fence, drops an echo
/// of `line_prefix` the model repeated, trims leading newlines and trailing
/// blanks, and keeps at most [`MAX_LINES`] lines and [`MAX_CHARS`] characters.
/// An empty result means no suggestion.
///
/// # Examples
///
/// ```
/// use vix_ai_core::fim::clean;
/// assert_eq!(clean("```\nfoo\n```", ""), "foo");
/// assert_eq!(clean("let x = 1;", "let x ="), " 1;");
/// ```
#[must_use]
pub fn clean(reply: &str, line_prefix: &str) -> String {
    // Normalize CRLF first so every later step can assume `\n`.
    let mut text = reply.replace("\r\n", "\n");
    if let Some(inner) = unfence(&text) {
        text = inner;
    }
    // Models often restate the current line; strip it so the insertion does
    // not duplicate it. A blank prefix is skipped (stripping whitespace would
    // eat meaningful leading indentation of the suggestion).
    if !line_prefix.trim().is_empty()
        && let Some(rest) = text.strip_prefix(line_prefix)
    {
        text = rest.to_string();
    }
    let text = text.trim_start_matches('\n');
    let mut out = String::new();
    for (i, line) in text.lines().take(MAX_LINES).enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(line);
    }
    // Cap in characters (not bytes) so a multi-byte char is never split.
    let mut out: String = out.chars().take(MAX_CHARS).collect();
    out.truncate(out.trim_end().len());
    out
}

/// The body of a reply wrapped in a Markdown fence, or `None` when it is not.
/// The info string after the opening fence (e.g. `rust`) is discarded.
fn unfence(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let after_open = trimmed.strip_prefix("```")?;
    let body = &after_open[after_open.find('\n')? + 1..];
    let body = body.strip_suffix("```").unwrap_or(body);
    Some(body.trim_end_matches('\n').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_puts_the_marker_between_the_two_sides() {
        assert_eq!(input("fn f(", ") {}"), "fn f(<CURSOR>) {}");
    }

    #[test]
    fn input_keeps_only_the_nearest_context() {
        let before = "a".repeat(PREFIX_CHARS + 50) + "Z";
        let after = "Y".to_string() + &"b".repeat(SUFFIX_CHARS + 50);
        let got = input(&before, &after);
        assert_eq!(
            got.chars().count(),
            PREFIX_CHARS + CURSOR.len() + SUFFIX_CHARS
        );
        assert!(got.contains("Z<CURSOR>Y"));
    }

    #[test]
    fn clean_strips_a_fence() {
        assert_eq!(clean("```rust\nlet x = 1;\n```", ""), "let x = 1;");
        assert_eq!(clean("```\nfoo\nbar\n```\n", ""), "foo\nbar");
    }

    #[test]
    fn clean_drops_an_echoed_line_prefix() {
        assert_eq!(clean("let x = 1;", "let x ="), " 1;");
        assert_eq!(clean("let x = 1;", "   "), "let x = 1;");
        assert_eq!(clean("1;", "let x = "), "1;");
    }

    #[test]
    fn clean_trims_and_limits() {
        assert_eq!(clean("\n\nfoo  \n", ""), "foo");
        let long = (0..20)
            .map(|i| format!("l{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(clean(&long, "").lines().count(), MAX_LINES);
        assert_eq!(clean(&"x".repeat(1000), "").chars().count(), MAX_CHARS);
        assert_eq!(clean("   \n", ""), "");
    }

    #[test]
    fn clean_normalizes_crlf() {
        assert_eq!(clean("a\r\nb", ""), "a\nb");
    }
}
