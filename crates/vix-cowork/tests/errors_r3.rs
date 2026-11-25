//! Round-3: the text and conversions of the typed errors.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests fail loudly on a broken fixture"
)]

use vix_cowork::{CoworkError, PathProblem, WireProblem};

#[test]
fn path_problems_read_as_short_reasons() {
    let cases = [
        (PathProblem::Empty, "empty"),
        (PathProblem::Absolute, "absolute"),
        (PathProblem::ParentEscape, "escapes the root"),
        (PathProblem::Control, "control character"),
        (PathProblem::TooLong, "too long"),
    ];
    for (p, text) in cases {
        assert_eq!(p.to_string(), text);
        assert_eq!(CoworkError::from(p), CoworkError::BadPath(p));
        assert_eq!(
            CoworkError::BadPath(p).to_string(),
            format!("bad path: {text}")
        );
    }
}

#[test]
fn wire_problems_name_the_offending_value() {
    let cases = [
        (WireProblem::Truncated, "truncated"),
        (WireProblem::UnknownTag(9), "unknown message tag 9"),
        (WireProblem::UnknownPermission(7), "unknown permission 7"),
        (WireProblem::BadUtf8, "invalid UTF-8"),
        (WireProblem::FieldTooLong, "field too long"),
        (WireProblem::TrailingBytes, "trailing bytes"),
        (WireProblem::Version(3), "unsupported protocol version 3"),
        (WireProblem::Reserved(200), "reserved message tag 200"),
        (WireProblem::BadTerminalSize, "terminal size out of range"),
    ];
    for (w, text) in cases {
        assert_eq!(w.to_string(), text);
        assert_eq!(CoworkError::from(w).to_string(), format!("wire: {text}"));
    }
}

#[test]
fn cowork_errors_display_without_document_text() {
    assert_eq!(
        CoworkError::TooLarge { size: 10, cap: 4 }.to_string(),
        "10 bytes exceeds the 4 byte cap"
    );
    assert_eq!(
        CoworkError::Binary.to_string(),
        "binary content is not shared"
    );
    assert_eq!(
        CoworkError::OutOfRange { offset: 9, len: 3 }.to_string(),
        "offset 9 is outside a document of 3 characters"
    );
    assert_eq!(
        CoworkError::PermissionDenied.to_string(),
        "permission denied"
    );
    assert_eq!(
        CoworkError::ForgedPeer.to_string(),
        "update authored as another peer"
    );
    assert_eq!(
        CoworkError::NotAnUpdate.to_string(),
        "not an incremental update"
    );
    assert_eq!(CoworkError::Crdt("bad".into()).to_string(), "crdt: bad");
    let _: &dyn std::error::Error = &CoworkError::Binary;
}

#[test]
fn only_reserved_wire_tags_are_skippable() {
    assert!(CoworkError::Wire(WireProblem::Reserved(0xC0)).is_reserved_message());
    assert!(!CoworkError::Wire(WireProblem::UnknownTag(0xC0)).is_reserved_message());
    assert!(!CoworkError::Binary.is_reserved_message());
}
