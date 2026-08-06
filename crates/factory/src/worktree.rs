//! Pinned worktrees: a tooled stage never flies over the live checkout.
//!
//! The territory's HEAD is resolved before the slip is minted (a territory
//! that cannot pin is a refusal, not a flight); the detached worktree is
//! materialized at stage start and recorded on the ledger. The worktree
//! outlives the stage — it is the stage's auditable, frozen territory and
//! the future write workspace. Only the sandbox around it is ephemeral.
//!
//! Promoted from the eval rig's pin (apps/eval), which now delegates here:
//! the eval suite and the factory pin territory with one implementation.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug)]
pub enum WorktreeError {
    Io {
        path: PathBuf,
        detail: String,
    },
    Git {
        detail: String,
    },
    /// The spec did not resolve to a commit — non-repo territory, unborn
    /// HEAD, or a bad ref.
    NotACommit {
        spec: String,
        detail: String,
    },
    WrongHead {
        worktree: PathBuf,
        expected: String,
        found: String,
    },
    Dirty {
        worktree: PathBuf,
        status: String,
    },
}

impl fmt::Display for WorktreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorktreeError::Io { path, detail } => {
                write!(f, "worktree io at {}: {detail}", path.display())
            }
            WorktreeError::Git { detail } => write!(f, "git: {detail}"),
            WorktreeError::NotACommit { spec, detail } => {
                write!(f, "'{spec}' does not resolve to a commit: {detail}")
            }
            WorktreeError::WrongHead {
                worktree,
                expected,
                found,
            } => write!(
                f,
                "worktree {} is at {found}, expected {expected}",
                worktree.display()
            ),
            WorktreeError::Dirty { worktree, status } => {
                write!(f, "worktree {} is dirty:\n{status}", worktree.display())
            }
        }
    }
}

impl std::error::Error for WorktreeError {}

/// How one git invocation went: spawn failures and nonzero exits are
/// different facts — a missing git binary must never masquerade as a bad
/// commit. (The taxonomy fix from PR #23's review, which its commit message
/// claimed but a failed edit script silently dropped — landed for real here.)
enum GitRun {
    Ok(String),
    /// git ran and said no; stderr says why.
    Refused(String),
}

/// Every git the factory runs starts here: global and system config are
/// silenced (no surprise diff drivers, filters, or hooksPath under a
/// code-owned receipt), and ambient GIT_* overrides cannot redirect
/// `-C dir` somewhere else. The factory's git behaves identically on
/// every machine. Commit identity, when a caller needs one, is passed
/// explicitly via `-c`.
fn git_command(dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .arg("-C")
        .arg(dir);
    cmd
}

fn run_git_raw(dir: &Path, args: &[&str]) -> Result<GitRun, WorktreeError> {
    let out = git_command(dir)
        .args(args)
        .output()
        .map_err(|e| WorktreeError::Git {
            detail: format!("could not run git: {e}"),
        })?;
    if !out.status.success() {
        return Ok(GitRun::Refused(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(GitRun::Ok(String::from_utf8_lossy(&out.stdout).to_string()))
}

/// Run git in a directory, capturing stdout; a nonzero exit is an error
/// carrying stderr. The one place the factory shells to git.
pub fn run_git(dir: &Path, args: &[&str]) -> Result<String, WorktreeError> {
    match run_git_raw(dir, args)? {
        GitRun::Ok(stdout) => Ok(stdout),
        GitRun::Refused(detail) => Err(WorktreeError::Git { detail }),
    }
}

/// Resolve a spec to a full commit SHA in `repo`. `head(repo)` is the
/// pre-mint validation: a territory whose HEAD cannot resolve refuses the
/// flight before a slip exists.
pub fn resolve_commit(repo: &Path, spec: &str) -> Result<String, WorktreeError> {
    // Only an actual rev-parse rejection is NotACommit; a git that could
    // not even run stays a Git error — the diagnosis names the disease.
    match run_git_raw(
        repo,
        &["rev-parse", "--verify", &format!("{spec}^{{commit}}")],
    )? {
        GitRun::Ok(out) => Ok(out.trim().to_string()),
        GitRun::Refused(detail) => Err(WorktreeError::NotACommit {
            spec: spec.to_string(),
            detail,
        }),
    }
}

pub fn head(repo: &Path) -> Result<String, WorktreeError> {
    resolve_commit(repo, "HEAD")
}

/// Materialize `commit` from `repo` as a fresh detached worktree at `dest`,
/// verified at exactly the pinned SHA and clean. `dest` must not exist —
/// a stage's worktree is born exactly once.
pub fn add_detached(repo: &Path, commit: &str, dest: &Path) -> Result<PathBuf, WorktreeError> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| WorktreeError::Io {
            path: parent.to_path_buf(),
            detail: e.to_string(),
        })?;
    }
    run_git(
        repo,
        &[
            "worktree",
            "add",
            "--detach",
            &dest.display().to_string(),
            commit,
        ],
    )?;
    verify(dest, commit)?;
    dest.canonicalize().map_err(|e| WorktreeError::Io {
        path: dest.to_path_buf(),
        detail: e.to_string(),
    })
}

/// The code-owned diff: everything the builder changed in the worktree,
/// relative to its pinned base — including brand-new files, which are
/// marked intent-to-add first (forced, so even an ignored path cannot hide
/// a mutation from the controller's review). The worktree is left intact:
/// it IS the artifact awaiting apply.
pub fn diff_against_base(worktree: &Path, base: &str) -> Result<String, WorktreeError> {
    run_git(worktree, &["add", "-A", "-N", "-f"])?;
    run_git(worktree, &["diff", "--binary", "--no-ext-diff", base])
}

/// Is the repository dirty? git status --porcelain, ignored files excluded
/// by git's own default — the dirty-transfer policy's precise predicate.
pub fn is_dirty(repo: &Path) -> Result<bool, WorktreeError> {
    Ok(!run_git(repo, &["status", "--porcelain"])?.trim().is_empty())
}

/// Commit everything in the (detached) worktree as the factory. The
/// worktree sits at the stamped base, so the commit's parent IS the base
/// by construction. Author identity ruling: the factory authors; the
/// granting engineer is named in the message — clearing a boundary is not
/// authorship.
pub fn commit_all(worktree: &Path, message: &str) -> Result<String, WorktreeError> {
    run_git(worktree, &["add", "-A", "-f"])?;
    run_git(
        worktree,
        &[
            "-c",
            "user.name=daemar",
            "-c",
            "user.email=daemar@localhost",
            "commit",
            "-q",
            "-m",
            message,
        ],
    )?;
    Ok(run_git(worktree, &["rev-parse", "HEAD"])?
        .trim()
        .to_string())
}

/// Fast-forward the repository onto `commit`. Git enforces the staleness
/// check atomically: this succeeds only if HEAD is the commit's ancestor.
pub fn merge_ff_only(repo: &Path, commit: &str) -> Result<(), WorktreeError> {
    run_git(repo, &["merge", "--ff-only", commit]).map(|_| ())
}

/// Is `commit` reachable from the repository's HEAD? The land leg's first
/// question: the factory cares that the change is in, not who landed it.
pub fn is_ancestor(repo: &Path, commit: &str) -> Result<bool, WorktreeError> {
    // Bypasses run_git_raw for the exit-code taxonomy (0 = yes, 1 = no),
    // but never the scrubbed command builder: the land leg's reachability
    // verdict must be as deterministic as every other gate.
    let out = git_command(repo)
        .args(["merge-base", "--is-ancestor", commit, "HEAD"])
        .output()
        .map_err(|e| WorktreeError::Git {
            detail: format!("could not run git: {e}"),
        })?;
    match out.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(WorktreeError::Git {
            detail: format!(
                "git merge-base --is-ancestor failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        }),
    }
}

/// Where a branch points, if it exists. Pure ref inspection.
pub fn branch_target(repo: &Path, name: &str) -> Result<Option<String>, WorktreeError> {
    match run_git_raw(
        repo,
        &["rev-parse", "--verify", &format!("refs/heads/{name}")],
    )? {
        GitRun::Ok(out) => Ok(Some(out.trim().to_string())),
        GitRun::Refused(_) => Ok(None),
    }
}

/// Park a commit on a branch: a pure ref creation, ZERO working-tree
/// interaction — the safe half of apply under any dirt.
pub fn branch_create(repo: &Path, name: &str, commit: &str) -> Result<(), WorktreeError> {
    run_git(repo, &["branch", name, commit]).map(|_| ())
}

/// Remove a retained worktree once its commit is safe in history. Failure
/// is the caller's to witness — cleanup is proven, never assumed.
pub fn remove(repo: &Path, worktree: &Path) -> Result<(), WorktreeError> {
    run_git(
        repo,
        &[
            "worktree",
            "remove",
            "--force",
            &worktree.display().to_string(),
        ],
    )
    .map(|_| ())
}

/// A worktree vouches for itself: exactly the pinned commit, nothing dirty.
pub fn verify(worktree: &Path, commit: &str) -> Result<(), WorktreeError> {
    let found = run_git(worktree, &["rev-parse", "HEAD"])?;
    if found.trim() != commit {
        return Err(WorktreeError::WrongHead {
            worktree: worktree.to_path_buf(),
            expected: commit.to_string(),
            found: found.trim().to_string(),
        });
    }
    let status = run_git(worktree, &["status", "--porcelain"])?;
    if !status.trim().is_empty() {
        return Err(WorktreeError::Dirty {
            worktree: worktree.to_path_buf(),
            status: status.trim().to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_seed(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.email=wt@test",
                "-c",
                "user.name=wt",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn scratch(name: &str) -> (PathBuf, String) {
        let root =
            std::env::temp_dir().join(format!("daemar-worktree-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let repo = root.join("repo");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        git_seed(&repo, &["init", "-q"]);
        std::fs::write(repo.join("src/lib.rs"), "pub fn answer() {}\n").unwrap();
        git_seed(&repo, &["add", "."]);
        git_seed(&repo, &["commit", "-q", "-m", "first"]);
        let commit = head(&repo).expect("head resolves");
        (root, commit)
    }

    #[test]
    fn a_stage_worktree_is_detached_at_head_and_born_exactly_once() {
        let (root, commit) = scratch("born-once");
        let dest = root.join("wt/slip-1/scout");
        let wt = add_detached(&root.join("repo"), &commit, &dest).expect("materializes");
        assert!(wt.join("src/lib.rs").exists());
        verify(&wt, &commit).expect("verifies");
        assert!(
            add_detached(&root.join("repo"), &commit, &dest).is_err(),
            "a second materialization at the same dest must refuse"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_territory_that_cannot_pin_refuses_before_any_flight() {
        let dir =
            std::env::temp_dir().join(format!("daemar-worktree-nogit-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        assert!(
            matches!(head(&dir), Err(WorktreeError::NotACommit { .. })),
            "a non-repo territory is not pinnable"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_diff_sees_edits_and_brand_new_files_alike() {
        let (root, commit) = scratch("diff");
        let dest = root.join("wt/slip-3/build");
        let wt = add_detached(&root.join("repo"), &commit, &dest).expect("materializes");
        std::fs::write(wt.join("src/lib.rs"), "pub fn answer() -> u8 { 43 }\n").unwrap();
        std::fs::write(wt.join("src/new_file.rs"), "pub fn fresh() {}\n").unwrap();
        let patch = diff_against_base(&wt, &commit).expect("diffs");
        assert!(
            patch.contains("-pub fn answer() {}") || patch.contains("43"),
            "{patch}"
        );
        assert!(
            patch.contains("new_file.rs"),
            "new files must not hide: {patch}"
        );
        let clean_wt = add_detached(&root.join("repo"), &commit, &root.join("wt/slip-4/build"))
            .expect("materializes");
        let empty = diff_against_base(&clean_wt, &commit).expect("diffs");
        assert!(empty.trim().is_empty(), "an untouched worktree diffs empty");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_tampered_worktree_fails_verification() {
        let (root, commit) = scratch("tampered");
        let dest = root.join("wt/slip-2/scout");
        let wt = add_detached(&root.join("repo"), &commit, &dest).expect("materializes");
        std::fs::write(wt.join("src/lib.rs"), "tampered\n").unwrap();
        assert!(matches!(
            verify(&wt, &commit),
            Err(WorktreeError::Dirty { .. })
        ));
        std::fs::remove_dir_all(&root).ok();
    }
}
