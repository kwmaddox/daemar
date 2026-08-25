//! One-shot sandboxed command execution.
//!
//! The contract this crate implements is `specs/sandbox.md` (behaviors
//! B1–B10). A command runs inside a VM (Apple `container`, one VM per
//! container) with no network path, against an overlay whose read-only lower
//! layer is the caller's worktree. The command edits its own view in place;
//! the host tree is untouched by construction. Every change comes back as a
//! [`ChangeSet`] and reaches the host only through [`ChangeSet::apply_to`],
//! which sanitizes on the way in.
//!
//! Substrate operating rules (from `docs/research/substrate-refutation.md`):
//! `--network none` is the only network posture; results cross the wall only
//! via our mounts — never `container cp`, never `--ssh`.
//!
//! Worktree note: only the worktree subtree is mounted. In a linked git
//! worktree, `.git` is a pointer file to a host-absolute path that does not
//! exist in the guest, so git commands inside the guest fail by construction
//! in this slice.

mod changeset;
mod driver;
mod error;
mod runner;

pub use changeset::{ApplyReport, Change, ChangeSet, RejectReason, UnsupportedKind};
pub use error::{DriverStage, Error};
pub use runner::{run, RunOutcome, RunSpec};

/// The `container` CLI version this crate was built and battery-tested
/// against. `run()` warns on stderr when the installed version differs —
/// the central egress primitive (`--network none`) is undocumented upstream,
/// so version drift is a security-relevant event, guarded by behavior B2.
pub const TESTED_CONTAINER_VERSION: &str = "1.2.2";

/// Default guest image, pinned by digest (resolved 2026-08-23 from
/// `docker.io/library/ubuntu:latest`, image created 2026-07-24). The image is
/// a toolset, not a boundary: the VM wall and `--network none` are the
/// security controls (see `docs/research/sandbox-image-selection.md`).
pub const DEFAULT_IMAGE: &str =
    "docker.io/library/ubuntu@sha256:678c6550cc43645e08669028bc177f50be4e7c5b8cca677067b1914d4afc7a03";
