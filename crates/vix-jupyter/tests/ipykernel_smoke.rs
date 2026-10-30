//! Optional smoke test against a real ipykernel. Skipped unless
//! `VIX_JUPYTER_PYTHON` names a Python that has `ipykernel` installed, for
//! example a throwaway virtual environment:
//!
//! ```sh
//! python3 -m venv /tmp/vix-ipykernel && /tmp/vix-ipykernel/bin/pip install ipykernel
//! VIX_JUPYTER_PYTHON=/tmp/vix-ipykernel/bin/python cargo test -p vix-jupyter --test ipykernel_smoke
//! ```
//!
//! CI never sets it; the fake kernel (`tests/fake_kernel.rs`) covers the
//! protocol without Python.

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::json;
use vix_jupyter::core::message::ReplyStatus;
use vix_jupyter::core::{KernelSpec, Output, Payload, StreamName};
use vix_jupyter::{DeathReason, KernelEvent, KernelHandle, LaunchOptions, RequestId};

fn until(handle: &KernelHandle, mut done: impl FnMut(&KernelEvent) -> bool) -> Vec<KernelEvent> {
    let end = Instant::now() + Duration::from_secs(60);
    let mut seen = Vec::new();
    loop {
        let left = end.saturating_duration_since(Instant::now());
        let event = handle
            .events()
            .recv_timeout(left)
            .unwrap_or_else(|_| panic!("timed out; saw {seen:#?}"));
        let stop = done(&event);
        seen.push(event);
        if stop {
            return seen;
        }
    }
}

fn run(handle: &KernelHandle, code: &str) -> (Vec<Output>, ReplyStatus) {
    let id: RequestId = handle.execute(code);
    let events = until(
        handle,
        |e| matches!(e, KernelEvent::Reply { request, .. } if *request == id),
    );
    let mut outputs = Vec::new();
    let mut status = ReplyStatus::Unknown;
    for e in events {
        match e {
            KernelEvent::Output { request, output } if request == id => outputs.push(output),
            KernelEvent::Reply { status: s, .. } => status = s,
            KernelEvent::Rejected(r) => panic!("a real kernel's message was rejected: {r}"),
            _ => {}
        }
    }
    (outputs, status)
}

#[test]
fn real_ipykernel_round_trip() {
    let Some(python) = std::env::var_os("VIX_JUPYTER_PYTHON") else {
        eprintln!("skipped: set VIX_JUPYTER_PYTHON to a Python with ipykernel");
        return;
    };
    let python = python.to_string_lossy().into_owned();
    let json = json!({
        "argv": [python, "-m", "ipykernel_launcher", "-f", "{connection_file}"],
        "display_name": "Python 3 (ipykernel)",
        "language": "python",
    })
    .to_string();
    let spec = KernelSpec::parse("python3", Path::new("/nonexistent"), &json).unwrap();
    let started = Instant::now();
    let handle = KernelHandle::launch(&spec, &LaunchOptions::default()).unwrap();
    let ready = until(&handle, |e| matches!(e, KernelEvent::Ready(_)));
    let Some(KernelEvent::Ready(info)) = ready.last() else {
        unreachable!()
    };
    eprintln!(
        "ready after {:?}: {} {}",
        started.elapsed(),
        info.implementation,
        info.language_info.version
    );
    assert_eq!(info.language_info.name, "python");

    let (out, status) = run(&handle, "print('hi')");
    assert_eq!(status, ReplyStatus::Ok);
    assert_eq!(
        out,
        [Output::Stream {
            name: StreamName::Stdout,
            text: "hi\n".into()
        }]
    );

    let (out, _) = run(
        &handle,
        "total = 0\nfor i in range(4):\n    total += i\ntotal",
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].plain_text(), "6");

    let (out, status) = run(&handle, "1/0");
    assert_eq!(status, ReplyStatus::Error);
    assert!(
        out[0]
            .plain_text()
            .starts_with("ZeroDivisionError: division by zero")
    );
    assert!(!out[0].plain_text().contains('\u{1b}'));

    let (out, _) = run(&handle, "import sys; print('err', file=sys.stderr)");
    assert_eq!(
        out,
        [Output::Stream {
            name: StreamName::Stderr,
            text: "err\n".into()
        }]
    );

    let png = "from IPython.display import Image, display\nimport base64\ndisplay(Image(base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgYGBgAAAABQABpfZFQAAAAABJRU5ErkJggg==')))";
    let (out, _) = run(&handle, png);
    let Output::Display { data, .. } = &out[0] else {
        panic!("{out:?}")
    };
    let choice = data.choose().unwrap();
    assert_eq!(choice.mime, "image/png");
    assert!(matches!(choice.payload, Payload::Base64(ref b) if b.starts_with("iVBORw0KGgo")));

    let (_, status) = run(&handle, "input()");
    assert_eq!(
        status,
        ReplyStatus::Error,
        "allow_stdin is false, so input() raises"
    );

    // Interrupt a long sleep (SIGINT to the launched process).
    let id = handle.execute("import time; time.sleep(30)");
    until(&handle, |e| {
        matches!(e, KernelEvent::State(vix_jupyter::KernelState::Busy))
    });
    std::thread::sleep(Duration::from_millis(300));
    let t = Instant::now();
    handle.interrupt();
    let events = until(
        &handle,
        |e| matches!(e, KernelEvent::Reply { request, .. } if *request == id),
    );
    assert!(t.elapsed() < Duration::from_secs(10));
    assert!(matches!(
        events.last(),
        Some(KernelEvent::Reply {
            status: ReplyStatus::Error,
            ..
        })
    ));

    // Restart clears state.
    run(&handle, "x = 41");
    handle.restart();
    until(&handle, |e| *e == KernelEvent::Restarted);
    until(&handle, |e| matches!(e, KernelEvent::Ready(_)));
    let (out, status) = run(&handle, "x");
    assert_eq!(status, ReplyStatus::Error);
    assert!(out[0].plain_text().starts_with("NameError"));

    handle.shutdown();
    until(&handle, |e| *e == KernelEvent::Dead(DeathReason::Shutdown));
}
