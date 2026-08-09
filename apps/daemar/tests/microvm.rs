//! The microVM proof flights: real microsandbox, a real kernel per stage.
//!
//! Two kinds of ceremony live here, and the difference matters.
//!
//! FLIGHT proofs run daemar's own ceremony over the microVM wall and assert
//! the ledger is unchanged — the whole point of the seam is that a slip
//! cannot tell which wall held it. WALL proofs go around the executor with
//! a general-purpose image to assert the boundary is a boundary, because
//! the cage image deliberately contains nothing that could test it: our
//! own tools are path-confined in our own code, so a passing read proves
//! our confinement, never the kernel's.
//!
//! #[ignore]d because they need what ordinary `cargo test` must not: the
//! sandbox runtime, hardware virtualization, and a loaded image. Honest on
//! a machine without those, but never a silent skip — they are invoked
//! explicitly (`cargo test -p daemar --test microvm -- --ignored`) after
//! the image is loaded (`just wall`).

mod common;
use common::*;

use std::path::PathBuf;
use std::process::Command;

use ledger::{EventKind, Kind, Status};

fn wall_territory(name: &str) -> PathBuf {
    territory_at(PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("microvm-{name}")))
}

fn msb(args: &[&str]) -> std::process::Output {
    let bin = std::env::var("MSB_BIN").unwrap_or_else(|_| {
        format!(
            "{}/.microsandbox/bin/msb",
            std::env::var("HOME").unwrap_or_default()
        )
    });
    Command::new(bin).args(args).output().expect("msb runs")
}

fn note_field<'a>(note: &'a str, key: &str) -> Option<&'a str> {
    note.split(&format!("{key}="))
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
}

fn notes_of(events: &[ledger::Event]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| {
            if let EventKind::Known(Kind::Note { text }) = &e.kind {
                Some(text.clone())
            } else {
                None
            }
        })
        .collect()
}

// ── Flight proofs: the ceremony, over the new wall ───────────────────────────

#[test]
#[ignore = "requires microsandbox and the daemar-cage:latest image loaded (just wall)"]
fn a_scout_flies_the_microvm_wall_and_the_ledger_cannot_tell() {
    let stub = stub_server();
    let f = factory("microvm-scout", &stub);
    let t = wall_territory("scout");

    stub.push_tool_call("call_1", "read", r#"{"path":"src/lib.rs"}"#);
    stub.push_tool_call("call_2", "list_files", r#"{"recursive":true}"#);
    stub.push_ok("MICROVM REPORT: answer() lives in src/lib.rs.");
    let out = daemar_cmd(
        &f,
        &["scout", "--repo", t.to_str().unwrap(), "map the territory"],
    )
    .env("DAEMAR_CAGE", "1")
    .output()
    .expect("run daemar");
    assert_eq!(
        out.status.code(),
        Some(0),
        "microvm scout: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let (_, slip, events) = the_slip(&f);
    assert_eq!(slip.status, Status::Accepted);
    assert_eq!(slip.tool_trail.len(), 2, "both calls crossed the wall");
    assert!(
        slip.tool_trail.iter().all(|t| t.ok),
        "{:?}",
        slip.tool_trail
    );

    // The epistemic pointer survives the wall: a read still carries its
    // 16-hex content hash. This is the seam's promise, whatever holds it.
    let read_hash_ok = events.iter().any(|e| {
        if let EventKind::Known(Kind::ToolCall { tool, hash, ok, .. }) = &e.kind {
            tool == "read" && *ok && hash.len() == 16
        } else {
            false
        }
    });
    assert!(
        read_hash_ok,
        "microVM reads carry the same epistemic pointer"
    );

    let notes = notes_of(&events);
    let started = notes
        .iter()
        .position(|n| n.starts_with("sandbox started:"))
        .expect("sandbox start note");
    let torn = notes
        .iter()
        .position(|n| n.starts_with("sandbox torn down:"))
        .expect("teardown note");
    assert!(started < torn, "{notes:?}");
    assert_eq!(
        note_field(&notes[started], "wall"),
        Some("microsandbox"),
        "the receipt names the wall that held the stage: {}",
        notes[started]
    );

    // The microVM is genuinely gone — teardown is proof, not intention.
    let id = note_field(&notes[started], "sandbox_id").expect("sandbox id on the note");
    let listed = String::from_utf8_lossy(&msb(&["ls"]).stdout).to_string();
    assert!(
        !listed.contains(id),
        "the sandbox must not survive the flight: {listed}"
    );
    std::fs::remove_dir_all(&t).ok();
}

#[test]
#[ignore = "requires microsandbox and the daemar-cage:latest image loaded (just wall)"]
fn the_write_guard_survives_the_change_of_transport() {
    let stub = stub_server();
    let f = factory("microvm-guard", &stub);
    let t = wall_territory("guard");

    // A delete of a file the model never read must be refused BEFORE the
    // wall is crossed — the host holds that record, whatever the transport.
    stub.push_tool_call("call_1", "delete", r#"{"path":"src/lib.rs"}"#);
    // Now the honest sequence: read, then delete.
    stub.push_tool_call("call_2", "read", r#"{"path":"src/lib.rs"}"#);
    stub.push_tool_call("call_3", "delete", r#"{"path":"src/lib.rs"}"#);
    stub.push_ok("guard held; the delete landed after a read.");
    let out = daemar_cmd(&f, &["build", "--repo", t.to_str().unwrap(), "answer 42"])
        .output()
        .expect("run daemar");
    assert_eq!(
        out.status.code(),
        Some(0),
        "microvm build: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let (_, slip, events) = the_slip(&f);
    let deletes: Vec<_> = slip
        .tool_trail
        .iter()
        .filter(|c| c.tool == "delete")
        .collect();
    assert_eq!(deletes.len(), 2, "both delete attempts are on the trail");
    assert!(!deletes[0].ok, "an unread file cannot be deleted");
    assert!(deletes[1].ok, "a read file can: {:?}", deletes[1]);

    // Provenance survived the transport change: the successful delete says
    // what it removed FROM, with no post-image to report.
    let before_recorded = events.iter().any(|e| {
        if let EventKind::Known(Kind::ToolCall {
            tool,
            ok,
            before_hash,
            ..
        }) = &e.kind
        {
            tool == "delete" && *ok && before_hash.as_ref().is_some_and(|h| h.len() == 16)
        } else {
            false
        }
    });
    assert!(
        before_recorded,
        "the caged delete records its preimage hash"
    );
    let diff_names_deletion = events.iter().any(|e| {
        if let EventKind::Known(Kind::SectionWritten { section, body, .. }) = &e.kind {
            section == "diff.v1" && body.contains("src/lib.rs")
        } else {
            false
        }
    });
    assert!(
        diff_names_deletion,
        "the diff receipt names the deleted file"
    );
    std::fs::remove_dir_all(&t).ok();
}

// ── Wall proofs: the boundary itself ─────────────────────────────────────────

/// A general-purpose image, because the cage image contains nothing that
/// could test a kernel. These assert what the CONTAINER wall could never
/// have claimed.
#[test]
#[ignore = "requires microsandbox and a local ubuntu image (msb pull ubuntu)"]
fn each_stage_gets_its_own_kernel_and_the_host_is_not_in_it() {
    let one = "daemar-proof-iso-1";
    let two = "daemar-proof-iso-2";
    for name in [one, two] {
        msb(&["rm", "-f", name]);
        let created = msb(&["create", "--name", name, "ubuntu"]);
        assert!(
            created.status.success(),
            "create {name}: {}",
            String::from_utf8_lossy(&created.stderr)
        );
    }
    let run = |name: &str, script: &str| -> String {
        let out = msb(&["exec", name, "--", "bash", "-c", script]);
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    // Its own kernel: a kernel-wide change in one sandbox cannot be seen
    // in the other. A container wall shares one kernel and fails this.
    run(
        one,
        "sysctl -w kernel.hostname=walled-one >/dev/null 2>&1 || true",
    );
    assert_eq!(run(one, "uname -n"), "walled-one", "the change took");
    assert_ne!(
        run(two, "uname -n"),
        "walled-one",
        "kernel state must not cross between sandboxes"
    );

    // The host is not in there. These paths exist on the controller's
    // machine and must be absent inside.
    assert_eq!(
        run(
            one,
            "ls /Users /System /Volumes >/dev/null 2>&1 && echo VISIBLE || echo ABSENT"
        ),
        "ABSENT",
        "no host filesystem inside the wall"
    );

    // Guest root writes land in an ephemeral overlay, not on the host.
    run(one, "touch /escaped-to-host");
    assert!(
        !std::path::Path::new("/escaped-to-host").exists(),
        "a guest-root write must never surface on the host"
    );

    for name in [one, two] {
        msb(&["rm", "-f", name]);
    }
}

#[test]
#[ignore = "requires microsandbox and a local ubuntu image (msb pull ubuntu)"]
fn a_walled_stage_has_no_network_at_all() {
    let name = "daemar-proof-nonet";
    msb(&["rm", "-f", name]);
    let created = msb(&["create", "--name", name, "--no-net", "ubuntu"]);
    assert!(
        created.status.success(),
        "create: {}",
        String::from_utf8_lossy(&created.stderr)
    );
    let run = |script: &str| -> String {
        let out = msb(&["exec", name, "--", "bash", "-c", script]);
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    assert_eq!(
        run("getent hosts example.com >/dev/null 2>&1 && echo RESOLVED || echo DENIED"),
        "DENIED",
        "name resolution is denied"
    );
    // And no escape by hard-coded address, which is how a denied name is
    // usually worked around.
    assert_eq!(
        run("timeout 6 bash -c 'echo > /dev/tcp/1.1.1.1/443' 2>/dev/null && echo OPEN || echo DENIED"),
        "DENIED",
        "raw-IP egress is denied too"
    );
    msb(&["rm", "-f", name]);
}
