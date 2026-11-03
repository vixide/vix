//! Message headers and the typed contents of the message types Vix uses.
//!
//! Only the subset in `spec/jupyter` is typed. Every other `msg_type`
//! parses as [`Content::Other`], so a newer kernel never breaks the client.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::output::{MimeBundle, StreamName};

/// The messaging protocol version Vix speaks.
pub const PROTOCOL_VERSION: &str = "5.3";

/// A message header (also the shape of `parent_header`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    /// Unique id of this message.
    pub msg_id: String,
    /// The sending session's id.
    #[serde(default)]
    pub session: String,
    /// The sending user's name.
    #[serde(default)]
    pub username: String,
    /// ISO 8601 timestamp; some kernels leave it out.
    #[serde(default)]
    pub date: String,
    /// The message type, such as `execute_request`.
    pub msg_type: String,
    /// The protocol version.
    #[serde(default)]
    pub version: String,
}

/// One whole message, minus its signature.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// ZeroMQ routing identities before the delimiter (empty on DEALER/SUB).
    pub identities: Vec<Vec<u8>>,
    /// This message's header.
    pub header: Header,
    /// The header of the request this answers, if any.
    pub parent_header: Option<Header>,
    /// Free-form metadata.
    pub metadata: Map<String, Value>,
    /// The content, typed by [`Content::parse`].
    pub content: Value,
    /// Extra binary buffers after the content.
    pub buffers: Vec<Vec<u8>>,
}

impl Message {
    /// A new message with no identities, parent, metadata or buffers.
    #[must_use]
    pub fn new(
        msg_type: &str,
        content: Value,
        msg_id: &str,
        session: &str,
        username: &str,
        date: &str,
    ) -> Self {
        Self {
            identities: Vec::new(),
            header: Header {
                msg_id: msg_id.to_owned(),
                session: session.to_owned(),
                username: username.to_owned(),
                date: date.to_owned(),
                msg_type: msg_type.to_owned(),
                version: PROTOCOL_VERSION.to_owned(),
            },
            parent_header: None,
            metadata: Map::new(),
            content,
            buffers: Vec::new(),
        }
    }

    /// The `msg_id` of the request this message answers, if any.
    #[must_use]
    pub fn parent_id(&self) -> Option<&str> {
        self.parent_header.as_ref().map(|h| h.msg_id.as_str())
    }

    /// This message's content, typed.
    ///
    /// # Errors
    ///
    /// When the content of a known type does not have that type's shape.
    pub fn typed(&self) -> Result<Content, serde_json::Error> {
        Content::parse(&self.header.msg_type, &self.content)
    }
}

/// Request contents Vix sends.
pub mod request {
    use super::{Value, json};

    /// `execute_request`: run `code`, never reading stdin (v1 has no stdin
    /// channel, so `input()` raises instead of hanging).
    #[must_use]
    pub fn execute(code: &str) -> Value {
        json!({
            "code": code,
            "silent": false,
            "store_history": true,
            "user_expressions": {},
            "allow_stdin": false,
            "stop_on_error": true,
        })
    }

    /// `kernel_info_request`, `interrupt_request`: an empty content.
    #[must_use]
    pub fn empty() -> Value {
        json!({})
    }

    /// `shutdown_request`.
    #[must_use]
    pub fn shutdown(restart: bool) -> Value {
        json!({ "restart": restart })
    }
}

/// A reply's status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReplyStatus {
    /// Succeeded.
    Ok,
    /// Failed; the content carries `ename`/`evalue`/`traceback`.
    Error,
    /// Not run because an earlier cell failed.
    Aborted,
    /// Anything else.
    #[serde(other)]
    Unknown,
}

/// The kernel's execution state from an IOPub `status` message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionState {
    /// Working on a request.
    Busy,
    /// Waiting for a request.
    Idle,
    /// Starting up (sent once).
    Starting,
    /// Anything else.
    #[serde(other)]
    Unknown,
}

/// `language_info` in a `kernel_info_reply`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LanguageInfo {
    /// Language name, such as `python`.
    #[serde(default)]
    pub name: String,
    /// Language version.
    #[serde(default)]
    pub version: String,
    /// MIME type for source files.
    #[serde(default)]
    pub mimetype: String,
    /// File extension, with the dot.
    #[serde(default)]
    pub file_extension: String,
}

/// `kernel_info_reply`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelInfoReply {
    /// Reply status.
    #[serde(default = "ok")]
    pub status: ReplyStatus,
    /// The protocol version the kernel speaks.
    #[serde(default)]
    pub protocol_version: String,
    /// Kernel implementation name, such as `ipython`.
    #[serde(default)]
    pub implementation: String,
    /// Kernel implementation version.
    #[serde(default)]
    pub implementation_version: String,
    /// The kernel's language.
    #[serde(default)]
    pub language_info: LanguageInfo,
    /// Startup banner.
    #[serde(default)]
    pub banner: String,
}

/// Serde default for `status` fields: a reply that omits the field is taken as successful.
fn ok() -> ReplyStatus {
    ReplyStatus::Ok
}

/// `execute_reply` (on shell).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecuteReply {
    /// Reply status.
    pub status: ReplyStatus,
    /// The cell's execution count.
    #[serde(default)]
    pub execution_count: Option<u64>,
    /// Exception name, on error.
    #[serde(default)]
    pub ename: Option<String>,
    /// Exception value, on error.
    #[serde(default)]
    pub evalue: Option<String>,
}

/// `execute_input` (on IOPub): the code being run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecuteInput {
    /// The code.
    #[serde(default)]
    pub code: String,
    /// Its execution count.
    #[serde(default)]
    pub execution_count: Option<u64>,
}

/// `stream`: stdout or stderr text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stream {
    /// Which stream.
    pub name: StreamName,
    /// The text.
    pub text: String,
}

/// `execute_result`: a cell's value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecuteResult {
    /// The cell's execution count.
    #[serde(default)]
    pub execution_count: Option<u64>,
    /// The value in each MIME type the kernel offers.
    pub data: MimeBundle,
}

/// `display_data` and `update_display_data`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayData {
    /// The value in each MIME type the kernel offers.
    pub data: MimeBundle,
    /// `transient`, which may carry a `display_id`.
    #[serde(default)]
    pub transient: Map<String, Value>,
}

impl DisplayData {
    /// The `transient.display_id`, if any.
    #[must_use]
    pub fn display_id(&self) -> Option<&str> {
        self.transient.get("display_id").and_then(Value::as_str)
    }
}

/// `error` (on IOPub).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorContent {
    /// Exception name.
    #[serde(default)]
    pub ename: String,
    /// Exception value.
    #[serde(default)]
    pub evalue: String,
    /// Traceback lines, usually with ANSI colors.
    #[serde(default)]
    pub traceback: Vec<String>,
}

/// `status` (on IOPub).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    /// The new state.
    pub execution_state: ExecutionState,
}

/// `interrupt_reply`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterruptReply {
    /// Reply status.
    #[serde(default = "ok")]
    pub status: ReplyStatus,
}

/// `shutdown_reply`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShutdownReply {
    /// Reply status.
    #[serde(default = "ok")]
    pub status: ReplyStatus,
    /// Whether this was a restart.
    #[serde(default)]
    pub restart: bool,
}

/// A received message's content, typed by `msg_type`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Content {
    /// `kernel_info_reply`.
    KernelInfoReply(KernelInfoReply),
    /// `execute_reply`.
    ExecuteReply(ExecuteReply),
    /// `execute_input`.
    ExecuteInput(ExecuteInput),
    /// `stream`.
    Stream(Stream),
    /// `execute_result`.
    ExecuteResult(ExecuteResult),
    /// `display_data`.
    DisplayData(DisplayData),
    /// `update_display_data`.
    UpdateDisplayData(DisplayData),
    /// `error`.
    Error(ErrorContent),
    /// `status`.
    Status(Status),
    /// `interrupt_reply`.
    InterruptReply(InterruptReply),
    /// `shutdown_reply`.
    ShutdownReply(ShutdownReply),
    /// Any other message type (comms, `clear_output`, …), ignored in v1.
    Other,
}

impl Content {
    /// Type `content` according to `msg_type`.
    ///
    /// # Errors
    ///
    /// When a known type's content does not have that type's shape.
    pub fn parse(msg_type: &str, content: &Value) -> Result<Self, serde_json::Error> {
        /// Deserialize `content` into the concrete type for each known message type; unknown types become [`Content::Other`] so a newer kernel's extra messages do not break the client.
        fn de<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T, serde_json::Error> {
            T::deserialize(v)
        }
        Ok(match msg_type {
            "kernel_info_reply" => Self::KernelInfoReply(de(content)?),
            "execute_reply" => Self::ExecuteReply(de(content)?),
            "execute_input" => Self::ExecuteInput(de(content)?),
            "stream" => Self::Stream(de(content)?),
            "execute_result" => Self::ExecuteResult(de(content)?),
            "display_data" => Self::DisplayData(de(content)?),
            "update_display_data" => Self::UpdateDisplayData(de(content)?),
            "error" => Self::Error(de(content)?),
            "status" => Self::Status(de(content)?),
            "interrupt_reply" => Self::InterruptReply(de(content)?),
            "shutdown_reply" => Self::ShutdownReply(de(content)?),
            _ => Self::Other,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execute_request_never_allows_stdin() {
        let v = request::execute("print(1)");
        assert_eq!(v["allow_stdin"], json!(false));
        assert_eq!(v["code"], json!("print(1)"));
    }

    #[test]
    fn unknown_types_and_states_are_tolerated() {
        assert_eq!(
            Content::parse("comm_open", &json!({"x": 1})).ok(),
            Some(Content::Other)
        );
        let c = Content::parse("status", &json!({"execution_state": "dreaming"})).ok();
        assert_eq!(
            c,
            Some(Content::Status(Status {
                execution_state: ExecutionState::Unknown
            }))
        );
    }

    #[test]
    fn wrong_shape_is_an_error() {
        assert!(Content::parse("stream", &json!({"name": 3})).is_err());
    }

    #[test]
    fn execute_reply_error() {
        let c = Content::parse(
            "execute_reply",
            &json!({"status": "error", "execution_count": 4, "ename": "ZeroDivisionError", "evalue": "division by zero", "traceback": []}),
        )
        .ok();
        let Some(Content::ExecuteReply(r)) = c else {
            panic!("not an execute_reply");
        };
        assert_eq!(r.status, ReplyStatus::Error);
        assert_eq!(r.execution_count, Some(4));
        assert_eq!(r.ename.as_deref(), Some("ZeroDivisionError"));
    }
}
