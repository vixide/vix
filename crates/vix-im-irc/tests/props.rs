//! Property tests: the codec and the state machine never panic on untrusted
//! input, and nothing they send can carry a second command.

use proptest::prelude::*;
use vix_im_irc::codec::{Line, Tag, escape_tag_value, server_time_millis, unescape_tag_value};
use vix_im_irc::split::{MAX_LINE, privmsg_lines, split_utf8};
use vix_im_irc::{Client, Command, Config};

fn assert_clean(lines: &[String]) {
    for l in lines {
        assert!(!l.contains(['\r', '\n', '\0']), "{l:?}");
        assert!(l.len() + 2 <= MAX_LINE, "{} bytes", l.len());
    }
}

fn registered() -> Client {
    let mut c = Client::new(Config {
        nick: "ana".into(),
        user: "ana".into(),
        realname: "Ana".into(),
        channels: vec![],
        sasl: None,
    });
    let _ = c.start();
    let _ = c.handle_line(":srv 001 ana :Welcome");
    c
}

fn middle() -> impl Strategy<Value = String> {
    "[^ :\r\n\0][^ \r\n\0]{0,10}"
}

prop_compose! {
    fn line()(
        tags in proptest::collection::vec(("[a-z+][a-z0-9/.-]{0,6}", proptest::option::of("[^\0]{1,10}")), 0..3),
        command in "[A-Z]{1,8}|[0-9]{3}",
        mids in proptest::collection::vec(middle(), 0..4),
        trailing in proptest::option::of("[^\r\n\0]{0,20}"),
    ) -> Line {
        let mut params = mids;
        if let Some(t) = trailing { params.push(t); }
        Line {
            tags: tags.into_iter().map(|(key, value)| Tag { key, value }).collect(),
            prefix: None,
            command,
            params,
        }
    }
}

proptest! {
    #[test]
    fn parse_never_panics(s in "\\PC*|[\\x00-\\x7f@: ;=\\\\]{0,64}") {
        let _ = Line::parse(&s);
        let _ = server_time_millis(&s);
        let _ = unescape_tag_value(&s);
    }

    #[test]
    fn write_then_parse_round_trips(l in line()) {
        let wire = l.to_wire().unwrap();
        prop_assert_eq!(Line::parse(&wire).unwrap(), l);
    }

    #[test]
    fn tag_escaping_round_trips(v in "[^\0]{0,30}") {
        prop_assert_eq!(unescape_tag_value(&escape_tag_value(&v)), v);
    }

    #[test]
    fn split_is_lossless_and_bounded(s in "\\PC{0,200}", budget in 4usize..40) {
        let parts = split_utf8(&s, budget);
        prop_assert_eq!(parts.concat(), s.as_str());
        for p in parts {
            prop_assert!(!p.is_empty() && p.len() <= budget);
        }
    }

    #[test]
    fn privmsg_lines_fit_and_carry_no_injection(text in "\\PC{0,600}|[a\r\n\0 ]{0,40}") {
        if let Ok(lines) = privmsg_lines("#vix", &text, false, MAX_LINE) {
            assert_clean(&lines);
            for l in &lines {
                let parsed = Line::parse(l).unwrap();
                prop_assert_eq!(parsed.command.as_str(), "PRIVMSG");
                prop_assert_eq!(parsed.params.len(), 2);
            }
        }
    }

    #[test]
    fn client_never_panics_and_sends_clean_lines(input in proptest::collection::vec("\\PC{0,80}|[A-Z0-9 :@!*+]{0,40}", 0..12)) {
        let mut c = registered();
        for l in &input {
            assert_clean(&c.handle_line(l).send);
        }
    }

    #[test]
    fn commands_cannot_inject(target in "[#a-z\r\n\0 ,:]{0,12}", text in "[a-zA-Z\r\n\0 :]{0,60}") {
        let mut c = registered();
        for cmd in [
            Command::Privmsg { target: target.clone(), text: text.clone() },
            Command::Action { target: target.clone(), text: text.clone() },
            Command::Join(target.clone()),
            Command::Part { channel: target.clone(), reason: Some(text.clone()) },
            Command::Topic { channel: target.clone(), topic: Some(text) },
            Command::Nick(target),
        ] {
            let step = c.command(cmd);
            assert_clean(&step.send);
            for l in &step.send {
                let parsed = Line::parse(l).unwrap();
                prop_assert!(["PRIVMSG", "JOIN", "PART", "TOPIC", "NICK"].contains(&parsed.command.as_str()));
            }
        }
    }
}
