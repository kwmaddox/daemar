use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("worktree {0} does not exist or is not a directory")]
    BadWorktree(PathBuf),

    #[error("command must not be empty")]
    EmptyCommand,

    #[error("failed to prepare session directory {path}: {source}")]
    Session {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to spawn `container`: {0} (is the container CLI installed and running?)")]
    Spawn(std::io::Error),

    #[error("run exceeded timeout of {0:?} and was killed")]
    Timeout(std::time::Duration),

    #[error("driver failed before the command ran ({stage}); container exit code {code:?}")]
    Driver { stage: &'static str, code: Option<i32> },

    #[error("failed to read run results from session: {0}")]
    Results(std::io::Error),

    #[error("malformed change archive: {0}")]
    Archive(String),

    #[error("io error during {0}: {1}")]
    Io(&'static str, std::io::Error),
}
