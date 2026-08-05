//! Territory pinning: materialize an immutable Git commit as a real
//! directory the UNCHANGED production tools can canonicalize and confine.
//!
//! The commit object is the pin. The runner refuses drift — wrong HEAD,
//! dirty checkout, short spec — instead of tolerating it: no current
//! live-tree line number is ever an answer key.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use factory::tools::content_hash;

#[derive(Debug)]
pub enum PinError {
    Io {
        path: PathBuf,
        detail: String,
    },
    Git {
        detail: String,
    },
    /// The spec did not resolve to exactly itself as a commit — short SHAs
    /// and refs are refused so the fixture stays unambiguous.
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

impl fmt::Display for PinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PinError::Io { path, detail } => write!(f, "pin io at {}: {detail}", path.display()),
            PinError::Git { detail } => write!(f, "git: {detail}"),
            PinError::NotACommit { spec, detail } => {
                write!(
                    f,
                    "'{spec}' is not a full commit SHA in this repo: {detail}"
                )
            }
            PinError::WrongHead {
                worktree,
                expected,
                found,
            } => write!(
                f,
                "pinned worktree {} is at {found}, expected {expected} — remove it and re-run",
                worktree.display()
            ),
            PinError::Dirty { worktree, status } => write!(
                f,
                "pinned worktree {} is dirty — remove it and re-run:\n{status}",
                worktree.display()
            ),
        }
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<String, PinError> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| PinError::Git {
            detail: format!("could not run git: {e}"),
        })?;
    if !out.status.success() {
        return Err(PinError::Git {
            detail: format!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Materialize `commit` from `repo` as a detached worktree under
/// `cache_root`, verified clean and at exactly the pinned SHA. Reuses an
/// existing checkout when it verifies; refuses it loudly when it does not.
pub fn materialize(repo: &Path, commit: &str, cache_root: &Path) -> Result<PathBuf, PinError> {
    let repo = repo.canonicalize().map_err(|e| PinError::Io {
        path: repo.to_path_buf(),
        detail: e.to_string(),
    })?;
    let resolved = git(
        &repo,
        &["rev-parse", "--verify", &format!("{commit}^{{commit}}")],
    )?;
    if resolved.trim() != commit {
        return Err(PinError::NotACommit {
            spec: commit.to_string(),
            detail: format!("resolves to {}", resolved.trim()),
        });
    }

    let dest = cache_root
        .join(content_hash(repo.display().to_string().as_bytes()))
        .join(commit);
    if !dest.exists() {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| PinError::Io {
                path: parent.to_path_buf(),
                detail: e.to_string(),
            })?;
        }
        git(
            &repo,
            &[
                "worktree",
                "add",
                "--detach",
                &dest.display().to_string(),
                commit,
            ],
        )?;
    }

    let head = git(&dest, &["rev-parse", "HEAD"])?;
    if head.trim() != commit {
        return Err(PinError::WrongHead {
            worktree: dest,
            expected: commit.to_string(),
            found: head.trim().to_string(),
        });
    }
    let status = git(&dest, &["status", "--porcelain"])?;
    if !status.trim().is_empty() {
        return Err(PinError::Dirty {
            worktree: dest,
            status: status.trim().to_string(),
        });
    }
    dest.canonicalize().map_err(|e| PinError::Io {
        path: dest,
        detail: e.to_string(),
    })
}

/// One line of a file AT the pinned commit, read from the object store
/// (`git show`), no worktree required. `None` when the line is out of range.
/// This is how the corpus test validates answer keys without materializing.
pub fn pinned_line(
    repo: &Path,
    commit: &str,
    path: &str,
    line: u64,
) -> Result<Option<String>, PinError> {
    let text = git(repo, &["show", &format!("{commit}:{path}")])?;
    let index = usize::try_from(line.saturating_sub(1)).unwrap_or(usize::MAX);
    Ok(text.lines().nth(index).map(str::to_string))
}
