//! Behavior of the pure Jupyter core through its public API: image
//! validation, connection endpoints, kernelspec search paths, typed
//! messages, the wire codec's error paths, Org results edits, and Babel
//! header parsing and result text.

use base64::Engine as _;
use serde_json::{Value, json};
use vix_jupyter_core::babel::{self, Collect, Eval, Handling, Header};
use vix_jupyter_core::connection::MAX_CONNECTION_FILE_BYTES;
use vix_jupyter_core::image::{self, ImageError, MAX_IMAGE_BYTES, MAX_SIDE};
use vix_jupyter_core::kernelspec::{self, MAX_KERNEL_JSON_BYTES};
use vix_jupyter_core::message::{ExecutionState, ReplyStatus, request};
use vix_jupyter_core::org::{self, ResultValue};
use vix_jupyter_core::wire::{self, DELIMITER};
use vix_jupyter_core::{
    Channel, ConnectionError, ConnectionInfo, Content, DecodeError, KernelSpec, KeyError, Limits,
    Message, MimeBundle, Os, Output, PathEnv, Payload, Signer, SpecError, StreamName, Transport,
};

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

// ----- image --------------------------------------------------------------------

/// A PNG signature and `IHDR` chunk for a `width` x `height` image (no pixel data).
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&[8, 2, 0, 0, 0]);
    bytes
}

/// A JPEG: start of image, `leading` segments, then a baseline frame header.
fn jpeg(leading: &[u8], width: u16, height: u16) -> Vec<u8> {
    let mut bytes = vec![0xff, 0xd8];
    bytes.extend_from_slice(leading);
    bytes.extend_from_slice(&[0xff, 0xc0, 0x00, 0x11, 8]);
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
    bytes
}

#[test]
fn png_and_jpeg_sizes_are_read_and_files_are_named_by_content() {
    let image = image::decode("image/png", &b64(&png(640, 480))).unwrap();
    assert_eq!((image.width, image.height), (640, 480));
    assert_eq!(image.extension(), "png");
    let name = image.file_name();
    assert_eq!(name.rsplit_once('.').map(|(_, ext)| ext), Some("png"));
    assert_eq!(name.len(), 16 + 4, "16 hex digits and the extension");
    assert_eq!(name, image.file_name(), "stable");

    // Whitespace inside the base64 (a wrapped line) is ignored.
    let wrapped = b64(&png(2, 3)).replace("AAAA", "AAAA\n");
    assert_eq!(image::decode("image/png", &wrapped).unwrap().width, 2);

    let photo = image::decode("image/jpeg", &b64(&jpeg(&[], 800, 600))).unwrap();
    assert_eq!(
        (photo.width, photo.height, photo.extension()),
        (800, 600, "jpg")
    );
    assert_eq!(
        photo.file_name().rsplit_once('.').map(|(_, ext)| ext),
        Some("jpg")
    );
}

#[test]
fn jpeg_markers_before_the_frame_header_are_skipped() {
    // An APP0 segment, a fill byte, a restart marker, and TEM: none is a frame.
    let app0 = [0xff, 0xe0, 0x00, 0x04, 0xaa, 0xbb];
    let image = image::decode("image/jpeg", &b64(&jpeg(&app0, 10, 20))).unwrap();
    assert_eq!((image.width, image.height), (10, 20));
    let lengthless = [0xff, 0xd0, 0xff, 0x01];
    assert_eq!(
        image::decode("image/jpeg", &b64(&jpeg(&lengthless, 7, 9)))
            .unwrap()
            .width,
        7
    );
    let padded = [0xff, 0xff, 0xff, 0xe0, 0x00, 0x02];
    assert_eq!(
        image::decode("image/jpeg", &b64(&jpeg(&padded, 5, 5)))
            .unwrap()
            .height,
        5
    );
}

#[test]
fn malformed_images_are_invalid() {
    let cases: [(&str, Vec<u8>); 7] = [
        ("image/png", b"not a png at all".to_vec()),
        ("image/png", png(1, 1).into_iter().take(20).collect()),
        ("image/jpeg", b"GIF89a".to_vec()),
        ("image/jpeg", vec![0xff, 0xd8, 0xff, 0xda, 0, 2]),
        ("image/jpeg", vec![0xff, 0xd8, 0xff, 0xd9]),
        ("image/jpeg", vec![0xff, 0xd8, 0x00, 0x00]),
        ("image/jpeg", vec![0xff, 0xd8, 0xff, 0xe0, 0x00, 0x01]),
    ];
    for (mime, bytes) in cases {
        assert_eq!(
            image::decode(mime, &b64(&bytes)),
            Err(ImageError::Invalid),
            "{mime} {bytes:?}"
        );
    }
    assert_eq!(
        image::decode("image/png", "!!!not base64!!!"),
        Err(ImageError::Invalid)
    );
    assert_eq!(
        image::decode("image/gif", "AAAA"),
        Err(ImageError::Unsupported)
    );
}

#[test]
fn oversized_images_are_refused_before_and_after_decoding() {
    // Refused from the text length alone, before decoding.
    let huge = "A".repeat(4 * (MAX_IMAGE_BYTES / 3 + 10));
    assert_eq!(image::decode("image/png", &huge), Err(ImageError::TooLarge));
    // Just over the cap once decoded.
    let mut bytes = png(1, 1);
    bytes.resize(MAX_IMAGE_BYTES + 1, 0);
    assert_eq!(
        image::decode("image/png", &b64(&bytes)),
        Err(ImageError::TooLarge)
    );
    // Exactly at the cap is fine.
    bytes.truncate(MAX_IMAGE_BYTES);
    assert!(image::decode("image/png", &b64(&bytes)).is_ok());
}

#[test]
fn too_many_pixels_and_zero_sizes_are_refused() {
    let side = MAX_SIDE + 1;
    assert_eq!(
        image::decode("image/png", &b64(&png(side, 1))),
        Err(ImageError::TooManyPixels {
            width: side,
            height: 1
        })
    );
    assert_eq!(
        image::decode("image/png", &b64(&png(MAX_SIDE, MAX_SIDE))),
        Err(ImageError::TooManyPixels {
            width: MAX_SIDE,
            height: MAX_SIDE
        })
    );
    assert_eq!(
        image::decode("image/png", &b64(&png(0, 5))),
        Err(ImageError::TooManyPixels {
            width: 0,
            height: 5
        })
    );
}

#[test]
fn image_errors_say_what_is_wrong() {
    assert!(
        ImageError::TooLarge
            .to_string()
            .contains(&MAX_IMAGE_BYTES.to_string())
    );
    assert_eq!(
        ImageError::TooManyPixels {
            width: 9,
            height: 8
        }
        .to_string(),
        "image of 9\u{d7}8 pixels is too large"
    );
    assert_eq!(ImageError::Invalid.to_string(), "not valid image data");
    assert_eq!(
        ImageError::Unsupported.to_string(),
        "unsupported image type"
    );
}

// ----- connection -----------------------------------------------------------------

fn connection(transport: Transport, ip: &str) -> ConnectionInfo {
    ConnectionInfo::new(transport, ip, [11, 12, 13, 14, 15], "secret", "k")
}

#[test]
fn connections_name_their_channels_endpoints_and_locality() {
    let tcp = connection(Transport::Tcp, "127.0.0.1");
    let ports: Vec<u16> = [
        Channel::Shell,
        Channel::IoPub,
        Channel::Stdin,
        Channel::Control,
        Channel::Heartbeat,
    ]
    .iter()
    .map(|c| tcp.port(*c))
    .collect();
    assert_eq!(ports, [11, 12, 13, 14, 15]);
    assert_eq!(tcp.endpoint(Channel::Shell), "tcp://127.0.0.1:11");
    assert_eq!(
        connection(Transport::Tcp, "::1").endpoint(Channel::IoPub),
        "tcp://[::1]:12"
    );
    assert_eq!(
        connection(Transport::Ipc, "/run/k").endpoint(Channel::Control),
        "ipc:///run/k-14"
    );

    assert!(tcp.is_local());
    assert!(connection(Transport::Tcp, "::1").is_local());
    assert!(connection(Transport::Ipc, "/run/k").is_local());
    assert!(!connection(Transport::Tcp, "192.0.2.7").is_local());
    assert!(!connection(Transport::Tcp, "kernel.example.org").is_local());
}

#[test]
fn a_connection_round_trips_through_json_and_signs_with_its_key() {
    let info = connection(Transport::Tcp, "127.0.0.1");
    assert_eq!(ConnectionInfo::parse(&info.to_json()).unwrap(), info);
    assert!(info.signer().is_ok());
    let mut keyless = info;
    keyless.key.clear();
    assert_eq!(keyless.signer().unwrap_err(), KeyError);
}

#[test]
fn connection_errors_say_what_is_wrong() {
    assert_eq!(
        ConnectionError::TooLarge.to_string(),
        "connection file too large"
    );
    assert_eq!(
        ConnectionError::Malformed.to_string(),
        "malformed connection file"
    );
    assert_eq!(
        ConnectionError::UnsupportedScheme("md5".into()).to_string(),
        "unsupported signature scheme \"md5\""
    );
    assert_eq!(
        ConnectionError::EmptyKey.to_string(),
        "connection file has an empty key"
    );
    assert_eq!(
        ConnectionInfo::parse(&" ".repeat(MAX_CONNECTION_FILE_BYTES + 1)),
        Err(ConnectionError::TooLarge)
    );
    assert_eq!(
        ConnectionInfo::parse("[1]"),
        Err(ConnectionError::Malformed)
    );
}

// ----- kernelspec -----------------------------------------------------------------

fn env_for(os: Os) -> PathEnv {
    let mut env = PathEnv::empty(os);
    env.home = Some("/home/u".into());
    env
}

#[test]
fn the_user_data_directory_follows_each_platforms_convention() {
    use std::path::PathBuf;
    let dir = |env: &PathEnv| kernelspec::user_data_dir(env);
    assert_eq!(
        dir(&env_for(Os::MacOs)),
        Some(PathBuf::from("/home/u/Library/Jupyter"))
    );
    assert_eq!(
        dir(&env_for(Os::Unix)),
        Some(PathBuf::from("/home/u/.local/share/jupyter"))
    );
    assert_eq!(
        dir(&env_for(Os::Windows)),
        Some(PathBuf::from("/home/u/.jupyter/data"))
    );
    let mut xdg = env_for(Os::Unix);
    xdg.xdg_data_home = Some("/xdg".into());
    assert_eq!(dir(&xdg), Some(PathBuf::from("/xdg/jupyter")));
    let mut win = env_for(Os::Windows);
    win.appdata = Some("C:/Roaming".into());
    assert_eq!(dir(&win), Some(PathBuf::from("C:/Roaming/jupyter")));
    let mut forced = env_for(Os::MacOs);
    forced.jupyter_data_dir = Some("/forced".into());
    assert_eq!(dir(&forced), Some(PathBuf::from("/forced")));
    assert_eq!(dir(&PathEnv::empty(Os::Unix)), None);
    // An exported-but-empty variable counts as unset.
    let mut empty = env_for(Os::Unix);
    empty.xdg_data_home = Some(PathBuf::new());
    assert_eq!(
        dir(&empty),
        Some(PathBuf::from("/home/u/.local/share/jupyter"))
    );
}

#[test]
fn data_dirs_come_best_first_without_duplicates() {
    use std::path::PathBuf;
    let mut env = env_for(Os::Unix);
    env.jupyter_path = Some("/opt/a::/opt/b:/opt/a".into());
    env.prefixes = vec!["/venv".into()];
    let dirs = kernelspec::data_dirs(&env);
    assert_eq!(
        dirs,
        [
            "/opt/a",
            "/opt/b",
            "/home/u/.local/share/jupyter",
            "/venv/share/jupyter",
            "/usr/local/share/jupyter",
            "/usr/share/jupyter",
        ]
        .map(PathBuf::from)
    );
    assert_eq!(
        kernelspec::kernel_dirs(&env)[0],
        PathBuf::from("/opt/a/kernels")
    );

    let mut win = env_for(Os::Windows);
    win.jupyter_path = Some("C:\\j;C:\\k".into());
    win.programdata = Some("C:\\ProgramData".into());
    let dirs = kernelspec::data_dirs(&win);
    assert_eq!(dirs[0], PathBuf::from("C:\\j"));
    assert_eq!(dirs[1], PathBuf::from("C:\\k"));
    assert_eq!(
        dirs.last(),
        Some(&PathBuf::from("C:\\ProgramData").join("jupyter"))
    );
}

#[test]
fn kernelspec_errors_say_what_is_wrong() {
    assert_eq!(SpecError::TooLarge.to_string(), "kernel.json too large");
    assert_eq!(SpecError::Malformed.to_string(), "malformed kernel.json");
    assert_eq!(SpecError::BadName.to_string(), "invalid kernel name");
    assert_eq!(SpecError::BadArgv.to_string(), "invalid kernel argv");
    let dir = std::path::Path::new("/k/x");
    let big = " ".repeat(MAX_KERNEL_JSON_BYTES + 1);
    assert_eq!(KernelSpec::parse("x", dir, &big), Err(SpecError::TooLarge));
    assert_eq!(KernelSpec::parse("x", dir, "{"), Err(SpecError::Malformed));
    assert_eq!(
        KernelSpec::parse("bad name", dir, r#"{"argv": ["a"]}"#),
        Err(SpecError::BadName)
    );
    assert_eq!(
        KernelSpec::parse("x", dir, r#"{"argv": []}"#),
        Err(SpecError::BadArgv)
    );
    assert_eq!(
        Os::current(),
        if cfg!(target_os = "macos") {
            Os::MacOs
        } else if cfg!(windows) {
            Os::Windows
        } else {
            Os::Unix
        }
    );
}

// ----- messages -------------------------------------------------------------------

#[test]
fn requests_never_allow_stdin_and_shutdown_carries_restart() {
    let execute = request::execute("1+1");
    assert_eq!(execute["code"], "1+1");
    assert_eq!(execute["allow_stdin"], false);
    assert_eq!(execute["stop_on_error"], true);
    assert_eq!(request::empty(), json!({}));
    assert_eq!(request::shutdown(true), json!({"restart": true}));
}

#[test]
fn every_known_message_type_is_typed_and_unknown_ones_are_other() {
    let cases: [(&str, Value); 9] = [
        (
            "kernel_info_reply",
            json!({"protocol_version": "5.3", "implementation": "x"}),
        ),
        ("execute_input", json!({"code": "1", "execution_count": 2})),
        (
            "display_data",
            json!({"data": {"text/plain": "a"}, "metadata": {}, "transient": {"display_id": "d1"}}),
        ),
        (
            "update_display_data",
            json!({"data": {"text/plain": "b"}, "metadata": {}, "transient": {"display_id": "d1"}}),
        ),
        ("interrupt_reply", json!({"status": "ok"})),
        ("shutdown_reply", json!({"status": "ok", "restart": true})),
        ("status", json!({"execution_state": "busy"})),
        (
            "error",
            json!({"ename": "E", "evalue": "v", "traceback": []}),
        ),
        ("comm_open", json!({"anything": 1})),
    ];
    for (msg_type, content) in cases {
        let message = Message::new(msg_type, content, "m1", "s", "u", "");
        let typed = message.typed().unwrap();
        match (msg_type, typed) {
            ("kernel_info_reply", Content::KernelInfoReply(_))
            | ("interrupt_reply", Content::InterruptReply(_))
            | ("shutdown_reply", Content::ShutdownReply(_))
            | ("comm_open", Content::Other) => {}
            ("execute_input", Content::ExecuteInput(i)) => assert_eq!(i.code, "1"),
            ("display_data", Content::DisplayData(d)) => assert_eq!(d.display_id(), Some("d1")),
            ("update_display_data", Content::UpdateDisplayData(d)) => {
                assert_eq!(d.display_id(), Some("d1"));
            }
            ("status", Content::Status(s)) => assert_eq!(s.execution_state, ExecutionState::Busy),
            ("error", Content::Error(e)) => assert_eq!(e.ename, "E"),
            (name, other) => panic!("{name} typed as {other:?}"),
        }
    }
}

#[test]
fn a_reply_without_status_is_taken_as_ok_and_a_wrong_shape_is_an_error() {
    let reply = Content::parse("interrupt_reply", &json!({})).unwrap();
    assert!(matches!(reply, Content::InterruptReply(_)));
    let exec = Content::parse("execute_reply", &json!({"status": "aborted"})).unwrap();
    assert!(matches!(exec, Content::ExecuteReply(r) if r.status == ReplyStatus::Aborted));
    assert!(Content::parse("execute_reply", &json!({"status": 7})).is_err());
    let message = Message::new("status", json!("nonsense"), "m", "s", "u", "");
    assert!(message.typed().is_err());
    assert_eq!(message.parent_id(), None);
}

// ----- wire -------------------------------------------------------------------------

fn signer() -> Signer {
    match Signer::new(b"0123456789abcdef") {
        Ok(signer) => signer,
        Err(e) => panic!("a non-empty key is accepted: {e}"),
    }
}

/// A signed frame list with the given raw `parent` frame.
fn frames_with_parent(parent: &[u8]) -> Vec<Vec<u8>> {
    let header = br#"{"msg_id":"m","session":"s","username":"u","date":"","msg_type":"status","version":"5.3"}"#;
    let metadata = b"{}";
    let content = br#"{"execution_state":"idle"}"#;
    let signature = signer().sign([header, parent, metadata, content]);
    vec![
        DELIMITER.to_vec(),
        signature.into_bytes(),
        header.to_vec(),
        parent.to_vec(),
        metadata.to_vec(),
        content.to_vec(),
    ]
}

#[test]
fn a_parent_header_may_be_null_empty_or_a_header_but_nothing_else() {
    for parent in [&b"null"[..], b"{}"] {
        let decoded =
            wire::decode(&frames_with_parent(parent), &signer(), Limits::default()).unwrap();
        assert_eq!(decoded.parent_header, None, "{parent:?}");
    }
    let real = br#"{"msg_id":"req","session":"s","username":"u","date":"","msg_type":"execute_request","version":"5.3"}"#;
    let decoded = wire::decode(&frames_with_parent(real), &signer(), Limits::default()).unwrap();
    assert_eq!(decoded.parent_id(), Some("req"));
    for bad in [&b"[1,2]"[..], b"{\"msg_id\": 5}", b"not json"] {
        assert_eq!(
            wire::decode(&frames_with_parent(bad), &signer(), Limits::default()),
            Err(DecodeError::Json("parent_header")),
            "{bad:?}"
        );
    }
}

#[test]
fn wire_errors_say_what_is_wrong() {
    assert_eq!(
        KeyError.to_string(),
        "the connection key is empty; Vix requires signed messages"
    );
    assert_eq!(DecodeError::TooManyFrames.to_string(), "too many frames");
    assert_eq!(DecodeError::TooLarge.to_string(), "message too large");
    assert_eq!(
        DecodeError::NoDelimiter.to_string(),
        "no <IDS|MSG> delimiter"
    );
    assert_eq!(DecodeError::Truncated.to_string(), "truncated message");
    assert_eq!(DecodeError::BadSignature.to_string(), "bad signature");
    assert_eq!(
        DecodeError::Json("content").to_string(),
        "malformed content"
    );
    let limits = Limits::default();
    assert_eq!(
        (limits.max_frames, limits.max_total_bytes),
        (64, 64 * 1024 * 1024)
    );
}

// ----- output -------------------------------------------------------------------------

fn bundle(value: &Value) -> MimeBundle {
    match serde_json::from_value(value.clone()) {
        Ok(bundle) => bundle,
        Err(e) => panic!("not a MIME bundle: {e}"),
    }
}

#[test]
fn markdown_is_text_and_an_unreadable_mime_value_is_nothing() {
    let md = bundle(&json!({"text/markdown": "# hi", "text/plain": "hi"}));
    let choice = md.choose().unwrap();
    assert_eq!(choice.mime, "text/markdown");
    assert_eq!(choice.payload, Payload::Text("# hi".into()));
    assert_eq!(bundle(&json!({"text/plain": 7})).text("text/plain"), None);
    assert_eq!(bundle(&json!({})).text("text/plain"), None);
}

#[test]
fn only_output_messages_become_outputs_and_a_bundle_without_text_has_none() {
    let stream = Content::parse("stream", &json!({"name": "stderr", "text": "warn"})).unwrap();
    assert_eq!(
        Output::from_content(stream),
        Some(Output::Stream {
            name: StreamName::Stderr,
            text: "warn".into()
        })
    );
    let result = Content::parse(
        "execute_result",
        &json!({"execution_count": 3, "data": {"text/plain": "5"}, "metadata": {}}),
    )
    .unwrap();
    assert!(matches!(
        Output::from_content(result),
        Some(Output::Result {
            execution_count: Some(3),
            ..
        })
    ));
    let update = Content::parse(
        "update_display_data",
        &json!({"data": {}, "metadata": {}, "transient": {}}),
    )
    .unwrap();
    let Some(Output::Display {
        update: is_update,
        display_id: None,
        data,
    }) = Output::from_content(update)
    else {
        panic!("expected a display update");
    };
    assert!(is_update);
    assert_eq!(
        Output::Display {
            update: true,
            display_id: None,
            data
        }
        .plain_text(),
        ""
    );
    assert_eq!(Output::from_content(Content::Other), None);
    let status = Content::parse("status", &json!({"execution_state": "idle"})).unwrap();
    assert_eq!(Output::from_content(status), None);
}

// ----- org ------------------------------------------------------------------------------

const BUFFER: &str = "#+begin_src python\n1\n#+end_src\n";

#[test]
fn results_of_every_shape_are_found_and_replaced() {
    let with = |results: &str| format!("{BUFFER}\n{results}\nafter\n");
    // An example block, closed.
    let closed = with("#+RESULTS:\n#+begin_example\na\nb\n#+end_example");
    assert_eq!(org::find_results(&closed, 2), Some((4, 9)));
    // An unclosed example block takes only its opening line.
    let unclosed = with("#+RESULTS:\n#+begin_example\na\nb");
    assert_eq!(org::find_results(&unclosed, 2), Some((4, 6)));
    // A file link.
    let link = with("#+RESULTS:\n[[file:a.png]]");
    assert_eq!(org::find_results(&link, 2), Some((4, 6)));
    // A results marker as the very last line has an empty body.
    let bare = format!("{BUFFER}\n#+RESULTS:");
    assert_eq!(org::find_results(&bare, 2), Some((4, 5)));
    // Colon lines, a table, and the end of the block.
    let colons = with("#+RESULTS[abc123]:\n: one\n:\n| t |");
    assert_eq!(org::find_results(&colons, 2), Some((4, 8)));
    // No results at all, or a different block in between.
    assert_eq!(org::find_results(BUFFER, 2), None);
    assert_eq!(
        org::find_results("#+begin_src x\n#+end_src\ntext\n#+RESULTS:\n: a\n", 1),
        None
    );
}

#[test]
fn results_edits_insert_replace_append_and_clear() {
    let block = org::results_block(&ResultValue::Text("42".into()));
    assert_eq!(block, "#+RESULTS:\n: 42\n");
    let inserted = org::apply(BUFFER, &org::results_edit(BUFFER, 2, Some(&block)));
    assert_eq!(inserted, format!("{BUFFER}\n{block}"));
    // Replacing swaps the old block for the new one.
    let newer = org::results_block(&ResultValue::Text("43".into()));
    let replaced = org::apply(&inserted, &org::results_edit(&inserted, 2, Some(&newer)));
    assert_eq!(replaced, format!("{BUFFER}\n{newer}"));
    // Appending adds the new lines after the old results.
    let appended = org::apply(&inserted, &org::results_append_edit(&inserted, 2, &newer));
    assert_eq!(appended, format!("{BUFFER}\n#+RESULTS:\n: 42\n: 43\n"));
    // With nothing to append to, it inserts like a normal write.
    assert_eq!(
        org::apply(BUFFER, &org::results_append_edit(BUFFER, 2, &block)),
        inserted
    );
    // Clearing removes the block and the blank line before it.
    assert_eq!(
        org::apply(&inserted, &org::results_edit(&inserted, 2, None)),
        BUFFER
    );
    // Clearing nothing changes nothing.
    assert_eq!(
        org::apply(BUFFER, &org::results_edit(BUFFER, 2, None)),
        BUFFER
    );
}

#[test]
fn results_blocks_escape_and_link() {
    assert_eq!(
        org::results_block(&ResultValue::Text(String::new())),
        "#+RESULTS:\n"
    );
    assert_eq!(
        org::results_block(&ResultValue::Text("a\n\nb\n".into())),
        "#+RESULTS:\n: a\n:\n: b\n"
    );
    assert_eq!(
        org::results_block(&ResultValue::File("p]q.png".into())),
        "#+RESULTS:\n[[file:p%5Dq.png]]\n"
    );
    let long = (0..org::MIN_LINES_FOR_BLOCK)
        .map(|i| {
            if i == 1 {
                "* heading-like".to_owned()
            } else {
                format!("l{i}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let block = org::results_block(&ResultValue::Text(long));
    assert!(block.starts_with("#+RESULTS:\n#+begin_example\n"));
    assert!(block.contains("\n,* heading-like\n"), "{block}");
    assert!(block.ends_with("#+end_example\n"));
}

#[test]
fn an_edit_past_the_end_appends_after_a_missing_newline() {
    let edit = org::ResultsEdit {
        start: 9,
        end: 9,
        text: "x\n".into(),
    };
    assert_eq!(org::apply("one\ntwo", &edit), "one\ntwo\nx\n");
    assert_eq!(org::apply("", &edit), "x\n");
}

// ----- babel ------------------------------------------------------------------------------

#[test]
fn header_arguments_cope_with_quotes_and_stray_words() {
    // An unterminated quote is read as plain words.
    assert_eq!(
        Header::parse(r#"#+begin_src py :file "open end"#)
            .file
            .as_deref(),
        Some("\"open")
    );
    // A lone colon is a value, not a key; words before any key are ignored.
    let header = Header::parse("#+begin_src py stray : :results output");
    assert_eq!(header.collect, Collect::Output);
    // `#+begin_src` with nothing after it, or glued to other text.
    assert_eq!(Header::parse("#+begin_src"), Header::default());
    assert_eq!(
        Header::parse("#+begin_srcs python :results output"),
        Header::default()
    );
    // Arguments with no language.
    assert_eq!(
        Header::parse("#+begin_src :results silent").handling,
        Handling::Silent
    );
    assert_eq!(Header::parse("#+begin_src py :eval").eval, Eval::Yes);
    assert_eq!(Header::parse("#+begin_src py :file").file, None);
}

fn display(pairs: &Value) -> Output {
    Output::Display {
        data: bundle(pairs),
        display_id: None,
        update: false,
    }
}

#[test]
fn result_text_follows_the_collection_mode() {
    let outputs = [
        Output::Stream {
            name: StreamName::Stdout,
            text: "out".into(),
        },
        Output::Result {
            execution_count: Some(1),
            data: bundle(&json!({"text/plain": "\u{1b}[1m42\u{1b}[0m\n"})),
        },
        display(&json!({"image/png": "iVBORw0K", "text/plain": "<Image>"})),
        display(&json!({"image/png": "iVBORw0K"})),
        display(&json!({"application/json": {}})),
        Output::Stream {
            name: StreamName::Stderr,
            text: String::new(),
        },
    ];
    // Value mode keeps values and images' text forms, in arrival order.
    assert_eq!(
        babel::result_text(&outputs, Collect::Value),
        "out\n42\n<Image>\n"
    );
    // Output mode keeps only what was printed.
    assert_eq!(babel::result_text(&outputs, Collect::Output), "out\n");
    assert_eq!(babel::result_text(&[], Collect::Value), "");
}

#[test]
fn the_first_image_is_found_among_the_outputs() {
    let outputs = [
        Output::Stream {
            name: StreamName::Stdout,
            text: "x".into(),
        },
        display(&json!({"text/plain": "no image"})),
        display(&json!({"image/jpeg": "AAAA", "text/plain": "<Image>"})),
        display(&json!({"image/png": "BBBB"})),
    ];
    assert_eq!(
        babel::first_image(&outputs),
        Some(("image/jpeg", "AAAA".to_owned()))
    );
    assert_eq!(babel::first_image(&outputs[..2]), None);
}
