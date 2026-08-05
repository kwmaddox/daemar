//! End-to-end CLI tests against a stub provider.
//!
//! The test seam is the one the architecture already ships: DAEMAR_BASE_URL.
//! A std-only TCP thread speaks just enough HTTP to serve scripted
//! chat-completions, so the whole ceremony — plan, cock, guards, grant,
//! continue on a different airframe, witnessed failure, dispose, refuse —
//! runs offline, deterministically, for zero tokens. Lineage: moghedien's
//! todoing_stub, the house pattern for testing a loop at its HTTP boundary.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

use ledger::{EventKind, Kind, Slip, Status};

// ── The stub provider ────────────────────────────────────────────────────────

struct Stub {
    base_url: String,
    script: Arc<Mutex<VecDeque<(u16, String)>>>,
}

impl Stub {
    fn push_ok(&self, text: &str) {
        let body = format!(
            r#"{{"choices":[{{"message":{{"content":{text}}}}}],"usage":{{"prompt_tokens":100,"completion_tokens":20,"total_tokens":120,"prompt_tokens_details":{{"cached_tokens":10}}}}}}"#,
            text = json_string(text)
        );
        self.script.lock().unwrap().push_back((200, body));
    }

    fn push_error(&self, code: u16, body: &str) {
        self.script
            .lock()
            .unwrap()
            .push_back((code, body.to_string()));
    }

    /// A turn where the model asks for one tool call.
    fn push_tool_call(&self, id: &str, name: &str, arguments: &str) {
        let body = format!(
            r#"{{"choices":[{{"message":{{"role":"assistant","content":null,"tool_calls":[{{"id":{id},"type":"function","function":{{"name":{name},"arguments":{args}}}}}]}}}}],"usage":{{"prompt_tokens":100,"completion_tokens":20,"total_tokens":120,"prompt_tokens_details":{{"cached_tokens":0}}}}}}"#,
            id = json_string(id),
            name = json_string(name),
            args = json_string(arguments)
        );
        self.script.lock().unwrap().push_back((200, body));
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn stub_server() -> Stub {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let base_url = format!("http://{}", listener.local_addr().expect("addr"));
    let script: Arc<Mutex<VecDeque<(u16, String)>>> = Arc::default();
    let responses = script.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            let mut body_start = None;
            let mut content_length = 0usize;
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&chunk[..n]);
                        if body_start.is_none() {
                            if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
                                let headers = String::from_utf8_lossy(&buf[..pos]).to_lowercase();
                                content_length = headers
                                    .lines()
                                    .find_map(|l| l.strip_prefix("content-length:"))
                                    .and_then(|v| v.trim().parse().ok())
                                    .unwrap_or(0);
                                body_start = Some(pos + 4);
                            }
                        }
                        if let Some(start) = body_start {
                            if buf.len() >= start + content_length {
                                break;
                            }
                        }
                    }
                }
            }
            let (code, body) = responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or((500, r#"{"error":"stub exhausted"}"#.to_string()));
            let response = format!(
                "HTTP/1.1 {code} S\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    Stub { base_url, script }
}

// ── The factory under test ───────────────────────────────────────────────────

struct Factory {
    ledgers: PathBuf,
    airframes: PathBuf,
    base_url: String,
}

fn factory(name: &str, stub: &Stub) -> Factory {
    let root = std::env::temp_dir().join(format!("daemar-cli-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).expect("mkdir");
    let airframes = root.join("airframes.toml");
    std::fs::write(
        &airframes,
        "[models.plan-model]\ninput = 2.0\ncached_input = 0.2\noutput = 4.0\n\n\
         [models.respond-model]\ninput = 1.0\noutput = 2.0\n",
    )
    .expect("write airframes");
    Factory {
        ledgers: root.join("ledgers"),
        airframes,
        base_url: stub.base_url.clone(),
    }
}

fn daemar(f: &Factory, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_daemar"))
        .args(args)
        .env("DAEMAR_LEDGERS", &f.ledgers)
        .env("DAEMAR_BASE_URL", &f.base_url)
        .env("OPENAI_API_KEY", "test-key")
        .env("DAEMAR_PLAN_MODEL", "plan-model")
        .env("DAEMAR_RESPOND_MODEL", "respond-model")
        .env("DAEMAR_SCOUT_MODEL", "plan-model")
        .env("DAEMAR_AIRFRAMES", &f.airframes)
        .env("USER", "testctl")
        .output()
        .expect("run daemar")
}

fn exit_code(output: &Output) -> i32 {
    output.status.code().expect("exit code")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

/// The one slip this factory has flown, folded fresh from disk.
fn the_slip(f: &Factory) -> (String, Slip, Vec<ledger::Event>) {
    let mut ledgers: Vec<_> = std::fs::read_dir(&f.ledgers)
        .expect("ledgers dir")
        .flatten()
        .map(|e| e.path())
        .collect();
    assert_eq!(ledgers.len(), 1, "expected exactly one ledger");
    let path = ledgers.remove(0);
    let id = path
        .file_stem()
        .expect("stem")
        .to_string_lossy()
        .to_string();
    let file = ledger::load_ledger(&path).expect("load");
    assert!(
        file.bad_lines.is_empty(),
        "writer produced unreadable lines"
    );
    let slip = ledger::fold(&file.events).expect("folds");
    (id, slip, file.events)
}

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

/// A seeded territory for the scout to investigate.
fn territory(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("daemar-territory-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(dir.join("src")).expect("mkdir");
    std::fs::write(dir.join("src/lib.rs"), "pub fn answer() -> u8 { 42 }\n").expect("seed");
    dir.canonicalize().expect("canonical territory")
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
