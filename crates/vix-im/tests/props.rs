//! Property tests for the sanitizer and the code-reference parser.

use proptest::prelude::*;
use vix_im::sanitize::is_bidi_control;
use vix_im::{CodeRef, Message, find_refs, history, parse_ref, sanitize, sanitize_multiline};

/// Text weighted towards the characters the sanitizer cares about.
fn hostile() -> impl Strategy<Value = String> {
    let pieces = prop_oneof![
        any::<char>().prop_map(String::from),
        Just("\u{1b}".to_string()),
        Just("\u{1b}[".to_string()),
        Just("\u{1b}]".to_string()),
        Just("\u{1b}\\".to_string()),
        Just("\u{1b}P".to_string()),
        Just("\u{7}".to_string()),
        Just("\u{9b}".to_string()),
        Just("\u{9c}".to_string()),
        Just("\u{9d}".to_string()),
        Just("\t".to_string()),
        Just("\r\n".to_string()),
        Just("\u{202E}".to_string()),
        Just("\u{2067}".to_string()),
        Just("\u{200D}".to_string()),
        Just("31;1m".to_string()),
        "[a-z ]{0,4}",
    ];
    proptest::collection::vec(pieces, 0..24).prop_map(|v| v.concat())
}

proptest! {
    #[test]
    fn output_is_clean(text in hostile()) {
        for out in [sanitize(&text), sanitize_multiline(&text)] {
            for c in out.chars() {
                prop_assert!(!(c.is_control() && c != '\n'), "control {:?} in {:?}", c, out);
                prop_assert!(!is_bidi_control(c), "bidi {:?} in {:?}", c, out);
            }
        }
        prop_assert!(!sanitize(&text).contains('\n'));
    }

    #[test]
    fn idempotent(text in hostile()) {
        let once = sanitize(&text);
        prop_assert_eq!(sanitize(&once), once);
        let once = sanitize_multiline(&text);
        prop_assert_eq!(sanitize_multiline(&once), once);
    }

    #[test]
    fn printable_ascii_unchanged(text in "[ -~]{0,80}") {
        prop_assert_eq!(sanitize(&text), text);
    }

    #[test]
    fn other_scripts_unchanged(text in "[\\p{Arabic}\\p{Devanagari}\\p{Han}\\p{Hebrew} ]{0,40}") {
        // Arabic and Hebrew blocks hold no bidi *controls* (those live in
        // General Punctuation), so nothing is removed.
        prop_assert_eq!(sanitize(&text), text);
    }

    #[test]
    fn references_round_trip(
        dir in "[a-z]{1,8}",
        file in "[a-z_]{1,8}",
        line in 1u32..100_000,
        extra in prop_oneof![Just(0u32), 1u32..500],
        column_form in any::<bool>(),
    ) {
        let mut r = CodeRef::new(format!("{dir}/{file}.rs"), line);
        if extra > 0 {
            if column_form {
                r.column = Some(extra);
            } else {
                r.end_line = Some(line + extra);
            }
        }
        let text = r.to_string();
        prop_assert_eq!(parse_ref(&text), Some(r.clone()));
        let body = format!("look at ({text}), please");
        let found = find_refs(&body);
        prop_assert_eq!(found.len(), 1);
        prop_assert_eq!(&body[found[0].range.clone()], text.as_str());
        prop_assert_eq!(&found[0].reference, &r);
    }

    #[test]
    fn find_refs_never_panics_and_ranges_are_valid(text in any::<String>()) {
        for m in find_refs(&text) {
            prop_assert!(text.is_char_boundary(m.range.start));
            prop_assert!(text.is_char_boundary(m.range.end));
            prop_assert!(m.reference.line >= 1);
        }
    }

    #[test]
    fn history_round_trips_clean_messages(author in "[a-z]{0,8}", body in "[ -~]{0,60}", t in any::<i64>()) {
        let m = Message::text(author, t, body);
        let text = history::encode(std::slice::from_ref(&m)).unwrap();
        prop_assert_eq!(history::decode(&text), vec![m]);
    }
}

#[test]
fn emoji_joiner_sequences_unchanged() {
    for text in [
        "👩\u{200D}💻 at work",
        "🏳️\u{200D}🌈",
        "क्\u{200D}ष",
        "می\u{200C}روم",
    ] {
        assert_eq!(sanitize(text), text);
    }
}
