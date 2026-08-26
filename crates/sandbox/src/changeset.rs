//! The change-set: what a run wrote, and sanitized promotion to the host.
//!
//! The guest exports its overlay upper layer as a tar (see `driver.rs`).
//! Everything here runs on the trusted host side and treats that archive as
//! adversarial input (behavior B6): entry paths are validated component-wise,
//! setuid/setgid bits are stripped, symlink entries whose targets escape the
//! destination are rejected, and no ownership is ever restored. This is the
//! "trusted parent does the promotion" pattern (Nix-style), motivated by the
//! `container cp` setuid advisory (GHSA-5h49-6pr7-9mv4).
//!
//! Known limitation (documented, rare): overlayfs marks "directory deleted
//! then recreated" with a `trusted.overlay.opaque` xattr, which the export
//! does not carry. Such a directory promotes as a plain merge. Revisit if a
//! real workload hits it.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use crate::error::Error;

/// Mode bits used when a tar header carries none. Security path (B6):
/// conservative rw-r--r-- — never executable, setuid-impossible.
const FALLBACK_MODE: u32 = 0o644;

/// Chunk size for the byte-compare in `same_content_and_mode`. The two
/// buffers must stay the same size for the `get(..n)` pairing to hold.
const COMPARE_CHUNK: usize = 8192;

/// Device number assumed when a char-device header carries none: any
/// nonzero value classifies the entry as a real device (unsupported),
/// never as a whiteout deletion — the conservative direction for
/// adversarial archive input (B6).
const NON_WHITEOUT_DEVICE: u32 = 1;

/// One reported change, relative to the worktree root (B5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// A file that did not exist in the worktree.
    Added {
        /// Path relative to the worktree root.
        path: PathBuf,
        /// Unix mode bits as recorded in the archive.
        mode: u32,
    },
    /// A file that exists in the worktree with different content or mode.
    Modified {
        /// Path relative to the worktree root.
        path: PathBuf,
        /// Unix mode bits as recorded in the archive.
        mode: u32,
    },
    /// A file or directory the run deleted (overlay whiteout).
    Deleted {
        /// Path relative to the worktree root.
        path: PathBuf,
    },
    /// A directory that did not exist in the worktree.
    DirAdded {
        /// Path relative to the worktree root.
        path: PathBuf,
    },
    /// A symlink the run created or replaced.
    Symlink {
        /// Path relative to the worktree root.
        path: PathBuf,
        /// The link target, exactly as recorded; validated only at
        /// promotion time.
        target: PathBuf,
    },
}

impl Change {
    /// The changed path, relative to the worktree root.
    #[must_use]
    fn path(&self) -> &Path {
        match self {
            Change::Added { path, .. }
            | Change::Modified { path, .. }
            | Change::Deleted { path }
            | Change::DirAdded { path }
            | Change::Symlink { path, .. } => path,
        }
    }
}

/// What a run changed. Holds the session directory alive: the backing
/// archive is deleted when the `ChangeSet` (via `RunOutcome`) is dropped.
#[derive(Debug)]
pub struct ChangeSet {
    entries: Vec<Change>,
    /// Entries the archive contained but this slice does not support
    /// (block devices, fifos, hard links, non-whiteout char devices).
    /// Never applied; surfaced so nothing is silently dropped.
    unsupported: Vec<(PathBuf, UnsupportedKind)>,
    tar_path: PathBuf,
    _session: SessionGuard,
}

/// Removes the per-run session directory on drop.
#[derive(Debug)]
pub(crate) struct SessionGuard(pub(crate) PathBuf);

impl Drop for SessionGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Why an archive entry is outside this slice's support (C6). `Clone`
/// because every [`ChangeSet::apply_to`] call copies these into its
/// report's rejected list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnsupportedKind {
    /// Non-whiteout character device.
    CharDevice,
    /// Symlink entry with no target recorded.
    SymlinkWithoutTarget,
    /// Any other entry type; carries the tar type's debug name (the
    /// foreign `tar::EntryType` stays out of the pub API).
    EntryType(String),
}

impl std::fmt::Display for UnsupportedKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnsupportedKind::CharDevice => f.write_str("char device"),
            UnsupportedKind::SymlinkWithoutTarget => f.write_str("symlink without target"),
            UnsupportedKind::EntryType(name) => write!(f, "unsupported entry type {name}"),
        }
    }
}

/// Why a promotion entry was refused (C6). `Display` describes the
/// refusal; io causes live in `source()` (C2), so chain printers keep
/// the detail.
#[derive(Debug)]
pub enum RejectReason {
    /// The archive entry's type is outside this slice's support.
    Unsupported(UnsupportedKind),
    /// The entry path is absolute.
    AbsolutePath,
    /// The entry path contains `..`.
    ParentEscape,
    /// The entry path traverses a symlink component inside the
    /// destination; carries the offending component's path.
    SymlinkComponent(PathBuf),
    /// The entry path has a non-normal component (prefix or root).
    NonNormalComponent,
    /// The symlink entry's target is absolute.
    SymlinkTargetAbsolute,
    /// The symlink entry's target lexically escapes the destination.
    SymlinkTargetEscapes,
    /// The symlink entry's target has a non-normal component.
    SymlinkTargetNonNormal,
    /// A deletion entry whose target does not exist in the destination.
    DeletionTargetMissing,
    /// Removing a deletion entry's target failed.
    DeleteFailed(std::io::Error),
    /// Creating a directory entry failed.
    MkdirFailed(std::io::Error),
    /// Creating a file entry's parent directory failed (distinct from
    /// `MkdirFailed`: the entry itself was a file, not a directory).
    MkdirParentFailed(std::io::Error),
    /// Writing a symlink entry failed.
    SymlinkFailed(std::io::Error),
    /// Writing a file entry failed.
    WriteFailed(std::io::Error),
}

impl std::fmt::Display for RejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RejectReason::Unsupported(kind) => write!(f, "{kind}"),
            RejectReason::AbsolutePath => f.write_str("absolute path"),
            RejectReason::ParentEscape => f.write_str("path contains .."),
            RejectReason::SymlinkComponent(at) => {
                write!(f, "path traverses symlink component {}", at.display())
            }
            RejectReason::NonNormalComponent => f.write_str("non-normal path component"),
            RejectReason::SymlinkTargetAbsolute => f.write_str("symlink target is absolute"),
            RejectReason::SymlinkTargetEscapes => f.write_str("symlink target escapes destination"),
            RejectReason::SymlinkTargetNonNormal => {
                f.write_str("symlink target has non-normal component")
            }
            RejectReason::DeletionTargetMissing => f.write_str("deletion target not found"),
            RejectReason::DeleteFailed(_) => f.write_str("delete failed"),
            RejectReason::MkdirFailed(_) => f.write_str("mkdir failed"),
            RejectReason::MkdirParentFailed(_) => f.write_str("mkdir parent failed"),
            RejectReason::SymlinkFailed(_) => f.write_str("symlink failed"),
            RejectReason::WriteFailed(_) => f.write_str("write failed"),
        }
    }
}

impl std::error::Error for RejectReason {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RejectReason::DeleteFailed(source)
            | RejectReason::MkdirFailed(source)
            | RejectReason::MkdirParentFailed(source)
            | RejectReason::SymlinkFailed(source)
            | RejectReason::WriteFailed(source) => Some(source),
            RejectReason::Unsupported(_)
            | RejectReason::AbsolutePath
            | RejectReason::ParentEscape
            | RejectReason::SymlinkComponent(_)
            | RejectReason::NonNormalComponent
            | RejectReason::SymlinkTargetAbsolute
            | RejectReason::SymlinkTargetEscapes
            | RejectReason::SymlinkTargetNonNormal
            | RejectReason::DeletionTargetMissing => None,
        }
    }
}

/// Outcome of [`ChangeSet::apply_to`]. Nothing is silently dropped: every
/// entry lands in exactly one of these lists.
#[derive(Debug, Default)]
pub struct ApplyReport {
    /// Entries written into the destination (files, dirs, symlinks).
    pub applied: Vec<PathBuf>,
    /// Deletion entries whose target existed and was removed.
    pub deleted: Vec<PathBuf>,
    /// Entries refused, with the reason.
    pub rejected: Vec<(PathBuf, RejectReason)>,
    /// Files whose setuid/setgid bits were stripped during promotion.
    pub stripped: Vec<PathBuf>,
}

impl ChangeSet {
    /// Parse the exported upper-layer tar. `worktree` is consulted read-only
    /// to classify entries (Added vs Modified) and to drop non-changes
    /// (copy-up artifacts with identical content and mode), so the report
    /// holds B5: nothing more, nothing less.
    pub(crate) fn from_tar(
        tar_path: PathBuf,
        worktree: &Path,
        session: SessionGuard,
    ) -> Result<Self, Error> {
        let file = fs::File::open(&tar_path).map_err(Error::Results)?;
        let mut archive = tar::Archive::new(file);
        let mut entries = Vec::new();
        let mut unsupported = Vec::new();

        for entry in archive.entries().map_err(Error::Archive)? {
            let mut entry = entry.map_err(Error::Archive)?;
            let rel = normalize_entry_path(&entry.path().map_err(Error::Archive)?);
            let Some(rel) = rel else { continue }; // "." / empty

            let header = entry.header();
            let mode = header.mode().unwrap_or(FALLBACK_MODE);
            #[expect(
                clippy::wildcard_enum_match_arm,
                reason = "tar::EntryType hides a #[doc(hidden)] __Nonexhaustive variant \
                          (pre-#[non_exhaustive] idiom), so no arm can name every variant; \
                          the wildcard is the unsupported-by-default bucket, not a panic \
                          path (C4)"
            )]
            match header.entry_type() {
                tar::EntryType::Char => {
                    let major = header
                        .device_major()
                        .ok()
                        .flatten()
                        .unwrap_or(NON_WHITEOUT_DEVICE);
                    let minor = header
                        .device_minor()
                        .ok()
                        .flatten()
                        .unwrap_or(NON_WHITEOUT_DEVICE);
                    if major == 0 && minor == 0 {
                        entries.push(Change::Deleted { path: rel });
                    } else {
                        unsupported.push((rel, UnsupportedKind::CharDevice));
                    }
                }
                tar::EntryType::Directory => {
                    // Copied-up parents of modified files appear in the upper
                    // layer without being a change themselves.
                    if !worktree.join(&rel).is_dir() {
                        entries.push(Change::DirAdded { path: rel });
                    }
                }
                tar::EntryType::Regular => {
                    let host = worktree.join(&rel);
                    if host.is_file() {
                        if same_content_and_mode(&mut entry, &host, mode)? {
                            continue; // copy-up artifact, not a change
                        }
                        entries.push(Change::Modified { path: rel, mode });
                    } else {
                        entries.push(Change::Added { path: rel, mode });
                    }
                }
                tar::EntryType::Symlink => match entry.link_name().map_err(Error::Archive)? {
                    Some(target) => entries.push(Change::Symlink {
                        path: rel,
                        target: target.into_owned(),
                    }),
                    None => unsupported.push((rel, UnsupportedKind::SymlinkWithoutTarget)),
                },
                other => {
                    unsupported.push((rel, UnsupportedKind::EntryType(format!("{other:?}"))));
                }
            }
        }

        Ok(ChangeSet {
            entries,
            unsupported,
            tar_path,
            _session: session,
        })
    }

    /// The reported changes, in archive order.
    #[must_use]
    pub fn entries(&self) -> &[Change] {
        &self.entries
    }

    /// Archive entries this slice does not support, with the reason.
    /// Never applied; surfaced so nothing is silently dropped.
    #[must_use]
    pub fn unsupported(&self) -> &[(PathBuf, UnsupportedKind)] {
        &self.unsupported
    }

    /// True when the run changed nothing (and nothing was unsupported).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.unsupported.is_empty()
    }

    /// Promote the changes into `dest` (behavior B6). `dest` is usually the
    /// original worktree but may be any directory. Sanitization is applied
    /// per entry; refusals are reported, never fatal. No ownership is
    /// restored; setuid/setgid bits are stripped and reported.
    ///
    /// # Errors
    ///
    /// [`Error::Results`] when the backing archive cannot be reopened,
    /// [`Error::Archive`] when it no longer parses. Per-entry failures are
    /// never errors — they land in [`ApplyReport::rejected`].
    pub fn apply_to(&self, dest: &Path) -> Result<ApplyReport, Error> {
        let mut report = ApplyReport::default();
        for (path, kind) in &self.unsupported {
            report
                .rejected
                .push((path.clone(), RejectReason::Unsupported(kind.clone())));
        }

        // Deletions and directory creations don't need archive content.
        // Files and symlinks are re-streamed from the archive below; collect
        // the sanitized target set first, keyed by borrows from `entries`
        // (no independent copy is needed — C11).
        let mut want: BTreeMap<&Path, &Change> = BTreeMap::new();
        for change in &self.entries {
            let rel = change.path();
            if let Err(why) = validate_dest_path(dest, rel) {
                report.rejected.push((rel.to_path_buf(), why));
                continue;
            }
            match change {
                Change::Deleted { path } => {
                    let target = dest.join(path);
                    let removed = match fs::symlink_metadata(&target) {
                        Ok(md) if md.is_dir() => fs::remove_dir_all(&target).map(|()| true),
                        Ok(_) => fs::remove_file(&target).map(|()| true),
                        Err(_) => Ok(false),
                    };
                    match removed {
                        Ok(true) => report.deleted.push(path.clone()),
                        Ok(false) => report
                            .rejected
                            .push((path.clone(), RejectReason::DeletionTargetMissing)),
                        Err(e) => report
                            .rejected
                            .push((path.clone(), RejectReason::DeleteFailed(e))),
                    }
                }
                Change::DirAdded { path } => match fs::create_dir_all(dest.join(path)) {
                    Ok(()) => report.applied.push(path.clone()),
                    Err(e) => report
                        .rejected
                        .push((path.clone(), RejectReason::MkdirFailed(e))),
                },
                Change::Symlink { path, target } => {
                    if let Err(why) = validate_symlink_target(path, target) {
                        report.rejected.push((path.clone(), why));
                        continue;
                    }
                    want.insert(path.as_path(), change);
                }
                Change::Added { path, .. } | Change::Modified { path, .. } => {
                    want.insert(path.as_path(), change);
                }
            }
        }

        // Stream file/symlink content out of the archive.
        let file = fs::File::open(&self.tar_path).map_err(Error::Results)?;
        let mut archive = tar::Archive::new(file);
        for entry in archive.entries().map_err(Error::Archive)? {
            let mut entry = entry.map_err(Error::Archive)?;
            let Some(rel) = normalize_entry_path(&entry.path().map_err(Error::Archive)?) else {
                continue;
            };
            let Some(change) = want.get(rel.as_path()) else {
                continue;
            };
            let target = dest.join(&rel);
            if let Some(parent) = target.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    report
                        .rejected
                        .push((rel.clone(), RejectReason::MkdirParentFailed(e)));
                    continue;
                }
            }
            match change {
                Change::Symlink { target: link, .. } => {
                    let _ = fs::remove_file(&target);
                    match std::os::unix::fs::symlink(link, &target) {
                        Ok(()) => report.applied.push(rel.clone()),
                        Err(e) => report
                            .rejected
                            .push((rel.clone(), RejectReason::SymlinkFailed(e))),
                    }
                }
                Change::Added { mode, .. } | Change::Modified { mode, .. } => {
                    let had_setid = mode & 0o6000 != 0;
                    let safe_mode = mode & 0o777;
                    match write_file(&mut entry, &target, safe_mode) {
                        Ok(()) => {
                            report.applied.push(rel.clone());
                            if had_setid {
                                report.stripped.push(rel.clone());
                            }
                        }
                        Err(e) => report
                            .rejected
                            .push((rel.clone(), RejectReason::WriteFailed(e))),
                    }
                }
                Change::Deleted { .. } | Change::DirAdded { .. } => {}
            }
        }

        Ok(report)
    }
}

fn write_file(
    entry: &mut tar::Entry<'_, fs::File>,
    target: &Path,
    mode: u32,
) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    // Never write through an existing symlink at the target itself.
    if let Ok(md) = fs::symlink_metadata(target) {
        if md.file_type().is_symlink() {
            fs::remove_file(target)?;
        }
    }
    let mut out = fs::File::create(target)?;
    std::io::copy(entry, &mut out)?;
    out.set_permissions(fs::Permissions::from_mode(mode))?;
    Ok(())
}

/// Normalize a tar entry path: strip the leading `./`, reject `.` itself.
fn normalize_entry_path(p: &Path) -> Option<PathBuf> {
    let cleaned: PathBuf = p
        .components()
        .filter(|c| !matches!(c, Component::CurDir))
        .collect();
    if cleaned.as_os_str().is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// Component-wise validation of a promotion path (B6): relative, no `..`,
/// no writing through an existing symlink component inside `dest`.
fn validate_dest_path(dest: &Path, rel: &Path) -> Result<(), RejectReason> {
    if rel.is_absolute() {
        return Err(RejectReason::AbsolutePath);
    }
    let mut probe = dest.to_path_buf();
    let components: Vec<_> = rel.components().collect();
    for (i, comp) in components.iter().enumerate() {
        match comp {
            Component::Normal(seg) => {
                probe.push(seg);
                // Intermediate components must not be symlinks, or a write
                // would follow them outside the destination.
                let is_last = i == components.len() - 1;
                if !is_last {
                    if let Ok(md) = fs::symlink_metadata(&probe) {
                        if md.file_type().is_symlink() {
                            return Err(RejectReason::SymlinkComponent(probe));
                        }
                    }
                }
            }
            Component::ParentDir => return Err(RejectReason::ParentEscape),
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir => {
                return Err(RejectReason::NonNormalComponent)
            }
        }
    }
    Ok(())
}

/// A promoted symlink entry may not point outside the destination tree:
/// absolute targets and lexical escapes are rejected (B6).
fn validate_symlink_target(rel: &Path, target: &Path) -> Result<(), RejectReason> {
    if target.is_absolute() {
        return Err(RejectReason::SymlinkTargetAbsolute);
    }
    // Resolve lexically from the symlink's parent directory: `depth` is how
    // many directories deep that parent sits inside the destination. `rel`
    // is normalized non-empty, so the subtraction cannot underflow.
    let mut depth = rel.components().count().saturating_sub(1);
    for comp in target.components() {
        match comp {
            Component::ParentDir => {
                let Some(up) = depth.checked_sub(1) else {
                    return Err(RejectReason::SymlinkTargetEscapes);
                };
                depth = up;
            }
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir => {
                return Err(RejectReason::SymlinkTargetNonNormal)
            }
        }
    }
    Ok(())
}

fn same_content_and_mode(
    entry: &mut tar::Entry<'_, fs::File>,
    host: &Path,
    tar_mode: u32,
) -> Result<bool, Error> {
    use std::os::unix::fs::PermissionsExt;
    let Ok(md) = fs::metadata(host) else {
        return Ok(false);
    };
    if md.permissions().mode() & 0o7777 != tar_mode & 0o7777 {
        return Ok(false);
    }
    if md.len() != entry.size() {
        return Ok(false);
    }
    let mut host_file = fs::File::open(host).map_err(|e| Error::Io("compare", e))?;
    let mut archive_buf = [0u8; COMPARE_CHUNK];
    let mut host_buf = [0u8; COMPARE_CHUNK];
    loop {
        let n = entry
            .read(&mut archive_buf)
            .map_err(|e| Error::Io("compare", e))?;
        if n == 0 {
            return Ok(true);
        }
        // `n <= buf.len()` by `Read`'s contract; if a subslice ever fails,
        // "differs" is the conservative answer (a spurious Modified entry,
        // never a dropped one).
        let (Some(archive_chunk), Some(host_chunk)) = (archive_buf.get(..n), host_buf.get_mut(..n))
        else {
            return Ok(false);
        };
        host_file
            .read_exact(host_chunk)
            .map_err(|e| Error::Io("compare", e))?;
        if archive_chunk != host_chunk {
            return Ok(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_paths_normalize() {
        assert_eq!(
            normalize_entry_path(Path::new("./a/b")),
            Some(PathBuf::from("a/b"))
        );
        assert_eq!(normalize_entry_path(Path::new("./")), None);
        assert_eq!(normalize_entry_path(Path::new(".")), None);
    }

    #[test]
    fn dest_path_validation_rejects_escapes() {
        let dest = std::env::temp_dir();
        assert!(validate_dest_path(&dest, Path::new("ok/file.txt")).is_ok());
        assert!(matches!(
            validate_dest_path(&dest, Path::new("../evil")),
            Err(RejectReason::ParentEscape)
        ));
        assert!(matches!(
            validate_dest_path(&dest, Path::new("a/../../evil")),
            Err(RejectReason::ParentEscape)
        ));
        assert!(matches!(
            validate_dest_path(&dest, Path::new("/abs")),
            Err(RejectReason::AbsolutePath)
        ));
    }

    #[test]
    fn dest_path_validation_rejects_symlink_components() {
        let base = std::env::temp_dir().join(format!("dmr-sanit-{}", std::process::id()));
        fs::create_dir_all(&base).unwrap();
        std::os::unix::fs::symlink("/", base.join("link")).unwrap();
        let err = validate_dest_path(&base, Path::new("link/etc/passwd")).unwrap_err();
        assert!(matches!(err, RejectReason::SymlinkComponent(_)));
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn symlink_target_validation() {
        // link at a/b -> ../ok stays inside; -> ../../out escapes.
        assert!(validate_symlink_target(Path::new("a/b"), Path::new("../ok")).is_ok());
        assert!(matches!(
            validate_symlink_target(Path::new("a/b"), Path::new("../../out")),
            Err(RejectReason::SymlinkTargetEscapes)
        ));
        assert!(matches!(
            validate_symlink_target(Path::new("a/b"), Path::new("/etc")),
            Err(RejectReason::SymlinkTargetAbsolute)
        ));
        assert!(validate_symlink_target(Path::new("top"), Path::new("sub/ok")).is_ok());
        // A top-level link has depth 0: any leading `..` escapes.
        assert!(matches!(
            validate_symlink_target(Path::new("top"), Path::new("../out")),
            Err(RejectReason::SymlinkTargetEscapes)
        ));
    }

    /// Locks `RejectReason`'s `Display` contract: the message describes the
    /// refusal; io causes live in `source()`, never in the text (C2).
    #[test]
    fn reject_reason_display_and_sources() {
        let io = || std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let with_source = [
            (RejectReason::DeleteFailed(io()), "delete failed"),
            (RejectReason::MkdirFailed(io()), "mkdir failed"),
            (RejectReason::MkdirParentFailed(io()), "mkdir parent failed"),
            (RejectReason::SymlinkFailed(io()), "symlink failed"),
            (RejectReason::WriteFailed(io()), "write failed"),
        ];
        for (reason, want) in &with_source {
            assert_eq!(reason.to_string(), *want);
            assert!(std::error::Error::source(reason).is_some(), "{want}");
        }

        let without_source = [
            (
                RejectReason::Unsupported(UnsupportedKind::CharDevice),
                "char device",
            ),
            (
                RejectReason::Unsupported(UnsupportedKind::SymlinkWithoutTarget),
                "symlink without target",
            ),
            (
                RejectReason::Unsupported(UnsupportedKind::EntryType("Fifo".into())),
                "unsupported entry type Fifo",
            ),
            (RejectReason::AbsolutePath, "absolute path"),
            (RejectReason::ParentEscape, "path contains .."),
            (
                RejectReason::SymlinkComponent(PathBuf::from("/d/link")),
                "path traverses symlink component /d/link",
            ),
            (
                RejectReason::NonNormalComponent,
                "non-normal path component",
            ),
            (
                RejectReason::SymlinkTargetAbsolute,
                "symlink target is absolute",
            ),
            (
                RejectReason::SymlinkTargetEscapes,
                "symlink target escapes destination",
            ),
            (
                RejectReason::SymlinkTargetNonNormal,
                "symlink target has non-normal component",
            ),
            (
                RejectReason::DeletionTargetMissing,
                "deletion target not found",
            ),
        ];
        for (reason, want) in &without_source {
            assert_eq!(reason.to_string(), *want);
            assert!(std::error::Error::source(reason).is_none(), "{want}");
        }
    }
}
