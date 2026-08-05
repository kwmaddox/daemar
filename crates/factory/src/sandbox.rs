//! The cage: a per-stage Docker sandbox in which tool execution happens.
//!
//! Architecture ruling: the engine's model loop and the API key stay
//! host-side — only tool execution crosses into the container, and the key
//! never enters it. The container's entire visible world is the stage's
//! worktree, bind-mounted at /workspace; no network, non-root, every
//! capability dropped. The image holds exactly one static binary — no
//! shell, no libc, nothing incidental to run.
//!
//! The spec is a struct with one hardcoded default, deliberately shaped to
//! become data later (`sandbox.toml` beside `airframes.toml`) — the roster
//! pattern: 1:1 in Rust until a second environment earns the file.

use std::fmt;
use std::path::Path;
use std::process::Command;

use crate::roster::ToolAccess;

/// One extra bind mount. Unused by the default spec; the field exists so
/// per-territory customization becomes data, not a redesign.
#[derive(Debug, Clone)]
pub struct Mount {
    pub host: String,
    pub container: String,
    pub read_only: bool,
}

/// Network policy, a closed set. Today the only member is None — a cage
/// with network is a future, deliberate decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkPolicy {
    None,
}

#[derive(Debug, Clone, Default)]
pub struct Resources {
    pub cpus: Option<String>,
    pub memory: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SandboxSpec {
    pub image: String,
    pub extra_mounts: Vec<Mount>,
    pub network: NetworkPolicy,
    /// uid:gid inside the container. Non-root always; 65532 is the
    /// conventional "nonroot" identity.
    pub user: String,
    pub resources: Resources,
}

impl Default for SandboxSpec {
    fn default() -> Self {
        SandboxSpec {
            image: "daemar-cage:latest".to_string(),
            extra_mounts: Vec::new(),
            network: NetworkPolicy::None,
            user: "65532:65532".to_string(),
            resources: Resources::default(),
        }
    }
}

/// Whether tool execution is caged. Phase 1: opt-in via DAEMAR_CAGE=1;
/// write-capable tool access will select the cage unconditionally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CageMode {
    Off,
    On,
}

#[derive(Debug)]
pub enum CageError {
    /// Docker itself is unavailable — refused before any slip is minted.
    DockerMissing {
        detail: String,
    },
    /// The image is not present locally; the cage never pulls implicitly.
    ImageMissing {
        image: String,
        detail: String,
    },
    Lifecycle {
        detail: String,
    },
}

impl fmt::Display for CageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CageError::DockerMissing { detail } => {
                write!(f, "docker is unavailable: {detail}")
            }
            CageError::ImageMissing { image, detail } => write!(
                f,
                "cage image '{image}' is not available locally — build it \
                 (docker build -f Dockerfile.cage -t {image} .): {detail}"
            ),
            CageError::Lifecycle { detail } => write!(f, "cage lifecycle: {detail}"),
        }
    }
}

impl std::error::Error for CageError {}

/// The command seam: docker is driven through this so every lifecycle
/// branch is unit-testable without docker. The system runner scrubs its
/// subprocess environment — the docker CLI itself never sees the vault.
pub trait CommandRunner {
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

/// Preflight, run BEFORE a slip is minted: a missing docker or image is a
/// refusal that costs nothing, not a witnessed failure.
pub fn preflight(runner: &dyn CommandRunner, spec: &SandboxSpec) -> Result<(), CageError> {
    let version = runner
        .run(
            "docker",
            &strings(&["version", "--format", "{{.Server.Version}}"]),
            None,
        )
        .map_err(|detail| CageError::DockerMissing { detail })?;
    if !version.success {
        return Err(CageError::DockerMissing {
            detail: version.stderr.trim().to_string(),
        });
    }
    let inspect = runner
        .run(
            "docker",
            &strings(&["image", "inspect", &spec.image, "--format", "{{.Id}}"]),
            None,
        )
        .map_err(|detail| CageError::DockerMissing { detail })?;
    if !inspect.success {
        return Err(CageError::ImageMissing {
            image: spec.image.clone(),
            detail: inspect.stderr.trim().to_string(),
        });
    }
    Ok(())
}

/// A running cage: one container per stage, `docker exec` per tool call,
/// forcibly removed at teardown.
pub struct Cage<'r> {
    runner: &'r dyn CommandRunner,
    pub container: String,
    /// Set when an exec fails: the cage can no longer vouch for itself and
    /// the stage must end as a witnessed failure.
    pub dead: bool,
}

impl<'r> Cage<'r> {
    /// Create and start the stage's container. The worktree mount follows
    /// the seat's tool access: a read-only seat gets a read-only world.
    pub fn start(
        runner: &'r dyn CommandRunner,
        spec: &SandboxSpec,
        worktree: &Path,
        access: ToolAccess,
        name_hint: &str,
    ) -> Result<Cage<'r>, CageError> {
        let mount = match access {
            ToolAccess::None => {
                return Err(CageError::Lifecycle {
                    detail: "a toolless seat has no business in a cage".to_string(),
                })
            }
            ToolAccess::ReadOnly => format!("{}:/workspace:ro", worktree.display()),
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
            &spec.user,
            "--cap-drop",
            "ALL",
            "--security-opt",
            "no-new-privileges",
            "-v",
            &mount,
            "-w",
            "/workspace",
        ]);
        if let Some(cpus) = &spec.resources.cpus {
            args.extend(strings(&["--cpus", cpus]));
        }
        if let Some(memory) = &spec.resources.memory {
            args.extend(strings(&["--memory", memory]));
        }
        for extra in &spec.extra_mounts {
            let ro = if extra.read_only { ":ro" } else { "" };
            args.extend(strings(&[
                "-v",
                &format!("{}:{}{ro}", extra.host, extra.container),
            ]));
        }
        args.push(spec.image.clone());
        args.extend(strings(&["/cage-executor", "hold"]));

        let out = runner
            .run("docker", &args, None)
            .map_err(|detail| CageError::Lifecycle { detail })?;
        if !out.success {
            return Err(CageError::Lifecycle {
                detail: format!("docker run failed: {}", out.stderr.trim()),
            });
        }
        Ok(Cage {
            runner,
            container: out.stdout.trim().to_string(),
            dead: false,
        })
    }

    /// One tool request across the boundary: request JSON on stdin, outcome
    /// JSON on stdout. Any transport failure marks the cage dead — the
    /// caller witnesses the failure instead of conversing with an absent cage.
    pub fn execute(&mut self, request_json: &str) -> Result<String, String> {
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

    /// Forcibly remove the container. A stage is not finalized until this
    /// succeeds: an unproven teardown fails the phase, report or no report.
    pub fn teardown(self) -> Result<(), CageError> {
        let out = self
            .runner
            .run("docker", &strings(&["rm", "-f", &self.container]), None)
            .map_err(|detail| CageError::Lifecycle { detail })?;
        if !out.success {
            return Err(CageError::Lifecycle {
                detail: format!("docker rm -f failed: {}", out.stderr.trim()),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Scripted docker: each call pops the next result; calls are recorded.
    struct FakeRunner {
        script: RefCell<Vec<Result<CmdOut, String>>>,
        calls: RefCell<Vec<Vec<String>>>,
    }

    impl FakeRunner {
        fn new(script: Vec<Result<CmdOut, String>>) -> Self {
            FakeRunner {
                script: RefCell::new(script),
                calls: RefCell::new(Vec::new()),
            }
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
    }

    impl CommandRunner for FakeRunner {
        fn run(
            &self,
            _program: &str,
            args: &[String],
            _stdin: Option<&str>,
        ) -> Result<CmdOut, String> {
            self.calls.borrow_mut().push(args.to_vec());
            self.script.borrow_mut().remove(0)
        }
    }

    #[test]
    fn missing_docker_and_missing_image_are_distinct_refusals() {
        let runner = FakeRunner::new(vec![Err("no docker binary".to_string())]);
        assert!(matches!(
            preflight(&runner, &SandboxSpec::default()),
            Err(CageError::DockerMissing { .. })
        ));

        let runner = FakeRunner::new(vec![
            FakeRunner::ok("27.0"),
            FakeRunner::fail("No such image"),
        ]);
        assert!(matches!(
            preflight(&runner, &SandboxSpec::default()),
            Err(CageError::ImageMissing { .. })
        ));
    }

    #[test]
    fn the_cage_starts_confined_and_read_only_for_a_read_only_seat() {
        let runner = FakeRunner::new(vec![FakeRunner::ok("abc123\n")]);
        let cage = Cage::start(
            &runner,
            &SandboxSpec::default(),
            Path::new("/wt"),
            ToolAccess::ReadOnly,
            "slip-scout",
        )
        .expect("starts");
        assert_eq!(cage.container, "abc123");
        let args = runner.calls.borrow()[0].join(" ");
        assert!(args.contains("--network none"), "{args}");
        assert!(args.contains("--user 65532:65532"), "{args}");
        assert!(args.contains("--cap-drop ALL"), "{args}");
        assert!(args.contains("no-new-privileges"), "{args}");
        assert!(args.contains("/wt:/workspace:ro"), "{args}");
        assert!(args.contains("/cage-executor hold"), "{args}");
        assert!(
            !args.contains("-e ") && !args.contains("--env"),
            "no environment crosses into the cage: {args}"
        );
    }

    #[test]
    fn a_failed_exec_marks_the_cage_dead() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("abc123\n"),
            FakeRunner::fail("container is not running"),
        ]);
        let mut cage = Cage::start(
            &runner,
            &SandboxSpec::default(),
            Path::new("/wt"),
            ToolAccess::ReadOnly,
            "x",
        )
        .expect("starts");
        assert!(cage.execute("{}").is_err());
        assert!(cage.dead, "a cage that failed an exec cannot be trusted");
    }

    #[test]
    fn a_failed_teardown_is_an_error_not_a_shrug() {
        let runner = FakeRunner::new(vec![
            FakeRunner::ok("abc123\n"),
            FakeRunner::fail("cannot remove"),
        ]);
        let cage = Cage::start(
            &runner,
            &SandboxSpec::default(),
            Path::new("/wt"),
            ToolAccess::ReadOnly,
            "x",
        )
        .expect("starts");
        assert!(matches!(cage.teardown(), Err(CageError::Lifecycle { .. })));
    }

    #[test]
    fn a_toolless_seat_is_refused_a_cage() {
        let runner = FakeRunner::new(vec![]);
        assert!(Cage::start(
            &runner,
            &SandboxSpec::default(),
            Path::new("/wt"),
            ToolAccess::None,
            "x",
        )
        .is_err());
    }
}
