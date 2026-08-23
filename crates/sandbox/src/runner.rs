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
use crate::error::Error;
use crate::{DEFAULT_IMAGE, TESTED_CONTAINER_VERSION};

/// What to run and where. `command` is argv — it is passed to the in-guest
/// driver as script arguments, never spliced into shell text.
#[derive(Debug, Clone)]
pub struct RunSpec {
    pub command: Vec<String>,
    /// Host directory mounted read-only as the overlay lower layer (B3, B4).
    pub worktree: PathBuf,
    /// Guest image. The image is a toolset, not a boundary; default is
    /// digest-pinned ubuntu ([`DEFAULT_IMAGE`]).
    pub image: String,
    /// Wall-clock limit; the container is killed and reaped on expiry (B8).
    pub timeout: Duration,
    pub cpus: Option<u32>,
    /// e.g. "2048M", "4G".
    pub memory: Option<String>,
}

impl RunSpec {
    pub fn new(command: Vec<String>, worktree: impl Into<PathBuf>) -> Self {
        RunSpec {
            command,
            worktree: worktree.into(),
            image: DEFAULT_IMAGE.to_string(),
            timeout: Duration::from_secs(300),
            cpus: None,
            memory: None,
        }
    }
}

#[derive(Debug)]
pub struct RunOutcome {
    /// The workload's exit code (B10) — not the container's.
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub changes: ChangeSet,
}

/// Execute one sandboxed run: create session, boot VM, run, collect, reap.
pub fn run(spec: &RunSpec) -> Result<RunOutcome, Error> {
    if spec.command.is_empty() {
        return Err(Error::EmptyCommand);
    }
    let worktree = spec
        .worktree
        .canonicalize()
        .map_err(|_| Error::BadWorktree(spec.worktree.clone()))?;
    if !worktree.is_dir() {
        return Err(Error::BadWorktree(spec.worktree.clone()));
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
    let guard = SessionGuard(session.clone());
    fs::write(bin_dir.join(driver::DRIVER_FILENAME), driver::driver_script()).map_err(|e| {
        Error::Session {
            path: bin_dir.clone(),
            source: e,
        }
    })?;

    let name = format!("daemar-{run_id}");
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
        .arg(&name)
        .arg("--mount")
        .arg(mount_arg(&worktree, driver::GUEST_LOWER, true))
        .arg("--mount")
        .arg(mount_arg(&out_dir, driver::GUEST_OUT, false))
        .arg("--mount")
        .arg(mount_arg(&bin_dir, driver::GUEST_BIN, true));
    if let Some(cpus) = spec.cpus {
        cmd.arg("--cpus").arg(cpus.to_string());
    }
    if let Some(memory) = &spec.memory {
        cmd.arg("--memory").arg(memory);
    }
    cmd.arg(&spec.image)
        .arg("sh")
        .arg(format!("{}/{}", driver::GUEST_BIN, driver::DRIVER_FILENAME))
        .args(&spec.command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(Error::Spawn)?;
    let stdout_reader = drain(child.stdout.take().expect("piped stdout"));
    let stderr_reader = drain(child.stderr.take().expect("piped stderr"));

    let status = match wait_with_timeout(&mut child, spec.timeout) {
        Some(status) => status,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            reap(&name);
            drop(guard);
            return Err(Error::Timeout(spec.timeout));
        }
    };
    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();

    let exit_code_file = out_dir.join(driver::EXIT_CODE_FILE);
    let tar_path = out_dir.join(driver::CHANGES_TAR);
    match fs::read_to_string(&exit_code_file) {
        Ok(raw) => {
            let exit_code: i32 = raw.trim().parse().map_err(|_| {
                Error::Archive(format!("unparseable workload exit code {raw:?}"))
            })?;
            if !tar_path.is_file() {
                reap(&name);
                return Err(Error::Driver {
                    stage: "export",
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
        Err(_) => {
            // The driver never reached the workload. Stage from the exit
            // code; anything else is a container-level failure (bad image,
            // runtime error) — stderr has the story.
            reap(&name);
            let stage = match status.code() {
                Some(driver::EXIT_MKDIR) => "mkdir",
                Some(driver::EXIT_OVERLAY) => "overlay mount",
                Some(driver::EXIT_CD) => "cd",
                _ => "container",
            };
            let tail = String::from_utf8_lossy(&stderr);
            let tail = tail.lines().rev().take(5).collect::<Vec<_>>();
            eprintln!(
                "daemar-sandbox: driver failed at {stage}: {}",
                tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
            );
            Err(Error::Driver {
                stage,
                code: status.code(),
            })
        }
    }
}

fn mount_arg(source: &std::path::Path, target: &str, readonly: bool) -> String {
    let mut arg = format!(
        "type=bind,source={},target={}",
        source.display(),
        target
    );
    if readonly {
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

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Option<std::process::ExitStatus> {
    let start = Instant::now();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Some(status);
        }
        if start.elapsed() >= timeout {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
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
        let arg = mount_arg(std::path::Path::new("/x/y"), "/daemar/lower", true);
        assert_eq!(arg, "type=bind,source=/x/y,target=/daemar/lower,readonly");
        let rw = mount_arg(std::path::Path::new("/x/out"), "/daemar/out", false);
        assert!(!rw.contains("readonly"));
    }

    #[test]
    fn empty_command_is_rejected() {
        let spec = RunSpec::new(vec![], "/tmp");
        assert!(matches!(run(&spec), Err(Error::EmptyCommand)));
    }

    #[test]
    fn missing_worktree_is_rejected() {
        let spec = RunSpec::new(
            vec!["true".into()],
            "/nonexistent/daemar/worktree/path",
        );
        assert!(matches!(run(&spec), Err(Error::BadWorktree(_))));
    }
}
