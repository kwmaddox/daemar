//! The in-guest driver script.
//!
//! A generated POSIX-sh script mounted read-only into the guest. It mounts
//! the overlay (B4), execs the workload with the caller's argv passed as
//! script arguments (no quoting/injection surface), and exports the entire
//! overlay upper layer as a tar into the rw out-mount. Tar carries whiteout
//! char-devices, modes, and symlinks natively, so all parsing happens on the
//! trusted host side.
//!
//! Run success is signaled by the container exit status alone (PER-80):
//! the driver exits 0 only after the full chain — workload ran, its exit
//! code recorded, overlay upper exported — has succeeded, and never
//! mirrors the workload's own exit code (that travels via `out/exit_code`
//! exclusively). The status is the one channel the workload cannot write:
//! this script lives on the read-only bin mount, runs as guest PID 1, and
//! the status is reported by the trusted host-side `container` CLI —
//! whereas the out-mount is guest-writable, so nothing in it can serve as
//! a success marker (any file there is forgeable by a racing workload
//! descendant). A nonzero status therefore always means failure, with the
//! stage encoded in the codes below.
//!
//! The archive is still written to a partial name and `mv`ed into place
//! only after tar succeeds. That rename is belt-and-braces, not
//! load-bearing: it keeps driver-written partials off the final name (GNU
//! tar does not remove its output on failure), but the runner trusts the
//! archive only under container status 0, never from its presence.

/// Exit codes for failures before the workload runs.
pub const EXIT_MKDIR: i32 = 124;
pub const EXIT_OVERLAY: i32 = 125;
pub const EXIT_CD: i32 = 126;
/// Export failure happens *after* the workload; `out/exit_code` is
/// already written in that case so the workload's code is preserved.
/// Because the driver exits 0 on success instead of mirroring the
/// workload's exit code, this status is unambiguous: a workload exiting
/// 127 yields container status 0, so 124–127 can only originate from
/// `fail()`.
pub const EXIT_EXPORT: i32 = 127;

/// Guest-side mount points. The host maps: worktree -> LOWER (ro),
/// session out/ -> OUT (rw), session bin/ -> BIN (ro). OVL is a runtime
/// tmpfs (`--tmpfs`), so guest writes never touch a host surface except OUT.
pub const GUEST_LOWER: &str = "/daemar/lower";
pub const GUEST_OUT: &str = "/daemar/out";
pub const GUEST_BIN: &str = "/daemar/bin";
pub const GUEST_OVL: &str = "/daemar/ovl";

pub const DRIVER_FILENAME: &str = "driver.sh";
pub const CHANGES_TAR: &str = "changes.tar";
/// In-progress name the export tar writes to; renamed to [`CHANGES_TAR`]
/// only on tar success. Belt-and-braces (the success signal is the
/// container exit status): it keeps driver-written partials off the final
/// name. The runner never reads this name.
pub const CHANGES_TAR_PARTIAL: &str = "changes.tar.partial";
pub const EXIT_CODE_FILE: &str = "exit_code";

pub fn driver_script() -> String {
    format!(
        r#"#!/bin/sh
set -u
fail() {{ echo "daemar-driver: $1" >&2; exit "$2"; }}
mkdir -p {GUEST_OVL}/upper {GUEST_OVL}/work {GUEST_OVL}/merged || fail "mkdir" {EXIT_MKDIR}
mount -t overlay overlay \
  -o lowerdir={GUEST_LOWER},upperdir={GUEST_OVL}/upper,workdir={GUEST_OVL}/work \
  {GUEST_OVL}/merged || fail "overlay mount" {EXIT_OVERLAY}
cd {GUEST_OVL}/merged || fail "cd" {EXIT_CD}
"$@"
rc=$?
echo "$rc" > {GUEST_OUT}/{EXIT_CODE_FILE}
tar -C {GUEST_OVL}/upper -cf {GUEST_OUT}/{CHANGES_TAR_PARTIAL} . || fail "export" {EXIT_EXPORT}
mv {GUEST_OUT}/{CHANGES_TAR_PARTIAL} {GUEST_OUT}/{CHANGES_TAR} || fail "export" {EXIT_EXPORT}
exit 0
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_is_posix_sh_with_expected_stages() {
        let s = driver_script();
        assert!(s.starts_with("#!/bin/sh"));
        // Workload argv arrives as script args, never spliced into the text.
        assert!(s.contains(r#""$@""#));
        assert!(!s.contains("bash"));
        // exit_code is written before the export, so an export failure
        // still leaves the workload's code readable.
        let code_pos = s.find(EXIT_CODE_FILE).unwrap();
        let tar_pos = s.find(CHANGES_TAR).unwrap();
        assert!(code_pos < tar_pos);
    }

    /// PER-80: the container exit status is the success signal — the
    /// driver exits 0 on full success and never mirrors the workload's
    /// exit code, so a nonzero status always means failure and 124–127
    /// can only originate from `fail()`.
    #[test]
    fn success_exits_zero_never_mirroring_workload_rc() {
        let s = driver_script();
        assert!(s.trim_end().ends_with("exit 0"));
        assert!(!s.contains(r#"exit "$rc""#));
        // The workload's rc still reaches the host — via the file channel.
        assert!(s.contains(&format!(r#"echo "$rc" > {GUEST_OUT}/{EXIT_CODE_FILE}"#)));
    }

    /// PER-79, demoted to belt-and-braces by PER-80: a *driver-written*
    /// partial never sits at the final name — tar targets only the
    /// partial name and the rename happens only on tar success. Not
    /// load-bearing (the success signal is the container exit status);
    /// it keeps the accidental-truncation class dead even if the
    /// status gate ever regresses.
    #[test]
    fn export_writes_partial_name_and_renames_on_success() {
        let s = driver_script();
        assert_ne!(CHANGES_TAR, CHANGES_TAR_PARTIAL);
        let tar_write = format!("-cf {GUEST_OUT}/{CHANGES_TAR_PARTIAL}");
        let rename = format!("mv {GUEST_OUT}/{CHANGES_TAR_PARTIAL} {GUEST_OUT}/{CHANGES_TAR}");
        let tar_pos = s.find(&tar_write).expect("tar writes the partial name");
        let mv_pos = s.find(&rename).expect("mv into place follows tar success");
        assert!(tar_pos < mv_pos);
        // The final name is never a write target — it only ever appears as
        // the rename destination.
        assert!(!s.contains(&format!("-cf {GUEST_OUT}/{CHANGES_TAR} ")));
    }

    #[test]
    fn driver_failure_codes_are_distinct() {
        let codes = [EXIT_MKDIR, EXIT_OVERLAY, EXIT_CD, EXIT_EXPORT];
        let mut dedup = codes.to_vec();
        dedup.dedup();
        assert_eq!(dedup.len(), codes.len());
    }
}
