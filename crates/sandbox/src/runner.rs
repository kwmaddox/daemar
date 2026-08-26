//! The one-shot run: spawn `container` as a supervised subprocess, wait with
//! a timeout, reap unconditionally, collect results from the session mounts.
//!
//! Subprocess supervision (not in-process VMM linkage) is a deliberate
//! security posture: the factory process is the trusted context, and the VMM
//! stays outside it. See `docs/research/substrate-refutation.md`.

use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::changeset::{ChangeSet, SessionGuard};
use crate::driver;
use crate::error::{DriverStage, Error};
use crate::{CONTAINER_PREFIX, DEFAULT_IMAGE, SESSION_ROOT, TESTED_CONTAINER_VERSION};

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
    let session = std::env::temp_dir().join(SESSION_ROOT).join(&run_id);
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

    let name = format!("{CONTAINER_PREFIX}{run_id}");
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

    // Success is signaled by the container exit status alone (PER-80):
    // the driver exits 0 only after the full chain succeeded and never
    // mirrors the workload's exit code, so classification depends on no
    // file the workload can write — everything in the rw out-mount is
    // forgeable by a racing workload descendant.
    let exit_code_file = out_dir.join(driver::EXIT_CODE_FILE);
    if status.code() != Some(0) {
        reap(&name);
        // Export failure needs the workload's recorded code; for every
        // other stage the file does not exist (or cannot be trusted).
        let workload_exit = fs::read_to_string(&exit_code_file)
            .ok()
            .and_then(|raw| raw.trim().parse().ok());
        return Err(driver_failure(
            status,
            workload_exit,
            &stderr,
            &mut std::io::stderr().lock(),
        ));
    }
    let Ok(raw) = fs::read_to_string(&exit_code_file) else {
        // Status 0 guarantees the driver wrote this file; its absence
        // means the protocol itself broke (or the guest was tampered
        // with beyond what we can narrate) — the timing-unknown bucket.
        let stage = DriverStage::Container;
        emit_stderr_tail(stage, &stderr, &mut std::io::stderr().lock());
        return Err(Error::Driver {
            stage,
            code: status.code(),
        });
    };
    let exit_code: i32 = match raw.trim().parse() {
        Ok(code) => code,
        // The raw file content is the whole story; a ParseIntError
        // carries nothing beyond it.
        Err(_) => return Err(Error::ExitCode { raw }),
    };
    let changes = ChangeSet::from_tar(out_dir.join(driver::CHANGES_TAR), &worktree, guard)?;
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

/// Classify a nonzero container exit: the driver failed (it exits 0 only
/// on full success). Stage from the driver's exit-code protocol —
/// [`driver::EXIT_EXPORT`] carries the workload's recorded exit code when
/// one was readable, and falls to the timing-unknown bucket when not;
/// anything outside the protocol is a container-level failure (bad image,
/// runtime error, signal death) — stderr has the story.
fn driver_failure(
    status: std::process::ExitStatus,
    workload_exit: Option<i32>,
    stderr: &[u8],
    out: &mut impl Write,
) -> Error {
    let stage = match status.code() {
        Some(driver::EXIT_MKDIR) => DriverStage::Mkdir,
        Some(driver::EXIT_OVERLAY) => DriverStage::OverlayMount,
        Some(driver::EXIT_CD) => DriverStage::Cd,
        Some(driver::EXIT_EXPORT) => match workload_exit {
            Some(workload_exit) => DriverStage::Export { workload_exit },
            None => DriverStage::Container,
        },
        _ => DriverStage::Container,
    };
    emit_stderr_tail(stage, stderr, out);
    Error::Driver {
        stage,
        code: status.code(),
    }
}

/// Operator diagnostic for any failed driver stage: quote a bounded tail
/// of the container's stderr, where the driver's `daemar-driver: <stage>`
/// line and the failing tool's own error live. Production callers pass a
/// locked stderr; tests pass a `Vec` and assert the emission semantics.
fn emit_stderr_tail(stage: DriverStage, stderr: &[u8], out: &mut impl Write) {
    let tail = String::from_utf8_lossy(stderr);
    let mut last_lines = tail
        .lines()
        .rev()
        .take(STDERR_TAIL_LINES)
        .collect::<Vec<_>>();
    last_lines.reverse();
    // Stream straight to the sink (C13): the Vec exists only for the
    // inherent reversal; best-effort, like every diagnostic write.
    let _ = write!(out, "daemar-sandbox: driver failed at {stage}: ");
    for (i, line) in last_lines.iter().enumerate() {
        if i > 0 {
            let _ = out.write_all(b" | ");
        }
        let _ = out.write_all(line.as_bytes());
    }
    let _ = out.write_all(b"\n");
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

    /// A real wait(2) status for a normally-exited child with this code.
    fn exited(code: i32) -> std::process::ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(code << 8)
    }

    /// PER-80: status-first classification. Stage codes map to their
    /// stages; export carries the recorded workload exit; an export
    /// status without a readable workload code, and anything outside the
    /// protocol, land in the timing-unknown Container bucket.
    #[test]
    fn nonzero_statuses_classify_by_the_driver_protocol() {
        let cases: [(i32, Option<i32>, DriverStage); 6] = [
            (driver::EXIT_MKDIR, None, DriverStage::Mkdir),
            (driver::EXIT_OVERLAY, None, DriverStage::OverlayMount),
            (driver::EXIT_CD, None, DriverStage::Cd),
            (
                driver::EXIT_EXPORT,
                Some(3),
                DriverStage::Export { workload_exit: 3 },
            ),
            (driver::EXIT_EXPORT, None, DriverStage::Container),
            (1, Some(1), DriverStage::Container),
        ];
        for (code, workload_exit, want) in cases {
            let mut sink = Vec::new();
            let err = driver_failure(exited(code), workload_exit, b"boom\n", &mut sink);
            let Error::Driver { stage, code: got } = err else {
                panic!("expected Driver error, got: {err}");
            };
            assert_eq!(stage, want);
            assert_eq!(got, Some(code));
            assert!(!sink.is_empty(), "classification must emit a diagnostic");
        }
    }

    /// PER-80 (folded Low): the diagnostic quotes a bounded *tail* — with
    /// more stderr lines than the bound, the last line is present and the
    /// first is not. Semantic assertions only; exact wording is not a
    /// contract (the Display-lock test in error.rs owns wording).
    #[test]
    fn stderr_tail_is_emitted_and_bounded() {
        use std::fmt::Write as _;
        let mut stderr = String::new();
        for i in 1..=STDERR_TAIL_LINES * 2 {
            let _ = writeln!(stderr, "line-{i}");
        }
        let mut sink = Vec::new();
        emit_stderr_tail(DriverStage::Cd, stderr.as_bytes(), &mut sink);
        let out = String::from_utf8(sink).unwrap();
        assert!(out.contains(&format!("line-{}", STDERR_TAIL_LINES * 2)));
        assert!(out.contains(&format!("line-{}", STDERR_TAIL_LINES + 1)));
        assert!(!out.contains("line-1 "));
        assert!(!out.contains(&format!("line-{STDERR_TAIL_LINES} ")));

        // Emission happens even with nothing to quote.
        let mut empty_sink = Vec::new();
        emit_stderr_tail(DriverStage::Cd, b"", &mut empty_sink);
        assert!(!empty_sink.is_empty());
    }

    #[test]
    fn missing_worktree_is_rejected() {
        let spec = RunSpec::new(vec!["true".into()], "/nonexistent/daemar/worktree/path");
        assert!(matches!(run(&spec), Err(Error::BadWorktree { .. })));
    }
}
