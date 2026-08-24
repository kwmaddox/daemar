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
    pub fn path(&self) -> &Path {
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
    unsupported: Vec<(PathBuf, String)>,
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

/// Outcome of [`ChangeSet::apply_to`]. Nothing is silently dropped: every
/// entry lands in exactly one of these lists.
#[derive(Debug, Default)]
pub struct ApplyReport {
    /// Entries written into the destination (files, dirs, symlinks).
    pub applied: Vec<PathBuf>,
    /// Deletion entries whose target existed and was removed.
    pub deleted: Vec<PathBuf>,
    /// Entries refused, with the reason (path escape, symlink escape,
    /// unsupported type, missing deletion target).
    pub rejected: Vec<(PathBuf, String)>,
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

        for entry in archive
            .entries()
            .map_err(|e| Error::Archive(e.to_string()))?
        {
            let mut entry = entry.map_err(|e| Error::Archive(e.to_string()))?;
            let rel =
                normalize_entry_path(&entry.path().map_err(|e| Error::Archive(e.to_string()))?);
            let Some(rel) = rel else { continue }; // "." / empty

            let header = entry.header();
            let mode = header.mode().unwrap_or(0o644);
            #[expect(
                clippy::wildcard_enum_match_arm,
                reason = "tar::EntryType hides a #[doc(hidden)] __Nonexhaustive variant \
                          (pre-#[non_exhaustive] idiom), so no arm can name every variant; \
                          the wildcard is the unsupported-by-default bucket, not a panic \
                          path (C4)"
            )]
            match header.entry_type() {
                tar::EntryType::Char => {
                    let major = header.device_major().ok().flatten().unwrap_or(1);
                    let minor = header.device_minor().ok().flatten().unwrap_or(1);
                    if major == 0 && minor == 0 {
                        entries.push(Change::Deleted { path: rel });
                    } else {
                        unsupported.push((rel, "char device".into()));
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
                tar::EntryType::Symlink => {
                    match entry
                        .link_name()
                        .map_err(|e| Error::Archive(e.to_string()))?
                    {
                        Some(target) => entries.push(Change::Symlink {
                            path: rel,
                            target: target.into_owned(),
                        }),
                        None => unsupported.push((rel, "symlink without target".into())),
                    }
                }
                other => {
                    unsupported.push((rel, format!("unsupported entry type {other:?}")));
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
    pub fn unsupported(&self) -> &[(PathBuf, String)] {
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
        for (path, why) in &self.unsupported {
            report.rejected.push((path.clone(), why.clone()));
        }

        // Deletions and directory creations don't need archive content.
        // Files and symlinks are re-streamed from the archive below; collect
        // the sanitized target set first.
        let mut want: BTreeMap<PathBuf, &Change> = BTreeMap::new();
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
                            .push((path.clone(), "deletion target not found".into())),
                        Err(e) => report
                            .rejected
                            .push((path.clone(), format!("delete failed: {e}"))),
                    }
                }
                Change::DirAdded { path } => match fs::create_dir_all(dest.join(path)) {
                    Ok(()) => report.applied.push(path.clone()),
                    Err(e) => report
                        .rejected
                        .push((path.clone(), format!("mkdir failed: {e}"))),
                },
                Change::Symlink { path, target } => {
                    if let Err(why) = validate_symlink_target(path, target) {
                        report.rejected.push((path.clone(), why));
                        continue;
                    }
                    want.insert(path.clone(), change);
                }
                Change::Added { path, .. } | Change::Modified { path, .. } => {
                    want.insert(path.clone(), change);
                }
            }
        }

        // Stream file/symlink content out of the archive.
        let file = fs::File::open(&self.tar_path).map_err(Error::Results)?;
        let mut archive = tar::Archive::new(file);
        for entry in archive
            .entries()
            .map_err(|e| Error::Archive(e.to_string()))?
        {
            let mut entry = entry.map_err(|e| Error::Archive(e.to_string()))?;
            let Some(rel) =
                normalize_entry_path(&entry.path().map_err(|e| Error::Archive(e.to_string()))?)
            else {
                continue;
            };
            let Some(change) = want.get(&rel) else {
                continue;
            };
            let target = dest.join(&rel);
            if let Some(parent) = target.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    report
                        .rejected
                        .push((rel.clone(), format!("mkdir parent failed: {e}")));
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
                            .push((rel.clone(), format!("symlink failed: {e}"))),
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
                            .push((rel.clone(), format!("write failed: {e}"))),
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
fn validate_dest_path(dest: &Path, rel: &Path) -> Result<(), String> {
    if rel.is_absolute() {
        return Err("absolute path".into());
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
                            return Err(format!(
                                "path traverses symlink component {}",
                                probe.display()
                            ));
                        }
                    }
                }
            }
            Component::ParentDir => return Err("path contains ..".into()),
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir => {
                return Err("non-normal path component".into())
            }
        }
    }
    Ok(())
}

/// A promoted symlink entry may not point outside the destination tree:
/// absolute targets and lexical escapes are rejected (B6).
fn validate_symlink_target(rel: &Path, target: &Path) -> Result<(), String> {
    if target.is_absolute() {
        return Err("symlink target is absolute".into());
    }
    // Resolve lexically from the symlink's parent directory: `depth` is how
    // many directories deep that parent sits inside the destination. `rel`
    // is normalized non-empty, so the subtraction cannot underflow.
    let mut depth = rel.components().count().saturating_sub(1);
    for comp in target.components() {
        match comp {
            Component::ParentDir => {
                let Some(up) = depth.checked_sub(1) else {
                    return Err("symlink target escapes destination".into());
                };
                depth = up;
            }
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::Prefix(_) | Component::RootDir => {
                return Err("symlink target has non-normal component".into())
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
    let mut archive_buf = [0u8; 8192];
    let mut host_buf = [0u8; 8192];
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
        assert!(validate_dest_path(&dest, Path::new("../evil")).is_err());
        assert!(validate_dest_path(&dest, Path::new("a/../../evil")).is_err());
        assert!(validate_dest_path(&dest, Path::new("/abs")).is_err());
    }

    #[test]
    fn dest_path_validation_rejects_symlink_components() {
        let base = std::env::temp_dir().join(format!("dmr-sanit-{}", std::process::id()));
        fs::create_dir_all(&base).unwrap();
        std::os::unix::fs::symlink("/", base.join("link")).unwrap();
        let err = validate_dest_path(&base, Path::new("link/etc/passwd")).unwrap_err();
        assert!(err.contains("symlink"));
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn symlink_target_validation() {
        // link at a/b -> ../ok stays inside; -> ../../out escapes.
        assert!(validate_symlink_target(Path::new("a/b"), Path::new("../ok")).is_ok());
        assert!(validate_symlink_target(Path::new("a/b"), Path::new("../../out")).is_err());
        assert!(validate_symlink_target(Path::new("a/b"), Path::new("/etc")).is_err());
        assert!(validate_symlink_target(Path::new("top"), Path::new("sub/ok")).is_ok());
        // A top-level link has depth 0: any leading `..` escapes.
        assert!(validate_symlink_target(Path::new("top"), Path::new("../out")).is_err());
    }
}
