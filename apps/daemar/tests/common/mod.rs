//! Shared test rig: the stub provider and the factory-under-test helpers.
//!
//! The test seam is the one the architecture already ships: DAEMAR_BASE_URL.
//! A std-only TCP thread speaks just enough HTTP to serve scripted
//! Responses API bodies, so whole ceremonies run offline, deterministically,
//! for zero tokens. Lineage: moghedien's todoing_stub, the house pattern
//! for testing a loop at its HTTP boundary. The stub also RECORDS every
//! request (line + body), so tests can assert what the factory actually
//! sent — migration behavior is proven at the wire, not inferred.
#![allow(dead_code)] // each test binary uses its own subset of the rig

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

use ledger::Slip;

// ── The stub provider ────────────────────────────────────────────────────────

/// One scripted reply; the hook (when present) runs just before the reply
/// is served — the deterministic seam for mid-flight fault injection.
pub struct Scripted {
    pub code: u16,
    pub body: String,
    pub hook: Option<Box<dyn FnOnce() + Send>>,
}

pub struct Stub {
    pub base_url: String,
    pub script: Arc<Mutex<VecDeque<Scripted>>>,
    /// Every request as (request line, raw body), in arrival order.
    pub requests: Arc<Mutex<Vec<(String, String)>>>,
}

impl Stub {
    pub fn push_ok(&self, text: &str) {
        // reasoning_tokens rides along on purpose: the parser must accept
        // it while receipts intentionally carry it inside output_tokens.
        let body = format!(
            r#"{{"output":[{{"type":"message","content":[{{"type":"output_text","text":{text}}}]}}],"usage":{{"input_tokens":100,"output_tokens":20,"total_tokens":120,"input_tokens_details":{{"cached_tokens":10}},"output_tokens_details":{{"reasoning_tokens":4}}}}}}"#,
            text = json_string(text)
        );
        self.push(200, body, None);
    }

    pub fn push_error(&self, code: u16, body: &str) {
        self.push(code, body.to_string(), None);
    }

    pub fn push(&self, code: u16, body: String, hook: Option<Box<dyn FnOnce() + Send>>) {
        self.script
            .lock()
            .unwrap()
            .push_back(Scripted { code, body, hook });
    }

    /// A turn where the model asks for one tool call. The opaque reasoning
    /// item is deliberate: a loop that fails to replay it cannot pass the
    /// wire tests — store:false makes replay load-bearing.
    pub fn push_tool_call(&self, id: &str, name: &str, arguments: &str) {
        let body = format!(
            r#"{{"output":[{{"type":"reasoning","id":{rsn},"summary":[]}},{{"type":"function_call","call_id":{id},"name":{name},"arguments":{args}}}],"usage":{{"input_tokens":100,"output_tokens":20,"total_tokens":120,"input_tokens_details":{{"cached_tokens":0}},"output_tokens_details":{{"reasoning_tokens":8}}}}}}"#,
            rsn = json_string(&format!("rsn_{id}")),
            id = json_string(id),
            name = json_string(name),
            args = json_string(arguments)
        );
        self.push(200, body, None);
    }

    /// A tool-call turn whose SERVING fires a hook first: the fault lands
    /// at a provable moment — after the prior turn's execs, before this one.
    pub fn push_tool_call_hooked(
        &self,
        id: &str,
        name: &str,
        arguments: &str,
        hook: Box<dyn FnOnce() + Send>,
    ) {
        self.push_tool_call(id, name, arguments);
        let mut script = self.script.lock().unwrap();
        let last = script.back_mut().expect("just pushed");
        last.hook = Some(hook);
    }
}

pub fn json_string(s: &str) -> String {
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

pub fn stub_server() -> Stub {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let base_url = format!("http://{}", listener.local_addr().expect("addr"));
    let script: Arc<Mutex<VecDeque<Scripted>>> = Arc::default();
    let requests: Arc<Mutex<Vec<(String, String)>>> = Arc::default();
    let responses = script.clone();
    let seen = requests.clone();
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
            let line = String::from_utf8_lossy(&buf)
                .lines()
                .next()
                .unwrap_or_default()
                .to_string();
            let sent_body = body_start
                .map(|start| {
                    String::from_utf8_lossy(&buf[start..(start + content_length).min(buf.len())])
                        .to_string()
                })
                .unwrap_or_default();
            seen.lock().unwrap().push((line, sent_body));
            let scripted = responses.lock().unwrap().pop_front();
            let (code, body) = match scripted {
                Some(scripted) => {
                    if let Some(hook) = scripted.hook {
                        hook();
                    }
                    (scripted.code, scripted.body)
                }
                None => (500, r#"{"error":"stub exhausted"}"#.to_string()),
            };
            let response = format!(
                "HTTP/1.1 {code} S\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    Stub {
        base_url,
        script,
        requests,
    }
}

// ── The factory under test ───────────────────────────────────────────────────

pub struct Factory {
    pub ledgers: PathBuf,
    pub airframes: PathBuf,
    pub worktrees: PathBuf,
    pub base_url: String,
}

pub fn factory(name: &str, stub: &Stub) -> Factory {
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
        worktrees: root.join("worktrees"),
        base_url: stub.base_url.clone(),
    }
}

/// A daemar invocation wired to this factory's env — callers pick how to
/// run it (`.output()` for the CLI tests, piped `.spawn()` for MCP).
pub fn daemar_cmd(f: &Factory, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_daemar"));
    cmd.args(args)
        .env("DAEMAR_LEDGERS", &f.ledgers)
        .env("DAEMAR_BASE_URL", &f.base_url)
        .env("OPENAI_API_KEY", "test-key")
        .env("DAEMAR_PLAN_MODEL", "plan-model")
        .env("DAEMAR_RESPOND_MODEL", "respond-model")
        .env("DAEMAR_SCOUT_MODEL", "plan-model")
        .env("DAEMAR_BUILD_MODEL", "plan-model")
        .env("DAEMAR_EFFORT", "medium")
        .env("DAEMAR_WORKTREES", &f.worktrees)
        .env("DAEMAR_AIRFRAMES", &f.airframes)
        .env("USER", "testctl")
        .env_remove("DAEMAR_HOME");
    cmd
}

pub fn daemar(f: &Factory, args: &[&str]) -> Output {
    daemar_cmd(f, args).output().expect("run daemar")
}

pub fn exit_code(output: &Output) -> i32 {
    output.status.code().expect("exit code")
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

/// The one slip this factory has flown, folded fresh from disk.
pub fn the_slip(f: &Factory) -> (String, Slip, Vec<ledger::Event>) {
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

/// A seeded, COMMITTED territory for tooled agents to investigate: every
/// tooled stage now pins the territory's HEAD into a detached worktree, so
/// a territory must be a git repo with at least one commit.
pub fn territory(name: &str) -> PathBuf {
    territory_at(
        std::env::temp_dir().join(format!("daemar-territory-{}-{name}", std::process::id())),
    )
}

/// The same seeded territory at a caller-chosen root — the cage tests need
/// territories under CARGO_TARGET_TMPDIR so Docker Desktop can mount them.
pub fn territory_at(dir: PathBuf) -> PathBuf {
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(dir.join("src")).expect("mkdir");
    std::fs::write(dir.join("src/lib.rs"), "pub fn answer() -> u8 { 42 }\n").expect("seed");
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args([
                "-c",
                "user.email=territory@test",
                "-c",
                "user.name=territory",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["init", "-q"]);
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "seed"]);
    dir.canonicalize().expect("canonical territory")
}

// ── A stamped build, fabricated at the wire ──────────────────────────────────

/// Everything a gate-leg ceremony needs: a REAL territory, a REAL detached
/// worktree with a real mutation, the REAL computed diff — and the build
/// ledger hand-authored around them exactly as a build flight writes it.
pub struct StampedBuild {
    pub slip_id: String,
    pub territory: PathBuf,
    pub worktree: PathBuf,
    pub base: String,
}

pub fn stamped_build(f: &Factory, name: &str, n: u8) -> StampedBuild {
    let slip_id = format!("00000000-0000-7000-8000-0000000000{n:02}");
    let territory = territory(name);
    let base = factory::worktree::head(&territory).expect("territory pins");
    let dest = f.worktrees.join(&slip_id).join("build");
    let wt = factory::worktree::add_detached(&territory, &base, &dest).expect("worktree");
    std::fs::write(wt.join("src/lib.rs"), "pub fn answer() -> u8 { 43 }\n").expect("mutate");
    let patch = factory::worktree::diff_against_base(&wt, &base).expect("diff");
    assert!(!patch.trim().is_empty());

    let receipt = format!(r#"{{"v":1,"base":"{base}","worktree":"{}"}}"#, wt.display());
    let mut w =
        ledger::LedgerWriter::create(&f.ledgers, ledger::SlipId(slip_id.clone())).expect("ledger");
    w.append(&ledger::Kind::SlipOpened {
        request: "make answer 43".into(),
        workflow: "build".into(),
        engineer: "testctl".into(),
        repo: territory.display().to_string(),
    })
    .unwrap();
    w.append(&ledger::Kind::PhaseStarted {
        phase: "build".into(),
        owner: "builder".into(),
        lane: ledger::Lane::Agent,
        engineer: "testctl".into(),
    })
    .unwrap();
    w.append(&ledger::Kind::SectionWritten {
        section: "build.v1".into(),
        by: "builder".into(),
        summary: "changed answer to 43".into(),
        body: "changed answer to 43".into(),
    })
    .unwrap();
    w.append(&ledger::Kind::PhaseEnded {
        phase: "build".into(),
        outcome: ledger::PhaseOutcome::Success,
    })
    .unwrap();
    w.append(&ledger::Kind::SectionWritten {
        section: "diff.v1".into(),
        by: "gate:diff".into(),
        summary: receipt,
        body: patch,
    })
    .unwrap();
    w.append(&ledger::Kind::ClearanceRequested {
        boundary: "build->apply".into(),
        by: "gate:diff".into(),
    })
    .unwrap();
    StampedBuild {
        slip_id,
        territory,
        worktree: wt,
        base,
    }
}

/// git in a test territory, asserting success.
pub fn territory_git(dir: &PathBuf, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.email=territory@test",
            "-c",
            "user.name=territory",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}
