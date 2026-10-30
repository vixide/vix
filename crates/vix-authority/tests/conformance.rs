//! The shared conformance suite (T711) against every authority in this crate.

use std::path::{Path, PathBuf};

use vix_authority::{Authority, FakeAuthority, LocalAuthority, conformance};

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "vix-authority-conformance-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn the_local_authority_conforms() {
    let dir = scratch("local");
    let failures = conformance::run(&LocalAuthority, &dir);
    assert_eq!(failures, Vec::<String>::new());
}

#[test]
fn the_fake_authority_conforms() {
    let dir = scratch("fake");
    let fake = FakeAuthority::new(&dir).unwrap();
    let failures = conformance::run(&fake, Path::new("/work"));
    assert_eq!(failures, Vec::<String>::new());
    // Everything happened inside the backing directory.
    assert!(!fake.backing().join("work").exists());
    assert!(fake.calls().iter().any(|c| c.op == "write_atomic"));
    assert_eq!(fake.name(), "fake");
    let _ = std::fs::remove_dir_all(&dir);
}
