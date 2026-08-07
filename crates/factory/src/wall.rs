//! The wall: the contract between a stage and whatever holds it.
//!
//! Architecture ruling, unchanged by any implementation behind this seam:
//! the engine's model loop and the API key stay HOST-side. Only tool
//! execution crosses the wall, and the key never crosses it. The stage's
//! worktree is the entire visible world inside.
//!
//! What lives here is the shape of that promise — a policy, a session, and
//! a proof of teardown — with no vocabulary from any particular runtime.
//! Containers, microVMs, and whatever comes next are implementations; the
//! ledger never learns which one ran a call, and `tool_call.v1` is
//! identical either way. That is the property this seam exists to protect.

use std::fmt;
use std::path::Path;

use crate::roster::ToolAccess;

/// One extra mount beyond the stage worktree. The default policy has none;
/// the field exists so per-territory customization becomes data, not a
/// redesign.
#[derive(Debug, Clone)]
pub struct Mount {
    pub host: String,
    pub guest: String,
    pub read_only: bool,
}

/// Network policy, a closed set. Today the only member is None — a walled
/// stage with network is a future, deliberate decision, declared per
/// territory, never inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkPolicy {
    None,
}

#[derive(Debug, Clone, Default)]
pub struct Resources {
    pub cpus: Option<String>,
    pub memory: Option<String>,
}

/// Everything a wall needs to hold one stage. Hardcoded default today;
/// per-territory data later — the roster pattern: 1:1 in Rust until a
/// second environment earns the file.
#[derive(Debug, Clone)]
pub struct StagePolicy {
    pub image: String,
    pub extra_mounts: Vec<Mount>,
    pub network: NetworkPolicy,
    /// Guest identity for implementations that take one. Non-root always.
    pub user: String,
    pub resources: Resources,
}

impl Default for StagePolicy {
    fn default() -> Self {
        StagePolicy {
            image: "daemar-cage:latest".to_string(),
            extra_mounts: Vec::new(),
            network: NetworkPolicy::None,
            user: "65532:65532".to_string(),
            resources: Resources::default(),
        }
    }
}

/// Whether tool execution is walled. Read-only seats carry a dial;
/// write-capable seats are walled unconditionally, everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallMode {
    Off,
    On,
}

/// How teardown proved the sandbox gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Teardown {
    /// We removed it — the ordinary ending.
    Removed,
    /// It was already gone: goal satisfied, anomaly worth a note.
    AlreadyGone,
}

#[derive(Debug)]
pub enum WallError {
    /// The wall runtime itself is unavailable — refused before any slip is
    /// minted, so it costs nothing.
    Unavailable {
        detail: String,
    },
    /// The stage image is not present; a wall never pulls implicitly.
    /// `remedy` is the implementation's own instruction for producing it.
    ImageMissing {
        image: String,
        remedy: String,
        detail: String,
    },
    Lifecycle {
        detail: String,
    },
}

impl fmt::Display for WallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WallError::Unavailable { detail } => {
                write!(f, "the sandbox runtime is unavailable: {detail}")
            }
            WallError::ImageMissing {
                image,
                remedy,
                detail,
            } => write!(
                f,
                "sandbox image '{image}' is not available locally — {remedy}: {detail}"
            ),
            WallError::Lifecycle { detail } => write!(f, "sandbox lifecycle: {detail}"),
        }
    }
}

impl std::error::Error for WallError {}

/// One stage's live sandbox. The session outlives individual tool calls:
/// how many times the transport is crossed is an implementation's business,
/// not the engine's.
pub trait StageWall {
    /// Opaque identifier of the running sandbox. Receipt metadata only —
    /// nothing in the fold or the workflow may branch on it.
    fn id(&self) -> &str;

    /// Carry one tool request across the wall and bring the outcome back.
    /// Any transport failure must mark the session dead: a wall that failed
    /// once cannot vouch for itself, and the stage ends witnessed rather
    /// than conversing with an absent world.
    fn send(&mut self, request_json: &str) -> Result<String, String>;

    /// True once the session can no longer be trusted.
    fn dead(&self) -> bool;

    /// Prove the sandbox gone. A stage is not finalized until this
    /// succeeds — an unproven teardown fails the phase, report or no report.
    fn terminate(self: Box<Self>) -> Result<Teardown, WallError>;
}

/// What opens walls. The implementation is chosen once, above the engine;
/// everything below it speaks only this trait.
pub trait WallOpener: Send + Sync {
    /// Names the implementation for receipts — e.g. "microsandbox".
    /// Audit metadata, never a branch condition.
    fn wall_name(&self) -> &'static str;

    /// Run BEFORE a slip is minted: a missing runtime or image is a refusal
    /// that costs nothing, not a witnessed failure.
    fn preflight(&self, policy: &StagePolicy) -> Result<(), WallError>;

    /// Open the stage's sandbox. The worktree is its whole world; the seat's
    /// access decides whether that world is writable.
    fn open(
        &self,
        policy: &StagePolicy,
        worktree: &Path,
        access: ToolAccess,
        name_hint: &str,
    ) -> Result<Box<dyn StageWall>, WallError>;
}
