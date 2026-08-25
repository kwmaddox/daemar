//! The one-shot run: spawn `container` as a supervised subprocess, wait with
//! a timeout, reap unconditionally, collect results from the session mounts.
//!
//! Subprocess supervision (not in-process VMM linkage) is a deliberate
//! security posture: the factory process is the trusted context, and the VMM
//! stays outside it. See `docs/research/substrate-refutation.md`.

use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::changeset::{ChangeSet, SessionGuard};
use crate::driver;
use crate::error::{DriverStage, Error};
use crate::{DEFAULT_IMAGE, TESTED_CONTAINER_VERSION};

/// A container image reference, passed verbatim to `container run`. The
/// image is a toolset, not a boundary (C7's type distinction is the
/// point); the container CLI validates the reference at run time, and a
/// bad one surfaces as a [`DriverStage::Container`] failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef(String);

impl ImageRef {
    /// The reference exactly as it will be passed to `container run`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for ImageRef {
    fn from(reference: String) -> Self {
        ImageRef(reference)
    }
}

impl Default for ImageRef {
    /// The digest-pinned default image ([`DEFAULT_IMAGE`]).
    fn default() -> Self {
        ImageRef(DEFAULT_IMAGE.to_string())
    }
}

impl std::fmt::Display for ImageRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What to run and where. `command` is argv — it is passed to the in-guest
/// driver as script arguments, never spliced into shell text.
#[derive(Debug, Clone)]
pub struct RunSpec {
    /// Argv of the workload. Passed to the in-guest driver as script
    /// arguments, never spliced into shell text; must not be empty.
    pub command: Vec<String>,
    /// Host directory mounted read-only as the overlay lower layer (B3, B4).
    pub worktree: PathBuf,
    /// Guest image. The image is a toolset, not a boundary; default is
    /// digest-pinned ubuntu ([`DEFAULT_IMAGE`]).
    pub image: ImageRef,
    /// Wall-clock limit; the container is killed and reaped on expiry (B8).
    pub timeout: Duration,
}

/// Default wall-clock limit for a run ([`RunSpec::timeout`]); pub so the
/// CLI's `--timeout` default derives from it and the value exists exactly
/// once (C14). The one defaults-const here; if a second accretes, fold
/// them into a config struct instead.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_mins(5);

impl RunSpec {
    /// A spec with the defaults: pinned image, default timeout.
    #[must_use]
    pub fn new(command: Vec<String>, worktree: impl Into<PathBuf>) -> Self {
        RunSpec {
            command,
            worktree: worktree.into(),
            image: ImageRef::default(),
            timeout: DEFAULT_TIMEOUT,
        }
    }
}

/// What one sandboxed run produced: the workload's own exit code, its
/// captured output, and the change-set the overlay recorded.
#[derive(Debug)]
pub struct RunOutcome {
    /// The workload's exit code (B10) — not the container's.
    pub exit_code: i32,
    /// Everything the workload wrote to stdout.
    pub stdout: Vec<u8>,
    /// Everything the workload wrote to stderr.
    pub stderr: Vec<u8>,
    /// What the run changed; reaches the host only via
    /// [`ChangeSet::apply_to`].
    pub changes: ChangeSet,
}

/// Execute one sandboxed run: create session, boot VM, run, collect, reap.
///
/// # Errors
///
/// [`Error::EmptyCommand`] / [`Error::BadWorktree`] for an invalid spec;
/// [`Error::Session`] / [`Error::Spawn`] when the host side cannot be set
/// up; [`Error::Timeout`] when the wall-clock limit expires (B8);
/// [`Error::Driver`] when the in-guest driver fails — before the workload
/// for the setup stages, after it for [`DriverStage::Export`];
/// [`Error::Results`] / [`Error::Archive`] / [`Error::ExitCode`] when the
/// results cannot be read back.
pub fn run(spec: &RunSpec) -> Result<RunOutcome, Error> {
    if spec.command.is_empty() {
        return Err(Error::EmptyCommand);
    }
    let worktree = match spec.worktree.canonicalize() {
        Ok(worktree) => worktree,
        Err(source) => {
            return Err(Error::BadWorktree {
                path: spec.worktree.clone(),
                source: Some(source),
            })
        }
    };
    if !worktree.is_dir() {
        return Err(Error::BadWorktree {
            path: spec.worktree.clone(),
            source: None,
        });
    }
    warn_on_version_drift();

    let run_id = uuid::Uuid::new_v4().to_string();
    let session = std::env::temp_dir().join("daemar-sandbox").join(&run_id);
    let bin_dir = session.join("bin");
    let out_dir = session.join("out");
    for d in [&bin_dir, &out_dir] {
        fs::create_dir_all(d).map_err(|e| Error::Session {
            path: d.clone(),
            source: e,
        })?;
    }
    // From here on, the guard owns cleanup on every path (B9).
    let guard = SessionGuard(session);
    fs::write(
        bin_dir.join(driver::DRIVER_FILENAME),
        driver::driver_script(),
    )
    .map_err(|e| Error::Session {
        path: bin_dir.clone(),
        source: e,
    })?;

    let name = format!("daemar-{run_id}");
    let mut cmd = container_command(spec, &worktree, &name, &bin_dir, &out_dir);
    let mut child = cmd.spawn().map_err(Error::Spawn)?;
    let stdout_reader = child.stdout.take().map(drain);
    let stderr_reader = child.stderr.take().map(drain);

    let Some(status) = wait_with_timeout(&mut child, spec.timeout) else {
        let _ = child.kill();
        let _ = child.wait();
        reap(&name);
        drop(guard);
        return Err(Error::Timeout(spec.timeout));
    };
    let stdout = join_drained(stdout_reader);
    let stderr = join_drained(stderr_reader);

    let exit_code_file = out_dir.join(driver::EXIT_CODE_FILE);
    let tar_path = out_dir.join(driver::CHANGES_TAR);
    let Ok(raw) = fs::read_to_string(&exit_code_file) else {
        // No exit code recorded: a driver setup stage failed before the
        // workload, or the container itself failed (timing unknown).
        reap(&name);
        return Err(driver_failure(status, &stderr));
    };
    let exit_code: i32 = match raw.trim().parse() {
        Ok(code) => code,
        // The raw file content is the whole story; a ParseIntError
        // carries nothing beyond it.
        Err(_) => return Err(Error::ExitCode { raw }),
    };
    if !tar_path.is_file() {
        reap(&name);
        return Err(Error::Driver {
            stage: DriverStage::Export {
                workload_exit: exit_code,
            },
            code: status.code(),
        });
    }
    let changes = ChangeSet::from_tar(tar_path, &worktree, guard)?;
    Ok(RunOutcome {
        exit_code,
        stdout,
        stderr,
        changes,
    })
}

/// Assemble the full `container run` invocation for one spec.
fn container_command(
    spec: &RunSpec,
    worktree: &std::path::Path,
    name: &str,
    bin_dir: &std::path::Path,
    out_dir: &std::path::Path,
) -> Command {
    let mut cmd = Command::new("container");
    cmd.arg("run")
        .arg("--rm")
        .arg("--progress")
        .arg("none")
        .arg("--network")
        .arg("none")
        .arg("--cap-add")
        .arg("CAP_SYS_ADMIN")
        .arg("--tmpfs")
        .arg(driver::GUEST_OVL)
        .arg("--name")
        .arg(name)
        .arg("--mount")
        .arg(mount_arg(worktree, driver::GUEST_LOWER, Mount::ReadOnly))
        .arg("--mount")
        .arg(mount_arg(out_dir, driver::GUEST_OUT, Mount::ReadWrite))
        .arg("--mount")
        .arg(mount_arg(bin_dir, driver::GUEST_BIN, Mount::ReadOnly));
    cmd.arg(spec.image.as_str())
        .arg("sh")
        .arg(format!("{}/{}", driver::GUEST_BIN, driver::DRIVER_FILENAME))
        .args(&spec.command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

/// Classify a container exit that produced no workload exit code: the
/// driver failed. Stage from the driver's exit-code protocol; anything
/// else is a container-level failure (bad image, runtime error) — stderr
/// has the story.
fn driver_failure(status: std::process::ExitStatus, stderr: &[u8]) -> Error {
    let stage = match status.code() {
        Some(driver::EXIT_MKDIR) => DriverStage::Mkdir,
        Some(driver::EXIT_OVERLAY) => DriverStage::OverlayMount,
        Some(driver::EXIT_CD) => DriverStage::Cd,
        _ => DriverStage::Container,
    };
    let tail = String::from_utf8_lossy(stderr);
    let mut last_lines = tail
        .lines()
        .rev()
        .take(STDERR_TAIL_LINES)
        .collect::<Vec<_>>();
    last_lines.reverse();
    eprintln!(
        "daemar-sandbox: driver failed at {stage}: {}",
        last_lines.join(" | ")
    );
    Error::Driver {
        stage,
        code: status.code(),
    }
}

/// Collect a drained stream. `None` means the pipe was never there (the
/// child was spawned with piped stdio, so this is unreachable in practice);
/// either way the answer is "no output".
fn join_drained(handle: Option<std::thread::JoinHandle<Vec<u8>>>) -> Vec<u8> {
    handle.map_or_else(Vec::new, |h| h.join().unwrap_or_default())
}

/// Guest mount posture. The read-only lower mount is a security property
/// (B3), so call sites must say which posture they mean (C15) — a
/// transposed bare `true` would compile; `Mount::ReadWrite` in the lower
/// slot cannot pass review silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mount {
    ReadOnly,
    ReadWrite,
}

fn mount_arg(source: &std::path::Path, target: &str, posture: Mount) -> String {
    let mut arg = format!("type=bind,source={},target={}", source.display(), target);
    if posture == Mount::ReadOnly {
        arg.push_str(",readonly");
    }
    arg
}

fn drain(mut stream: impl Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stream.read_to_end(&mut buf);
        buf
    })
}

/// Child-poll cadence in [`wait_with_timeout`]: coarse enough to stay
/// cheap, fine enough that timeout overshoot is invisible next to
/// multi-second run timeouts.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Stderr lines quoted in the driver-failure diagnostic: enough for the
/// failing command and its error, without flooding the terminal.
const STDERR_TAIL_LINES: usize = 5;

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Option<std::process::ExitStatus> {
    let start = Instant::now();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Some(status);
        }
        if start.elapsed() >= timeout {
            return None;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Force-remove the container. Best-effort: the normal path is `--rm`
/// auto-removal; this covers timeout and error paths (B9).
fn reap(name: &str) {
    let _ = Command::new("container")
        .args(["rm", "-f", name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Warn (once) when the installed `container` CLI differs from the tested
/// version. `--network none` is undocumented upstream, so drift is
/// security-relevant; behavior B2's battery test is the hard guard.
fn warn_on_version_drift() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let out = Command::new("container").arg("--version").output();
        if let Ok(out) = out {
            let text = String::from_utf8_lossy(&out.stdout);
            let installed = text
                .split_whitespace()
                .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()));
            if let Some(v) = installed {
                if v != TESTED_CONTAINER_VERSION {
                    eprintln!(
                        "daemar-sandbox: container CLI {v} differs from tested \
                         {TESTED_CONTAINER_VERSION}; re-run the battery (specs/sandbox.md B2)"
                    );
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_args_render() {
        let arg = mount_arg(
            std::path::Path::new("/x/y"),
            "/daemar/lower",
            Mount::ReadOnly,
        );
        assert_eq!(arg, "type=bind,source=/x/y,target=/daemar/lower,readonly");
        let rw = mount_arg(
            std::path::Path::new("/x/out"),
            "/daemar/out",
            Mount::ReadWrite,
        );
        assert!(!rw.contains("readonly"));
    }

    #[test]
    fn empty_command_is_rejected() {
        let spec = RunSpec::new(vec![], "/tmp");
        assert!(matches!(run(&spec), Err(Error::EmptyCommand)));
    }

    #[test]
    fn missing_worktree_is_rejected() {
        let spec = RunSpec::new(vec!["true".into()], "/nonexistent/daemar/worktree/path");
        assert!(matches!(run(&spec), Err(Error::BadWorktree { .. })));
    }
}
