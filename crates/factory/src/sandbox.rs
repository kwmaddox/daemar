//! The Docker wall: a per-stage container implementing [`crate::wall`].
//!
//! This is one implementation of the wall contract, not the contract
//! itself. It holds the container's entire visible world to the stage's
//! worktree, bind-mounted at /workspace: no network, non-root, every
//! capability dropped, and an image carrying exactly one static binary —
//! no shell, no libc, nothing incidental to run.
//!
//! Its known ceiling, and the reason the seam above it exists: containers
//! share the host kernel. A stage running genuinely hostile code wants a
//! kernel of its own, which is a different implementation's job.

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use crate::roster::ToolAccess;
use crate::wall::{StagePolicy, StageWall, Teardown, WallError, WallOpener};

/// The command seam: docker is driven through this so every lifecycle
/// branch is unit-testable without docker. The system runner scrubs its
/// subprocess environment — the docker CLI itself never sees the vault.
pub trait CommandRunner: Send + Sync {
    fn run(&self, program: &str, args: &[String], stdin: Option<&str>) -> Result<CmdOut, String>;
}

pub struct CmdOut {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[String], stdin: Option<&str>) -> Result<CmdOut, String> {
        use std::io::Write;
        let mut cmd = Command::new(program);
        cmd.args(args);
        // The scrub: no inherited environment reaches docker or the
        // container. PATH alone survives so the CLI can be found.
        cmd.env_clear();
        if let Some(path) = std::env::var_os("PATH") {
            cmd.env("PATH", path);
        }
        // Docker Desktop needs HOME to find its own config/credentials.
        if let Some(home) = std::env::var_os("HOME") {
            cmd.env("HOME", home);
        }
        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| e.to_string())?;
        if let Some(input) = stdin {
            child
                .stdin
                .as_mut()
                .expect("stdin piped above")
                .write_all(input.as_bytes())
                .map_err(|e| e.to_string())?;
        }
        drop(child.stdin.take());
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        Ok(CmdOut {
            success: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        })
    }
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| s.to_string()).collect()
}

/// Opens Docker walls. Holds the command seam so tests can script docker.
pub struct DockerOpener {
    runner: Arc<dyn CommandRunner>,
}

impl DockerOpener {
    pub fn new(runner: Arc<dyn CommandRunner>) -> Self {
        DockerOpener { runner }
    }

    /// The production opener: the real docker CLI, environment scrubbed.
    pub fn system() -> Self {
        DockerOpener::new(Arc::new(SystemRunner))
    }
}

impl WallOpener for DockerOpener {
    fn wall_name(&self) -> &'static str {
        "docker"
    }

    fn preflight(&self, policy: &StagePolicy) -> Result<(), WallError> {
        let version = self
            .runner
            .run(
                "docker",
                &strings(&["version", "--format", "{{.Server.Version}}"]),
                None,
            )
            .map_err(|detail| WallError::Unavailable { detail })?;
        if !version.success {
            return Err(WallError::Unavailable {
                detail: version.stderr.trim().to_string(),
            });
        }
        let inspect = self
            .runner
            .run(
                "docker",
                &strings(&["image", "inspect", &policy.image, "--format", "{{.Id}}"]),
                None,
            )
            .map_err(|detail| WallError::Unavailable { detail })?;
        if !inspect.success {
            return Err(WallError::ImageMissing {
                image: policy.image.clone(),
                remedy: format!(
                    "build it (docker build -f Dockerfile.cage -t {} .)",
                    policy.image
                ),
                detail: inspect.stderr.trim().to_string(),
            });
        }
        Ok(())
    }

    fn open(
        &self,
        policy: &StagePolicy,
        worktree: &Path,
        access: ToolAccess,
        name_hint: &str,
    ) -> Result<Box<dyn StageWall>, WallError> {
        // The mount and the identity both follow the seat's access: a
        // read-only seat gets a read-only world under the fixed nonroot
        // uid; a write seat gets a writable mount under the WORKTREE
        // OWNER's identity — the only non-root identity guaranteed able to
        // write a freshly materialized worktree across engines. A
        // root-owned worktree is refused: no root wall, ever.
        let (mount, user) = match access {
            ToolAccess::None => {
                return Err(WallError::Lifecycle {
                    detail: "a toolless seat has no business behind a wall".to_string(),
                })
            }
            ToolAccess::ReadOnly => (
                format!("{}:/workspace:ro", worktree.display()),
                policy.user.clone(),
            ),
            ToolAccess::ReadWrite => {
                use std::os::unix::fs::MetadataExt;
                let meta = std::fs::metadata(worktree).map_err(|e| WallError::Lifecycle {
                    detail: format!("cannot stat worktree for a writable wall: {e}"),
                })?;
                if meta.uid() == 0 {
                    return Err(WallError::Lifecycle {
                        detail: "worktree is root-owned — refusing a root wall".to_string(),
                    });
                }
                (
                    format!("{}:/workspace", worktree.display()),
                    format!("{}:{}", meta.uid(), meta.gid()),
                )
            }
        };
        let name = format!("daemar-cage-{name_hint}-{}", std::process::id());
        let mut args = strings(&[
            "run",
            "-d",
            "--name",
            &name,
            "--network",
            "none",
            "--user",
            &user,
            "--cap-drop",
            "ALL",
            "--security-opt",
            "no-new-privileges",
            "-v",
            &mount,
            "-w",
            "/workspace",
        ]);
        if let Some(cpus) = &policy.resources.cpus {
            args.extend(strings(&["--cpus", cpus]));
        }
        if let Some(memory) = &policy.resources.memory {
            args.extend(strings(&["--memory", memory]));
        }
        for extra in &policy.extra_mounts {
            let ro = if extra.read_only { ":ro" } else { "" };
            args.extend(strings(&[
                "-v",
                &format!("{}:{}{ro}", extra.host, extra.guest),
            ]));
        }
        args.push(policy.image.clone());
        args.extend(strings(&["/cage-executor", "hold"]));

        let out = self
            .runner
            .run("docker", &args, None)
            .map_err(|detail| WallError::Lifecycle { detail })?;
        if !out.success {
            return Err(WallError::Lifecycle {
                detail: format!("docker run failed: {}", out.stderr.trim()),
            });
        }
        Ok(Box::new(DockerWall {
            runner: Arc::clone(&self.runner),
            container: out.stdout.trim().to_string(),
            dead: false,
        }))
    }
}

/// A running Docker wall: one container per stage, `docker exec` per tool
/// call, forcibly removed at teardown.
pub struct DockerWall {
    runner: Arc<dyn CommandRunner>,
    container: String,
    dead: bool,
}

impl StageWall for DockerWall {
    fn id(&self) -> &str {
        &self.container
    }

    fn send(&mut self, request_json: &str) -> Result<String, String> {
        let out = self
            .runner
            .run(
                "docker",
                &strings(&["exec", "-i", &self.container, "/cage-executor", "request"]),
                Some(request_json),
            )
            .inspect_err(|_| {
                self.dead = true;
            })?;
        if !out.success {
            self.dead = true;
            return Err(format!("docker exec failed: {}", out.stderr.trim()));
        }
        Ok(out.stdout)
    }

    fn dead(&self) -> bool {
        self.dead
    }

    /// "No such container" IS proof — the goal is the container being gone —
    /// but it is an anomaly worth witnessing: something other than us
    /// removed it.
    fn terminate(self: Box<Self>) -> Result<Teardown, WallError> {
        let out = self
            .runner
            .run("docker", &strings(&["rm", "-f", &self.container]), None)
            .map_err(|detail| WallError::Lifecycle { detail })?;
        if out.success {
            return Ok(Teardown::Removed);
        }
        if out.stderr.contains("No such container") {
            return Ok(Teardown::AlreadyGone);
        }
        let mut detail = String::new();
        let _ = write!(detail, "docker rm -f failed: {}", out.stderr.trim());
        Err(WallError::Lifecycle { detail })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Scripted docker: each call pops the next result; calls are recorded.
    struct FakeRunner {
        script: Mutex<Vec<Result<CmdOut, String>>>,
        calls: Mutex<Vec<Vec<String>>>,
    }

    impl FakeRunner {
        fn new(script: Vec<Result<CmdOut, String>>) -> Arc<Self> {
            Arc::new(FakeRunner {
                script: Mutex::new(script),
                calls: Mutex::new(Vec::new()),
            })
        }
        fn ok(stdout: &str) -> Result<CmdOut, String> {
            Ok(CmdOut {
                success: true,
                stdout: stdout.to_string(),
                stderr: String::new(),
            })
        }
        fn fail(stderr: &str) -> Result<CmdOut, String> {
            Ok(CmdOut {
                success: false,
                stdout: String::new(),
                stderr: stderr.to_string(),
            })
        }
        fn args(&self, nth: usize) -> String {
            self.calls.lock().unwrap()[nth].join(" ")
        }
    }

    impl CommandRunner for FakeRunner {
        fn run(
            &self,
            _program: &str,
            args: &[String],
            _stdin: Option<&str>,
        ) -> Result<CmdOut, String> {
            self.calls.lock().unwrap().push(args.to_vec());
            self.script.lock().unwrap().remove(0)
        }
    }

    fn opener(runner: Arc<FakeRunner>) -> DockerOpener {
        DockerOpener::new(runner)
    }

    #[test]
    fn a_missing_runtime_and_a_missing_image_are_distinct_refusals() {
        let runner = FakeRunner::new(vec![Err("no docker binary".to_string())]);
        assert!(matches!(
            opener(runner).preflight(&StagePolicy::default()),
            Err(WallError::Unavailable { .. })
        ));

        let runner = FakeRunner::new(vec![
            FakeRunner::ok("27.0"),
            FakeRunner::fail("No such image"),
        ]);
        assert!(matches!(
            opener(runner).preflight(&StagePolicy::default()),
            Err(WallError::ImageMissing { .. })
        ));
    }

    #[test]
    fn a_read_only_seat_gets_a_confined_read_only_world() {
        let runner = FakeRunner::new(vec![FakeRunner::ok("abc123\n")]);
        let wall = opener(Arc::clone(&runner))
            .open(
                &StagePolicy::default(),
                Path::new("/wt"),
                ToolAccess::ReadOnly,
                "slip-scout",
            )
            .expect("opens");
        assert_eq!(wall.id(), "abc123");
        let args = runner.args(0);
        assert!(args.contains("--network none"), "{args}");
        assert!(args.contains("--user 65532:65532"), "{args}");
        assert!(args.contains("--cap-drop ALL"), "{args}");
        assert!(args.contains("no-new-privileges"), "{args}");
        assert!(args.contains("/wt:/workspace:ro"), "{args}");
        assert!(
            args.contains("/cage-executor hold"),
            "the executor is the only program the guest runs: {args}"
        );
        assert!(
            !args.contains("-e ") && !args.contains("--env"),
            "no environment crosses the wall: {args}"
        );
    }

    #[test]
    fn a_write_seat_gets_a_writable_world_under_the_worktree_owner() {
        let runner = FakeRunner::new(vec![FakeRunner::ok("abc123\n")]);
        let dir = std::env::temp_dir().join(format!("daemar-rw-wall-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wall = opener(Arc::clone(&runner))
            .open(
                &StagePolicy::default(),
                &dir,
                ToolAccess::ReadWrite,
                "slip-build",
            )
            .expect("opens");
        assert_eq!(wall.id(), "abc123");
        let args = runner.args(0);
        assert!(
            args.contains(&format!("{}:/workspace ", dir.display()))
                || args.contains(&format!("{}:/workspace -w", dir.display())),
            "the write seat's world is writable: {args}"
        );
        assert!(!args.contains(":/workspace:ro"), "{args}");
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::metadata(&dir).unwrap();
        assert!(
            args.contains(&format!("--user {}:{}", meta.uid(), meta.gid())),
            "the wall wears the worktree owner's identity: {args}"
        );
        assert!(
            !args.contains("65532"),
            "not the fixed read-only identity: {args}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_failed_send_marks_the_session_dead() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("abc123\n"),
            FakeRunner::fail("container is not running"),
        ]);
        let mut wall = opener(runner)
            .open(
                &StagePolicy::default(),
                Path::new("/wt"),
                ToolAccess::ReadOnly,
                "x",
            )
            .expect("opens");
        assert!(wall.send("{}").is_err());
        assert!(
            wall.dead(),
            "a wall that failed a request cannot be trusted"
        );
    }

    #[test]
    fn a_failed_teardown_is_an_error_not_a_shrug() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("abc123\n"),
            FakeRunner::fail("cannot remove"),
        ]);
        let wall = opener(runner)
            .open(
                &StagePolicy::default(),
                Path::new("/wt"),
                ToolAccess::ReadOnly,
                "x",
            )
            .expect("opens");
        assert!(matches!(wall.terminate(), Err(WallError::Lifecycle { .. })));
    }

    #[test]
    fn an_already_gone_sandbox_satisfies_teardown_as_an_anomaly() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("abc123\n"),
            FakeRunner::fail("Error response from daemon: No such container: abc123"),
        ]);
        let wall = opener(runner)
            .open(
                &StagePolicy::default(),
                Path::new("/wt"),
                ToolAccess::ReadOnly,
                "x",
            )
            .expect("opens");
        assert_eq!(
            wall.terminate().expect("gone is gone"),
            Teardown::AlreadyGone,
            "the goal is the sandbox being gone — however that happened"
        );
    }

    #[test]
    fn a_toolless_seat_is_refused_a_wall() {
        let runner = FakeRunner::new(vec![]);
        assert!(opener(runner)
            .open(
                &StagePolicy::default(),
                Path::new("/wt"),
                ToolAccess::None,
                "x",
            )
            .is_err());
    }

    #[test]
    fn the_opener_names_itself_for_the_receipt() {
        assert_eq!(DockerOpener::system().wall_name(), "docker");
    }
}
