//! Shared test rig: the stub provider and the factory-under-test helpers.
//!
//! The test seam is the one the architecture already ships: DAEMAR_BASE_URL.
//! A std-only TCP thread speaks just enough HTTP to serve scripted
//! chat-completions, so whole ceremonies run offline, deterministically,
//! for zero tokens. Lineage: moghedien's todoing_stub, the house pattern
//! for testing a loop at its HTTP boundary.
#![allow(dead_code)] // each test binary uses its own subset of the rig

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

use ledger::Slip;

// ── The stub provider ────────────────────────────────────────────────────────

pub struct Stub {
    pub base_url: String,
    pub script: Arc<Mutex<VecDeque<(u16, String)>>>,
}

impl Stub {
    pub fn push_ok(&self, text: &str) {
        let body = format!(
            r#"{{"choices":[{{"message":{{"content":{text}}}}}],"usage":{{"prompt_tokens":100,"completion_tokens":20,"total_tokens":120,"prompt_tokens_details":{{"cached_tokens":10}}}}}}"#,
            text = json_string(text)
        );
        self.script.lock().unwrap().push_back((200, body));
    }

    pub fn push_error(&self, code: u16, body: &str) {
        self.script
            .lock()
            .unwrap()
            .push_back((code, body.to_string()));
    }

    /// A turn where the model asks for one tool call.
    pub fn push_tool_call(&self, id: &str, name: &str, arguments: &str) {
        let body = format!(
            r#"{{"choices":[{{"message":{{"role":"assistant","content":null,"tool_calls":[{{"id":{id},"type":"function","function":{{"name":{name},"arguments":{args}}}}}]}}}}],"usage":{{"prompt_tokens":100,"completion_tokens":20,"total_tokens":120,"prompt_tokens_details":{{"cached_tokens":0}}}}}}"#,
            id = json_string(id),
            name = json_string(name),
            args = json_string(arguments)
        );
        self.script.lock().unwrap().push_back((200, body));
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

pub struct Factory {
    pub ledgers: PathBuf,
    pub airframes: PathBuf,
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

/// A seeded territory for tooled agents to investigate.
pub fn territory(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("daemar-territory-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(dir.join("src")).expect("mkdir");
    std::fs::write(dir.join("src/lib.rs"), "pub fn answer() -> u8 { 42 }\n").expect("seed");
    dir.canonicalize().expect("canonical territory")
}
