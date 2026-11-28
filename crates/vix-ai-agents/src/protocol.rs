//! One native provider turn over newline-delimited JSON on standard I/O.
//! The host owns the subprocess, display sanitization and explicit approval.
use crate::Provider;
use serde_json::{Value, json};

/// A single provider request awaiting a user decision.
#[derive(Debug)]
pub struct Approval {
    /// The full request to display before allowing an operation.
    pub details: Value,
    reply: ApprovalReply,
}

#[derive(Debug)]
enum ApprovalReply {
    Codex(Value),
    Claude { id: Value, input: Value },
}

impl Approval {
    /// Answer only this request. Claude requires the unchanged input on allow.
    #[must_use]
    pub fn answer(self, allow: bool) -> Value {
        match self.reply {
            ApprovalReply::Codex(id) => {
                json!({"id":id,"result":{"decision": if allow {"accept"} else {"decline"}}})
            }
            ApprovalReply::Claude { id, input } => {
                let decision = if allow {
                    json!({"behavior":"allow","updatedInput":input})
                } else {
                    json!({"behavior":"deny","message":"User declined"})
                };
                json!({"type":"control_response","response":{"subtype":"success","request_id":id,"response":decision}})
            }
        }
    }
}

/// The host's next step after a provider message.
#[derive(Debug)]
pub enum Event {
    /// Write a protocol message to the provider.
    Send(Value),
    /// Persist the provider-specific resume ID.
    Session(String),
    /// A completed block of assistant text.
    Text(String),
    /// A progress description for a tool operation.
    Tool(String),
    /// Show a decision that belongs to this turn.
    Ask(Approval),
    /// The provider finished the turn.
    Done,
    /// The provider refused or failed the turn.
    Failed(String),
}

/// State for one prompt. Each turn gets a fresh subprocess and resumes by ID.
pub struct Turn {
    provider: Provider,
    cwd: String,
    session: Option<String>,
    prompt: String,
}

impl Turn {
    /// Start a new turn, optionally resuming this provider's saved session.
    #[must_use]
    pub fn new(provider: Provider, cwd: String, session: Option<String>, prompt: String) -> Self {
        Self {
            provider,
            cwd,
            session,
            prompt,
        }
    }

    /// The installed program and arguments, passed directly to the OS.
    #[must_use]
    pub fn argv(&self) -> Vec<String> {
        let mut args = vec![self.provider.executable().to_owned()];
        match self.provider {
            Provider::Codex => args.push("app-server".into()),
            Provider::Claude => {
                args.extend(
                    [
                        "-p",
                        "--input-format",
                        "stream-json",
                        "--output-format",
                        "stream-json",
                        "--verbose",
                        "--permission-mode",
                        "default",
                        "--permission-prompt-tool",
                        "stdio",
                    ]
                    .map(str::to_owned),
                );
                if let Some(id) = &self.session {
                    args.extend(["--resume".into(), id.clone()]);
                }
            }
        }
        args
    }

    /// The first message sent to the subprocess.
    #[must_use]
    pub fn initialize(&self) -> Value {
        match self.provider {
            Provider::Codex => {
                json!({"method":"initialize","id":1,"params":{"clientInfo":{"name":"vix","title":"Vix","version":env!("CARGO_PKG_VERSION")}}})
            }
            Provider::Claude => {
                json!({"type":"control_request","request_id":"init","request":{"subtype":"initialize","hooks":{}}})
            }
        }
    }

    /// Interpret an incoming message from this turn's subprocess.
    #[must_use]
    pub fn receive(&mut self, msg: &Value) -> Vec<Event> {
        match self.provider {
            Provider::Codex => self.codex(msg),
            Provider::Claude => self.claude(msg),
        }
    }

    fn codex(&mut self, msg: &Value) -> Vec<Event> {
        if let Some(error) = msg.get("error") {
            return vec![Event::Failed(error.to_string())];
        }
        if let Some(result) = msg.get("result") {
            return match msg.get("id").and_then(Value::as_u64) {
                Some(1) => {
                    let mut params = serde_json::Map::from_iter([
                        ("cwd".into(), json!(self.cwd)),
                        ("sandbox".into(), json!("workspace-write")),
                        ("approvalPolicy".into(), json!("on-request")),
                    ]);
                    let method = if let Some(id) = &self.session {
                        params.insert("threadId".into(), json!(id));
                        "thread/resume"
                    } else {
                        "thread/start"
                    };
                    vec![
                        Event::Send(json!({"method":"initialized"})),
                        Event::Send(json!({"method":method,"id":2,"params":params})),
                    ]
                }
                Some(2) => {
                    let Some(id) = result.pointer("/thread/id").and_then(Value::as_str) else {
                        return vec![Event::Failed(result.to_string())];
                    };
                    self.session = Some(id.to_owned());
                    vec![
                        Event::Session(id.to_owned()),
                        Event::Send(
                            json!({"method":"turn/start","id":3,"params":{"threadId":id,"input":[{"type":"text","text":self.prompt}]}}),
                        ),
                    ]
                }
                _ => vec![],
            };
        }
        let method = msg
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let params = &msg["params"];
        if let Some(id) = msg.get("id") {
            if matches!(
                method,
                "item/commandExecution/requestApproval" | "item/fileChange/requestApproval"
            ) && self.owns_codex(params)
            {
                return vec![Event::Ask(Approval {
                    details: params.clone(),
                    reply: ApprovalReply::Codex(id.clone()),
                })];
            }
            return vec![Event::Send(
                json!({"id":id,"error":{"code":-32601,"message":"Unsupported request"}}),
            )];
        }
        if !self.owns_codex(params) {
            return vec![];
        }
        match method {
            "item/completed"
                if params.pointer("/item/type").and_then(Value::as_str) == Some("agentMessage") =>
            {
                params
                    .pointer("/item/text")
                    .and_then(Value::as_str)
                    .map(|s| Event::Text(s.to_owned()))
                    .into_iter()
                    .collect()
            }
            "item/started"
                if params.pointer("/item/type").and_then(Value::as_str)
                    == Some("commandExecution") =>
            {
                params
                    .pointer("/item/command")
                    .and_then(Value::as_str)
                    .map(|s| Event::Tool(s.to_owned()))
                    .into_iter()
                    .collect()
            }
            "turn/completed"
                if params.pointer("/turn/status").and_then(Value::as_str) == Some("completed") =>
            {
                vec![Event::Done]
            }
            "turn/completed" => vec![Event::Failed(params["turn"].to_string())],
            _ => vec![],
        }
    }

    fn owns_codex(&self, params: &Value) -> bool {
        self.session
            .as_deref()
            .is_some_and(|id| params.get("threadId").and_then(Value::as_str) == Some(id))
    }

    fn claude(&self, msg: &Value) -> Vec<Event> {
        match msg["type"].as_str() {
            Some("control_response")
                if msg.pointer("/response/request_id").and_then(Value::as_str) == Some("init") =>
            {
                if msg.pointer("/response/subtype").and_then(Value::as_str) != Some("success") {
                    return vec![Event::Failed(msg["response"].to_string())];
                }
                vec![Event::Send(
                    json!({"type":"user","message":{"role":"user","content":self.prompt},"parent_tool_use_id":null,"session_id":self.session.as_deref().unwrap_or_default()}),
                )]
            }
            Some("control_request") => {
                let req = &msg["request"];
                if req["subtype"] == "can_use_tool" {
                    vec![Event::Ask(Approval {
                        details: req.clone(),
                        reply: ApprovalReply::Claude {
                            id: msg["request_id"].clone(),
                            input: req["input"].clone(),
                        },
                    })]
                } else {
                    vec![Event::Send(
                        json!({"type":"control_response","response":{"subtype":"error","request_id":msg["request_id"],"error":"Unsupported request"}}),
                    )]
                }
            }
            Some("system") if msg["subtype"] == "init" => msg["session_id"]
                .as_str()
                .map(|s| Event::Session(s.to_owned()))
                .into_iter()
                .collect(),
            Some("assistant") => msg
                .pointer("/message/content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|block| match block["type"].as_str() {
                    Some("text") => block["text"].as_str().map(|s| Event::Text(s.to_owned())),
                    Some("tool_use") => block["name"].as_str().map(|s| Event::Tool(s.to_owned())),
                    _ => None,
                })
                .collect(),
            Some("result") if msg["subtype"] == "success" && msg["is_error"] != true => {
                vec![Event::Done]
            }
            Some("result") => vec![Event::Failed(
                msg.get("result")
                    .or_else(|| msg.get("errors"))
                    .unwrap_or(&msg["subtype"])
                    .to_string(),
            )],
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_foreign_codex_thread_cannot_request_approval_or_finish_our_turn() {
        let mut turn = Turn::new(
            Provider::Codex,
            ".".into(),
            Some("ours".into()),
            "go".into(),
        );
        let msg = json!({"id":42,"method":"item/commandExecution/requestApproval","params":{"threadId":"other"}});
        assert!(
            matches!(&turn.receive(&msg)[..], [Event::Send(reply)] if reply.get("error").is_some())
        );
        let done = json!({"method":"turn/completed","params":{"threadId":"other","turn":{"status":"completed"}}});
        assert!(turn.receive(&done).is_empty());
    }

    #[test]
    fn claude_unknown_requests_fail_and_a_decline_has_no_updated_input() {
        let mut turn = Turn::new(Provider::Claude, ".".into(), None, "go".into());
        let ask = json!({"type":"control_request","request_id":"r","request":{"subtype":"can_use_tool","tool_name":"Edit","input":{"x":1}}});
        let Event::Ask(approval) = turn.receive(&ask).pop().unwrap() else {
            panic!("expected approval")
        };
        let reply = approval.answer(false);
        assert_eq!(reply["response"]["request_id"], "r");
        assert_eq!(reply["response"]["response"]["behavior"], "deny");
        assert!(reply.pointer("/response/response/updatedInput").is_none());
        let unknown =
            json!({"type":"control_request","request_id":"x","request":{"subtype":"future"}});
        assert!(
            matches!(&turn.receive(&unknown)[..],[Event::Send(reply)] if reply["response"]["subtype"]=="error")
        );
    }
}
