//! Drives the real `AgentProcess` and `Client` against the scripted fake agent
//! (`fake_agent.py`) over real pipes. No network.
#![cfg(feature = "process")]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use vix_acp::process::AgentProcess;
use vix_acp::{Client, Event, Phase};

/// The first of `python3` and `python` that runs and is Python 3: Windows
/// usually has only `python` (and its `python3` may be the Store stub).
fn python() -> Option<&'static str> {
    ["python3", "python"].into_iter().find(|p| {
        std::process::Command::new(p)
            .arg("--version")
            .output()
            .is_ok_and(|o| {
                o.status.success()
                    && String::from_utf8_lossy(&[o.stdout, o.stderr].concat()).contains("Python 3")
            })
    })
}

fn python_available() -> bool {
    python().is_some()
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
        let py = python().unwrap_or("python3");
        let argv = vec![py.into(), script(), log.display().to_string()];
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

/// Poll `ready` every 20 ms for up to 20 s (the gate runs under heavy load).
fn eventually(mut ready: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if ready() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    ready()
}

#[test]
fn stderr_keeps_a_bounded_tail_and_an_isolated_agent_takes_its_tools_down() {
    let Some(py) = python() else {
        return;
    };
    let dir = temp("isolated");
    let pid_file = dir.join("tool.pid");
    // 20 KiB of stderr, a marker, a long-lived "tool" child, then a line on
    // stdout; the agent then waits for stdin.
    let code = format!(
        "import subprocess,sys\n\
         sys.stderr.write('x'*20000+'END-OF-STDERR'); sys.stderr.flush()\n\
         p=subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)'])\n\
         open({:?},'w').write(str(p.pid))\n\
         print('{{\"ready\":true}}', flush=True)\n\
         sys.stdin.readline()\n",
        pid_file.display().to_string()
    );
    let argv = vec![py.to_owned(), "-c".to_owned(), code];
    let mut agent = AgentProcess::spawn_isolated(&argv, &dir).unwrap_or_else(|e| panic!("{e}"));
    let mut seen = Vec::new();
    assert!(eventually(|| {
        seen.extend(agent.drain());
        !seen.is_empty() && agent.stderr_tail().ends_with("END-OF-STDERR")
    }));
    assert!(agent.stderr_tail().len() <= vix_acp::process::STDERR_TAIL_BYTES);
    assert_eq!(seen[0]["ready"], true);
    // A queued write still reaches the agent.
    agent
        .send(&serde_json::json!({"go": 1}))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(eventually(|| pid_file.exists()));
    let tool: u32 = std::fs::read_to_string(&pid_file)
        .unwrap_or_else(|e| panic!("{e}"))
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("{e}"));
    drop(agent);
    #[cfg(unix)]
    assert!(
        eventually(|| !std::process::Command::new("kill")
            .args(["-0", &tool.to_string()])
            .status()
            .is_ok_and(|s| s.success())),
        "the tool process outlived its agent"
    );
    #[cfg(not(unix))]
    let _ = tool;
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn closing_stdin_ends_a_reading_agent_and_later_sends_fail() {
    let Some(py) = python() else {
        return;
    };
    let dir = temp("close-stdin");
    let argv = vec![
        py.to_owned(),
        "-c".to_owned(),
        "import sys; sys.stdin.read()".to_owned(),
    ];
    let mut agent = AgentProcess::spawn_isolated(&argv, &dir).unwrap_or_else(|e| panic!("{e}"));
    agent.close_stdin();
    assert!(eventually(|| agent.try_wait().is_ok_and(|s| s.is_some())));
    assert!(agent.send(&serde_json::json!({})).is_err());
    assert!(agent.is_closed());
    let _ = std::fs::remove_dir_all(dir);
}
