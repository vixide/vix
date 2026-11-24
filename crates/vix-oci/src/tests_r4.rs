//! Round-4 behavior tests: the remaining argv builders, the parsers' less
//! common record shapes, devcontainer JSONC edge cases and error plumbing.

use crate::argv::{self, ComposeOp, Kind, RunOptions};
use crate::devcontainer::{Up, parse_config, parse_up, strip_jsonc, up_argv, workspace_folder};
use crate::{OciError, parse};

fn s(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|p| (*p).to_string()).collect()
}

#[test]
fn inspect_names_each_kind_and_refuses_bad_ids() {
    assert_eq!(
        argv::inspect(Kind::Container, "c1").unwrap(),
        s(&["container", "inspect", "c1"])
    );
    assert_eq!(
        argv::inspect(Kind::Image, "i1").unwrap(),
        s(&["image", "inspect", "i1"])
    );
    assert_eq!(
        argv::inspect(Kind::Volume, "v1").unwrap(),
        s(&["volume", "inspect", "v1"])
    );
    assert_eq!(
        argv::inspect(Kind::Network, "n1").unwrap(),
        s(&["network", "inspect", "n1"])
    );
    assert_eq!(
        argv::inspect(Kind::Pod, "p1").unwrap(),
        s(&["pod", "inspect", "p1"])
    );
    assert!(
        matches!(argv::inspect(Kind::Pod, "-x"), Err(OciError::InvalidArgument(a)) if a == "-x")
    );
}

#[test]
fn listing_covers_every_kind() {
    assert_eq!(
        argv::list(Kind::Container, true),
        s(&["ps", "-a", "--no-trunc", "--format", "json"])
    );
    assert_eq!(
        argv::list(Kind::Image, false),
        s(&["images", "--format", "json"])
    );
    assert_eq!(
        argv::list(Kind::Volume, false),
        s(&["volume", "ls", "--format", "json"])
    );
    assert_eq!(
        argv::list(Kind::Network, false),
        s(&["network", "ls", "--no-trunc", "--format", "json"])
    );
    assert_eq!(
        argv::list(Kind::Pod, false),
        s(&["pod", "ps", "--format", "json"])
    );
}

#[test]
fn all_container_ops_and_logs_and_shell() {
    use argv::ContainerOp as Op;
    for (op, verb) in [
        (Op::Start, "start"),
        (Op::Stop, "stop"),
        (Op::Pause, "pause"),
        (Op::Unpause, "unpause"),
    ] {
        assert_eq!(argv::container_op(op, "abc").unwrap(), s(&[verb, "abc"]));
        assert!(!op.is_destructive());
    }
    assert_eq!(
        argv::container_op(Op::Remove { force: false }, "abc").unwrap(),
        s(&["rm", "abc"])
    );
    assert_eq!(
        argv::logs("abc", 50).unwrap(),
        s(&["logs", "--tail", "50", "abc"])
    );
    assert!(argv::logs("a b", 1).is_err());
    assert_eq!(
        argv::exec_shell("abc").unwrap(),
        s(&["exec", "-it", "abc", "sh"])
    );
    assert!(argv::exec_shell("").is_err());
}

#[test]
fn run_places_flags_before_the_image_and_checks_each_part() {
    let o = RunOptions {
        image: "alpine:3".into(),
        name: Some("web".into()),
        ports: vec!["8080:80".into(), "9090:90".into()],
        detach: true,
    };
    assert_eq!(
        argv::run(&o).unwrap(),
        s(&[
            "run", "-d", "--name", "web", "-p", "8080:80", "-p", "9090:90", "alpine:3"
        ])
    );
    let attached = RunOptions {
        image: "alpine".into(),
        ..RunOptions::default()
    };
    assert_eq!(
        argv::run(&attached).unwrap(),
        s(&["run", "-it", "--rm", "alpine"])
    );
    let bad_name = RunOptions {
        name: Some("--x".into()),
        ..attached.clone()
    };
    assert!(argv::run(&bad_name).is_err());
    let bad_port = RunOptions {
        ports: vec!["-p".into()],
        ..attached
    };
    assert!(argv::run(&bad_port).is_err());
    assert!(argv::run(&RunOptions::default()).is_err());
}

#[test]
fn tag_pull_push_and_remove() {
    assert_eq!(argv::tag("a:1", "b:2").unwrap(), s(&["tag", "a:1", "b:2"]));
    assert!(argv::tag("a:1", "--b").is_err());
    assert!(argv::tag("-a", "b").is_err());
    assert_eq!(argv::pull("alpine").unwrap(), s(&["pull", "alpine"]));
    assert_eq!(argv::push("alpine").unwrap(), s(&["push", "alpine"]));
    assert!(argv::pull("").is_err());
    assert!(argv::push("-x").is_err());
    assert_eq!(
        argv::remove(Kind::Container, "c", true).unwrap(),
        s(&["rm", "-f", "c"])
    );
    assert_eq!(
        argv::remove(Kind::Image, "i", false).unwrap(),
        s(&["rmi", "i"])
    );
    assert_eq!(
        argv::remove(Kind::Volume, "v", true).unwrap(),
        s(&["volume", "rm", "-f", "v"])
    );
    assert_eq!(
        argv::remove(Kind::Pod, "p", true).unwrap(),
        s(&["pod", "rm", "-f", "p"])
    );
    // `-f` is never emitted for networks.
    assert_eq!(
        argv::remove(Kind::Network, "n", true).unwrap(),
        s(&["network", "rm", "n"])
    );
    assert!(argv::remove(Kind::Network, "-n", false).is_err());
}

#[test]
fn build_compose_login_and_prune() {
    assert_eq!(
        argv::build("My Dir/Dockerfile", Some("app:1"), "ctx dir").unwrap(),
        s(&["build", "-f", "My Dir/Dockerfile", "-t", "app:1", "ctx dir"])
    );
    assert_eq!(
        argv::build("Dockerfile", None, ".").unwrap(),
        s(&["build", "-f", "Dockerfile", "."])
    );
    assert!(argv::build("Dockerfile", Some("--t"), ".").is_err());
    assert!(argv::build("-f", None, ".").is_err());
    assert!(argv::build("Dockerfile", None, "").is_err());

    assert_eq!(
        argv::compose(&ComposeOp::Up, Some("dc.yml")).unwrap(),
        s(&["compose", "-f", "dc.yml", "up", "-d"])
    );
    assert_eq!(
        argv::compose(&ComposeOp::Down, None).unwrap(),
        s(&["compose", "down"])
    );
    assert_eq!(
        argv::compose(&ComposeOp::Ps, None).unwrap(),
        s(&["compose", "ps"])
    );
    assert_eq!(
        argv::compose(&ComposeOp::Services, None).unwrap(),
        s(&["compose", "config", "--services"])
    );
    assert_eq!(
        argv::compose(
            &ComposeOp::Logs {
                service: Some("web".into()),
                tail: 20
            },
            None
        )
        .unwrap(),
        s(&["compose", "logs", "--no-color", "--tail", "20", "web"])
    );
    assert_eq!(
        argv::compose(
            &ComposeOp::Logs {
                service: None,
                tail: 5
            },
            None
        )
        .unwrap(),
        s(&["compose", "logs", "--no-color", "--tail", "5"])
    );
    assert!(
        argv::compose(
            &ComposeOp::Logs {
                service: Some("-x".into()),
                tail: 1
            },
            None
        )
        .is_err()
    );
    assert!(argv::compose(&ComposeOp::Up, Some("-f")).is_err());

    assert_eq!(
        argv::login("ghcr.io", "me").unwrap(),
        s(&["login", "--username", "me", "--password-stdin", "ghcr.io"])
    );
    assert!(argv::login("ghcr.io", "-me").is_err());
    assert!(argv::login("", "me").is_err());

    assert_eq!(
        argv::prune(Kind::Container).unwrap(),
        s(&["container", "prune", "-f"])
    );
    assert_eq!(
        argv::prune(Kind::Image).unwrap(),
        s(&["image", "prune", "-f"])
    );
    assert_eq!(
        argv::prune(Kind::Volume).unwrap(),
        s(&["volume", "prune", "-f"])
    );
    assert_eq!(
        argv::prune(Kind::Network).unwrap(),
        s(&["network", "prune", "-f"])
    );
    assert!(matches!(argv::prune(Kind::Pod), Err(OciError::InvalidArgument(a)) if a == "pod"));
}

#[test]
fn render_escapes_embedded_quotes_and_empty_args() {
    let args = s(&["run", "it's", ""]);
    assert_eq!(argv::render("docker", &args), "docker run 'it'\\''s' ''");
}

#[test]
fn errors_display_and_convert() {
    let nf: OciError = std::io::Error::from(std::io::ErrorKind::NotFound).into();
    assert!(matches!(nf, OciError::NotInstalled));
    assert_eq!(nf.to_string(), "no container engine found");
    let other: OciError = std::io::Error::from(std::io::ErrorKind::PermissionDenied).into();
    assert!(matches!(other, OciError::Io(_)));
    assert!(
        other
            .to_string()
            .starts_with("could not run the container engine")
    );
    assert!(std::error::Error::source(&other).is_some());
    assert!(std::error::Error::source(&nf).is_none());
    let failed = OciError::Failed {
        code: Some(3),
        stderr: "boom".into(),
    };
    assert_eq!(failed.to_string(), "engine exited with 3: boom");
    let killed = OciError::Failed {
        code: None,
        stderr: "sig".into(),
    };
    assert_eq!(killed.to_string(), "engine was terminated: sig");
    assert_eq!(
        OciError::InvalidArgument("x y".into()).to_string(),
        "refused argument \"x y\""
    );
    assert_eq!(
        OciError::Parse("bad".into()).to_string(),
        "unreadable engine output: bad"
    );
}

#[test]
fn run_reports_a_missing_program_as_not_installed() {
    let missing = std::path::Path::new("/nonexistent-dir-r4/engine");
    assert!(matches!(
        crate::run(missing, &[], None),
        Err(OciError::NotInstalled)
    ));
    assert!(matches!(
        crate::run(missing, &s(&["ps"]), Some("pw")),
        Err(OciError::NotInstalled)
    ));
}

#[test]
fn records_reject_non_array_json_that_starts_like_one() {
    // A leading `[` that is not valid JSON at all.
    assert!(matches!(parse::records("[oops"), Err(OciError::Parse(_))));
}

#[test]
fn podman_container_ports_are_shown_and_deduplicated() {
    let out = r#"[{"Id":"abcdef0123456789","Names":["web"],"Image":"nginx","State":"paused",
        "Ports":[{"host_port":8080,"container_port":80,"protocol":"tcp"},
                 {"host_port":8080,"container_port":81,"protocol":"udp"},
                 {"container_port":90,"protocol":"tcp"}]}]"#;
    let c = parse::containers(out).unwrap();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].ports, "8080->80/tcp, 8080->81/udp");
    assert_eq!(c[0].host_ports, vec![8080]);
    assert!(c[0].is_paused() && c[0].is_running());
    assert_eq!(c[0].short_id(), "abcdef012345");
    // No `Ports` at all.
    let none = parse::containers(r#"{"ID":"x","State":"exited"}"#).unwrap();
    assert!(none[0].ports.is_empty() && none[0].host_ports.is_empty());
    assert!(!none[0].is_running());
}

#[test]
fn images_read_docker_columns_and_podman_references() {
    let docker =
        r#"{"ID":"sha1","Repository":"alpine","Tag":"3","Size":"7MB","CreatedSince":"2 days ago"}"#;
    let i = &parse::images(docker).unwrap()[0];
    assert_eq!(
        (i.repository.as_str(), i.tag.as_str(), i.size.as_str()),
        ("alpine", "3", "7MB")
    );
    assert_eq!(i.created, "2 days ago");
    assert_eq!(i.reference(), "alpine:3");

    let podman = r#"[{"Id":"sha2","Names":["docker.io/library/busybox:1.36"],"Size":12300000,"Created":0},
                     {"Id":"sha3","Size":3}]"#;
    let imgs = parse::images(podman).unwrap();
    assert_eq!(imgs[0].repository, "docker.io/library/busybox");
    assert_eq!(imgs[0].tag, "1.36");
    assert_eq!(imgs[0].size, "12.3MB");
    // Untagged: the reference falls back to the id.
    assert_eq!(imgs[1].repository, "<none>");
    assert_eq!(imgs[1].reference(), "sha3");
    assert_eq!(imgs[1].size, "3B");
}

#[test]
fn sizes_step_up_units_and_saturate() {
    let huge = format!(
        r#"{{"ID":"x","Repository":"r","Tag":"t","Size":{}}}"#,
        u64::MAX
    );
    let size = parse::images(&huge).unwrap().remove(0).size;
    assert!(
        size.ends_with("TB") || size.ends_with("PB") || size.ends_with("EB"),
        "{size}"
    );
    let kb = parse::images(r#"{"ID":"x","Repository":"r","Tag":"t","Size":4096}"#)
        .unwrap()
        .remove(0)
        .size;
    assert_eq!(kb, "4.0kB");
}

#[test]
fn volumes_networks_pods_and_text_helpers() {
    let v =
        parse::volumes(r#"{"Name":"data","Driver":"local","Mountpoint":"/var/lib/data"}"#).unwrap();
    assert_eq!(
        (
            v[0].name.as_str(),
            v[0].driver.as_str(),
            v[0].mountpoint.as_str()
        ),
        ("data", "local", "/var/lib/data")
    );
    let n = parse::networks(r#"{"ID":"n1","Name":"bridge","Driver":"bridge","Scope":"local"}"#)
        .unwrap();
    assert_eq!((n[0].id.as_str(), n[0].scope.as_str()), ("n1", "local"));
    let p = parse::pods(
        r#"[{"Id":"p1","Name":"a","Status":"Running","Containers":[{},{}]},
            {"Id":"p2","Name":"b","Containers":3},
            {"Id":"p3","Name":"c","Containers":"many"}]"#,
    )
    .unwrap();
    assert_eq!(
        p.iter().map(|x| x.containers).collect::<Vec<_>>(),
        vec![2, 3, 0]
    );
    assert_eq!(
        parse::lines(" web \n\n db\n"),
        vec!["web".to_string(), "db".to_string()]
    );
    assert_eq!(parse::pretty("{\"a\":1}"), "{\n  \"a\": 1\n}");
    assert_eq!(parse::pretty("not json"), "not json");
    assert!(parse::containers("{broken").is_err());
}

#[test]
fn jsonc_comments_escapes_and_trailing_commas() {
    let text = "{\n // line\n \"url\": \"http://x//y\\\"//z\", /* block */ \"a\": [1, 2,], \"b\": {\"c\": 1,},\n}";
    let v: serde_json::Value = serde_json::from_str(&strip_jsonc(text)).unwrap();
    assert_eq!(v["url"], "http://x//y\"//z");
    assert_eq!(v["a"], serde_json::json!([1, 2]));
    assert_eq!(v["b"]["c"], 1);
    // An unterminated string keeps what it has; a lone trailing backslash is kept.
    assert_eq!(strip_jsonc("\"abc\\"), "\"abc\\");
}

#[test]
fn devcontainer_config_up_and_workspace_folder() {
    let cfg = parse_config("{ // c\n \"name\": \"Dev\", \"workspaceFolder\": \"/w/${localWorkspaceFolderBasename}\", \"remoteUser\": \"\" }").unwrap();
    assert_eq!(cfg.name.as_deref(), Some("Dev"));
    assert_eq!(cfg.remote_user, None);
    assert!(matches!(parse_config("[1]"), Err(OciError::Parse(m)) if m.contains("not an object")));
    assert!(matches!(parse_config("{"), Err(OciError::Parse(_))));

    assert_eq!(
        up_argv("/w", None).unwrap(),
        s(&["up", "--workspace-folder", "/w"])
    );
    assert_eq!(
        up_argv("/w", Some("podman")).unwrap(),
        s(&["up", "--workspace-folder", "/w", "--docker-path", "podman"])
    );
    assert!(up_argv("/w", Some("-x")).is_err());
    assert!(up_argv("", None).is_err());

    let up = parse_up("log line\n{\"x\":1}\n{\"outcome\":\"success\",\"containerId\":\"c1\",\"remoteUser\":\"vscode\"}\n").unwrap();
    assert_eq!(up.container_id, "c1");
    assert_eq!(up.remote_user.as_deref(), Some("vscode"));
    assert!(matches!(parse_up("nothing"), Err(OciError::Parse(_))));
    assert!(matches!(
        parse_up("{\"outcome\":\"success\"}"),
        Err(OciError::Parse(m)) if m.contains("no container")
    ));
    match parse_up("{\"outcome\":\"error\",\"message\":\"m\",\"description\":\"d\"}") {
        Err(OciError::Failed { code: None, stderr }) => assert_eq!(stderr, "m: d"),
        other => panic!("{other:?}"),
    }

    assert_eq!(workspace_folder(&up, &cfg, "proj"), "/w/proj");
    assert_eq!(
        workspace_folder(&up, &parse_config("{}").unwrap(), "proj"),
        "/workspaces/proj"
    );
    let mounted = Up {
        remote_workspace_folder: Some("/mnt".into()),
        ..up
    };
    assert_eq!(workspace_folder(&mounted, &cfg, "proj"), "/mnt");
}
