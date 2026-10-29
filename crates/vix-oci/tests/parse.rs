//! Parser tests against fixtures.
//!
//! The fixtures follow the documented `--format json` shapes of Docker (one
//! JSON object per line) and Podman (one array); they were written by hand
//! from those shapes, not captured from a live engine, so re-check them
//! against a real engine's output when one is to hand.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests fail loudly on a broken fixture"
)]

use vix_oci::parse;

#[test]
fn docker_ps_ndjson() {
    let c = parse::containers(include_str!("fixtures/docker-ps.ndjson")).unwrap();
    assert_eq!(c.len(), 2);
    assert_eq!(c[0].name, "web");
    assert_eq!(c[0].image, "nginx:1.27");
    assert!(c[0].is_running() && !c[0].is_paused());
    assert_eq!(c[0].host_ports, vec![8080]);
    assert_eq!(c[0].short_id(), "3f1c9a7b2d4e");
    assert_eq!(c[0].status, "Up 2 days");
    assert_eq!(c[1].name, "sleeper,alias");
    assert!(!c[1].is_running());
    assert_eq!(c[1].host_ports.len(), 0);
}

#[test]
fn podman_ps_array() {
    let c = parse::containers(include_str!("fixtures/podman-ps.json")).unwrap();
    assert_eq!(c.len(), 2);
    assert_eq!(c[0].name, "web");
    assert_eq!(c[0].ports, "8080->80/tcp");
    assert_eq!(c[0].host_ports, vec![8080]);
    assert_eq!(c[0].created, "2025-10-20");
    assert!(c[0].is_running());
    assert_eq!(c[1].ports, "");
    assert_eq!(c[1].state, "exited");
}

#[test]
fn images_both_shapes() {
    let d = parse::images(include_str!("fixtures/docker-images.ndjson")).unwrap();
    assert_eq!(d[0].reference(), "nginx:1.27");
    assert_eq!(d[0].size, "192MB");
    assert_eq!(d[0].created, "4 weeks ago");
    assert_eq!(d[1].reference(), "sha256:2222222222222222");

    let p = parse::images(include_str!("fixtures/podman-images.json")).unwrap();
    assert_eq!(p[0].repository, "docker.io/library/alpine");
    assert_eq!(p[0].tag, "3.19");
    assert_eq!(p[0].size, "7.4MB");
    assert_eq!(p[0].created, "2023-11-14T22:13:20Z");
    assert_eq!(p[1].repository, "<none>");
    assert_eq!(p[1].size, "1.5kB");
    assert_eq!(p[1].reference(), p[1].id);
}

#[test]
fn volumes_networks_and_pods() {
    let v = parse::volumes(include_str!("fixtures/docker-volumes.ndjson")).unwrap();
    assert_eq!(
        (v[0].name.as_str(), v[0].driver.as_str()),
        ("pgdata", "local")
    );
    let v = parse::volumes(include_str!("fixtures/podman-volumes.json")).unwrap();
    assert!(v[0].mountpoint.ends_with("_data"));

    let n = parse::networks(include_str!("fixtures/docker-networks.ndjson")).unwrap();
    assert_eq!(n.len(), 2);
    assert_eq!(n[1].name, "appnet");
    assert_eq!(n[1].scope, "local");
    let n = parse::networks(include_str!("fixtures/podman-networks.json")).unwrap();
    assert_eq!(n[0].name, "podman");
    assert_eq!(n[0].driver, "bridge");

    let p = parse::pods(include_str!("fixtures/podman-pods.json")).unwrap();
    assert_eq!((p[0].name.as_str(), p[0].containers), ("mypod", 2));
}

#[test]
fn malformed_output_is_a_typed_error_and_empty_is_empty() {
    assert!(parse::containers("not json").is_err());
    assert_eq!(parse::images("").unwrap().len(), 0);
}

#[test]
fn services_and_pretty() {
    assert_eq!(parse::lines("web\n\n db \n"), ["web", "db"]);
    assert!(parse::pretty("[{\"a\":1}]").contains("\n  "));
    assert_eq!(parse::pretty("nope"), "nope");
}
