//! The crate's failure surface: one explicit enum, hand-written `Display`
//! and `Error` impls (conventions.md C1–C3). The cause of a failure lives
//! in `source()`, never interpolated into the message text — callers and
//! chain printers decide how much of the chain to show.

use std::path::PathBuf;

/// Which stage of the in-guest driver failed. `Display` renders the short
/// stage label ("mkdir", "overlay mount", "cd", "export", "container")
/// used in diagnostics; failure *timing* is worded by [`Error::Driver`]'s
/// `Display` arms, which distinguish pre-workload stages from the
/// post-workload export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverStage {
    /// `mkdir -p` of the overlay dirs failed (pre-workload).
    Mkdir,
    /// The overlay mount failed (pre-workload).
    OverlayMount,
    /// `cd` into the merged overlay failed (pre-workload).
    Cd,
    /// Export of the change tar failed — *after* the workload ran; its
    /// exit code was already recorded and is carried here.
    Export {
        /// The workload's own exit code, read back before the export
        /// failure was discovered.
        workload_exit: i32,
    },
    /// Container-level failure (bad image, runtime error); whether the
    /// workload ran is unknown.
    Container,
}

impl std::fmt::Display for DriverStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            DriverStage::Mkdir => "mkdir",
            DriverStage::OverlayMount => "overlay mount",
            DriverStage::Cd => "cd",
            DriverStage::Export { .. } => "export",
            DriverStage::Container => "container",
        })
    }
}

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
    /// The in-guest driver failed; `stage` says where. The pre-workload
    /// stages mean the workload never ran; [`DriverStage::Export`] means
    /// it ran and its exit code is carried in the stage.
    Driver {
        /// Which driver stage failed.
        stage: DriverStage,
        /// The container process exit code, when it exited at all.
        code: Option<i32>,
    },
    /// Could not read the run's results back from the session mounts.
    Results(std::io::Error),
    /// The exported change archive was malformed; the tar error is the
    /// source.
    Archive(std::io::Error),
    /// The driver's exit-code file held something other than an integer.
    ExitCode {
        /// Raw file content (parser input; no structured form exists).
        raw: String,
    },
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
            Error::Driver {
                stage: DriverStage::Export { workload_exit },
                code,
            } => write!(
                f,
                "driver failed to export changes after the command ran (workload exit \
                 code {workload_exit}); container exit code {code:?}"
            ),
            Error::Driver {
                stage: DriverStage::Container,
                code,
            } => write!(
                f,
                "container failed (bad image or runtime error); container exit code {code:?}"
            ),
            Error::Driver { stage, code } => write!(
                f,
                "driver failed before the command ran ({stage}); container exit code {code:?}"
            ),
            Error::Results(_) => write!(f, "failed to read run results from session"),
            Error::Archive(_) => write!(f, "malformed change archive"),
            Error::ExitCode { raw } => write!(f, "unparseable workload exit code {raw:?}"),
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
            | Error::Archive(source)
            | Error::BadWorktree {
                source: Some(source),
                ..
            } => Some(source),
            Error::BadWorktree { source: None, .. }
            | Error::EmptyCommand
            | Error::Timeout(_)
            | Error::Driver { .. }
            | Error::ExitCode { .. } => None,
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
            // PER-76: each Driver timing class renders its own truthful
            // wording — pre-run stages, post-run export (with the workload's
            // recorded exit code), and timing-unknown container failures.
            (
                Error::Driver {
                    stage: DriverStage::Mkdir,
                    code: Some(124),
                },
                "driver failed before the command ran (mkdir); container exit code Some(124)",
            ),
            (
                Error::Driver {
                    stage: DriverStage::OverlayMount,
                    code: Some(125),
                },
                "driver failed before the command ran (overlay mount); container exit code Some(125)",
            ),
            (
                Error::Driver {
                    stage: DriverStage::Export { workload_exit: 0 },
                    code: Some(127),
                },
                "driver failed to export changes after the command ran (workload exit code 0); \
                 container exit code Some(127)",
            ),
            (
                Error::Driver {
                    stage: DriverStage::Container,
                    code: Some(1),
                },
                "container failed (bad image or runtime error); container exit code Some(1)",
            ),
            (
                Error::Results(io_err()),
                "failed to read run results from session",
            ),
            (Error::Archive(io_err()), "malformed change archive"),
            (
                Error::ExitCode {
                    raw: "abc\n".into(),
                },
                "unparseable workload exit code \"abc\\n\"",
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
            Error::Archive(io_err()),
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
                stage: DriverStage::Cd,
                code: None,
            },
            Error::Driver {
                stage: DriverStage::Export { workload_exit: 3 },
                code: Some(127),
            },
            Error::ExitCode { raw: String::new() },
        ];
        for err in &without_cause {
            assert!(err.source().is_none(), "unexpected source: {err}");
        }
    }
}
