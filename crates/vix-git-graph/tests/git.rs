//! Integration tests against a real temporary git repository.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "tests fail loudly on a broken fixture"
)]

use std::path::Path;
use std::process::Command;

use vix_git_graph::{
    CheckoutTarget, GraphError, Panel, assign_lanes, checkout, cherry_pick, create_branch_at,
    fetch_page, list_refs,
};

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=T", "-c", "user.email=t@example.com"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn commit(dir: &Path, file: &str, msg: &str) {
    std::fs::write(dir.join(file), msg).unwrap();
    git(dir, &["add", file]);
    git(dir, &["commit", "-m", msg]);
}

/// main: a - b - M(merge) ; topic: a - c - (merged into M)
fn repo() -> tempfile_dir::Dir {
    let d = tempfile_dir::Dir::new();
    let p = d.path();
    git(p, &["init", "-q", "-b", "main"]);
    commit(p, "a", "a");
    git(p, &["switch", "-q", "-c", "topic"]);
    commit(p, "c", "c");
    git(p, &["switch", "-q", "main"]);
    commit(p, "b", "b");
    git(p, &["merge", "-q", "--no-ff", "-m", "M", "topic"]);
    git(p, &["tag", "v1"]);
    d
}

/// A tiny temp-dir helper so the test needs no extra dependency.
mod tempfile_dir {
    use std::path::{Path, PathBuf};

    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    pub struct Dir(PathBuf);

    impl Dir {
        pub fn new() -> Self {
            let n = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let seq = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let p = std::env::temp_dir()
                .join(format!("vix-git-graph-{}-{n}-{seq}", std::process::id()));
            std::fs::create_dir_all(&p).unwrap();
            Dir(p)
        }
        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }
}

#[test]
fn pages_decorations_and_lanes_come_from_real_history() {
    let d = repo();
    let all = fetch_page(d.path(), 0, 200, None).unwrap();
    assert_eq!(all.len(), 4);
    assert_eq!(all[0].subject, "M");
    assert_eq!(all[0].parents.len(), 2);
    assert!(all[0].refs.iter().any(|r| r.name == "main" && r.is_head));
    assert!(all[0].refs.iter().any(|r| r.name == "v1"));
    // Lanes: the merge fans out to two lanes, and the root closes them.
    let rows = assign_lanes(&all);
    assert_eq!(rows[0].outgoing.len(), 2);
    assert!(rows.iter().any(|r| r.lane == 1));

    // Paging: two commits at a time, no overlap, same order.
    let first = fetch_page(d.path(), 0, 2, None).unwrap();
    let second = fetch_page(d.path(), 2, 2, None).unwrap();
    assert_eq!(first.len() + second.len(), 4);
    let joined: Vec<_> = first.iter().chain(&second).map(|c| &c.sha).collect();
    assert_eq!(joined, all.iter().map(|c| &c.sha).collect::<Vec<_>>());
    assert_eq!(fetch_page(d.path(), 4, 2, None).unwrap().len(), 0);

    // Branch filter: topic alone is c and a.
    let topic = fetch_page(d.path(), 0, 200, Some("topic")).unwrap();
    assert_eq!(topic.len(), 3 - 1);
    let refs = list_refs(d.path()).unwrap();
    assert!(refs.contains(&"main".to_string()) && refs.contains(&"v1".to_string()));
    assert!(matches!(
        fetch_page(d.path(), 0, 5, Some("--all")),
        Err(GraphError::InvalidArgument(_))
    ));
}

#[test]
fn empty_repository_is_an_empty_page() {
    let d = tempfile_dir::Dir::new();
    git(d.path(), &["init", "-q", "-b", "main"]);
    assert_eq!(fetch_page(d.path(), 0, 200, None).unwrap().len(), 0);
    let p = Panel::new(Vec::new(), Vec::new(), true);
    assert!(p.is_empty() && p.selected_commit().is_none());
}

#[test]
fn checkout_branch_create_branch_and_cherry_pick() {
    let d = repo();
    let p = d.path();
    let all = fetch_page(p, 0, 200, None).unwrap();

    // The commit "c" carries the local branch `topic`: check that out.
    let c = all.iter().find(|c| c.subject == "c").unwrap();
    assert_eq!(
        checkout(p, c).unwrap(),
        CheckoutTarget::Branch("topic".to_string())
    );

    // The root commit has no branch label: detached HEAD.
    let a = all.iter().find(|c| c.subject == "a").unwrap();
    assert!(matches!(
        checkout(p, a).unwrap(),
        CheckoutTarget::Detached(_)
    ));

    // New branch at a commit.
    create_branch_at(
        p,
        "from-b",
        &all.iter().find(|c| c.subject == "b").unwrap().sha,
    )
    .unwrap();
    assert!(list_refs(p).unwrap().contains(&"from-b".to_string()));
    assert!(create_branch_at(p, "-x", &a.sha).is_err());
    assert!(create_branch_at(p, "ok", "--all").is_err());

    // Cherry-pick the topic commit onto `a`'s branch: file `c` appears.
    git(p, &["switch", "-q", "-c", "pick", &a.sha]);
    cherry_pick(p, &c.sha).unwrap();
    assert!(p.join("c").exists());
    // Picking it again is an empty pick: git fails and the pick is aborted.
    assert!(cherry_pick(p, &c.sha).is_err());
    assert!(cherry_pick(p, "zz").is_err());
}
