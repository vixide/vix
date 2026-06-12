//! Spell-checking for the Vix editor.
//!
//! Wraps the pure-Rust [`spellbook`] Hunspell checker. A [`SpellChecker`] loads a
//! `index.aff` + `index.dic` pair — e.g. from the repo's `dictionaries/<locale>/`
//! set (the [wooorm/dictionaries] layout) — and answers two questions: is a word
//! spelled correctly ([`SpellChecker::check`]) and what are some corrections
//! ([`SpellChecker::suggest`]). It also keeps a session **user dictionary** (added
//! words) and an **ignore** set.
//!
//! For editor integration, [`SpellChecker::misspellings_in`] tokenizes a slice of
//! text (intended to be a comment or string range) and returns the character
//! spans of the misspelled words, skipping things that look like code:
//! all-caps acronyms (`HTTP`), camel/Pascal-case identifiers (`fooBar`), and very
//! short tokens.
//!
//! [`spellbook`]: https://crates.io/crates/spellbook
//! [wooorm/dictionaries]: https://github.com/wooorm/dictionaries

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use std::collections::HashSet;
use std::path::Path;

use spellbook::Dictionary;

/// Default cap on how many suggestions [`SpellChecker::suggest`] returns.
pub const DEFAULT_MAX_SUGGESTIONS: usize = 7;

/// An error loading or parsing a dictionary.
#[derive(Debug)]
pub enum Error {
    /// Reading an `.aff`/`.dic` file failed.
    Io(std::io::Error),
    /// `spellbook` could not parse the dictionary.
    Parse(String),
    /// No dictionary directory matched the requested locale (or its fallbacks).
    NotFound(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "dictionary I/O error: {e}"),
            Error::Parse(e) => write!(f, "dictionary parse error: {e}"),
            Error::NotFound(loc) => write!(f, "no dictionary found for locale '{loc}'"),
        }
    }
}

impl std::error::Error for Error {}

/// A loaded dictionary plus the session's user/ignore words.
pub struct SpellChecker {
    dict: Dictionary,
    user: HashSet<String>,
    ignored: HashSet<String>,
    max_suggestions: usize,
}

impl SpellChecker {
    /// Build a checker from in-memory `.aff` and `.dic` contents.
    ///
    /// # Errors
    /// Returns [`Error::Parse`] if `spellbook` rejects the dictionary.
    pub fn from_strings(aff: &str, dic: &str) -> Result<Self, Error> {
        let dict = Dictionary::new(aff, dic).map_err(|e| Error::Parse(format!("{e:?}")))?;
        Ok(SpellChecker {
            dict,
            user: HashSet::new(),
            ignored: HashSet::new(),
            max_suggestions: DEFAULT_MAX_SUGGESTIONS,
        })
    }

    /// Load the dictionary for `locale` from `dir`, reading
    /// `<dir>/<resolved>/index.aff` and `index.dic`. The locale is resolved
    /// against a fallback chain (e.g. `en-GB` → `en`, then `en`), so a regional
    /// request still finds the base language.
    ///
    /// # Errors
    /// Returns [`Error::NotFound`] when no candidate directory has both files,
    /// [`Error::Io`] on a read failure, or [`Error::Parse`] on a parse failure.
    pub fn load(dir: &Path, locale: &str) -> Result<Self, Error> {
        for candidate in locale_candidates(locale) {
            let sub = dir.join(&candidate);
            let aff = sub.join("index.aff");
            let dic = sub.join("index.dic");
            if aff.is_file() && dic.is_file() {
                let aff_s = std::fs::read_to_string(&aff).map_err(Error::Io)?;
                let dic_s = std::fs::read_to_string(&dic).map_err(Error::Io)?;
                return Self::from_strings(&aff_s, &dic_s);
            }
        }
        Err(Error::NotFound(locale.to_string()))
    }

    /// Override the suggestion cap (default [`DEFAULT_MAX_SUGGESTIONS`]).
    #[must_use]
    pub fn with_max_suggestions(mut self, max: usize) -> Self {
        self.max_suggestions = max;
        self
    }

    /// Whether `word` is spelled correctly: in the ignore set, the user
    /// dictionary, or the loaded dictionary. A sentence-initial capital is
    /// accepted by also trying the lowercased form.
    #[must_use]
    pub fn check(&self, word: &str) -> bool {
        if self.ignored.contains(word) || self.user.contains(word) {
            return true;
        }
        if self.dict.check(word) {
            return true;
        }
        let lower = word.to_lowercase();
        lower != word && (self.user.contains(&lower) || self.dict.check(&lower))
    }

    /// Up to `max_suggestions` corrections for a (presumably misspelled) word.
    #[must_use]
    pub fn suggest(&self, word: &str) -> Vec<String> {
        let mut out = Vec::new();
        self.dict.suggest(word, &mut out);
        out.truncate(self.max_suggestions);
        out
    }

    /// Add a word to the session user dictionary (treated as correct hereafter).
    pub fn add_word(&mut self, word: &str) {
        self.user.insert(word.to_string());
    }

    /// Ignore a word for the rest of the session (treated as correct, but not a
    /// permanent dictionary addition).
    pub fn ignore_word(&mut self, word: &str) {
        self.ignored.insert(word.to_string());
    }

    /// Find misspelled words in `text`, returning their character spans offset by
    /// `base` (the char position of `text` within the larger buffer). Tokens that
    /// look like code — all-caps acronyms, camel/Pascal-case identifiers, and
    /// very short words — are skipped.
    #[must_use]
    pub fn misspellings_in(&self, text: &str, base: usize) -> Vec<(usize, usize)> {
        tokenize(text)
            .into_iter()
            .filter(|(_, _, w)| should_check(w))
            .filter(|(_, _, w)| !self.check(w))
            .map(|(start, end, _)| (base + start, base + end))
            .collect()
    }
}

/// The locale-directory fallback chain: the locale itself, its base language
/// (before the first `-`), and `en` as a final resort — deduplicated, order kept.
fn locale_candidates(locale: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut push = |c: String| {
        if !c.is_empty() && !out.contains(&c) {
            out.push(c);
        }
    };
    push(locale.to_string());
    if let Some((base, _)) = locale.split_once('-') {
        push(base.to_string());
    }
    push("en".to_string());
    out
}

/// Split `text` into `(start_char, end_char_exclusive, word)` tokens: maximal
/// runs of alphabetic characters, allowing an apostrophe between two letters
/// (so `don't` stays one token).
fn tokenize(text: &str) -> Vec<(usize, usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < n {
        if !chars[i].is_alphabetic() {
            i += 1;
            continue;
        }
        let start = i;
        let mut word = String::new();
        while i < n {
            let c = chars[i];
            if c.is_alphabetic() {
                word.push(c);
                i += 1;
            } else if (c == '\'' || c == '\u{2019}')
                && !word.is_empty()
                && i + 1 < n
                && chars[i + 1].is_alphabetic()
            {
                word.push('\'');
                i += 1;
            } else {
                break;
            }
        }
        tokens.push((start, i, word));
    }
    tokens
}

/// Whether a token is worth spell-checking (i.e. looks like a prose word, not
/// code): at least three letters, not an all-caps acronym, and no interior
/// capital (which would mark a camel/Pascal-case identifier).
fn should_check(word: &str) -> bool {
    let letters = word.chars().filter(|c| c.is_alphabetic()).count();
    if letters < 3 {
        return false;
    }
    let has_upper = word.chars().any(char::is_uppercase);
    let all_caps = has_upper && word.chars().filter(|c| c.is_alphabetic()).all(|c| c.is_uppercase());
    if all_caps {
        return false;
    }
    // An uppercase letter anywhere but the first position => identifier-like.
    if word.chars().skip(1).any(char::is_uppercase) {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const AFF: &str = "SET UTF-8\nTRY esianrtolcdugmphbyfvkwz\n";
    const DIC: &str = "5\nhello\nworld\ncode\nspell\ncheck\n";

    fn checker() -> SpellChecker {
        SpellChecker::from_strings(AFF, DIC).expect("tiny dict parses")
    }

    #[test]
    fn check_known_and_unknown_words() {
        let sc = checker();
        assert!(sc.check("hello"));
        assert!(sc.check("world"));
        assert!(!sc.check("helo"));
        // Sentence-initial capital is accepted via the lowercased fallback.
        assert!(sc.check("Hello"));
    }

    #[test]
    fn user_dictionary_and_ignore_make_words_pass() {
        let mut sc = checker();
        assert!(!sc.check("vix"));
        sc.add_word("vix");
        assert!(sc.check("vix"));
        assert!(!sc.check("foobarbaz"));
        sc.ignore_word("foobarbaz");
        assert!(sc.check("foobarbaz"));
    }

    #[test]
    fn misspellings_report_char_spans_with_base_offset() {
        let sc = checker();
        // "helo" (0..4) is wrong; "world" (5..10) is right.
        let spans = sc.misspellings_in("helo world", 0);
        assert_eq!(spans, vec![(0, 4)]);
        // With a base offset, spans shift accordingly.
        let spans = sc.misspellings_in("helo world", 100);
        assert_eq!(spans, vec![(100, 104)]);
    }

    #[test]
    fn code_like_tokens_are_skipped() {
        let sc = checker();
        // All-caps acronym, camelCase, PascalCase, and a 2-letter word: all skipped.
        let spans = sc.misspellings_in("HTTP fooBar BazQux ok", 0);
        assert!(spans.is_empty(), "got: {spans:?}");
    }

    #[test]
    fn apostrophes_stay_within_a_token() {
        let toks = tokenize("don't stop");
        assert_eq!(toks[0].2, "don't");
        assert_eq!(toks[0], (0, 5, "don't".to_string()));
    }

    // Smoke test against the repo's real `dictionaries/en` Hunspell files.
    // Ignored by default because that directory is large and not always present;
    // run with `cargo test -p vix-spellcheck -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs the repo's large, sometimes-absent ./dictionaries/en set"]
    fn real_en_dictionary_loads_and_suggests() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../dictionaries");
        let sc = SpellChecker::load(&dir, "en").expect("load en dictionary");
        assert!(sc.check("hello"));
        assert!(sc.check("dictionary"));
        assert!(!sc.check("teh"));
        let sugg = sc.suggest("teh");
        println!("suggest(teh) = {sugg:?}");
        assert!(sugg.iter().any(|s| s == "the"), "expected 'the' among {sugg:?}");
        let spans = sc.misspellings_in("This sentnce has a typoe in it.", 0);
        println!("misspelled spans = {spans:?}");
        assert_eq!(spans.len(), 2, "two misspelled words");
    }

    #[test]
    fn locale_fallback_chain_dedupes_and_appends_en() {
        assert_eq!(locale_candidates("en-GB"), vec!["en-GB", "en"]);
        assert_eq!(locale_candidates("fr"), vec!["fr", "en"]);
        assert_eq!(locale_candidates("en"), vec!["en"]);
        assert_eq!(locale_candidates("pt-BR"), vec!["pt-BR", "pt", "en"]);
    }
}
