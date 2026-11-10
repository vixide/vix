//! Property tests for the JSON-lines codec: decoding never panics, and
//! every request and reply survives a round trip on one line.

use proptest::prelude::*;
use vix_matrix_protocol::{
    MessageKind, Reply, Request, decode_reply, decode_request, encode_reply, encode_request,
};

/// Any of the three message kinds.
fn kind() -> impl Strategy<Value = MessageKind> {
    prop_oneof![
        Just(MessageKind::Text),
        Just(MessageKind::Emote),
        Just(MessageKind::Notice)
    ]
}

proptest! {
    #[test]
    fn decoding_any_text_never_panics(line in any::<String>()) {
        let _ = decode_request(&line);
        let _ = decode_reply(&line);
    }

    #[test]
    fn decoding_json_like_text_never_panics(
        line in r#"\{("[a-z_]{1,8}":("[^"]{0,8}"|[0-9]{1,3}|true|null|\[\]),?){0,6}\}"#
    ) {
        let _ = decode_request(&line);
        let _ = decode_reply(&line);
    }

    #[test]
    fn a_send_round_trips_on_one_line(room in any::<String>(), body in any::<String>(), kind in kind()) {
        let request = Request::Send { room, body, kind };
        let line = encode_request(&request);
        prop_assert!(!line.contains('\n') && !line.contains('\r'));
        prop_assert_eq!(decode_request(&line), Ok(request));
    }

    #[test]
    fn a_message_round_trips_on_one_line(
        body in any::<String>(),
        sender in any::<String>(),
        name in proptest::option::of(any::<String>()),
        time in any::<i64>(),
        kind in kind(),
        encrypted in any::<bool>(),
        verified in any::<bool>(),
    ) {
        let reply = Reply::Message {
            room: "!r:h".into(),
            event_id: "$e".into(),
            sender,
            sender_name: name,
            time,
            body,
            kind,
            encrypted,
            verified,
        };
        let line = encode_reply(&reply);
        prop_assert!(!line.contains('\n') && !line.contains('\r'));
        prop_assert_eq!(decode_reply(&line), Ok(reply));
    }
}
