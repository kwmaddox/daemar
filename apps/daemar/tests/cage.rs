//! The caged proof flights: real Docker, the real image, the real executor.
//!
//! These tests are #[ignore]d so `cargo test` stays honest on machines
//! without Docker — but they are NOT optional: CI invokes them explicitly
//! (`cargo test -p daemar --test cage -- --ignored`) after building the
//! image, so the cage is a required check, never a silent skip.
//!
//! Territories live under CARGO_TARGET_TMPDIR (inside the repo) so Docker
//! Desktop's default file sharing can bind-mount them.

mod common;
use common::*;

use std::path::PathBuf;
use std::process::Command;

use ledger::{EventKind, Kind, Status};

fn cage_territory(name: &str) -> PathBuf {
    territory_at(PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("territory-{name}")))
}

fn docker(args: &[&str]) -> std::process::Output {
    Command::new("docker")
        .args(args)
        .output()
        .expect("docker runs")
}

fn note_field<'a>(note: &'a str, key: &str) -> Option<&'a str> {
    note.split(&format!("{key}="))
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
}

#[test]
#[ignore = "requires docker and the daemar-cage:latest image"]
fn the_caged_scout_proof_flight() {
    let stub = stub_server();
    let f = factory("cage-proof", &stub);
    let t = cage_territory("cage-proof");

    // Three tools, then the report: every call crosses the container.
    stub.push_tool_call("call_1", "read", r#"{"path":"src/lib.rs"}"#);
    stub.push_tool_call("call_2", "list_files", r#"{"recursive":true}"#);
    stub.push_tool_call("call_3", "search", r#"{"pattern":"answer"}"#);
    stub.push_ok("CAGED REPORT: answer() lives in src/lib.rs.");
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
        "caged scout: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let (_, slip, events) = the_slip(&f);
    assert_eq!(slip.status, Status::Accepted);
    assert_eq!(slip.tool_trail.len(), 3, "three caged calls on the trail");
    assert!(
        slip.tool_trail.iter().all(|t| t.ok),
        "{:?}",
        slip.tool_trail
    );

    // The ledger shape is executor-blind: the read still carries its hash.
    let read_hash_ok = events.iter().any(|e| {
        if let EventKind::Known(Kind::ToolCall { tool, hash, ok, .. }) = &e.kind {
            tool == "read" && *ok && hash.len() == 16
        } else {
            false
        }
    });
    assert!(read_hash_ok, "caged reads carry the same epistemic pointer");

    // Lifecycle notes: materialized, started, torn down — in that order.
    let notes: Vec<String> = events
        .iter()
        .filter_map(|e| {
            if let EventKind::Known(Kind::Note { text }) = &e.kind {
                Some(text.clone())
            } else {
                None
            }
        })
        .collect();
    let materialized = notes
        .iter()
        .position(|n| n.starts_with("worktree materialized:"))
        .expect("materialization note");
    let started = notes
        .iter()
        .position(|n| n.starts_with("sandbox started:"))
        .expect("sandbox start note");
    let torn = notes
        .iter()
        .position(|n| n.starts_with("sandbox torn down:"))
        .expect("teardown note");
    assert!(materialized < started && started < torn, "{notes:?}");

    // The container is genuinely gone.
    let container = note_field(&notes[started], "container").expect("container id on the note");
    let inspect = docker(&["inspect", container]);
    assert!(
        !inspect.status.success(),
        "the container must not survive the flight"
    );
    std::fs::remove_dir_all(&t).ok();
}

#[test]
#[ignore = "requires docker and the daemar-cage:latest image"]
fn the_builder_mutates_a_caged_worktree_and_the_diff_cocks() {
    let stub = stub_server();
    let f = factory("cage-build", &stub);
    let t = cage_territory("cage-build");

    // Read, edit, create — then report. DAEMAR_CAGE is deliberately UNSET:
    // a write seat cages unconditionally.
    stub.push_tool_call("call_1", "read", r#"{"path":"src/lib.rs"}"#);
    stub.push_tool_call(
        "call_2",
        "edit",
        r#"{"path":"src/lib.rs","old":"42","new":"43"}"#,
    );
    stub.push_tool_call(
        "call_3",
        "write",
        r#"{"path":"src/fresh.rs","content":"pub fn fresh() {}\n"}"#,
    );
    stub.push_ok("BUILT: answer now returns 43; added src/fresh.rs.");
    let out = daemar_cmd(
        &f,
        &["build", "--repo", t.to_str().unwrap(), "make answer 43"],
    )
    .output()
    .expect("run daemar");
    assert_eq!(
        out.status.code(),
        Some(0),
        "build flight: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let (_, slip, events) = the_slip(&f);
    assert_eq!(
        slip.cocked.as_deref(),
        Some("build->apply"),
        "a nonempty diff cocks at the apply boundary"
    );
    assert_eq!(slip.tool_trail.len(), 3);
    assert!(
        slip.tool_trail.iter().all(|t| t.ok),
        "{:?}",
        slip.tool_trail
    );

    // The SOURCE territory is untouched; the WORKTREE carries the change.
    let source = std::fs::read_to_string(t.join("src/lib.rs")).unwrap();
    assert!(source.contains("42"), "the live checkout is never touched");
    let wt_note = events
        .iter()
        .find_map(|e| {
            if let EventKind::Known(Kind::Note { text }) = &e.kind {
                text.strip_prefix("worktree materialized: ")
                    .map(str::to_string)
            } else {
                None
            }
        })
        .expect("materialization note");
    let wt_path = note_field(&wt_note, "path").expect("path on the note");
    let mutated = std::fs::read_to_string(std::path::Path::new(wt_path).join("src/lib.rs"))
        .expect("the worktree is retained as the apply artifact");
    assert!(mutated.contains("43"), "the worktree carries the edit");
    assert!(
        std::path::Path::new(wt_path).join("src/fresh.rs").exists(),
        "the worktree carries the new file"
    );

    // The edit receipt reconstructs what-changed-from-what.
    let provenance = events.iter().any(|e| {
        if let EventKind::Known(Kind::ToolCall {
            tool,
            ok,
            hash,
            before_hash,
            ..
        }) = &e.kind
        {
            tool == "edit"
                && *ok
                && hash.len() == 16
                && before_hash.as_ref().map(String::len) == Some(16)
        } else {
            false
        }
    });
    assert!(
        provenance,
        "the mutation carries pre- and post-image hashes"
    );

    // The code-owned diff testifies to both mutations; gate:diff signs it.
    let diff = slip
        .sections
        .iter()
        .find(|s| s.section == "diff.v1")
        .expect("diff.v1 on the slip");
    assert_eq!(diff.by, "gate:diff");
    assert!(diff.body.contains("43"), "the edit is in the patch");
    assert!(
        diff.body.contains("fresh.rs"),
        "the new file is in the patch"
    );
    assert!(
        slip.sections.iter().any(|s| s.section == "build.v1"),
        "the builder's own report rides beside the diff"
    );

    // The cage is gone; the worktree remains.
    let notes: Vec<String> = events
        .iter()
        .filter_map(|e| {
            if let EventKind::Known(Kind::Note { text }) = &e.kind {
                Some(text.clone())
            } else {
                None
            }
        })
        .collect();
    let started = notes
        .iter()
        .find(|n| n.starts_with("sandbox started:"))
        .expect("start note");
    let container = note_field(started, "container").expect("container id");
    assert!(!docker(&["inspect", container]).status.success());
    std::fs::remove_dir_all(&t).ok();
}

#[test]
#[ignore = "requires docker and the daemar-cage:latest image"]
fn the_full_write_ceremony_lands_a_caged_change() {
    // Build through the cage, grant with the pen, continue through the
    // gate: request to reachable commit, end to end.
    let stub = stub_server();
    let f = factory("cage-land", &stub);
    let t = cage_territory("cage-land");

    stub.push_tool_call("call_1", "read", r#"{"path":"src/lib.rs"}"#);
    stub.push_tool_call(
        "call_2",
        "edit",
        r#"{"path":"src/lib.rs","old":"42","new":"43"}"#,
    );
    stub.push_ok("BUILT: answer returns 43.");
    let out = daemar_cmd(
        &f,
        &["build", "--repo", t.to_str().unwrap(), "make answer 43"],
    )
    .output()
    .expect("run daemar");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let (id, slip, _) = the_slip(&f);
    assert_eq!(slip.cocked.as_deref(), Some("build->apply"));

    let out = daemar(&f, &["grant", &id]);
    assert_eq!(exit_code(&out), 0, "{}", stderr(&out));
    let out = daemar(&f, &["continue", &id]);
    assert_eq!(exit_code(&out), 0, "{}", stderr(&out));

    let source = std::fs::read_to_string(t.join("src/lib.rs")).unwrap();
    assert!(source.contains("43"), "the change is in the territory");
    let log = Command::new("git")
        .arg("-C")
        .arg(&t)
        .args(["log", "-1", "--format=%an|%s"])
        .output()
        .expect("git");
    let log = String::from_utf8_lossy(&log.stdout).to_string();
    assert!(log.starts_with("daemar|"), "{log}");
    assert!(log.contains(&id), "the landed commit names its slip: {log}");
    let (_, slip, _) = the_slip(&f);
    assert_eq!(slip.status, Status::Accepted);
    std::fs::remove_dir_all(&t).ok();
}

#[test]
#[ignore = "requires docker and the daemar-cage:latest image"]
fn a_killed_cage_is_a_witnessed_failure() {
    let stub = stub_server();
    let f = factory("cage-killed", &stub);
    let t = cage_territory("cage-killed");

    // Turn 1: a tool call, executed against a healthy cage. Turn 2's reply
    // is HOOKED: before it is served — provably after exec one, before exec
    // two — the test rips the container out from under the flight. The
    // fault lands deterministically; no polling race.
    stub.push_tool_call("call_1", "read", r#"{"path":"src/lib.rs"}"#);
    let ledgers = f.ledgers.clone();
    stub.push_tool_call_hooked(
        "call_2",
        "read",
        r#"{"path":"src/lib.rs"}"#,
        Box::new(move || {
            // By now the sandbox-start note is on the ledger; find and kill.
            let container = std::fs::read_dir(&ledgers)
                .expect("ledgers dir")
                .flatten()
                .find_map(|entry| {
                    let text = std::fs::read_to_string(entry.path()).ok()?;
                    let line = text.lines().find(|l| l.contains("sandbox started"))?;
                    let id = line.split("container=").nth(1)?.split_whitespace().next()?;
                    Some(id.trim_end_matches(['"', '\\']).to_string())
                })
                .expect("the sandbox-start note names its container");
            let killed = Command::new("docker")
                .args(["rm", "-f", &container])
                .output()
                .expect("docker runs");
            assert!(killed.status.success(), "the hook could kill the container");
        }),
    );
    stub.push_ok("never reached");
    let out = daemar_cmd(
        &f,
        &["scout", "--repo", t.to_str().unwrap(), "doomed flight"],
    )
    .env("DAEMAR_CAGE", "1")
    .output()
    .expect("run daemar");
    assert_eq!(
        out.status.code(),
        Some(1),
        "a dead cage is a failed flight: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let (_, slip, events) = the_slip(&f);
    assert_eq!(
        slip.status,
        Status::InFlight,
        "witnessed failure leaves the slip open for disposition"
    );
    assert!(slip.failed.is_some(), "the failed phase is derived");
    let witnessed = events.iter().any(|e| {
        if let EventKind::Known(Kind::Note { text }) = &e.kind {
            text.contains("cage failed mid-stage") || text.contains("cage failure")
        } else {
            false
        }
    });
    assert!(witnessed, "the cage failure is named on the ledger");
    std::fs::remove_dir_all(&t).ok();
}
