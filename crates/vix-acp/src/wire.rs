//! Newline-delimited JSON framing and JSON-RPC 2.0 message shapes.
//!
//! This is the bottom layer of the ACP client: it turns raw agent stdout bytes
//! into [`serde_json::Value`]s ([`LineDecoder`]) and builds the outgoing
//! request/notification/response envelopes. It does no IO and holds no
//! protocol state -- [`crate::client::Client`] sits on top and owns the
//! request-id bookkeeping and session phases.
//!
//! Invariant: every encoded message is exactly one line (compact JSON has no
//! raw newlines; embedded ones are escaped), because ACP frames by `\n`.
//!
//! Owning spec: `crates/vix-acp/spec/index.md`.

use serde_json::{Value, json};

/// The longest line the decoder will buffer (16 MiB).
pub const MAX_LINE: usize = 16 * 1024 * 1024;

/// Serialize `value` as one line: compact JSON plus `\n`.
///
/// Serialization of a [`Value`] cannot realistically fail; on the impossible
/// error an empty body (just the newline) is emitted rather than panicking, and
/// the peer's decoder skips it as a blank line.
///
/// # Examples
///
/// ```
/// let bytes = vix_acp::wire::encode_line(&serde_json::json!({"a": 1}));
/// assert_eq!(bytes, b"{\"a\":1}\n");
/// ```
#[must_use]
pub fn encode_line(value: &Value) -> Vec<u8> {
    let mut out = serde_json::to_vec(value).unwrap_or_default();
    out.push(b'\n');
    out
}

/// Incremental decoder for the agent's stdout.
///
/// Feed arbitrary byte chunks with [`push`](Self::push) (a chunk may end in
/// the middle of a line) and drain whole messages with [`pop`](Self::pop).
/// Memory is bounded by [`MAX_LINE`] so a misbehaving agent cannot exhaust it.
#[derive(Default)]
pub struct LineDecoder {
    /// Bytes received but not yet consumed by [`LineDecoder::pop`].
    buf: Vec<u8>,
    /// Discarding bytes up to the next newline after an oversized line.
    skipping: bool,
    /// Count of lines dropped as oversized or not valid JSON.
    malformed: usize,
}

impl LineDecoder {
    /// A new, empty decoder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Append freshly-read bytes.
    ///
    /// A line longer than [`MAX_LINE`] is discarded whole (counted as
    /// malformed) and the decoder resynchronises at the next newline, so one
    /// oversized message never poisons the rest of the stream.
    pub fn push(&mut self, bytes: &[u8]) {
        for &b in bytes {
            if b == b'\n' {
                // The newline ends an oversized line too (clearing `skipping`),
                // but the buffer was already emptied, so a lone `\n` is left
                // behind that `pop` treats as a blank line.
                self.buf.push(b);
                self.skipping = false;
            } else if self.skipping {
                // Dropped: we are inside an oversized line.
            } else if self.buf.len() >= MAX_LINE {
                self.buf.clear();
                self.skipping = true;
                self.malformed += 1;
            } else {
                self.buf.push(b);
            }
        }
    }

    /// Pop the next complete JSON line. Blank and unparsable lines are skipped
    /// (the latter counted in [`Self::malformed`]).
    pub fn pop(&mut self) -> Option<Value> {
        loop {
            let end = self.buf.iter().position(|&b| b == b'\n')?;
            let line: Vec<u8> = self.buf.drain(..=end).collect();
            // Strip the trailing newline; `end` is its index so `len >= 1`.
            let text = &line[..line.len() - 1];
            if text.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            match serde_json::from_slice(text) {
                Ok(v) => return Some(v),
                Err(_) => self.malformed += 1,
            }
        }
    }

    /// How many lines were skipped as not JSON or too long.
    #[must_use]
    pub fn malformed(&self) -> usize {
        self.malformed
    }
}

/// A JSON-RPC request (expects a response carrying the same `id`).
#[must_use]
pub fn request(id: u64, method: &str, params: &Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

/// A JSON-RPC notification (no `id`, so no response is expected).
#[must_use]
pub fn notification(method: &str, params: &Value) -> Value {
    json!({"jsonrpc": "2.0", "method": method, "params": params})
}

/// A successful JSON-RPC response to the agent's request `id`.
#[must_use]
pub fn response(id: &Value, result: &Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

/// An error JSON-RPC response to the agent's request `id`.
#[must_use]
pub fn error_response(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// A JSON-RPC error object.
#[derive(Clone, Debug, PartialEq)]
pub struct RpcError {
    /// The error code.
    pub code: i64,
    /// The human-readable message.
    pub message: String,
}

/// A message from the agent.
#[derive(Clone, Debug, PartialEq)]
pub enum Incoming {
    /// A response to one of our requests.
    Response {
        /// The id of our request.
        id: u64,
        /// Its result or error.
        result: Result<Value, RpcError>,
    },
    /// A request from the agent, which expects a response with the same id.
    Request {
        /// The agent's id (any JSON scalar), echoed in the response.
        id: Value,
        /// The method name.
        method: String,
        /// The parameters (`null` when absent).
        params: Value,
    },
    /// A notification from the agent.
    Notification {
        /// The method name.
        method: String,
        /// The parameters (`null` when absent).
        params: Value,
    },
}

/// Classify a decoded JSON value, or `None` if it is not a JSON-RPC message.
#[must_use]
pub fn parse_incoming(value: &Value) -> Option<Incoming> {
    let obj = value.as_object()?;
    let method = obj.get("method").and_then(Value::as_str);
    // A JSON `null` id is treated as absent, as JSON-RPC allows agents that
    // send `"id": null` on notifications.
    let id = obj.get("id").filter(|v| !v.is_null());
    let params = obj.get("params").cloned().unwrap_or(Value::Null);
    match (method, id) {
        (Some(m), Some(id)) => Some(Incoming::Request {
            id: id.clone(),
            method: m.to_string(),
            params,
        }),
        (Some(m), None) => Some(Incoming::Notification {
            method: m.to_string(),
            params,
        }),
        (None, Some(id)) => {
            // Our request ids are always `u64`; a response with any other id
            // type cannot answer one of ours and is dropped (`None`).
            let id = id.as_u64()?;
            let result = if let Some(e) = obj.get("error") {
                Err(RpcError {
                    // -32000 is the JSON-RPC "server error" default for a
                    // malformed error object.
                    code: e.get("code").and_then(Value::as_i64).unwrap_or(-32000),
                    message: e
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                })
            } else {
                Ok(obj.get("result").cloned().unwrap_or(Value::Null))
            };
            Some(Incoming::Response { id, result })
        }
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_is_one_line() {
        let bytes = encode_line(&json!({"a": "x\ny"}));
        assert_eq!(bytes.split(|&b| b == b'\n').count(), 2);
        assert_eq!(*bytes.last().unwrap(), b'\n');
    }

    #[test]
    fn decoder_handles_partial_blank_and_garbage_lines() {
        let mut d = LineDecoder::new();
        d.push(b"{\"a\":1}\n\nnot json\n{\"b\"");
        assert_eq!(d.pop(), Some(json!({"a": 1})));
        assert_eq!(d.pop(), None);
        assert_eq!(d.malformed(), 1);
        d.push(b":2}\n");
        assert_eq!(d.pop(), Some(json!({"b": 2})));
        assert_eq!(d.pop(), None);
    }

    #[test]
    fn oversized_line_is_dropped_and_stream_resyncs() {
        let mut d = LineDecoder::new();
        d.push(&vec![b'x'; MAX_LINE + 10]);
        d.push(b"more\n{\"ok\":true}\n");
        assert_eq!(d.pop(), Some(json!({"ok": true})));
        assert_eq!(d.malformed(), 1);
    }

    #[test]
    fn classifies_messages() {
        let r = parse_incoming(&json!({"jsonrpc":"2.0","id":3,"result":{"x":1}}));
        assert_eq!(
            r,
            Some(Incoming::Response {
                id: 3,
                result: Ok(json!({"x": 1}))
            })
        );
        let e = parse_incoming(&json!({"id":4,"error":{"code":-1,"message":"no"}}));
        assert_eq!(
            e,
            Some(Incoming::Response {
                id: 4,
                result: Err(RpcError {
                    code: -1,
                    message: "no".into()
                })
            })
        );
        assert!(matches!(
            parse_incoming(&json!({"id":"a","method":"m"})),
            Some(Incoming::Request { .. })
        ));
        assert!(matches!(
            parse_incoming(&json!({"method":"n","params":{}})),
            Some(Incoming::Notification { .. })
        ));
        assert_eq!(parse_incoming(&json!({"hello": 1})), None);
        assert_eq!(parse_incoming(&json!([1])), None);
    }
}
