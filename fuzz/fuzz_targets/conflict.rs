//! Fuzz the merge-conflict marker parser.
//!
//! Conflict markers come from `git`, but the buffer they land in is edited by
//! hand, so half-deleted markers, nested-looking ones, and markers without a
//! separator all occur. `find` must never panic and must return byte offsets
//! that slice the text on char boundaries — the resolver splices at them.

#![no_main]

use libfuzzer_sys::fuzz_target;
use vix_conflict_tool::{Resolution, find};

fuzz_target!(|data: &[u8]| {
    let (head, text) = data.split_at(data.len().min(1));
    let Ok(text) = std::str::from_utf8(text) else {
        return;
    };
    // `find` takes a 0-based *line*, and a cursor can sit past the last one.
    let lines = text.split_inclusive('\n').count();
    let at = head.first().map_or(0, |b| usize::from(*b)) % (lines + 1);

    if let Some(conflict) = find(text, at) {
        assert!(conflict.start < conflict.end, "conflict spans no lines");
        assert!(conflict.end <= lines, "conflict ends past the last line");
        // The two sides are made of whole lines taken from the text, so the
        // resolver can splice them back in verbatim.
        assert!(
            text.contains(conflict.ours.as_str()),
            "ours is not from the text"
        );
        assert!(
            text.contains(conflict.theirs.as_str()),
            "theirs is not from the text"
        );
        for how in [Resolution::Ours, Resolution::Theirs, Resolution::Both] {
            let _ = conflict.resolved(how);
        }
    }
});
