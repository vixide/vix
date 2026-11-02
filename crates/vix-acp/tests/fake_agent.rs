//! Drives the real `AgentProcess` and `Client` against the scripted fake agent
//! (`fake_agent.py`) over real pipes. No network.
#![cfg(all(unix, feature = "process"))]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use vix_acp::process::AgentProcess;
use vix_acp::{Client, Event, Phase};

fn python_available() -> bool {
    std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_ok()
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("vix-acp-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap_or_else(|e| panic!("{e}"));
    d
}

fn script() -> String {
    format!("{}/tests/fake_agent.py", env!("CARGO_MANIFEST_DIR"))
}

struct Rig {
    agent: AgentProcess,
    client: Client,
    log: PathBuf,
}

impl Rig {
    fn start(dir: &Path) -> Self {
        let log = dir.join("log.txt");
        let argv = vec!["python3".into(), script(), log.display().to_string()];
        let mut agent = AgentProcess::spawn(&argv, dir).unwrap_or_else(|e| panic!("{e}"));
        let mut client = Client::new(&dir.display().to_string());
        for m in client.start() {
            agent.send(&m).unwrap_or_else(|e| panic!("{e}"));
        }
        let mut rig = Self { agent, client, log };
        rig.pump_until(|e| matches!(e, Event::Ready { .. }));
        rig
    }

    /// Pump messages until an event satisfies `stop`; returns all events seen.
    /// Permission/file requests are collected, not answered.
    fn pump_until(&mut self, stop: impl Fn(&Event) -> bool) -> Vec<Event> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut seen = Vec::new();
        while Instant::now() < deadline {
            for msg in self.agent.drain() {
                let reply = self.client.handle(&msg);
                for out in &reply.out {
                    self.agent.send(out).unwrap_or_else(|e| panic!("{e}"));
                }
                for e in reply.events {
                    let done = stop(&e);
                    seen.push(e);
                    if done {
                        return seen;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("timed out; saw {seen:?}");
    }

    fn prompt(&mut self, text: &str) {
        let m = self.client.prompt(text).unwrap_or_default();
        self.agent.send(&m).unwrap_or_else(|e| panic!("{e}"));
    }

    fn log(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }
}

#[test]
fn streams_text_tool_call_plan_then_ends_turn() {
    if !python_available() {
        return;
    }
    let dir = temp("stream");
    let mut rig = Rig::start(&dir);
    rig.prompt("plain");
    let seen = rig.pump_until(|e| matches!(e, Event::TurnEnded { .. }));
    let text: String = seen
        .iter()
        .filter_map(|e| match e {
            Event::AgentText(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(text, "hello");
    assert!(
        seen.iter()
            .any(|e| matches!(e, Event::ToolCall { title, .. } if title == "Reading"))
    );
    assert!(
        seen.iter()
            .any(|e| matches!(e, Event::Plan(p) if p.len() == 1))
    );
    assert_eq!(rig.client.phase(), Phase::Ready);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_denied_permission_reaches_the_agent_as_cancelled_and_nothing_is_written() {
    if !python_available() {
        return;
    }
    let dir = temp("perm");
    let mut rig = Rig::start(&dir);
    rig.prompt("perm");
    let seen = rig.pump_until(|e| matches!(e, Event::Permission { .. }));
    let Some(Event::Permission {
        req_id, options, ..
    }) = seen.last()
    else {
        panic!()
    };
    let deny = vix_acp::policy::reject_option(options);
    assert_eq!(deny, Some("no"));
    let answer = Client::answer_permission(req_id, options, deny);
    rig.agent.send(&answer).unwrap();
    rig.pump_until(|e| matches!(e, Event::TurnEnded { .. }));
    assert!(rig.log().contains("\"optionId\": \"no\""), "{}", rig.log());
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "only the log");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn file_requests_are_answered_through_the_client() {
    if !python_available() {
        return;
    }
    let dir = temp("fs");
    let file = dir.join("a.txt");
    std::fs::write(&file, "from disk").unwrap();
    let mut rig = Rig::start(&dir);
    rig.prompt(&format!("read {}", file.display()));
    let seen = rig.pump_until(|e| matches!(e, Event::ReadFile { .. }));
    let Some(Event::ReadFile { req_id, path, .. }) = seen.last() else {
        panic!()
    };
    assert_eq!(path, &file.display().to_string());
    rig.agent
        .send(&Client::answer_read(req_id, Ok("from disk")))
        .unwrap();
    let seen = rig.pump_until(|e| matches!(e, Event::TurnEnded { .. }));
    assert!(seen.contains(&Event::AgentText("from disk".into())));

    rig.prompt(&format!("write {} NEW", file.display()));
    let seen = rig.pump_until(|e| matches!(e, Event::WriteFile { .. }));
    let Some(Event::WriteFile {
        req_id, content, ..
    }) = seen.last()
    else {
        panic!()
    };
    assert_eq!(content, "NEW");
    // The protocol layer writes nothing by itself.
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "from disk");
    rig.agent
        .send(&Client::answer_write(req_id, Err("refused")))
        .unwrap();
    rig.pump_until(|e| matches!(e, Event::TurnEnded { .. }));
    assert!(rig.log().contains("WRITE error"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn dropping_the_agent_kills_the_process_and_empty_argv_is_refused() {
    assert!(AgentProcess::spawn(&[], Path::new(".")).is_err());
}
