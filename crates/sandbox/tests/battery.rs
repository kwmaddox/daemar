//! The adversarial battery: every test cites the behavior(s) it proves from
//! `specs/sandbox.md`. These boot real VMs via the `container` CLI, so they
//! are `#[ignore]`d by default — run with:
//!
//!   cargo test -p daemar-sandbox --test battery -- --ignored --test-threads=1
//!
//! Ported from the hands-on proof in
//! `docs/research/apple-container-proof.md`, plus the rules earned by
//! `docs/research/substrate-refutation.md` (egress asserted on raw IP, never
//! DNS; the probe itself validated against an open-network control).

// clippy's `allow-*-in-tests` config does not reach integration-test
// targets (rust-clippy#13981), so the C4 test exemption is granted here
// explicitly. `#![expect]` per C5: this self-reports if the battery ever
// stops using these.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test code is exempt from C4; allow-*-in-tests cannot cover \
              integration targets (rust-clippy#13981)"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use daemar_sandbox::{Change, RunSpec};

/// One VM at a time: guest memory is never returned to macOS, and serial
/// runs keep failures attributable.
fn serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn fixture_worktree(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("daemar-battery").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("subdir")).unwrap();
    fs::write(dir.join("existing.txt"), "original content\n").unwrap();
    fs::write(dir.join("doomed.txt"), "delete me\n").unwrap();
    fs::write(dir.join("subdir/nested.txt"), "nested\n").unwrap();
    dir
}

fn sh(worktree: &Path, script: &str) -> RunSpec {
    RunSpec::new(vec!["sh".into(), "-c".into(), script.into()], worktree)
}

/// B9: no daemar container may remain after any run.
fn assert_no_leaked_containers() {
    let out = Command::new("container")
        .args(["ls", "-a"])
        .output()
        .unwrap();
    let listing = String::from_utf8_lossy(&out.stdout);
    assert!(
        !listing.contains("daemar-"),
        "leaked containers (B9):\n{listing}"
    );
}

/// Snapshot a tree as (relative path, kind, mode, content-hash) tuples.
fn tree_snapshot(root: &Path) -> Vec<String> {
    use std::os::unix::fs::PermissionsExt;
    fn walk(root: &Path, dir: &Path, acc: &mut Vec<String>) {
        let mut names: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        names.sort();
        for path in names {
            let rel = path.strip_prefix(root).unwrap().to_path_buf();
            let md = fs::symlink_metadata(&path).unwrap();
            let mode = md.permissions().mode();
            if md.is_dir() {
                acc.push(format!("d {mode:o} {}", rel.display()));
                walk(root, &path, acc);
            } else {
                let body = fs::read(&path).unwrap_or_default();
                acc.push(format!("f {mode:o} {} {:x?}", rel.display(), body));
            }
        }
    }
    let mut acc = Vec::new();
    walk(root, root, &mut acc);
    acc
}

// ── B1 ───────────────────────────────────────────────────────────────────

#[test]
#[ignore = "boots a real VM via the container CLI; run explicitly with --ignored"]
fn b1_command_runs_in_a_vm_with_its_own_kernel() {
    let _guard = serial();
    let wt = fixture_worktree("b1");
    // A Linux kernel on a macOS host is already proof of a VM; distinct
    // boot_ids across runs prove a fresh kernel per run, not a shared guest.
    let boot_id = |_: usize| {
        let outcome = daemar_sandbox::run(&sh(&wt, "cat /proc/sys/kernel/random/boot_id")).unwrap();
        assert_eq!(outcome.exit_code, 0);
        String::from_utf8(outcome.stdout)
            .unwrap()
            .trim()
            .to_string()
    };
    let first = boot_id(0);
    let second = boot_id(1);
    assert!(!first.is_empty());
    assert_ne!(first, second, "two runs shared one kernel (B1)");
    assert_no_leaked_containers();
}

// ── B2 ───────────────────────────────────────────────────────────────────

/// Gateway IP of the substrate's default NAT network (host side of vmnet).
/// A host listener there is reachable from a default-network guest even when
/// host VPN tunnels break internet forwarding (observed: issue #1307 class),
/// so probe validation does not depend on external connectivity.
fn nat_gateway_ip() -> String {
    let out = Command::new("container")
        .args(["network", "list"])
        .output()
        .unwrap();
    let listing = String::from_utf8_lossy(&out.stdout);
    let subnet = listing
        .lines()
        .find(|l| l.starts_with("default"))
        .and_then(|l| l.split_whitespace().nth(1))
        .expect("default network subnet");
    let base = subnet.split('/').next().unwrap();
    let mut octets: Vec<&str> = base.split('.').collect();
    octets[3] = "1";
    octets.join(".")
}

/// A raw-IP TCP probe: bash's /dev/tcp performs a real connect(2); exit 0
/// iff the connection succeeded.
fn tcp_probe(ip: &str, port: u16) -> String {
    format!("timeout 5 bash -c 'echo probe > /dev/tcp/{ip}/{port}'")
}

#[test]
#[ignore = "boots a real VM via the container CLI; run explicitly with --ignored"]
fn b2_probe_detects_open_egress_control() {
    let _guard = serial();
    // The #2062 lesson: a probe that fails for an unrelated reason makes an
    // isolation test pass for the wrong reason. Validate the probe against a
    // host-side listener over the substrate's OPEN default network
    // (bypassing the library, which hard-codes --network none): the connect
    // must SUCCEED and the payload must arrive, or the B2 assertion below
    // proves nothing. (Probing a public IP instead would couple the control
    // to internet egress, which host VPNs are known to break.)
    let listener = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let accept = std::thread::spawn(move || {
        listener.set_nonblocking(false).unwrap();
        let mut buf = Vec::new();
        if let Ok((mut sock, _)) = listener.accept() {
            use std::io::Read;
            let _ = sock.read_to_end(&mut buf);
        }
        buf
    });

    let gateway = nat_gateway_ip();
    let out = Command::new("container")
        .args([
            "run",
            "--rm",
            "--progress",
            "none",
            daemar_sandbox::DEFAULT_IMAGE,
            "bash",
            "-c",
        ])
        .arg(tcp_probe(&gateway, port))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "egress probe failed on an open network — probe is invalid, B2 unprovable: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let received = accept.join().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&received).trim(),
        "probe",
        "listener never received the probe payload"
    );
}

#[test]
#[ignore = "boots a real VM via the container CLI; run explicitly with --ignored"]
fn b2_no_network_path_raw_ip_fails() {
    let _guard = serial();
    let wt = fixture_worktree("b2");
    // Under the library's --network none: TCP to a public IP, UDP send, AND
    // TCP to a live host-side listener on the NAT gateway must all fail —
    // there is no path even to the host. Asserted on raw IP, never on name
    // resolution (B2).
    let listener = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let gateway = nat_gateway_ip();

    let outcome = daemar_sandbox::run(&sh(
        &wt,
        &format!(
            "{pub_probe}; tcp=$?; \
             timeout 5 bash -c 'echo x > /dev/udp/8.8.8.8/53'; udp=$?; \
             {gw_probe}; gw=$?; \
             echo tcp=$tcp udp=$udp gw=$gw; \
             test $tcp -ne 0 -a $udp -ne 0 -a $gw -ne 0",
            pub_probe = tcp_probe("1.1.1.1", 443),
            gw_probe = tcp_probe(&gateway, port),
        ),
    ))
    .unwrap();
    assert_eq!(
        outcome.exit_code,
        0,
        "raw-IP egress was possible (B2): {}",
        String::from_utf8_lossy(&outcome.stdout)
    );
    drop(listener);
    assert_no_leaked_containers();
}

// ── B3 / B4 / B7 ─────────────────────────────────────────────────────────

#[test]
#[ignore = "boots a real VM via the container CLI; run explicitly with --ignored"]
fn b3_host_worktree_byte_identical_after_hostile_run() {
    let _guard = serial();
    let wt = fixture_worktree("b3");
    let before = tree_snapshot(&wt);
    let outcome = daemar_sandbox::run(&sh(
        &wt,
        "echo clobber > existing.txt && rm doomed.txt && chmod 777 subdir/nested.txt \
         && echo new > added.txt && echo direct > /daemar/lower/breach.txt; true",
    ))
    .unwrap();
    assert_eq!(outcome.exit_code, 0);
    // The direct write to the ro lower mount must have failed (B3/B4).
    assert!(String::from_utf8_lossy(&outcome.stderr).contains("Read-only"));
    assert_eq!(before, tree_snapshot(&wt), "host worktree mutated (B3)");
    assert_no_leaked_containers();
}

#[test]
#[ignore = "boots a real VM via the container CLI; run explicitly with --ignored"]
fn b4_command_edits_worktree_in_place_in_its_own_view() {
    let _guard = serial();
    let wt = fixture_worktree("b4");
    // The command sees existing files at its cwd and edits them in place —
    // its view, not the host's (B4).
    let outcome = daemar_sandbox::run(&sh(
        &wt,
        "grep -q original existing.txt && echo edited >> existing.txt && tail -1 existing.txt",
    ))
    .unwrap();
    assert_eq!(outcome.exit_code, 0);
    assert_eq!(String::from_utf8_lossy(&outcome.stdout).trim(), "edited");
    assert_eq!(
        fs::read_to_string(wt.join("existing.txt")).unwrap(),
        "original content\n"
    );
    assert_no_leaked_containers();
}

#[test]
#[ignore = "boots a real VM via the container CLI; run explicitly with --ignored"]
fn b7_nothing_outside_worktree_mount_is_readable() {
    let _guard = serial();
    let wt = fixture_worktree("b7");
    // Host secret in a SIBLING of the worktree — outside the mounted subtree.
    let secret_dir = wt.parent().unwrap().join("b7-secret-sibling");
    fs::create_dir_all(&secret_dir).unwrap();
    fs::write(secret_dir.join("secret.txt"), "S3CRET-MARKER\n").unwrap();
    let host_secret = secret_dir.join("secret.txt");

    // The proof-doc vectors: parent traversal, absolute host path,
    // symlink-to-root, /proc/self/root.
    let script = format!(
        "ln -s / slash 2>/dev/null; \
         for p in /daemar/lower/../b7-secret-sibling/secret.txt \
                  '{abs}' \
                  slash{abs} \
                  /proc/self/root{abs}; do \
           cat \"$p\" 2>/dev/null; \
         done; true",
        abs = host_secret.display()
    );
    let outcome = daemar_sandbox::run(&sh(&wt, &script)).unwrap();
    assert_eq!(outcome.exit_code, 0);
    assert!(
        !String::from_utf8_lossy(&outcome.stdout).contains("S3CRET-MARKER"),
        "host secret readable from guest (B7)"
    );
    assert_no_leaked_containers();
}

// ── B5 / B6 ──────────────────────────────────────────────────────────────

#[test]
#[ignore = "boots a real VM via the container CLI; run explicitly with --ignored"]
fn b5_changeset_reports_exactly_the_changes() {
    let _guard = serial();
    let wt = fixture_worktree("b5");
    let outcome = daemar_sandbox::run(&sh(
        &wt,
        "echo new > added.txt && echo more >> existing.txt && rm doomed.txt \
         && mkdir newdir && echo deep > newdir/deep.txt",
    ))
    .unwrap();
    assert_eq!(outcome.exit_code, 0);

    let mut got: Vec<String> = outcome
        .changes
        .entries()
        .iter()
        .map(|c| match c {
            Change::Added { path, .. } => format!("added {}", path.display()),
            Change::Modified { path, .. } => format!("modified {}", path.display()),
            Change::Deleted { path } => format!("deleted {}", path.display()),
            Change::DirAdded { path } => format!("dir {}", path.display()),
            Change::Symlink { path, .. } => format!("symlink {}", path.display()),
        })
        .collect();
    got.sort();
    assert_eq!(
        got,
        vec![
            "added added.txt",
            "added newdir/deep.txt",
            "deleted doomed.txt",
            "dir newdir",
            "modified existing.txt",
        ],
        "change-set is not exactly the changes (B5)"
    );
    assert!(outcome.changes.unsupported().is_empty());
    assert_no_leaked_containers();
}

#[test]
#[ignore = "boots a real VM via the container CLI; run explicitly with --ignored"]
fn b6_promotion_sanitizes_setuid_and_symlink_escapes() {
    use std::os::unix::fs::PermissionsExt;
    let _guard = serial();
    let wt = fixture_worktree("b6");
    let outcome = daemar_sandbox::run(&sh(
        &wt,
        "cp /bin/true suid-bin && chmod 4755 suid-bin \
         && ln -s /etc/passwd abs-link \
         && ln -s ../../escape rel-escape-link \
         && ln -s subdir/nested.txt ok-link",
    ))
    .unwrap();
    assert_eq!(outcome.exit_code, 0);

    let dest = std::env::temp_dir().join("daemar-battery").join("b6-dest");
    let _ = fs::remove_dir_all(&dest);
    fs::create_dir_all(&dest).unwrap();
    let report = outcome.changes.apply_to(&dest).unwrap();

    // setuid stripped (B6)
    assert!(report.stripped.iter().any(|p| p.ends_with("suid-bin")));
    let mode = fs::metadata(dest.join("suid-bin"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o7000, 0, "setuid survived promotion (B6)");

    // escaping symlinks rejected, in-tree symlink allowed (B6)
    let rejected: Vec<_> = report.rejected.iter().map(|(p, _)| p.clone()).collect();
    assert!(rejected.iter().any(|p| p.ends_with("abs-link")));
    assert!(rejected.iter().any(|p| p.ends_with("rel-escape-link")));
    assert!(dest.join("abs-link").symlink_metadata().is_err());
    assert!(dest.join("ok-link").symlink_metadata().is_ok());
    assert_no_leaked_containers();
}

#[test]
#[ignore = "boots a real VM via the container CLI; run explicitly with --ignored"]
fn b6_promotion_roundtrip_into_worktree() {
    let _guard = serial();
    let wt = fixture_worktree("b6rt");
    let outcome = daemar_sandbox::run(&sh(
        &wt,
        "echo hi > new.txt && rm doomed.txt && echo tail >> existing.txt",
    ))
    .unwrap();
    let report = outcome.changes.apply_to(&wt).unwrap();
    assert!(
        report.rejected.is_empty(),
        "unexpected rejects: {:?}",
        report.rejected
    );
    assert_eq!(fs::read_to_string(wt.join("new.txt")).unwrap(), "hi\n");
    assert!(!wt.join("doomed.txt").exists());
    assert_eq!(
        fs::read_to_string(wt.join("existing.txt")).unwrap(),
        "original content\ntail\n"
    );
    assert_no_leaked_containers();
}

// ── B8 / B9 ──────────────────────────────────────────────────────────────

#[test]
#[ignore = "boots a real VM via the container CLI; run explicitly with --ignored"]
fn b8_b9_timeout_kills_and_leaves_nothing_behind() {
    let _guard = serial();
    let wt = fixture_worktree("b8");
    let mut spec = sh(&wt, "sleep 120");
    spec.timeout = Duration::from_secs(20);
    let err = daemar_sandbox::run(&spec).unwrap_err();
    assert!(
        matches!(err, daemar_sandbox::Error::Timeout(_)),
        "expected timeout, got: {err}"
    );
    // Reap is asynchronous-ish; give the runtime a beat, then insist (B9).
    std::thread::sleep(Duration::from_secs(2));
    assert_no_leaked_containers();
    // Session temp state is gone too (B9).
    let sessions = std::env::temp_dir().join("daemar-sandbox");
    let leftovers = fs::read_dir(&sessions).map_or(0, Iterator::count);
    assert_eq!(leftovers, 0, "session dirs left behind (B9)");
}

// ── B10 ──────────────────────────────────────────────────────────────────

#[test]
#[ignore = "boots a real VM via the container CLI; run explicitly with --ignored"]
fn b10_stdout_stderr_exit_code_roundtrip() {
    let _guard = serial();
    let wt = fixture_worktree("b10");
    let outcome =
        daemar_sandbox::run(&sh(&wt, "echo to-stdout; echo to-stderr >&2; exit 7")).unwrap();
    assert_eq!(outcome.exit_code, 7, "exit code not faithful (B10)");
    assert_eq!(String::from_utf8_lossy(&outcome.stdout).trim(), "to-stdout");
    assert!(String::from_utf8_lossy(&outcome.stderr).contains("to-stderr"));
    assert!(outcome.changes.is_empty());
    assert_no_leaked_containers();
}
