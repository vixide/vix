//! Structural search & replace (T201): a small pattern language with
//! metavariable "holes" -- `$X` matches one balanced unit (a single token,
//! or a whole bracketed group when the next source token opens one),
//! `$$X` matches a run of zero or more tokens -- matched against source
//! text token-by-token rather than by regex. Whitespace and formatting
//! differences between the pattern and the source are ignored; a hole
//! captures the *original* source text it matched (the exact byte slice,
//! not a reconstruction from tokens), so a replacement preserves whatever
//! formatting the captured code already had.
//!
//! **Scope note**: the improvement-plan task this implements describes
//! reusing tree-sitter for structural matching, falling back to
//! "bracket-balanced text matching" when no grammar is loaded. This
//! implements only the latter, as the *primary* mechanism, not a fallback:
//! genuine tree-sitter-based matching would need per-grammar
//! node-equivalence handling across the ~15 grammars Vix loads, a
//! substantially larger project than the token-based approach here, which
//! already delivers the core value -- parameterized, bracket-aware
//! structural replace -- for every language Vix supports, uniformly.
//!
//! A hole's name follows ordinary identifier rules (starts with a letter or
//! `_`, then letters/digits/`_`) so `$5` in a template renders as the
//! literal text `$5`, not a lookup for a hole named `"5"`. A single hole
//! (`$X`) matches exactly *one lexical unit* -- one token, or one whole
//! bracketed group when the next token opens one -- never a multi-token
//! sequence like `x > 0`; use `$$X` for that.
//!
//! Holes are matched lazily (shortest first, like Comby's own default): a
//! `$$X` capture stays empty unless the rest of the pattern needs it to
//! grow, so a trailing `$$X` with nothing after it in the pattern matches
//! an empty string rather than "everything to the end of the source".
//!
//! No attempt is made at guaranteeing polynomial-time matching for
//! pathological patterns (many holes, adversarial input); realistic
//! patterns against realistic file sizes are fast in practice, which is
//! what this optimizes for.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::pedantic)]

use std::fmt;

/// A hole's captured text, as `(name, (start, end))` byte spans -- the
/// shape [`Match::captures`] carries, and what an in-progress match builds
/// up as it goes.
type Captures = Vec<(String, (usize, usize))>;

/// A lexical token's structural role: only whether it opens/closes a
/// bracket matters for matching; everything else is compared by exact text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// `(`, `[`, `{`
    Open,
    /// `)`, `]`, `}`
    Close,
    /// An identifier, number, string literal, operator run, or any other
    /// single punctuation character.
    Other,
}

/// One token's structural kind and byte span in the text it came from.
#[derive(Clone, Copy, Debug)]
struct Token {
    kind: Kind,
    start: usize,
    end: usize,
}

/// Punctuation characters that glue into one multi-character token (`==`,
/// `->`, `::`, `&&`, …). Deliberately excludes `$` -- the pattern hole
/// marker -- so it always tokenizes on its own.
const PUNCT: &str = "!#%&*+-./:<=>?@^|~";

/// Skip whitespace from `i`, then read one token starting there. Returns
/// the token and the byte offset just past it, or `None` at end of input.
/// Shared by [`tokenize`] (source text) and [`Pattern::compile`] (a
/// pattern's literal portions), so token boundaries line up between them.
fn next_token_at(text: &str, mut i: usize) -> Option<(Token, usize)> {
    while i < text.len() {
        let c = text[i..].chars().next()?;
        if c.is_whitespace() {
            i += c.len_utf8();
        } else {
            break;
        }
    }
    let c = text[i..].chars().next()?;
    if "([{".contains(c) {
        let end = i + c.len_utf8();
        return Some((
            Token {
                kind: Kind::Open,
                start: i,
                end,
            },
            end,
        ));
    }
    if ")]}".contains(c) {
        let end = i + c.len_utf8();
        return Some((
            Token {
                kind: Kind::Close,
                start: i,
                end,
            },
            end,
        ));
    }
    if c == '"' || c == '\'' || c == '`' {
        let end = scan_string_literal(text, i, c);
        return Some((
            Token {
                kind: Kind::Other,
                start: i,
                end,
            },
            end,
        ));
    }
    if c.is_alphanumeric() || c == '_' {
        let end = scan_run(text, i, |c2| c2.is_alphanumeric() || c2 == '_');
        return Some((
            Token {
                kind: Kind::Other,
                start: i,
                end,
            },
            end,
        ));
    }
    if PUNCT.contains(c) {
        let end = scan_run(text, i, |c2| PUNCT.contains(c2));
        return Some((
            Token {
                kind: Kind::Other,
                start: i,
                end,
            },
            end,
        ));
    }
    // A single character not covered above (`,`, `;`, `$`, …): its own
    // one-character token.
    let end = i + c.len_utf8();
    Some((
        Token {
            kind: Kind::Other,
            start: i,
            end,
        },
        end,
    ))
}

/// The byte offset just past a `quote`-delimited string literal starting at
/// `i` in `text` (which must be the opening quote), honoring `\`-escapes.
/// Runs to the matching closing quote, or to the end of `text` if it's
/// never closed.
fn scan_string_literal(text: &str, i: usize, quote: char) -> usize {
    let mut j = i + quote.len_utf8();
    while j < text.len() {
        let c2 = text[j..].chars().next().unwrap_or(quote);
        if c2 == '\\' {
            j += c2.len_utf8();
            if let Some(c3) = text.get(j..).and_then(|s| s.chars().next()) {
                j += c3.len_utf8();
            }
            continue;
        }
        j += c2.len_utf8();
        if c2 == quote {
            break;
        }
    }
    j
}

/// The byte offset just past the longest run starting at `i` in `text`
/// whose characters all satisfy `keep`.
fn scan_run(text: &str, i: usize, keep: impl Fn(char) -> bool) -> usize {
    let mut j = i;
    while j < text.len() {
        let c = text[j..].chars().next().unwrap_or('\0');
        if keep(c) {
            j += c.len_utf8();
        } else {
            break;
        }
    }
    j
}

/// Tokenize `text` in full.
fn tokenize(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some((tok, next)) = next_token_at(text, i) {
        out.push(tok);
        i = next;
    }
    out
}

/// One element of a compiled pattern: literal text to match exactly, or a
/// named hole.
#[derive(Clone, Debug)]
enum PatternToken {
    /// Matches one source token of this `Kind` whose text equals this
    /// string exactly.
    Literal(Kind, String),
    /// A metavariable: `multi: false` is `$NAME` (one balanced unit),
    /// `multi: true` is `$$NAME` (zero or more tokens).
    Hole { name: String, multi: bool },
}

/// Why [`Pattern::compile`] rejected a pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileError {
    /// The pattern had no tokens at all (empty, or all whitespace).
    Empty,
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompileError::Empty => write!(f, "empty pattern"),
        }
    }
}

impl std::error::Error for CompileError {}

/// One structural match in a source text: its overall byte span, and each
/// named hole's own byte span within that source (in pattern order; a hole
/// used more than once in the pattern appears more than once here, each
/// capturing independently).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    /// Start byte offset (inclusive) of the whole match.
    pub start: usize,
    /// End byte offset (exclusive) of the whole match.
    pub end: usize,
    /// `(hole name, (start, end))` for each hole the pattern captured.
    pub captures: Captures,
}

/// A compiled structural pattern, ready to search source text.
#[derive(Clone, Debug)]
pub struct Pattern {
    tokens: Vec<PatternToken>,
}

impl Pattern {
    /// Compile `pattern` (e.g. `"if $COND { $$BODY }"`) into a matcher.
    ///
    /// # Errors
    ///
    /// Returns [`CompileError::Empty`] for an empty (or all-whitespace)
    /// pattern.
    pub fn compile(pattern: &str) -> Result<Pattern, CompileError> {
        let mut tokens = Vec::new();
        let mut i = 0;
        loop {
            while i < pattern.len() {
                let Some(c) = pattern[i..].chars().next() else {
                    break;
                };
                if c.is_whitespace() {
                    i += c.len_utf8();
                } else {
                    break;
                }
            }
            if i >= pattern.len() {
                break;
            }
            if pattern.as_bytes()[i] == b'$' {
                let mut j = i + 1;
                let multi = pattern.get(j..).is_some_and(|s| s.starts_with('$'));
                if multi {
                    j += 1;
                }
                let name_start = j;
                let mut name_end = j;
                for (k, c) in pattern[j..].char_indices() {
                    let ok = if k == 0 {
                        c.is_alphabetic() || c == '_'
                    } else {
                        c.is_alphanumeric() || c == '_'
                    };
                    if ok {
                        name_end = j + k + c.len_utf8();
                    } else {
                        break;
                    }
                }
                if name_end > name_start {
                    tokens.push(PatternToken::Hole {
                        name: pattern[name_start..name_end].to_string(),
                        multi,
                    });
                    i = name_end;
                    continue;
                }
                // A bare `$` (or `$$`) not followed by a name: fall through
                // and read it as an ordinary literal token instead.
            }
            let Some((tok, next)) = next_token_at(pattern, i) else {
                break;
            };
            tokens.push(PatternToken::Literal(
                tok.kind,
                pattern[tok.start..tok.end].to_string(),
            ));
            i = next;
        }
        if tokens.is_empty() {
            return Err(CompileError::Empty);
        }
        Ok(Pattern { tokens })
    }

    /// Every non-overlapping match in `source`, left to right.
    #[must_use]
    pub fn find_all(&self, source: &str) -> Vec<Match> {
        let toks = tokenize(source);
        let mut out = Vec::new();
        let mut i = 0;
        while i < toks.len() {
            if let Some((end_idx, captures)) = self.try_match_at(&toks, i, source) {
                let start = toks[i].start;
                let end = if end_idx > i {
                    toks[end_idx - 1].end
                } else {
                    toks[i].start
                };
                out.push(Match {
                    start,
                    end,
                    captures,
                });
                i = end_idx.max(i + 1);
            } else {
                i += 1;
            }
        }
        out
    }

    /// The first match starting at or after byte offset `from` in `source`.
    #[must_use]
    pub fn find_from(&self, source: &str, from: usize) -> Option<Match> {
        self.find_all(source).into_iter().find(|m| m.start >= from)
    }

    fn try_match_at(
        &self,
        toks: &[Token],
        start_idx: usize,
        source: &str,
    ) -> Option<(usize, Captures)> {
        let mut caps = Vec::new();
        let end = self.match_from(0, toks, start_idx, source, &mut caps)?;
        Some((end, caps))
    }

    /// Match `self.tokens[pi..]` against `toks[ti..]`, extending `caps` with
    /// any hole captures. Returns the source-token index just past the
    /// match on success.
    fn match_from(
        &self,
        pi: usize,
        toks: &[Token],
        ti: usize,
        source: &str,
        caps: &mut Captures,
    ) -> Option<usize> {
        let Some(pat_tok) = self.tokens.get(pi) else {
            // Pattern exhausted: the match ends exactly here.
            return Some(ti);
        };
        match pat_tok {
            PatternToken::Literal(kind, text) => {
                let t = toks.get(ti)?;
                if t.kind == *kind && &source[t.start..t.end] == text {
                    self.match_from(pi + 1, toks, ti + 1, source, caps)
                } else {
                    None
                }
            }
            PatternToken::Hole { name, multi: false } => {
                let unit_end = consume_unit(toks, ti)?;
                let span = (toks[ti].start, toks[unit_end - 1].end);
                let mut next_caps = caps.clone();
                next_caps.push((name.clone(), span));
                let result = self.match_from(pi + 1, toks, unit_end, source, &mut next_caps)?;
                *caps = next_caps;
                Some(result)
            }
            PatternToken::Hole { name, multi: true } => {
                let mut depth = 0i32;
                let mut j = ti;
                loop {
                    if depth == 0 {
                        let span = if j > ti {
                            (toks[ti].start, toks[j - 1].end)
                        } else {
                            let p = toks.get(ti).map_or(source.len(), |t| t.start);
                            (p, p)
                        };
                        let mut next_caps = caps.clone();
                        next_caps.push((name.clone(), span));
                        if let Some(result) =
                            self.match_from(pi + 1, toks, j, source, &mut next_caps)
                        {
                            *caps = next_caps;
                            return Some(result);
                        }
                    }
                    let t = toks.get(j)?;
                    match t.kind {
                        Kind::Open => depth += 1,
                        Kind::Close => depth -= 1,
                        Kind::Other => {}
                    }
                    if depth < 0 {
                        return None;
                    }
                    j += 1;
                }
            }
        }
    }
}

/// If `toks[i]` opens a bracket, the index just past its matching closer;
/// otherwise `i + 1` (a single plain token). `None` if `i` is out of range
/// or the bracket never closes.
fn consume_unit(toks: &[Token], i: usize) -> Option<usize> {
    let t = toks.get(i)?;
    if t.kind != Kind::Open {
        return Some(i + 1);
    }
    let mut depth = 1;
    let mut j = i + 1;
    while depth > 0 {
        let t2 = toks.get(j)?;
        match t2.kind {
            Kind::Open => depth += 1,
            Kind::Close => depth -= 1,
            Kind::Other => {}
        }
        j += 1;
    }
    Some(j)
}

/// Substitute `$NAME`/`$$NAME` placeholders in `template` with `m`'s
/// captured source text (the exact `source[start..end]` slice for the
/// *first* capture of that name), and copy any other text through as-is. A
/// placeholder naming a hole the pattern didn't capture renders as an empty
/// string rather than erroring, and a bare `$`/`$$` not followed by a name
/// is copied through literally.
#[must_use]
pub fn render_replacement(template: &str, source: &str, m: &Match) -> String {
    let mut out = String::new();
    let mut i = 0;
    while let Some(c) = template[i..].chars().next() {
        if c != '$' {
            out.push(c);
            i += c.len_utf8();
            continue;
        }
        let mut j = i + 1;
        let multi = template.get(j..).is_some_and(|s| s.starts_with('$'));
        if multi {
            j += 1;
        }
        let name_start = j;
        let mut name_end = j;
        for (k, ch) in template[j..].char_indices() {
            let ok = if k == 0 {
                ch.is_alphabetic() || ch == '_'
            } else {
                ch.is_alphanumeric() || ch == '_'
            };
            if ok {
                name_end = j + k + ch.len_utf8();
            } else {
                break;
            }
        }
        if name_end == name_start {
            out.push('$');
            i += 1;
            continue;
        }
        let name = &template[name_start..name_end];
        if let Some((_, (cs, ce))) = m.captures.iter().find(|(n, _)| n == name) {
            out.push_str(&source[*cs..*ce]);
        }
        i = name_end;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pattern: &str, source: &str) -> Vec<Match> {
        Pattern::compile(pattern).unwrap().find_all(source)
    }

    fn text(source: &str, span: (usize, usize)) -> &str {
        &source[span.0..span.1]
    }

    #[test]
    fn literal_only_pattern_matches_ignoring_whitespace() {
        let hits = m("a + b", "x\n  a  +   b  \ny");
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn compile_rejects_an_empty_pattern() {
        assert_eq!(Pattern::compile("").unwrap_err(), CompileError::Empty);
        assert_eq!(
            Pattern::compile("   \n\t").unwrap_err(),
            CompileError::Empty
        );
    }

    #[test]
    fn single_hole_matches_one_plain_token() {
        let hits = m("print($X)", "print(42); print(a);");
        assert_eq!(hits.len(), 2);
        assert_eq!(text("print(42); print(a);", hits[0].captures[0].1), "42");
        assert_eq!(text("print(42); print(a);", hits[1].captures[0].1), "a");
    }

    #[test]
    fn single_hole_captures_a_whole_balanced_group() {
        // `$X` matches exactly one lexical unit -- here the entire
        // parenthesized argument, since it's one bracketed group. A `$X`
        // can't span *multiple* units (a group plus an operator plus
        // another token); `$$X` is for that -- see the next few tests.
        let source = "f((a + b))";
        let hits = m("f($X)", source);
        assert_eq!(hits.len(), 1);
        assert_eq!(text(source, hits[0].captures[0].1), "(a + b)");
    }

    #[test]
    fn single_hole_does_not_cross_into_a_sibling_group() {
        // `$X` should capture only `a`, not spill into the following `(b)`.
        let source = "f(a, (b))";
        let hits = m("f($X, $Y)", source);
        assert_eq!(hits.len(), 1);
        assert_eq!(text(source, hits[0].captures[0].1), "a");
        assert_eq!(text(source, hits[0].captures[1].1), "(b)");
    }

    #[test]
    fn multi_hole_captures_a_run_of_tokens_up_to_the_next_literal() {
        let source = "call(a, b, c)";
        let hits = m("call($$ARGS)", source);
        assert_eq!(hits.len(), 1);
        assert_eq!(text(source, hits[0].captures[0].1), "a, b, c");
    }

    #[test]
    fn multi_hole_is_lazy_and_can_capture_nothing() {
        let source = "call()";
        let hits = m("call($$ARGS)", source);
        assert_eq!(hits.len(), 1);
        assert_eq!(text(source, hits[0].captures[0].1), "");
    }

    #[test]
    fn multi_hole_does_not_cross_an_enclosing_close_bracket() {
        // The pattern's own closing `)` must still end the match at the
        // right place, not have `$$ARGS` swallow it and keep going.
        let source = "call(a, b) after";
        let hits = m("call($$ARGS)", source);
        assert_eq!(hits.len(), 1);
        assert_eq!(text(source, hits[0].captures[0].1), "a, b");
        assert_eq!(&source[hits[0].start..hits[0].end], "call(a, b)");
    }

    #[test]
    fn if_statement_pattern_matches_body_hole() {
        // `x > 0` is three lexical units (`x`, `>`, `0`), so the condition
        // needs the multi-hole `$$COND`, not a single `$COND` (which could
        // only capture one of those three tokens).
        let source = "if x > 0 {\n    do_thing();\n    do_other();\n}\nrest();";
        let hits = m("if $$COND { $$BODY }", source);
        assert_eq!(hits.len(), 1);
        assert_eq!(text(source, hits[0].captures[0].1), "x > 0");
        assert_eq!(
            text(source, hits[0].captures[1].1),
            "do_thing();\n    do_other();"
        );
    }

    #[test]
    fn string_literals_tokenize_as_one_unit_including_escapes() {
        let source = r#"log("a \"quoted\" value")"#;
        let hits = m(r"log($X)", source);
        assert_eq!(hits.len(), 1);
        assert_eq!(
            text(source, hits[0].captures[0].1),
            r#""a \"quoted\" value""#
        );
    }

    #[test]
    fn find_all_finds_multiple_non_overlapping_matches() {
        let source = "add(1, 2); add(3, 4); add(5, 6);";
        let hits = m("add($X, $Y)", source);
        assert_eq!(hits.len(), 3);
    }

    #[test]
    fn no_match_yields_an_empty_list() {
        assert!(m("nonexistent($X)", "totally unrelated code here").is_empty());
    }

    #[test]
    fn find_from_skips_matches_before_the_given_offset() {
        let source = "add(1, 2); add(3, 4);";
        let p = Pattern::compile("add($X, $Y)").unwrap();
        let first = p.find_from(source, 0).unwrap();
        assert_eq!(&source[first.start..first.end], "add(1, 2)");
        let second = p.find_from(source, first.end).unwrap();
        assert_eq!(&source[second.start..second.end], "add(3, 4)");
        assert!(p.find_from(source, second.end).is_none());
    }

    #[test]
    fn render_replacement_substitutes_captured_source_text() {
        let source = "old_name(a, b)";
        let hit = Pattern::compile("old_name($$ARGS)")
            .unwrap()
            .find_all(source)
            .remove(0);
        let out = render_replacement("new_name($$ARGS)", source, &hit);
        assert_eq!(out, "new_name(a, b)");
    }

    #[test]
    fn render_replacement_renders_an_unknown_placeholder_as_empty() {
        let m = Match {
            start: 0,
            end: 0,
            captures: vec![],
        };
        assert_eq!(
            render_replacement("before $MISSING after", "", &m),
            "before  after"
        );
    }

    #[test]
    fn render_replacement_keeps_a_bare_dollar_literal() {
        let m = Match {
            start: 0,
            end: 0,
            captures: vec![],
        };
        assert_eq!(render_replacement("cost: $5", "", &m), "cost: $5");
    }
}
