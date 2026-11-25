//! Reading an acknowledgement (`MSA`, optional `ERR`) out of a reply.

use crate::message::Message;
use crate::path::Path;

/// The acknowledgement code in `MSA-1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AckCode {
    /// `AA` -- application accept.
    ApplicationAccept,
    /// `AE` -- application error.
    ApplicationError,
    /// `AR` -- application reject.
    ApplicationReject,
    /// `CA` -- commit accept (enhanced mode).
    CommitAccept,
    /// `CE` -- commit error (enhanced mode).
    CommitError,
    /// `CR` -- commit reject (enhanced mode).
    CommitReject,
    /// Any other (or missing) code.
    Unknown,
}

impl AckCode {
    /// Read a code from its two-letter text.
    #[must_use]
    pub fn from_code(code: &str) -> AckCode {
        match code.trim() {
            "AA" => AckCode::ApplicationAccept,
            "AE" => AckCode::ApplicationError,
            "AR" => AckCode::ApplicationReject,
            "CA" => AckCode::CommitAccept,
            "CE" => AckCode::CommitError,
            "CR" => AckCode::CommitReject,
            _ => AckCode::Unknown,
        }
    }

    /// Whether this is a positive acknowledgement (`AA` or `CA`).
    #[must_use]
    pub fn is_accept(self) -> bool {
        matches!(self, AckCode::ApplicationAccept | AckCode::CommitAccept)
    }

    /// Whether this is only a commit-stage code (`CA`, `CE`, `CR`): the
    /// receiver took custody of the message, which says nothing about whether
    /// the application processed it.
    #[must_use]
    pub fn is_commit_stage(self) -> bool {
        matches!(
            self,
            AckCode::CommitAccept | AckCode::CommitError | AckCode::CommitReject
        )
    }

    /// Whether this is a negative acknowledgement (a NAK): an error or reject.
    #[must_use]
    pub fn is_nak(self) -> bool {
        matches!(
            self,
            AckCode::ApplicationError
                | AckCode::ApplicationReject
                | AckCode::CommitError
                | AckCode::CommitReject
        )
    }
}

/// The interesting parts of an acknowledgement message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ack {
    /// The parsed code.
    pub code: AckCode,
    /// The code as sent (`MSA-1`), for display.
    pub code_text: String,
    /// The control id of the message being acknowledged (`MSA-2`).
    pub control_id: String,
    /// The human-readable text (`MSA-3`), if any. Free text from the remote
    /// system, so callers must never log it.
    pub text: String,
    /// The `ERR-3` code (or `ERR-1` in v2.3 and earlier) of the first `ERR`
    /// segment, if any.
    pub error_code: Option<String>,
}

/// The control id (`MSH-10`) of an outbound message, or `None` when absent.
#[must_use]
pub fn control_id_of(msg: &Message) -> Option<String> {
    Path::parse("MSH-10")
        .ok()
        .and_then(|p| msg.get(&p))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

impl Ack {
    /// Whether this acknowledgement answers the message whose `MSH-10` is
    /// `sent_id`. A missing or differing `MSA-2` never matches.
    #[must_use]
    pub fn acknowledges(&self, sent_id: &str) -> bool {
        !sent_id.is_empty() && self.control_id.trim() == sent_id
    }

    /// Read the acknowledgement from a reply. `None` when the reply has no
    /// `MSA` segment (so it is not an acknowledgement at all).
    #[must_use]
    pub fn from_message(msg: &Message) -> Option<Ack> {
        msg.segment("MSA")?;
        // A field that is absent (or a path that does not parse) reads as empty text.
        let get = |p: &str| -> String {
            let Ok(path) = Path::parse(p) else {
                return String::new();
            };
            let Some(value) = msg.get(&path) else {
                return String::new();
            };
            value
        };
        let code_text = get("MSA-1");
        let error_code = msg.segment("ERR").map(|_| {
            let v = get("ERR-3.1");
            if v.is_empty() { get("ERR-1") } else { v }
        });
        Some(Ack {
            code: AckCode::from_code(&code_text),
            code_text,
            control_id: get("MSA-2"),
            text: get("MSA-3"),
            error_code: error_code.filter(|c| !c.is_empty()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_accept() {
        let m = Message::parse("MSH|^~\\&|R|F|S|F|2024||ACK|1|P|2.5\rMSA|AA|MSG0001");
        let a = Ack::from_message(&m).unwrap();
        assert_eq!(a.code, AckCode::ApplicationAccept);
        assert!(a.code.is_accept() && !a.code.is_nak());
        assert_eq!(a.control_id, "MSG0001");
    }

    #[test]
    fn reads_a_nak_with_error_segment() {
        let m = Message::parse(
            "MSH|^~\\&|R\rMSA|AE|7|bad thing\rERR||PID^1|101^Required field missing^HL70357",
        );
        let a = Ack::from_message(&m).unwrap();
        assert!(a.code.is_nak());
        assert_eq!(a.text, "bad thing");
        assert_eq!(a.error_code.as_deref(), Some("101"));
    }

    #[test]
    fn matches_the_sent_control_id() {
        let sent = Message::parse("MSH|^~\\&|S|F|R|F|2024||ADT^A01|MSG0001|P|2.5\rPID|1");
        assert_eq!(control_id_of(&sent).as_deref(), Some("MSG0001"));
        let ack = |id: &str| {
            Ack::from_message(&Message::parse(&format!("MSH|^~\\&|R\rMSA|AA|{id}"))).unwrap()
        };
        assert!(ack("MSG0001").acknowledges("MSG0001"));
        assert!(!ack("OTHER").acknowledges("MSG0001"));
        assert!(!ack("").acknowledges("MSG0001"));
        assert!(!ack("").acknowledges(""));
        let none = Message::parse("MSH|^~\\&|S\rPID|1");
        assert_eq!(control_id_of(&none), None);
    }

    #[test]
    fn commit_codes_are_not_application_outcomes() {
        assert!(AckCode::CommitAccept.is_commit_stage());
        assert!(!AckCode::ApplicationAccept.is_commit_stage());
    }

    #[test]
    fn a_non_ack_is_none() {
        assert!(Ack::from_message(&Message::parse("MSH|^~\\&|R\rPID|1")).is_none());
        assert_eq!(AckCode::from_code("??"), AckCode::Unknown);
    }

    #[test]
    fn every_code_reads_back_and_classifies() {
        use AckCode::*;
        for (text, code) in [
            ("AA", ApplicationAccept),
            ("AE", ApplicationError),
            ("AR", ApplicationReject),
            ("CA", CommitAccept),
            ("CE", CommitError),
            ("CR", CommitReject),
            (" AA ", ApplicationAccept),
            ("ZZ", Unknown),
        ] {
            assert_eq!(AckCode::from_code(text), code, "{text:?}");
        }
        assert!(CommitAccept.is_accept() && !CommitAccept.is_nak());
        assert!(CommitError.is_nak() && CommitReject.is_nak());
        assert!(!Unknown.is_accept() && !Unknown.is_nak());
    }

    #[test]
    fn a_sparse_msa_reads_missing_fields_as_empty_and_err_falls_back_to_err1() {
        let msg = Message::parse("MSH|^~\\&|APP\rMSA|AE\rERR|207");
        let ack = Ack::from_message(&msg).expect("has MSA");
        assert_eq!(ack.code, AckCode::ApplicationError);
        assert_eq!(ack.control_id, "");
        assert_eq!(ack.text, "");
        assert_eq!(ack.error_code.as_deref(), Some("207"));
        assert_eq!(Ack::from_message(&Message::parse("MSH|^~\\&|APP")), None);
    }
}
