//! The runtime walls: the one production implementation of [`factory::wall`].
//!
//! Why this is its own crate: the SDK is async and carries the VMM itself,
//! and `crates/factory` is a runtime-free sync core (AGENTS.md). So the
//! runtime stops here. This adapter owns a Tokio runtime, blocks on it, and
//! hands the engine a synchronous `StageWall`. Nothing below the seam learns
//! that a hypervisor is involved — and `apps/cage-executor` never depends on
//! this crate, so the guest binary provably cannot link it.
//!
//! Every executable that constructs a `Config` injects its opener from here:
//! the factory ships the seam, never an implementation, so a config cannot
//! exist without declaring who holds its stages.
//!
//! What the microVM wall buys, measured before it was chosen: the guest runs
//! its OWN kernel, so a stage cannot reach the host kernel it is not sharing;
//! secrets can be mediated at the network boundary instead of merely
//! excluded; and network policy is per-host rather than all-or-nothing. The
//! cost is honest and paid here: this crate links the VMM.

use std::path::Path;
use std::sync::Arc;

use factory::roster::ToolAccess;
use factory::wall::{StagePolicy, StageWall, Teardown, WallError, WallOpener};
use microsandbox::Sandbox;
use tokio::runtime::Runtime;

/// The guest path the stage's worktree is mounted at — the whole world.
const WORKSPACE: &str = "/workspace";
/// The one program the guest is expected to run.
const EXECUTOR: &str = "/cage-executor";

/// The production opener. There is exactly one wall; the seam stays so a
/// second one is a construction change, not a redesign.
///
/// `DAEMAR_WALL` is retired: it once selected between the Docker wall and
/// this one, and the Docker wall is gone. A value left in the environment is
/// stale automation and must fail loudly — never a silent fallback, never a
/// silent ignore.
pub fn opener() -> Result<Arc<dyn WallOpener>, String> {
    match std::env::var("DAEMAR_WALL").ok().as_deref() {
        None | Some("") => {}
        Some(value) => {
            return Err(format!(
                "DAEMAR_WALL='{value}' is retired — microsandbox is the only wall; unset it"
            ))
        }
    }
    let opener = MicrosandboxOpener::new().map_err(|e| e.to_string())?;
    Ok(Arc::new(opener))
}

/// Why this host cannot hold a microVM, if it cannot.
///
/// The SDK carries the VMM, so "is the runtime installed" is answered by
/// linking. What stays host-dependent is hardware virtualization, and every
/// platform answers it differently: Apple silicon through
/// Hypervisor.framework, Linux through KVM. This is asked BEFORE a slip is
/// minted so an incapable host is a free refusal rather than a failed
/// stage — and "absent" is kept distinct from "not yours", because those
/// have different remedies. (CI taught us the second one: hosted runners
/// have /dev/kvm and simply do not grant it to the runner user.)
fn unsupported_host() -> Option<String> {
    #[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
    {
        return Some("microVM walls on macOS require Apple silicon".to_string());
    }
    #[cfg(target_os = "linux")]
    {
        let kvm = Path::new("/dev/kvm");
        if !kvm.exists() {
            return Some(
                "/dev/kvm is absent — microVMs need hardware virtualization \
                 (inside a VM, enable nested virt)"
                    .to_string(),
            );
        }
        if std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(kvm)
            .is_err()
        {
            return Some(
                "/dev/kvm exists but this user cannot open it — add the user \
                 to the kvm group"
                    .to_string(),
            );
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        return Some(
            "microVM walls are supported on macOS (Apple silicon) and Linux (KVM)".to_string(),
        );
    }
    #[allow(unreachable_code)]
    None
}

/// Opens microsandbox walls. Holds the Tokio runtime the whole process
/// shares: one runtime, created once, blocked on from the sync engine.
pub struct MicrosandboxOpener {
    runtime: Arc<Runtime>,
}

impl MicrosandboxOpener {
    pub fn new() -> Result<Self, WallError> {
        let runtime = Runtime::new().map_err(|e| WallError::Unavailable {
            detail: format!("cannot start an async runtime for the sandbox SDK: {e}"),
        })?;
        Ok(MicrosandboxOpener {
            runtime: Arc::new(runtime),
        })
    }
}

impl WallOpener for MicrosandboxOpener {
    fn wall_name(&self) -> &'static str {
        "microsandbox"
    }

    /// Preflight proves the host can hold a wall at all, before a slip is
    /// minted. A machine without hardware virtualization must refuse here —
    /// loudly and for free — rather than fail a stage later.
    fn preflight(&self, _policy: &StagePolicy) -> Result<(), WallError> {
        match unsupported_host() {
            Some(detail) => Err(WallError::Unavailable { detail }),
            None => Ok(()),
        }
    }

    fn open(
        &self,
        policy: &StagePolicy,
        worktree: &Path,
        access: ToolAccess,
        name_hint: &str,
    ) -> Result<Box<dyn StageWall>, WallError> {
        let read_only = match access {
            ToolAccess::None => {
                return Err(WallError::Lifecycle {
                    detail: "a toolless seat has no business behind a wall".to_string(),
                })
            }
            ToolAccess::ReadOnly => true,
            ToolAccess::ReadWrite => false,
        };
        let name = format!("daemar-{name_hint}-{}", std::process::id());
        let host = worktree.to_path_buf();
        let image = policy.image.clone();
        let sandbox = self.runtime.block_on(async {
            let mut builder = Sandbox::builder(&name)
                .image(image)
                // The worktree is the entire visible world; a read-only seat
                // gets a read-only one.
                .volume(WORKSPACE, |m| {
                    let m = m.bind(&host);
                    if read_only {
                        m.readonly()
                    } else {
                        m
                    }
                })
                .workdir(WORKSPACE)
                // No network unless a territory declares otherwise, and no
                // territory can today. Deny is the default, never inferred.
                .disable_network()
                // A name collision is a stale sandbox from a crashed run,
                // not a reason to refuse a flight.
                .replace();
            for extra in &policy.extra_mounts {
                let host = extra.host.clone();
                let ro = extra.read_only;
                builder = builder.volume(extra.guest.clone(), move |m| {
                    let m = m.bind(&host);
                    if ro {
                        m.readonly()
                    } else {
                        m
                    }
                });
            }
            builder.create().await
        });
        match sandbox {
            Ok(sandbox) => Ok(Box::new(MicrosandboxWall {
                runtime: Arc::clone(&self.runtime),
                sandbox: Some(sandbox),
                name,
                dead: false,
            })),
            Err(error) => Err(WallError::Lifecycle {
                detail: format!("sandbox create failed: {error}"),
            }),
        }
    }
}

/// A running microsandbox wall: one microVM per stage.
pub struct MicrosandboxWall {
    runtime: Arc<Runtime>,
    /// Taken at teardown; `Some` for the whole live lifetime.
    sandbox: Option<Sandbox>,
    name: String,
    dead: bool,
}

impl StageWall for MicrosandboxWall {
    fn id(&self) -> &str {
        &self.name
    }

    /// One tool request across the wall, in the wire protocol the container
    /// wall established: request JSON on stdin, outcome JSON on stdout. The
    /// transport changed; the protocol did not.
    fn send(&mut self, request_json: &str) -> Result<String, String> {
        let Some(sandbox) = self.sandbox.as_ref() else {
            self.dead = true;
            return Err("sandbox already torn down".to_string());
        };
        let request = request_json.to_string();
        let out = self.runtime.block_on(async {
            sandbox
                .exec_with(EXECUTOR, |e| e.args(["request"]).stdin_bytes(request))
                .await
        });
        match out {
            Ok(out) => match out.stdout() {
                Ok(stdout) => Ok(stdout),
                Err(error) => {
                    self.dead = true;
                    Err(format!("sandbox stdout was unreadable: {error}"))
                }
            },
            Err(error) => {
                self.dead = true;
                Err(format!("sandbox exec failed: {error}"))
            }
        }
    }

    fn dead(&self) -> bool {
        self.dead
    }

    /// Stop, then remove: the SDK refuses to remove a running sandbox, and
    /// a stage is not finalized until the world it ran in is proven gone.
    /// An already-absent sandbox satisfies the goal — and is still an
    /// anomaly worth witnessing.
    fn terminate(mut self: Box<Self>) -> Result<Teardown, WallError> {
        let Some(sandbox) = self.sandbox.take() else {
            return Ok(Teardown::AlreadyGone);
        };
        let name = self.name.clone();
        self.runtime.block_on(async move {
            // A stop failure is not yet fatal: removal is the real proof,
            // and a sandbox that died on its own stops badly but removes fine.
            let stopped = sandbox.stop().await;
            match Sandbox::remove(&name).await {
                Ok(()) => Ok(Teardown::Removed),
                Err(error) => {
                    let detail = error.to_string();
                    if detail.contains("not found") || detail.contains("does not exist") {
                        return Ok(Teardown::AlreadyGone);
                    }
                    Err(WallError::Lifecycle {
                        detail: match stopped {
                            Ok(()) => format!("sandbox remove failed: {detail}"),
                            Err(stop_error) => format!(
                                "sandbox stop failed ({stop_error}) and remove failed: {detail}"
                            ),
                        },
                    })
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The retired dial must refuse loudly whatever it says — even the name
    /// of the wall that won. Env mutation, so this test owns the variable:
    /// cargo runs tests in one process, but no other test touches DAEMAR_WALL.
    #[test]
    fn a_leftover_wall_dial_refuses_instead_of_being_ignored() {
        for stale in ["docker", "microsandbox", "firecracker"] {
            std::env::set_var("DAEMAR_WALL", stale);
            let refused = opener().err().expect("a set DAEMAR_WALL must refuse");
            assert!(refused.contains("retired"), "{refused}");
            assert!(refused.contains(stale), "{refused}");
        }
        std::env::remove_var("DAEMAR_WALL");
        assert!(opener().is_ok(), "unset is the only accepted state");
    }
}
