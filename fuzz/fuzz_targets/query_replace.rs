//! Fuzz the find/query-replace engine (`vix-find-panel`).
//!
//! `matches`/`next_match`/`replace_all`/`replace_one` all convert between byte
//! and character offsets over arbitrary UTF-8 (multi-byte characters are the
//! whole reason `char_to_byte` exists), and `unescape` walks a user-typed
//! replacement template one character at a time. A buffer full of combining
//! marks, a query that is itself a pathological regex, or a replacement
//! template with a trailing backslash must not panic.

#![no_main]

use libfuzzer_sys::fuzz_target;
use regex::Regex;
use vix_find_panel::{
    PathFilter, SearchBar, matches, next_match, replace_all, replace_one, unescape,
};

fuzz_target!(|data: &[u8]| {
    // Byte 0 picks the query length and a few flags; the rest splits into the
    // search text and (when replacing) a replacement/template.
    let Some((&flags, rest)) = data.split_first() else {
        return;
    };
    let regex_mode = flags & 1 != 0;
    let whole_word = flags & 2 != 0;
    let case_sensitive = flags & 4 != 0;
    let query_len = usize::from(flags >> 3).min(rest.len());
    let (query_bytes, rest) = rest.split_at(query_len);
    let Ok(query) = std::str::from_utf8(query_bytes) else {
        return;
    };
    let Ok(text) = std::str::from_utf8(rest) else {
        return;
    };

    let mut bar = SearchBar::new(true);
    bar.query = query.to_string();
    bar.regex = regex_mode;
    bar.whole_word = whole_word;
    bar.case_sensitive = case_sensitive;
    bar.replace = text.chars().take(64).collect(); // a plausible template too

    // `unescape` must be total over any string, and idempotent on a string
    // with no backslashes (nothing left to unescape).
    let unescaped = unescape(&bar.replace);
    if !bar.replace.contains('\\') {
        assert_eq!(
            unescape(&unescaped),
            unescaped,
            "no backslashes: unescape is a no-op"
        );
    }

    let Some(pattern) = bar.pattern() else {
        return; // empty query
    };
    let Ok(re) = Regex::new(&pattern) else {
        return; // `whole_word`/case wrapping produced an invalid pattern
    };

    let hits = matches(text, &re);
    // Every hit is an ordered, in-bounds char range, and starts are non-decreasing.
    let char_len = text.chars().count();
    let mut prev_start = 0;
    for (i, &(s, e)) in hits.iter().enumerate() {
        assert!(s <= e, "match {i} is inverted: {s}..{e}");
        assert!(
            e <= char_len,
            "match {i} ends past the text: {e} > {char_len}"
        );
        assert!(i == 0 || s >= prev_start, "matches are not in order");
        prev_start = s;
    }

    if let Some((s, e)) = next_match(text, &re, 0) {
        assert!(s <= e && e <= char_len, "next_match out of range: {s}..{e}");
    }

    let template = if regex_mode {
        unescape(&bar.replace)
    } else {
        bar.replace.clone()
    };
    let (_replaced, count) = replace_all(text, &re, regex_mode, &template);
    assert_eq!(
        count,
        hits.len(),
        "replace_all's count disagrees with matches()"
    );

    // `replace_one` at every hit's start must succeed and return a resume
    // offset no further than the whole (possibly grown) text.
    for &(s, _) in &hits {
        if let Some((new_text, resume)) = replace_one(text, &re, regex_mode, &template, s) {
            assert!(
                resume <= new_text.chars().count(),
                "resume offset past the end of the replaced text"
            );
        }
    }

    let _ = PathFilter::new(query, &bar.replace).allows(text);
});
