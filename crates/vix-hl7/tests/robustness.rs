//! Property tests: the parser, escape codec and deframer never panic and keep
//! their invariants on arbitrary input.

use proptest::prelude::*;
use vix_hl7::{Delimiters, Message, Path, escape, mllp, pretty, unescape};

proptest! {
    #[test]
    fn parse_never_panics(text in any::<String>()) {
        let m = Message::parse(&text);
        let _ = pretty(&m);
        let _ = m.to_er7();
    }

    #[test]
    fn parse_never_panics_on_hl7_like_text(
        text in "[A-Z0-9|^~\\\\&\r\n\u{0B}\u{1C}#a-z ]{0,200}"
    ) {
        let m = Message::parse(&text);
        for seg in &m.segments {
            for f in &seg.fields {
                prop_assert!(!f.repetitions.is_empty());
            }
        }
        let _ = pretty(&m);
    }

    #[test]
    fn er7_round_trips_well_formed_messages(
        fields in proptest::collection::vec("[A-Za-z0-9 ^~&]{0,12}", 0..8)
    ) {
        let text = format!("MSH|^~\\&|X\rPID|{}", fields.join("|"));
        prop_assert_eq!(Message::parse(&text).to_er7(), text);
    }

    #[test]
    fn escape_round_trips(s in any::<String>()) {
        let d = Delimiters::default();
        let s = s.replace('\r', "");
        prop_assert_eq!(unescape(&escape(&s, &d), &d), s);
    }

    #[test]
    fn unescape_never_panics(s in any::<String>()) {
        let _ = unescape(&s, &Delimiters::default());
    }

    #[test]
    fn path_parse_never_panics(s in any::<String>()) {
        let _ = Path::parse(&s);
    }

    #[test]
    fn mllp_round_trips(payload in proptest::collection::vec(
        any::<u8>().prop_filter("not a frame byte", |b| *b != mllp::START && *b != mllp::END),
        0..200,
    ), split in 0usize..250) {
        let framed = mllp::frame(&payload);
        let cut = split.min(framed.len());
        let mut d = mllp::Deframer::new(1024);
        let mut out = d.push(&framed[..cut]).unwrap();
        out.extend(d.push(&framed[cut..]).unwrap());
        prop_assert_eq!(out, vec![payload]);
    }

    #[test]
    fn deframer_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..300)) {
        let mut d = mllp::Deframer::new(64);
        let _ = d.push(&bytes);
    }
}
