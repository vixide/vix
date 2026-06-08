//! Interactive query-replace: step through matches one at a time, deciding
//! `y` (replace), `n` (skip), `!` (replace the rest), or `q` (quit) for each.

use regex::Regex;

#[derive(Clone, Copy)]
pub enum Decision {
    Replace,
    Skip,
    ReplaceRest,
    Quit,
}

/// State for an in-progress query-replace session. The driving logic lives in
/// `App`, which owns the buffer the matches live in.
pub struct QueryReplace {
    pub re: Regex,
    /// Replacement template — already un-escaped when in regex mode, so the
    /// regex engine only has capture groups left to expand.
    pub template: String,
    /// Whether to expand `$1`/`${name}` capture references in the template.
    pub regex: bool,
    /// Character offsets `[start, end)` of the match currently highlighted.
    pub current: (usize, usize),
    /// How many replacements have been applied so far.
    pub replaced: usize,
    /// The original query text, for the prompt label.
    pub label: String,
}
