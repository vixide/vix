//! Greedy word-wrap of a single paragraph to a column width, hard-breaking
//! any word longer than the width so no line ever overflows.
//!
//! Extracted (Run H, T526) after `vix-welcome-panel` and `vix-ai-panel` were
//! found each hand-rolling an independent copy of this algorithm, with a
//! real, undocumented behavior difference: the welcome panel left an
//! over-long word to overflow the line, the AI panel hard-broke it
//! character-by-character. The difference looked accidental rather than
//! deliberate, so it was surfaced as a product decision rather than picked
//! apart silently -- "always break over-long words" (the AI panel's prior
//! behavior) is the merged answer this crate implements.
//!
//! Not `vix-textops::wrap`/`wrap_chunk`, which wrap the full, multi-line
//! *editor buffer* (cursor-aware, reflow-on-edit) -- a harder, more stateful
//! problem this crate deliberately stays out of. `wrap_line` takes one
//! already-known paragraph (no embedded newlines) and a width; splitting a
//! multi-line source into paragraphs is the caller's job.

#![cfg_attr(
    not(test),
    deny(
        clippy::arithmetic_side_effects,
        clippy::integer_division,
        clippy::modulo_arithmetic,
        clippy::as_conversions,
        clippy::indexing_slicing,
        clippy::string_slice
    )
)]

/// Greedily word-wrap one paragraph (no embedded newlines) to `width`
/// columns. A blank line yields a single empty string; `width` `0` returns
/// the line unchanged. A word longer than `width` is hard-broken
/// character-by-character so no returned line ever exceeds `width`.
#[must_use]
pub fn wrap_line(line: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![line.to_string()];
    }
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_len = 0usize;
    for word in line.split_whitespace() {
        let wlen = word.chars().count();
        if wlen > width {
            // Over-long word: flush the line so far, then hard-break the
            // word itself into width-sized chunks.
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            let mut chunk_len = 0usize;
            for ch in word.chars() {
                if chunk_len == width {
                    out.push(std::mem::take(&mut cur));
                    chunk_len = 0;
                }
                cur.push(ch);
                #[expect(
                    clippy::arithmetic_side_effects,
                    reason = "RCA-EXC-2: the check above resets chunk_len when it equals width, so before this increment it is below width, which is at most usize::MAX"
                )]
                {
                    chunk_len += 1;
                }
            }
            cur_len = chunk_len;
            continue;
        }
        if cur_len == 0 {
            cur.push_str(word);
            cur_len = wlen;
        } else if let Some(joined) = cur_len
            .checked_add(1)
            .and_then(|len| len.checked_add(wlen))
            .filter(|joined| *joined <= width)
        {
            cur.push(' ');
            cur.push_str(word);
            cur_len = joined;
        } else {
            out.push(std::mem::take(&mut cur));
            cur.push_str(word);
            cur_len = wlen;
        }
    }
    if !cur.is_empty() || out.is_empty() {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_on_spaces_without_exceeding_width() {
        assert_eq!(wrap_line("the quick brown", 9), vec!["the quick", "brown"]);
    }

    #[test]
    fn hard_breaks_a_word_longer_than_the_width() {
        assert_eq!(wrap_line("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn an_over_long_word_flushes_the_pending_line_first() {
        // "hi" fits on its own line; "abcdefghij" (10 chars) must not be
        // squeezed onto the same line at width 4, nor lose "hi" in the process.
        assert_eq!(
            wrap_line("hi abcdefghij", 4),
            vec!["hi", "abcd", "efgh", "ij"]
        );
    }

    #[test]
    fn a_word_that_exactly_fills_the_line_still_joins() {
        // "ab cd" is 5 columns: it fits width 5, not width 4.
        assert_eq!(wrap_line("ab cd", 5), vec!["ab cd"]);
        assert_eq!(wrap_line("ab cd", 4), vec!["ab", "cd"]);
        // The running length counts each joining space: "ab cd" is 5, so "ef"
        // fits width 8 but not width 7.
        assert_eq!(wrap_line("ab cd ef", 8), vec!["ab cd ef"]);
        assert_eq!(wrap_line("ab cd ef", 7), vec!["ab cd", "ef"]);
    }

    #[test]
    fn blank_line_yields_one_empty_string() {
        assert_eq!(wrap_line("", 9), vec![""]);
    }

    #[test]
    fn zero_width_returns_the_line_unchanged() {
        assert_eq!(wrap_line("anything at all", 0), vec!["anything at all"]);
    }

    #[test]
    fn runs_of_whitespace_collapse_like_split_whitespace() {
        assert_eq!(wrap_line("a   b\tc", 20), vec!["a b c"]);
    }
}
