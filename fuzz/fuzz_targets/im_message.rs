//! Fuzz the IM core's handling of received text.
//!
//! Every message, nick, topic, history line and thread file comes from
//! someone else (an IRC server, a Cowork peer, a file pulled from a
//! repository), so the sanitizer, the reference and mention finders, the
//! slash-command parser, the JSONL history reader and the thread re-anchorer
//! must all be total, and the sanitizer must actually remove what it
//! promises to.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    let clean = vix_im::sanitize(text);
    assert!(clean.chars().all(|c| !c.is_control()));
    assert!(!clean.chars().any(vix_im::sanitize::is_bidi_control));
    assert_eq!(vix_im::sanitize(&clean), clean);
    let multi = vix_im::sanitize_multiline(text);
    assert!(multi.chars().all(|c| c == '\n' || !c.is_control()));

    for m in vix_im::find_refs(text) {
        // The host slices the body at these offsets to underline a reference.
        assert!(text.is_char_boundary(m.range.start) && text.is_char_boundary(m.range.end));
        assert!(m.reference.line >= 1);
        let _ = vix_im::resolve_in_root(std::path::Path::new("/w"), &m.reference);
    }
    for (range, nick) in vix_im::find_mentions(text) {
        assert_eq!(&text[range][1..], nick);
    }
    let _ = vix_im::parse_input(text);
    let _ = vix_im::history::decode(text);

    if let Ok(thread) = vix_im::Thread::from_json(text) {
        let lines: Vec<&str> = text.lines().collect();
        let _ = vix_im::reanchor(&thread, &lines);
    }
});
