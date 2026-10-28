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

impl Ack {
    /// Read the acknowledgement from a reply. `None` when the reply has no
    /// `MSA` segment (so it is not an acknowledgement at all).
    #[must_use]
    pub fn from_message(msg: &Message) -> Option<Ack> {
        msg.segment("MSA")?;
        let get = |p: &str| {
            Path::parse(p)
                .ok()
                .and_then(|p| msg.get(&p))
                .unwrap_or_default()
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
    fn a_non_ack_is_none() {
        assert!(Ack::from_message(&Message::parse("MSH|^~\\&|R\rPID|1")).is_none());
        assert_eq!(AckCode::from_code("??"), AckCode::Unknown);
    }
}
