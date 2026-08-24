//! The in-guest driver script.
//!
//! A generated POSIX-sh script mounted read-only into the guest. It mounts
//! the overlay (B4), execs the workload with the caller's argv passed as
//! script arguments (no quoting/injection surface), and exports the entire
//! overlay upper layer as a tar into the rw out-mount. Tar carries whiteout
//! char-devices, modes, and symlinks natively, so all parsing happens on the
//! trusted host side.
//!
//! Driver-stage failures are distinguished from workload exit codes by the
//! absence of `out/exit_code`: the driver writes that file only after the
//! workload has run, so a container exit without it means the driver itself
//! failed (stage encoded in the exit code below).

/// Exit codes for failures before the workload runs. Only meaningful when
/// `out/exit_code` was never written.
pub const EXIT_MKDIR: i32 = 124;
pub const EXIT_OVERLAY: i32 = 125;
pub const EXIT_CD: i32 = 126;
/// Export failure happens *after* the workload; `out/exit_code` is still
/// written in that case so the workload's code is preserved, but
/// `out/changes.tar` will be absent.
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
tar -C {GUEST_OVL}/upper -cf {GUEST_OUT}/{CHANGES_TAR} . || fail "export" {EXIT_EXPORT}
exit "$rc"
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

    #[test]
    fn driver_failure_codes_are_distinct() {
        let codes = [EXIT_MKDIR, EXIT_OVERLAY, EXIT_CD, EXIT_EXPORT];
        let mut dedup = codes.to_vec();
        dedup.dedup();
        assert_eq!(dedup.len(), codes.len());
    }
}
