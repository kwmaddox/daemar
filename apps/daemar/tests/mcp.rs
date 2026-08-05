//! The MCP tower, end to end: spawn `daemar mcp`, speak newline-delimited
//! JSON-RPC over its pipes, and verify the flights land on real ledgers —
//! offline, against the stub provider, for zero tokens.

mod common;
use common::*;

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdout, Command, Stdio};

use ledger::Status;
use serde_json::{json, Value};

struct Tower {
    child: Child,
    reader: BufReader<ChildStdout>,
}

impl Tower {
    fn launch(f: &Factory) -> Tower {
        let mut cmd: Command = daemar_cmd(f, &["mcp"]);
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn daemar mcp");
        let reader = BufReader::new(child.stdout.take().expect("stdout"));
        Tower { child, reader }
    }

    fn send(&mut self, message: Value) {
        let stdin = self.child.stdin.as_mut().expect("stdin");
        writeln!(stdin, "{message}").expect("write request");
    }

    /// Send a request and read the next response line.
    fn call(&mut self, message: Value) -> Value {
        self.send(message);
        let mut line = String::new();
        self.reader.read_line(&mut line).expect("read response");
        serde_json::from_str(&line).expect("response is JSON")
    }

    /// The MCP opening ceremony; returns the initialize result.
    fn handshake(&mut self, client: &str) -> Value {
        let response = self.call(json!({
            "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": client, "version": "0.0.1" },
            },
        }));
        self.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        response["result"].clone()
    }

    /// tools/call sugar: returns (text, isError).
    fn tool(&mut self, id: u64, name: &str, arguments: Value) -> (String, bool) {
        let response = self.call(json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": arguments },
        }));
        let result = &response["result"];
        (
            result["content"][0]["text"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            result["isError"].as_bool().unwrap_or(false),
        )
    }
}

impl Drop for Tower {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn the_tower_serves_a_prompt_flight_signed_by_its_client() {
    let stub = stub_server();
    let f = factory("mcp-prompt", &stub);
    let mut tower = Tower::launch(&f);

    let init = tower.handshake("moggy");
    assert_eq!(init["serverInfo"]["name"], "daemar");
    assert_eq!(init["protocolVersion"], "2025-06-18");

    // The toolbox is the delegated surface: flights and reads, no pens.
    let listed = tower.call(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" }));
    let names: Vec<&str> = listed["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|t| t["name"].as_str().expect("name"))
        .collect();
    assert_eq!(names, vec!["scout", "plan", "prompt", "continue", "slip"]);
    assert!(
        !names.contains(&"grant"),
        "a delegated agent may request clearances, never sign them"
    );

    stub.push_ok("THE ANSWER from the tower.");
    let (text, is_error) = tower.tool(2, "prompt", json!({ "request": "say the answer" }));
    assert!(!is_error, "{text}");
    assert!(text.contains("THE ANSWER from the tower."), "{text}");
    assert!(
        text.contains("accepted"),
        "the footer carries the verdict: {text}"
    );

    // The flight is real: one ledger, closed accepted, signed by the client.
    let (id, slip, _) = the_slip(&f);
    assert_eq!(slip.status, Status::Accepted);
    assert_eq!(
        slip.engineer, "mcp:moggy",
        "slips opened over MCP name their client"
    );

    // The slip tool folds the same truth back to the client.
    let (text, is_error) = tower.tool(3, "slip", json!({ "slip_id": id }));
    assert!(!is_error, "{text}");
    assert!(text.contains("Accepted"), "{text}");
    assert!(text.contains("mcp:moggy"), "{text}");
}

#[test]
fn a_planned_flight_cocks_and_continue_is_refused_until_granted() {
    let stub = stub_server();
    let f = factory("mcp-plan", &stub);
    let t = territory("mcp-plan");
    let mut tower = Tower::launch(&f);
    tower.handshake("moggy");

    // The grounded planner reads, plans, cocks — over MCP.
    stub.push_tool_call("call_1", "read", r#"{"path":"src/lib.rs"}"#);
    stub.push_ok("PLAN: change src/lib.rs.");
    let (text, is_error) = tower.tool(
        2,
        "plan",
        json!({
            "request": "make answer return 43",
            "territory": t.to_str().unwrap(),
        }),
    );
    assert!(!is_error, "{text}");
    assert!(text.contains("COCKED at plan->respond"), "{text}");

    let (id, slip, _) = the_slip(&f);
    assert_eq!(slip.cocked.as_deref(), Some("plan->respond"));
    assert_eq!(slip.tool_trail.len(), 1, "the read is on the trail");

    // The boundary holds against the agent: continue without a grant is a
    // refusal the client can read, not a flight.
    let (text, is_error) = tower.tool(3, "continue", json!({ "slip_id": id }));
    assert!(is_error);
    assert!(text.contains("awaiting clearance"), "{text}");
    let (_, slip, _) = the_slip(&f);
    assert_eq!(slip.status, Status::InFlight, "nothing flew");
    std::fs::remove_dir_all(&t).ok();
}

#[test]
fn refusals_and_unknown_methods_are_answers_not_crashes() {
    let stub = stub_server();
    let f = factory("mcp-guards", &stub);
    let mut tower = Tower::launch(&f);
    tower.handshake("moggy");

    // A bad territory is a refusal; no slip is minted.
    let (text, is_error) = tower.tool(
        2,
        "scout",
        json!({
            "request": "look around",
            "territory": "/nonexistent/territory",
        }),
    );
    assert!(is_error);
    assert!(text.starts_with("refused:"), "{text}");
    assert!(!f.ledgers.exists(), "no slip minted from a refused call");

    // Missing arguments refuse by name.
    let (text, is_error) = tower.tool(3, "plan", json!({ "request": "no territory" }));
    assert!(is_error);
    assert!(text.contains("territory"), "{text}");

    // Unknown tool: an error result the model can read.
    let (text, is_error) = tower.tool(4, "grant", json!({ "slip_id": "x" }));
    assert!(is_error);
    assert!(text.contains("unknown tool"), "{text}");

    // Unknown method: a JSON-RPC error, id echoed.
    let response = tower.call(json!({ "jsonrpc": "2.0", "id": 5, "method": "resources/list" }));
    assert_eq!(response["error"]["code"], -32601);
    assert_eq!(response["id"], 5);

    // Ping still answers: the loop survived every refusal above.
    let response = tower.call(json!({ "jsonrpc": "2.0", "id": 6, "method": "ping" }));
    assert!(response["error"].is_null());
}
