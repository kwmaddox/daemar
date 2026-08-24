//! The crate's failure surface: one explicit enum, hand-written `Display`
//! and `Error` impls (conventions.md C1–C3). The cause of a failure lives
//! in `source()`, never interpolated into the message text — callers and
//! chain printers decide how much of the chain to show.

use std::path::PathBuf;

/// Everything that can go wrong in a sandboxed run, from spec validation
/// through promotion. Variants are the failure contract: callers branch on
/// structure, never on message text.
#[derive(Debug)]
pub enum Error {
    /// The requested worktree does not exist or is not a directory. The io
    /// cause of the failed canonicalization, when there is one, says which
    /// (not-found vs permission-denied); the `is_dir` rejection has none.
    BadWorktree {
        /// The worktree path as the caller supplied it (pre-canonicalization).
        path: PathBuf,
        /// Why canonicalization failed; `None` when the path resolved but
        /// was not a directory.
        source: Option<std::io::Error>,
    },
    /// `RunSpec::command` was empty — there is nothing to run.
    EmptyCommand,
    /// Could not prepare the per-run session directory on the host.
    Session {
        /// The session path that could not be created or written.
        path: PathBuf,
        /// The underlying filesystem failure.
        source: std::io::Error,
    },
    /// Could not spawn the `container` CLI at all.
    Spawn(std::io::Error),
    /// The run exceeded its wall-clock limit and was killed (behavior B8).
    Timeout(std::time::Duration),
    /// The in-guest driver failed before (or while exporting after) the
    /// workload; the workload itself never reported an exit code.
    Driver {
        /// Which driver stage failed: "mkdir", "overlay mount", "cd",
        /// "export", or "container" for container-level failures.
        stage: &'static str,
        /// The container process exit code, when it exited at all.
        code: Option<i32>,
    },
    /// Could not read the run's results back from the session mounts.
    Results(std::io::Error),
    /// The exported change archive was malformed or unreadable.
    Archive(String),
    /// An io failure during a named host-side operation (e.g. "compare").
    Io(&'static str, std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::BadWorktree { path, .. } => write!(
                f,
                "worktree {} does not exist or is not a directory",
                path.display()
            ),
            Error::EmptyCommand => write!(f, "command must not be empty"),
            Error::Session { path, .. } => {
                write!(f, "failed to prepare session directory {}", path.display())
            }
            Error::Spawn(_) => write!(
                f,
                "failed to spawn `container` (is the container CLI installed and running?)"
            ),
            Error::Timeout(limit) => {
                write!(f, "run exceeded timeout of {limit:?} and was killed")
            }
            Error::Driver { stage, code } => write!(
                f,
                "driver failed before the command ran ({stage}); container exit code {code:?}"
            ),
            Error::Results(_) => write!(f, "failed to read run results from session"),
            Error::Archive(what) => write!(f, "malformed change archive: {what}"),
            Error::Io(during, _) => write!(f, "io error during {during}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Session { source, .. }
            | Error::Spawn(source)
            | Error::Results(source)
            | Error::Io(_, source)
            | Error::BadWorktree {
                source: Some(source),
                ..
            } => Some(source),
            Error::BadWorktree { source: None, .. }
            | Error::EmptyCommand
            | Error::Timeout(_)
            | Error::Driver { .. }
            | Error::Archive(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as _;
    use std::io;
    use std::time::Duration;

    fn io_err() -> io::Error {
        io::Error::new(io::ErrorKind::PermissionDenied, "permission denied")
    }

    /// Locks the canonical Display text of every variant: the message
    /// describes the failure and never interpolates the cause (C2/C3).
    #[test]
    fn display_text_is_locked() {
        let cases: Vec<(Error, &str)> = vec![
            (
                Error::BadWorktree {
                    path: PathBuf::from("/w"),
                    source: Some(io_err()),
                },
                "worktree /w does not exist or is not a directory",
            ),
            (Error::EmptyCommand, "command must not be empty"),
            (
                Error::Session {
                    path: PathBuf::from("/s"),
                    source: io_err(),
                },
                "failed to prepare session directory /s",
            ),
            (
                Error::Spawn(io_err()),
                "failed to spawn `container` (is the container CLI installed and running?)",
            ),
            (
                Error::Timeout(Duration::from_secs(5)),
                "run exceeded timeout of 5s and was killed",
            ),
            (
                Error::Driver {
                    stage: "overlay mount",
                    code: Some(125),
                },
                "driver failed before the command ran (overlay mount); container exit code Some(125)",
            ),
            (
                Error::Results(io_err()),
                "failed to read run results from session",
            ),
            (
                Error::Archive("truncated header".into()),
                "malformed change archive: truncated header",
            ),
            (Error::Io("compare", io_err()), "io error during compare"),
        ];
        for (err, want) in cases {
            assert_eq!(err.to_string(), want);
        }
    }

    #[test]
    fn causes_live_in_the_source_chain() {
        let with_cause = [
            Error::Session {
                path: PathBuf::from("/s"),
                source: io_err(),
            },
            Error::Spawn(io_err()),
            Error::Results(io_err()),
            Error::Io("compare", io_err()),
            Error::BadWorktree {
                path: PathBuf::from("/w"),
                source: Some(io_err()),
            },
        ];
        for err in &with_cause {
            assert!(err.source().is_some(), "missing source: {err}");
        }

        let without_cause = [
            Error::BadWorktree {
                path: PathBuf::from("/w"),
                source: None,
            },
            Error::EmptyCommand,
            Error::Timeout(Duration::from_secs(1)),
            Error::Driver {
                stage: "cd",
                code: None,
            },
            Error::Archive(String::new()),
        ];
        for err in &without_cause {
            assert!(err.source().is_none(), "unexpected source: {err}");
        }
    }
}
