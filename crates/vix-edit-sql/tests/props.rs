//! Property tests for the SQL statement splitter and keyword formatter
//! (T831): `split_statements` never panics and returns trimmed, non-empty
//! pieces of its input in order; `Editor::to_text` writes statements that
//! split back to the same statements; `format_sql` only changes letter case
//! and is idempotent.

use proptest::prelude::*;
use vix_edit_sql::{Editor, format_sql, split_statements};

/// One well-formed SQL token: words, closed strings and comments (each may
/// hold a `;`), blanks and top-level semicolons.
fn token() -> impl Strategy<Value = String> {
    prop_oneof![
        "(select|from|where|insert|values|[a-z]{1,6}|[0-9]{1,3})",
        "[ \n\t]{1,2}",
        Just(";".to_string()),
        "'[a-z; \"-]{0,5}'",
        "\"[a-z; '-]{0,5}\"",
        "--[a-z; '\"*/]{0,6}\n",
        "/\\*[a-z; '\"\n-]{0,6}\\*/",
    ]
}

/// Well-formed SQL text: a run of tokens.
fn sql() -> impl Strategy<Value = String> {
    prop::collection::vec(token(), 0..16).prop_map(|t| t.concat())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Any text splits without panicking into trimmed, non-empty pieces,
    /// each a piece of the input, in input order.
    #[test]
    fn split_returns_trimmed_pieces_of_the_input_in_order(text in any::<String>()) {
        let mut from = 0;
        for stmt in split_statements(&text) {
            prop_assert!(!stmt.is_empty());
            prop_assert_eq!(stmt.trim(), stmt.as_str());
            let rest = text.get(from..).unwrap_or_default();
            let Some(at) = rest.find(&stmt) else {
                return Err(TestCaseError::fail(format!("{stmt:?} not in order in {text:?}")));
            };
            from += at + stmt.len();
        }
    }

    /// Saving the statement view and reopening it gives the same statements,
    /// whatever strings and comments (with or without `;`) they hold.
    #[test]
    fn to_text_splits_back_to_the_same_statements(text in sql()) {
        let editor = Editor::from_text(&text);
        let saved = editor.to_text();
        let reopened = Editor::from_text(&saved);
        prop_assert_eq!(reopened.len(), editor.len(), "saved as {:?}", saved);
        for i in 0..editor.len() {
            prop_assert_eq!(reopened.statement(i), editor.statement(i), "saved as {:?}", saved);
        }
        prop_assert_eq!(reopened.to_text(), saved);
    }

    /// The formatter only changes the case of ASCII letters, and running it
    /// twice changes nothing more.
    #[test]
    fn format_changes_only_case_and_is_idempotent(text in prop_oneof![sql(), any::<String>()]) {
        let once = format_sql(&text);
        prop_assert_eq!(once.to_ascii_lowercase(), text.to_ascii_lowercase());
        prop_assert_eq!(format_sql(&once), once);
    }
}
