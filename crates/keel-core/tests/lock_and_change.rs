#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use keel_core::git::RevKind;
use keel_core::lock::{LOCK_VERSION, LockError, LockedRepo, Lockfile};
use keel_core::manifest::Manifest;
use keel_core::resolver;
use keel_core::workspace::branch_for;

fn sample_lock() -> Lockfile {
    Lockfile {
        version: LOCK_VERSION,
        repos: vec![LockedRepo {
            name: "kernel".into(),
            url: "git@gitlab.company.com:firmware/kernel.git".into(),
            path: PathBuf::from("kernel"),
            rev: "a".repeat(40),
            source_rev: "v6.1.2".into(),
            branch: "keel/v6.1.2".into(),
            groups: vec!["firmware".into()],
        }],
    }
}

#[test]
fn lockfile_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("keel.lock");
    let lock = sample_lock();
    lock.save(&path).unwrap();
    let loaded = Lockfile::load(&path).unwrap();
    assert_eq!(lock, loaded);
    assert_eq!(loaded.get("kernel").unwrap().source_rev, "v6.1.2");
    assert!(loaded.get("ghost").is_none());
}

#[test]
fn lockfile_rejects_future_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("keel.lock");
    std::fs::write(&path, "version = 99\n").unwrap();
    assert!(matches!(
        Lockfile::load(&path),
        Err(LockError::UnsupportedVersion(99))
    ));
}

#[test]
fn branch_policy_never_detaches() {
    assert_eq!(branch_for("main", RevKind::Branch), "main");
    assert_eq!(branch_for("v6.1.2", RevKind::Tag), "keel/v6.1.2");
    assert_eq!(branch_for("release/2.x", RevKind::Tag), "keel/release-2.x");
    let sha = "b".repeat(40);
    assert_eq!(branch_for(&sha, RevKind::Sha), format!("keel/{sha}"));
}

#[test]
fn resolve_all_covers_every_repo_with_overlays() {
    let manifest: Manifest = r#"
[remote.r]
url = "git@example.com:org"

[repo.a]
remote = "r"
repo = "a.git"
rev = "main"

[repo.b]
remote = "r"
repo = "b.git"
rev = "v1"

[stack.p]
repos = ["a"]

[overlay.dev.repo.b]
rev = "main"
"#
    .parse()
    .unwrap();

    let all = resolver::resolve_all(&manifest, &[]).unwrap();
    assert_eq!(all.len(), 2, "lock covers all repos, not just stack p");

    let dev = resolver::resolve_all(&manifest, &["dev".into()]).unwrap();
    assert_eq!(dev[1].rev, "main");
}

#[test]
fn group_filter_limits_resolution() {
    let manifest: Manifest = r#"
[remote.r]
url = "git@example.com:org"

[repo.kernel]
remote = "r"
repo = "kernel.git"
rev = "main"
groups = ["firmware"]

[repo.docs]
remote = "r"
repo = "docs.git"
rev = "main"
groups = ["docs"]

[repo.tools]
remote = "r"
repo = "tools.git"
rev = "main"

[stack.all]
repos = ["kernel", "docs", "tools"]
"#
    .parse()
    .unwrap();

    let mut res = resolver::resolve(&manifest, "all", &[]).unwrap();
    resolver::filter_groups(&mut res, &["firmware".into()]);
    assert_eq!(res.repos.len(), 1);
    assert_eq!(res.repos[0].name, "kernel");

    let mut all = resolver::resolve(&manifest, "all", &[]).unwrap();
    resolver::filter_groups(&mut all, &[]);
    assert_eq!(all.repos.len(), 3, "empty filter keeps everything");

    assert!(resolver::group_match(&[], &[]));
    assert!(
        !resolver::group_match(&[], &["firmware".into()]),
        "ungrouped repos are excluded by an active group filter"
    );
}
