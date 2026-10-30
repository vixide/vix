//! Property tests: the codec round-trips, never panics on hostile frames,
//! and rejects any tampering with a signed part; ANSI stripping leaves no
//! control characters; Org results insert and clear symmetrically.

use proptest::prelude::*;
use serde_json::{Value, json};
use vix_jupyter_core::ansi::strip_ansi;
use vix_jupyter_core::org::{self, ResultValue};
use vix_jupyter_core::wire::{self, DELIMITER, DecodeError, Limits, Signer};
use vix_jupyter_core::{Message, Output};

fn signer() -> Signer {
    let Ok(signer) = Signer::new(b"0123456789abcdef0123456789abcdef") else {
        unreachable!("a non-empty key is accepted")
    };
    signer
}

fn json_leaf() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(|n| json!(n)),
        ".{0,24}".prop_map(Value::String),
    ]
}

fn json_value() -> impl Strategy<Value = Value> {
    json_leaf().prop_recursive(3, 24, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
            prop::collection::btree_map("[a-z_/]{1,10}", inner, 0..4)
                .prop_map(|m| Value::Object(m.into_iter().collect())),
        ]
    })
}

fn message() -> impl Strategy<Value = Message> {
    (
        "[a-z_]{1,20}",
        json_value(),
        "[a-f0-9-]{1,36}",
        prop::collection::vec(prop::collection::vec(any::<u8>(), 0..8), 0..3),
        prop::collection::vec(prop::collection::vec(any::<u8>(), 0..16), 0..3),
        any::<bool>(),
    )
        .prop_map(
            |(msg_type, content, id, identities, buffers, with_parent)| {
                let mut m = Message::new(
                    &msg_type,
                    content,
                    &id,
                    "session",
                    "vix",
                    "2026-10-30T00:00:00Z",
                );
                // An identity equal to the delimiter would be read as the delimiter.
                m.identities = identities.into_iter().filter(|i| i != DELIMITER).collect();
                m.buffers = buffers;
                if with_parent {
                    m.parent_header =
                        Some(Message::new("execute_request", json!({}), "p", "s", "u", "").header);
                }
                m
            },
        )
}

proptest! {
    #[test]
    fn round_trip(m in message()) {
        let frames = wire::encode(&m, &signer());
        prop_assert_eq!(wire::decode(&frames, &signer(), Limits::default()), Ok(m));
    }

    #[test]
    fn tampering_with_a_signed_part_is_rejected(m in message(), part in 0usize..5, at in any::<usize>(), bit in 0u8..8) {
        let mut frames = wire::encode(&m, &signer());
        let base = m.identities.len() + 1; // signature frame
        let frame = &mut frames[base + part];
        if frame.is_empty() {
            frame.push(b'x');
        } else {
            let i = at % frame.len();
            frame[i] ^= 1 << bit;
        }
        let result = wire::decode(&frames, &signer(), Limits::default());
        // Flipping the signature may also turn it into valid-looking hex of
        // another value; either way it is never accepted.
        prop_assert_eq!(result, Err(DecodeError::BadSignature));
    }

    #[test]
    fn hostile_frames_never_panic(frames in prop::collection::vec(prop::collection::vec(any::<u8>(), 0..64), 0..12), delim_at in any::<prop::sample::Index>()) {
        let mut frames = frames;
        if !frames.is_empty() {
            let i = delim_at.index(frames.len());
            frames[i] = DELIMITER.to_vec();
        }
        let _ = wire::decode(&frames, &signer(), Limits::default());
    }

    #[test]
    fn ansi_strip_leaves_no_controls(s in "(\\PC|\u{1b}|\\[|[0-9;]|\u{7}|\r|\n|\t|\u{9b}){0,64}") {
        let out = strip_ansi(&s);
        prop_assert!(!out.chars().any(|c| c.is_control() && c != '\n' && c != '\t'));
        prop_assert_eq!(strip_ansi(&out), out);
    }

    #[test]
    fn org_insert_then_clear_restores(body in prop::collection::vec("[ -~]{0,20}", 0..14), before in "[a-z ]{0,10}", after in "[a-z ]{0,10}") {
        let buffer = format!("{before}\n#+begin_src python\nx\n#+end_src\n{after}\n");
        // `after` must not itself look like results.
        prop_assume!(!after.trim().is_empty());
        let block = org::results_block(&ResultValue::Text(body.join("\n")));
        let with = org::apply(&buffer, &org::results_edit(&buffer, 3, Some(&block)));
        prop_assert_eq!(org::find_results(&with, 3).is_some(), true);
        let again = org::apply(&with, &org::results_edit(&with, 3, Some(&block)));
        prop_assert_eq!(&again, &with);
        let cleared = org::apply(&with, &org::results_edit(&with, 3, None));
        prop_assert_eq!(cleared, buffer);
    }
}

/// IOPub contents in the shape ipykernel 7 sends them (as captured in the
/// T734 spike): each must type and convert to the expected output.
#[test]
fn ipykernel_fixtures() {
    let cases: [(&str, Value, &str); 5] = [
        (
            "stream",
            json!({"name": "stdout", "text": "hello\n"}),
            "hello\n",
        ),
        (
            "execute_result",
            json!({"execution_count": 3, "data": {"text/plain": "2"}, "metadata": {}}),
            "2",
        ),
        (
            "error",
            json!({"ename": "ZeroDivisionError", "evalue": "division by zero", "traceback": [
                "\u{1b}[31m---------------------------------------------------------------------------\u{1b}[39m",
                "\u{1b}[31mZeroDivisionError\u{1b}[39m: division by zero"
            ]}),
            "ZeroDivisionError: division by zero\n---------------------------------------------------------------------------\nZeroDivisionError: division by zero",
        ),
        (
            "display_data",
            json!({"data": {"image/png": "iVBORw0KGgo=\n", "text/plain": "<IPython.core.display.Image object>"}, "metadata": {}, "transient": {}}),
            "<IPython.core.display.Image object>",
        ),
        (
            "display_data",
            json!({"data": {"text/html": "<b>hi</b>", "text/plain": "<IPython.core.display.HTML object>"}, "metadata": {}, "transient": {"display_id": "d1"}}),
            "<IPython.core.display.HTML object>",
        ),
    ];
    for (msg_type, content, text) in cases {
        let m = Message::new(msg_type, content, "id", "s", "u", "");
        let frames = wire::encode(&m, &signer());
        let back = wire::decode(&frames, &signer(), Limits::default()).unwrap();
        let output = Output::from_content(back.typed().unwrap()).unwrap();
        assert_eq!(output.plain_text(), text, "{msg_type}");
    }
}
