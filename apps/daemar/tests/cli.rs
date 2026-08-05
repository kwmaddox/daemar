//! End-to-end CLI tests against a stub provider.
//!
//! The rig (stub server, factory helpers) lives in tests/common — shared
//! with the MCP tower tests, which drive the same binary over stdio.

mod common;
use common::*;

use ledger::{EventKind, Kind, Status};

// ── The ceremony, end to end ─────────────────────────────────────────────────

#[test]
fn the_full_planned_ceremony_offline() {
    let stub = stub_server();
    let f = factory("ceremony", &stub);

    // Plan: terra-stand-in plans, the slip cocks, the process exits 0.
    stub.push_ok("THE PLAN: cover A, then B.");
    let out = daemar(&f, &["plan", "compare A and B"]);
    assert_eq!(exit_code(&out), 0, "plan flight: {}", stderr(&out));
    let (id, slip, _) = the_slip(&f);
    assert_eq!(slip.status, Status::InFlight);
    assert_eq!(slip.cocked.as_deref(), Some("plan->respond"));
    assert_eq!(slip.holding, None);

    // Guard: continue before grant is refused with instructions.
    let out = daemar(&f, &["continue", &id]);
    assert_eq!(exit_code(&out), 2);
    assert!(
        stderr(&out).contains("awaiting clearance"),
        "{}",
        stderr(&out)
    );

    // Grant: signed by the engineer; the slip is now holding, not cocked.
    let out = daemar(&f, &["grant", &id]);
    assert_eq!(exit_code(&out), 0, "{}", stderr(&out));
    let (_, slip, _) = the_slip(&f);
    assert_eq!(slip.cocked, None);
    assert_eq!(slip.holding.as_deref(), Some("plan->respond"));

    // Guard: a second grant finds nothing waiting.
    let out = daemar(&f, &["grant", &id]);
    assert_eq!(exit_code(&out), 2);
    assert!(stderr(&out).contains("not awaiting"), "{}", stderr(&out));

    // Continue: a fresh process flies respond on the OTHER airframe, from
    // the printout — context rebuilt purely from the ledger.
    stub.push_ok("THE ANSWER, following the plan.");
    let out = daemar(&f, &["continue", &id]);
    assert_eq!(exit_code(&out), 0, "{}", stderr(&out));
    let (_, slip, events) = the_slip(&f);
    assert_eq!(slip.status, Status::Accepted);
    assert_eq!(slip.holding, None);
    assert!(slip.cost > 0.0, "receipts are real");

    // The ledger's story: both airframes flew, the grant is signed, and the
    // respond phase's printout carried the plan and the request.
    let mut models = Vec::new();
    let mut printout_ok = false;
    let mut grant_signed = false;
    for event in &events {
        if let EventKind::Known(Kind::ModelRequested { model, user, .. }) = &event.kind {
            models.push(model.clone());
            if model == "respond-model" {
                printout_ok =
                    user.contains("THE PLAN: cover A, then B.") && user.contains("compare A and B");
            }
        }
        if let EventKind::Known(Kind::ClearanceGranted { by, .. }) = &event.kind {
            grant_signed = by == "testctl";
        }
    }
    assert_eq!(
        models,
        vec!["plan-model", "respond-model"],
        "two airframes, one slip"
    );
    assert!(
        printout_ok,
        "the printout must carry the plan and the request"
    );
    assert!(grant_signed, "clearances are signed");
}

#[test]
fn failure_is_witnessed_then_disposed() {
    let stub = stub_server();
    let f = factory("witnessed", &stub);

    // The provider fails; the flight reports, leaves the slip OPEN, exits 1.
    stub.push_error(500, r#"{"error":"stub meltdown"}"#);
    let out = daemar(&f, &["do the thing"]);
    assert_eq!(exit_code(&out), 1);
    assert!(
        stderr(&out).contains("dispose"),
        "failure names its remedy: {}",
        stderr(&out)
    );
    let (id, slip, _) = the_slip(&f);
    assert_eq!(
        slip.status,
        Status::InFlight,
        "machines never close their own failures"
    );
    assert_eq!(slip.failed.as_deref(), Some("respond"));

    // Disposition closes it, signed.
    let out = daemar(&f, &["dispose", &id, "stub meltdown, witnessed"]);
    assert_eq!(exit_code(&out), 0, "{}", stderr(&out));
    let (_, slip, events) = the_slip(&f);
    assert_eq!(slip.status, Status::Rejected);
    assert_eq!(slip.failed, None);
    let signed = events.iter().any(|e| {
        if let EventKind::Known(Kind::SlipClosed { by, reason, .. }) = &e.kind {
            by == "testctl" && reason.contains("witnessed")
        } else {
            false
        }
    });
    assert!(signed, "dispositions are signed with their reason");

    // History is not re-litigated.
    let out = daemar(&f, &["dispose", &id, "again"]);
    assert_eq!(exit_code(&out), 2);
    assert!(stderr(&out).contains("already closed"), "{}", stderr(&out));
}

#[test]
fn the_scout_reads_the_territory_and_reports() {
    let stub = stub_server();
    let f = factory("scout", &stub);
    let t = territory("scout");

    // Turn 1: the model asks to read the seeded file. Turn 2: it reports.
    stub.push_tool_call("call_1", "read", r#"{"path":"src/lib.rs"}"#);
    stub.push_ok("FOUND: src/lib.rs defines answer(), returning 42.");
    let out = daemar(
        &f,
        &[
            "scout",
            "--repo",
            t.to_str().unwrap(),
            "where is answer defined",
        ],
    );
    assert_eq!(exit_code(&out), 0, "scout flight: {}", stderr(&out));

    let (_, slip, events) = the_slip(&f);
    assert_eq!(slip.status, Status::Accepted);
    assert_eq!(
        slip.repo,
        t.display().to_string(),
        "the slip remembers its territory"
    );
    assert_eq!(slip.tool_trail.len(), 1);
    assert!(slip.tool_trail[0].ok);
    assert_eq!(slip.tool_trail[0].tool, "read");

    // The tool call is on the ledger with its epistemic pointer.
    let pinned = events.iter().any(|e| {
        if let EventKind::Known(Kind::ToolCall { tool, ok, hash, .. }) = &e.kind {
            tool == "read" && *ok && hash.len() == 16
        } else {
            false
        }
    });
    assert!(pinned, "reads carry a content-hash pointer");
    let reported = slip
        .sections
        .iter()
        .any(|s| s.section == "scout.v1" && s.body.contains("FOUND"));
    assert!(reported, "the scout files a scout.v1 section");
    std::fs::remove_dir_all(&t).ok();
}

#[test]
fn the_planner_reads_the_territory_before_planning() {
    // The grounded planner: same seat shape as the scout — tools, turn
    // loop, trail — but the stage cocks at the boundary instead of closing.
    let stub = stub_server();
    let f = factory("grounded-plan", &stub);
    let t = territory("grounded-plan");

    // Turn 1: the planner asks to read the file its plan will touch.
    // Turn 2: it files the plan.
    stub.push_tool_call("call_1", "read", r#"{"path":"src/lib.rs"}"#);
    stub.push_ok("PLAN: change answer() in src/lib.rs:1 to return 43.");
    let out = daemar(
        &f,
        &[
            "plan",
            "--repo",
            t.to_str().unwrap(),
            "make answer return 43",
        ],
    );
    assert_eq!(exit_code(&out), 0, "plan flight: {}", stderr(&out));

    let (_, slip, events) = the_slip(&f);
    assert_eq!(slip.status, Status::InFlight);
    assert_eq!(
        slip.cocked.as_deref(),
        Some("plan->respond"),
        "a grounded plan still cocks at the boundary"
    );
    assert_eq!(
        slip.repo,
        t.display().to_string(),
        "the slip remembers its territory"
    );
    assert_eq!(slip.tool_trail.len(), 1);
    assert!(slip.tool_trail[0].ok);
    assert_eq!(slip.tool_trail[0].tool, "read");
    let read_on_plan_phase = events.iter().any(|e| {
        if let EventKind::Known(Kind::ToolCall {
            phase, tool, ok, ..
        }) = &e.kind
        {
            phase == "plan" && tool == "read" && *ok
        } else {
            false
        }
    });
    assert!(read_on_plan_phase, "the read belongs to the plan phase");
    let planned = slip
        .sections
        .iter()
        .any(|s| s.section == "plan.v1" && s.by == "planner" && s.body.contains("src/lib.rs"));
    assert!(planned, "the planner files a plan.v1 section, signed");
    std::fs::remove_dir_all(&t).ok();
}

#[test]
fn object_form_tool_arguments_are_preserved_not_dropped() {
    // Some OpenAI-compatible providers send `arguments` as a JSON object
    // instead of the spec's string. Those inputs must reach the tool.
    let stub = stub_server();
    let f = factory("object-args", &stub);
    let t = territory("object-args");
    let body = r#"{"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"c1","type":"function","function":{"name":"read","arguments":{"path":"src/lib.rs"}}}]}}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#;
    stub.push_error(200, body); // raw body, 200: the object-args shape verbatim
    stub.push_ok("done");
    let out = daemar(
        &f,
        &["scout", "--repo", t.to_str().unwrap(), "read the lib"],
    );
    assert_eq!(exit_code(&out), 0, "{}", stderr(&out));
    let (_, slip, _) = the_slip(&f);
    assert_eq!(slip.tool_trail.len(), 1);
    assert!(
        slip.tool_trail[0].ok,
        "object-form args must execute: {}",
        slip.tool_trail[0].summary
    );
    std::fs::remove_dir_all(&t).ok();
}

#[test]
fn a_bare_repo_flag_is_a_usage_error_not_a_flight() {
    let stub = stub_server();
    let f = factory("bare-flag", &stub);
    let out = daemar(&f, &["scout", "--repo"]);
    assert_eq!(exit_code(&out), 2);
    assert!(stderr(&out).contains("usage"), "{}", stderr(&out));
    let out = daemar(&f, &["scout", "--territory", "question"]);
    assert_eq!(exit_code(&out), 2, "unknown flags refuse rather than fly");
    assert!(
        !f.ledgers.exists(),
        "no slip may be minted from a malformed invocation"
    );
}

#[test]
fn the_scout_cannot_leave_its_territory_and_the_refusal_is_logged() {
    let stub = stub_server();
    let f = factory("scout-confined", &stub);
    let t = territory("confined");

    // The model tries to escape; the refusal becomes a tool result it can
    // read, and the flight continues to a report.
    stub.push_tool_call("call_1", "read", r#"{"path":"../../../../etc/hosts"}"#);
    stub.push_ok("Understood — staying inside the territory.");
    let out = daemar(
        &f,
        &["scout", "--repo", t.to_str().unwrap(), "read the host file"],
    );
    assert_eq!(exit_code(&out), 0, "{}", stderr(&out));

    let (_, slip, _) = the_slip(&f);
    assert_eq!(slip.status, Status::Accepted);
    assert_eq!(slip.tool_trail.len(), 1);
    assert!(
        !slip.tool_trail[0].ok,
        "the escape attempt is a logged failure"
    );
    assert!(
        slip.tool_trail[0].summary.contains("outside the territory")
            || slip.tool_trail[0].summary.contains("cannot resolve"),
        "refusal names itself: {}",
        slip.tool_trail[0].summary
    );
    std::fs::remove_dir_all(&t).ok();
}

#[test]
fn a_toolless_seat_that_requests_tools_is_a_witnessed_failure() {
    // The responder holds no tools; a provider that sends tool calls anyway
    // is misbehaving. The engine must witness it and stop — not pay turns
    // conversing with it.
    let stub = stub_server();
    let f = factory("toolless-tools", &stub);
    stub.push_tool_call("call_1", "read", r#"{"path":"anything"}"#);
    let out = daemar(&f, &["just answer the question"]);
    assert_eq!(exit_code(&out), 1);
    assert!(
        stderr(&out).contains("holds none"),
        "the failure names itself: {}",
        stderr(&out)
    );
    let (_, slip, _) = the_slip(&f);
    assert_eq!(
        slip.status,
        Status::InFlight,
        "witnessed failure leaves the slip open for disposition"
    );
    assert_eq!(slip.failed.as_deref(), Some("respond"));
    assert_eq!(slip.tool_trail.len(), 1, "the refused call is on the trail");
    assert!(!slip.tool_trail[0].ok);
}

#[test]
fn refuse_is_a_verdict_and_closes_directly() {
    let stub = stub_server();
    let f = factory("refused", &stub);

    stub.push_ok("a plan the controller will not like");
    let out = daemar(&f, &["plan", "do something questionable"]);
    assert_eq!(exit_code(&out), 0, "{}", stderr(&out));
    let (id, _, _) = the_slip(&f);

    let out = daemar(&f, &["refuse", &id, "not on my board"]);
    assert_eq!(exit_code(&out), 0, "{}", stderr(&out));
    let (_, slip, events) = the_slip(&f);
    assert_eq!(
        slip.status,
        Status::Rejected,
        "a refusal carries its verdict"
    );
    assert_eq!(slip.cocked, None);
    let refused = events.iter().any(|e| {
        if let EventKind::Known(Kind::ClearanceRefused { by, reason, .. }) = &e.kind {
            by == "testctl" && reason == "not on my board"
        } else {
            false
        }
    });
    assert!(refused, "the refusal is on the ledger, signed");

    // Nothing continues past a refusal.
    let out = daemar(&f, &["continue", &id]);
    assert_eq!(exit_code(&out), 2);
    assert!(stderr(&out).contains("already closed"), "{}", stderr(&out));
}

#[test]
fn daemar_home_roots_the_ledgers_and_airframes() {
    let stub = stub_server();
    let f = factory("home", &stub);
    let home = std::env::temp_dir().join(format!("daemar-home-{}", std::process::id()));
    std::fs::remove_dir_all(&home).ok();
    std::fs::create_dir_all(&home).expect("mkdir home");
    std::fs::copy(&f.airframes, home.join("airframes.toml")).expect("seed airframes");

    // No DAEMAR_LEDGERS / DAEMAR_AIRFRAMES in the env: the relative defaults
    // must resolve against DAEMAR_HOME, not wherever the process happens
    // to be standing.
    stub.push_ok("answered from a homed process");
    let out = daemar_cmd(&f, &["fly from home"])
        .env_remove("DAEMAR_LEDGERS")
        .env_remove("DAEMAR_AIRFRAMES")
        .env("DAEMAR_HOME", &home)
        .output()
        .expect("run daemar");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        home.join("ledgers").exists(),
        "the slip lands under DAEMAR_HOME/ledgers"
    );
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("cost unrecorded"),
        "airframes.toml was found via DAEMAR_HOME"
    );
    std::fs::remove_dir_all(&home).ok();
}

#[cfg(unix)]
#[test]
fn the_tower_fetches_its_own_key_from_the_vault() {
    use std::os::unix::fs::PermissionsExt;

    let stub = stub_server();
    let f = factory("vault", &stub);
    let home = std::env::temp_dir().join(format!("daemar-vault-{}", std::process::id()));
    std::fs::remove_dir_all(&home).ok();
    std::fs::create_dir_all(home.join("secrets")).expect("mkdir");
    std::fs::write(home.join("secrets/daemar.enc.env"), "ciphertext\n").expect("seed");

    // A stand-in sops on PATH: "decrypts" to a dotenv holding the key. The
    // real binary is exercised the same way — stdout parsed in-process.
    let bin = home.join("bin");
    std::fs::create_dir_all(&bin).expect("mkdir bin");
    std::fs::write(
        bin.join("sops"),
        "#!/bin/sh\necho '# decrypted'\necho 'OPENAI_API_KEY=key-from-vault'\n",
    )
    .expect("write fake sops");
    std::fs::set_permissions(bin.join("sops"), std::fs::Permissions::from_mode(0o755))
        .expect("chmod");
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    // No key in the exec environment — the process must fetch its own.
    stub.push_ok("flown on a vault key");
    let out = daemar_cmd(&f, &["prove the vault"])
        .env_remove("OPENAI_API_KEY")
        .env("DAEMAR_HOME", &home)
        .env("PATH", path)
        .output()
        .expect("run daemar");
    assert_eq!(
        out.status.code(),
        Some(0),
        "the vault supplied the key: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::remove_dir_all(&home).ok();
}
