//! Territory pinning for eval cases: fixture-facing rules (full 40-hex SHA
//! only, content-addressed cache, reuse-with-verification) layered over the
//! factory's worktree machinery — one materialization implementation for
//! the whole tower, promoted from this module into `factory::worktree`.

use std::fmt;
use std::path::{Path, PathBuf};

use factory::tools::content_hash;
use factory::worktree::{self, WorktreeError};

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

impl From<WorktreeError> for PinError {
    fn from(error: WorktreeError) -> Self {
        match error {
            WorktreeError::Io { path, detail } => PinError::Io { path, detail },
            WorktreeError::Git { detail } => PinError::Git { detail },
            WorktreeError::NotACommit { spec, detail } => PinError::NotACommit { spec, detail },
            WorktreeError::WrongHead {
                worktree,
                expected,
                found,
            } => PinError::WrongHead {
                worktree,
                expected,
                found,
            },
            WorktreeError::Dirty { worktree, status } => PinError::Dirty { worktree, status },
        }
    }
}

/// Materialize `commit` from `repo` as a detached worktree under
/// `cache_root`, verified clean and at exactly the pinned SHA. Reuses an
/// existing checkout when it verifies; refuses it loudly when it does not.
pub fn materialize(repo: &Path, commit: &str, cache_root: &Path) -> Result<PathBuf, PinError> {
    let repo = repo.canonicalize().map_err(|e| PinError::Io {
        path: repo.to_path_buf(),
        detail: e.to_string(),
    })?;
    let resolved = worktree::resolve_commit(&repo, commit)?;
    if resolved != commit {
        return Err(PinError::NotACommit {
            spec: commit.to_string(),
            detail: format!("resolves to {resolved}"),
        });
    }

    let dest = cache_root
        .join(content_hash(repo.display().to_string().as_bytes()))
        .join(commit);
    if !dest.exists() {
        return Ok(worktree::add_detached(&repo, commit, &dest)?);
    }
    worktree::verify(&dest, commit)?;
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
    let text = worktree::run_git(repo, &["show", &format!("{commit}:{path}")])?;
    let index = usize::try_from(line.saturating_sub(1)).unwrap_or(usize::MAX);
    Ok(text.lines().nth(index).map(str::to_string))
}
