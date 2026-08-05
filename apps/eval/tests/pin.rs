//! Pin behavior against a scratch Git repo: the pin must be exact, clean,
//! and refused loudly on any drift. No model, no network, no daemar repo
//! state touched — everything lives in temp directories.

use std::path::{Path, PathBuf};
use std::process::Command;

use daemar_eval::pin::{self, PinError};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.email=eval@test",
            "-c",
            "user.name=eval",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn scratch_repo(name: &str) -> (PathBuf, String) {
    let root = std::env::temp_dir().join(format!("daemar-eval-pin-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let repo = root.join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    git(&repo, &["init", "-q"]);
    std::fs::write(repo.join("src/lib.rs"), "pub fn answer() {}\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "first"]);
    let commit = git(&repo, &["rev-parse", "HEAD"]).trim().to_string();
    (root, commit)
}

#[test]
fn a_pinned_worktree_materializes_detached_at_exactly_the_commit() {
    let (root, commit) = scratch_repo("materialize");
    let cache = root.join("cache");
    let worktree = pin::materialize(&root.join("repo"), &commit, &cache).expect("materializes");
    assert!(worktree.join("src/lib.rs").exists());
    let head = git(&worktree, &["rev-parse", "HEAD"]);
    assert_eq!(head.trim(), commit);
    // Idempotent: a second call reuses the verified checkout.
    let again = pin::materialize(&root.join("repo"), &commit, &cache).expect("reuses");
    assert_eq!(worktree, again);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn short_specs_and_unknown_commits_are_refused_before_any_checkout() {
    let (root, commit) = scratch_repo("short");
    let cache = root.join("cache");
    let short = &commit[..12];
    match pin::materialize(&root.join("repo"), short, &cache) {
        Err(PinError::Git { .. }) | Err(PinError::NotACommit { .. }) => {}
        other => panic!("short spec must be refused, got {other:?}"),
    }
    let unknown = "1".repeat(40);
    assert!(pin::materialize(&root.join("repo"), &unknown, &cache).is_err());
    assert!(!cache.exists() || cache.read_dir().unwrap().next().is_none());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_dirty_pinned_worktree_is_refused_not_tolerated() {
    let (root, commit) = scratch_repo("dirty");
    let cache = root.join("cache");
    let worktree = pin::materialize(&root.join("repo"), &commit, &cache).expect("materializes");
    std::fs::write(worktree.join("src/lib.rs"), "tampered\n").unwrap();
    match pin::materialize(&root.join("repo"), &commit, &cache) {
        Err(PinError::Dirty { .. }) => {}
        other => panic!("dirty worktree must be refused, got {other:?}"),
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn pinned_line_reads_from_the_object_store_without_a_worktree() {
    let (root, commit) = scratch_repo("show");
    let line =
        pin::pinned_line(&root.join("repo"), &commit, "src/lib.rs", 1).expect("git show works");
    assert_eq!(line.as_deref(), Some("pub fn answer() {}"));
    let missing =
        pin::pinned_line(&root.join("repo"), &commit, "src/lib.rs", 99).expect("in-repo path");
    assert_eq!(missing, None);
    std::fs::remove_dir_all(&root).ok();
}
