//! The tower as protocol: an MCP server over stdio.
//!
//! One process per client, spawned by the client itself; the ledgers
//! directory is the shared state, and exit-and-resume makes every operation
//! stateless — this server is just another skin over `factory::workflows`.
//! stdout belongs to JSON-RPC (newline-delimited, per the MCP stdio
//! transport); logs go to stderr.
//!
//! Identity: the client names itself in the `initialize` handshake, and every
//! slip it opens is signed `engineer: "mcp:<client>"`. The name is
//! self-asserted — fine while the trust boundary is one user on one machine;
//! it becomes an authenticated principal when the tower moves behind HTTP.
//!
//! Authority: flights only. The controller's pens (grant/refuse/dispose) are
//! deliberately NOT exposed — a delegated agent may request clearances, never
//! sign them. Human-stamp boundaries refuse agent signatures by default.

use std::io::{BufRead, Write};
use std::process::ExitCode;

use serde_json::{json, Value};

use factory::config::{self, Config};
use factory::workflows::{self, FlightError, FlightReport};

pub fn serve() -> ExitCode {
    let mut config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("daemar mcp: {error}");
            return ExitCode::from(2);
        }
    };
    if let Err(error) = crate::microsandbox_wall::select(&mut config) {
        eprintln!("daemar mcp: {error}");
        return ExitCode::from(2);
    }

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            respond(
                &mut out,
                error_response(Value::Null, -32700, "parse error: not JSON"),
            );
            continue;
        };
        let id = message.get("id").filter(|v| !v.is_null()).cloned();
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match (method, id) {
            ("initialize", Some(id)) => {
                let client = message
                    .pointer("/params/clientInfo/name")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                config.engineer = format!("mcp:{client}");
                eprintln!("daemar mcp: client {client} connected");
                let version = message
                    .pointer("/params/protocolVersion")
                    .and_then(Value::as_str)
                    .unwrap_or("2025-06-18");
                respond(
                    &mut out,
                    result_response(
                        id,
                        json!({
                            "protocolVersion": version,
                            "capabilities": { "tools": {} },
                            "serverInfo": {
                                "name": "daemar",
                                "version": env!("CARGO_PKG_VERSION"),
                            },
                        }),
                    ),
                );
            }
            ("ping", Some(id)) => respond(&mut out, result_response(id, json!({}))),
            ("tools/list", Some(id)) => {
                respond(
                    &mut out,
                    result_response(id, json!({ "tools": tool_defs() })),
                );
            }
            ("tools/call", Some(id)) => {
                let tool = message
                    .pointer("/params/name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                eprintln!("daemar mcp: {} → {tool}", config.engineer);
                let arguments = message
                    .pointer("/params/arguments")
                    .cloned()
                    .unwrap_or(Value::Null);
                let (text, is_error) = call_tool(&config, &tool, &arguments);
                respond(
                    &mut out,
                    result_response(
                        id,
                        json!({
                            "content": [{ "type": "text", "text": text }],
                            "isError": is_error,
                        }),
                    ),
                );
            }
            (_, Some(id)) => {
                respond(
                    &mut out,
                    error_response(id, -32601, &format!("method not found: {method}")),
                );
            }
            // Notifications (initialized, cancelled, …) need no reply.
            (_, None) => {}
        }
    }
    ExitCode::SUCCESS
}

fn respond(out: &mut impl Write, message: Value) {
    // A stdout we cannot write is a client that hung up: nothing to salvage.
    let _ = writeln!(out, "{message}");
    let _ = out.flush();
}

fn result_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn tool_defs() -> Value {
    let request = json!({ "type": "string", "description": "The request, in plain prose." });
    let territory = json!({
        "type": "string",
        "description": "The territory this flight operates on: an absolute path to a \
                        repository the tower can reach. Tools are confined to it.",
    });
    let slip_id = json!({ "type": "string", "description": "The slip id (UUID)." });
    json!([
        {
            "name": "scout",
            "description": "Read-only reconnaissance over a territory. The scout reads \
                            real files (every read logged with a content hash) and \
                            reports what lives where, with paths cited. Use this \
                            instead of doing your own discovery.",
            "inputSchema": {
                "type": "object",
                "properties": { "request": request, "territory": territory },
                "required": ["request", "territory"],
            },
        },
        {
            "name": "plan",
            "description": "A grounded plan: the planner investigates the territory \
                            with read-only tools, then files an implementable plan \
                            citing real paths. The slip then COCKS at plan->respond \
                            awaiting the controller's clearance — this tool cannot \
                            grant it.",
            "inputSchema": {
                "type": "object",
                "properties": { "request": request, "territory": territory },
                "required": ["request", "territory"],
            },
        },
        {
            "name": "prompt",
            "description": "The one-stage prompt workflow: the responder answers the \
                            request directly (no tools) and the slip closes accepted.",
            "inputSchema": {
                "type": "object",
                "properties": { "request": request },
                "required": ["request"],
            },
        },
        {
            "name": "continue",
            "description": "Fly the stage after a granted clearance, context rebuilt \
                            purely from the ledger — the responder for plan->respond, \
                            the deterministic gate legs for build->apply and \
                            apply->land. Refused unless the controller's pen has \
                            granted the boundary; this tool cannot grant anything.",
            "inputSchema": {
                "type": "object",
                "properties": { "slip_id": slip_id },
                "required": ["slip_id"],
            },
        },
        {
            "name": "slip",
            "description": "Read one slip's current state, folded fresh from its \
                            ledger: status, attention, phases, sections, receipts.",
            "inputSchema": {
                "type": "object",
                "properties": { "slip_id": slip_id },
                "required": ["slip_id"],
            },
        },
        {
            "name": "board",
            "description": "The board's open strips: every in-flight slip as one \
                            terse line, attention first — failed, cocked, holding, \
                            then active traffic. Discover slips awaiting clearance \
                            or disposition; read one in full with the slip tool.",
            "inputSchema": { "type": "object", "properties": {} },
        },
    ])
}

fn call_tool(config: &Config, tool: &str, arguments: &Value) -> (String, bool) {
    let arg = |key: &str| {
        arguments
            .get(key)
            .and_then(Value::as_str)
            .filter(|v| !v.trim().is_empty())
            .map(str::to_string)
    };
    let missing = |key: &str| (format!("refused: required argument {key} is missing"), true);
    match tool {
        "prompt" => match arg("request") {
            Some(request) => flight_result(workflows::prompt_flight(config, &request)),
            None => missing("request"),
        },
        "scout" => match (arg("request"), arg("territory")) {
            (Some(request), Some(territory)) => {
                flight_result(workflows::scout_flight(config, &request, &territory))
            }
            (None, _) => missing("request"),
            (_, None) => missing("territory"),
        },
        "plan" => match (arg("request"), arg("territory")) {
            (Some(request), Some(territory)) => {
                flight_result(workflows::plan_flight(config, &request, &territory))
            }
            (None, _) => missing("request"),
            (_, None) => missing("territory"),
        },
        "continue" => match arg("slip_id") {
            Some(slip_id) => flight_result(workflows::continue_flight(config, &slip_id)),
            None => missing("slip_id"),
        },
        "slip" => match arg("slip_id") {
            Some(slip_id) => slip_summary(&slip_id),
            None => missing("slip_id"),
        },
        "board" => board_summary(),
        _ => (format!("unknown tool: {tool}"), true),
    }
}

fn flight_result(outcome: Result<FlightReport, FlightError>) -> (String, bool) {
    match outcome {
        Ok(report) => {
            let id = &report.slip_id;
            let footer = match report.cocked_at {
                Some(boundary) => format!(
                    "slip {id} · COCKED at {boundary} — awaiting the controller's \
                     clearance (daemar grant {id}) · {} tokens · ${:.4}",
                    report.tokens, report.cost
                ),
                None => format!(
                    "slip {id} · accepted · {} tokens · ${:.4}",
                    report.tokens, report.cost
                ),
            };
            (
                format!("{}\n\n---\n{footer}", report.text.trim_end()),
                false,
            )
        }
        Err(FlightError::Refused(message)) => (format!("refused: {message}"), true),
        Err(FlightError::Failed { slip_id }) => (
            format!(
                "witnessed failure: slip {slip_id} failed — the reason is on the \
                 ledger, and the slip stays open for the controller's disposition"
            ),
            true,
        ),
        Err(FlightError::Ledger(error)) => (format!("ledger failure: {error}"), true),
    }
}

/// The board as strip lines: open slips only, attention first. Ordering
/// follows the web board's bays (failed, cocked, holding, then traffic);
/// within attention bays the longest-quiet strip leads — it has waited
/// longest for the controller. One line per slip, no bodies: the slip tool
/// is the disclosure.
fn board_summary() -> (String, bool) {
    let dir = config::ledgers_dir();
    let report = match ledger::load_dir(std::path::Path::new(&dir)) {
        Ok(report) => report,
        // A board we cannot read must not masquerade as an empty one.
        Err(error) => return (format!("board unavailable: {error}"), true),
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut failed = Vec::new();
    let mut cocked = Vec::new();
    let mut holding = Vec::new();
    let mut flying = Vec::new();
    for folded in &report.slips {
        let slip = &folded.slip;
        if slip.status != ledger::Status::InFlight {
            continue;
        }
        if slip.failed.is_some() {
            failed.push(slip);
        } else if slip.cocked.is_some() {
            cocked.push(slip);
        } else if slip.holding.is_some() {
            holding.push(slip);
        } else {
            flying.push(slip);
        }
    }
    let quiet = |slip: &ledger::Slip| ledger::parse_ts(&slip.last_ts).unwrap_or(0);
    failed.sort_by_key(|s| quiet(s));
    cocked.sort_by_key(|s| quiet(s));
    holding.sort_by_key(|s| quiet(s));
    flying.sort_by_key(|s| std::cmp::Reverse(quiet(s)));

    let strips: Vec<&ledger::Slip> = failed
        .into_iter()
        .chain(cocked)
        .chain(holding)
        .chain(flying)
        .collect();
    if strips.is_empty() {
        return ("— clean board — no open slips".to_string(), false);
    }
    let lines: Vec<String> = strips
        .into_iter()
        .map(|slip| {
            let state = if let Some(phase) = &slip.failed {
                format!("FAILED at {phase} — awaiting disposition")
            } else if let Some(boundary) = &slip.cocked {
                format!("COCKED at {boundary} — awaiting clearance")
            } else if let Some(boundary) = &slip.holding {
                format!("holding at {boundary} — awaiting continue")
            } else {
                "in flight".to_string()
            };
            let territory = std::path::Path::new(&slip.repo)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| slip.repo.clone());
            let id = &slip.id;
            let workflow = &slip.workflow;
            let engineer = &slip.engineer;
            let tokens = slip.tokens;
            let cost = slip.cost;
            let age = human_age(now.saturating_sub(quiet(slip)));
            format!(
                "{id} · {workflow} · {state} · {engineer} · {territory} · \
                 {tokens} tok · ${cost:.4} · {age} ago"
            )
        })
        .collect();
    (lines.join("\n"), false)
}

fn human_age(secs: u64) -> String {
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m", secs / 60),
        3600..=86_399 => format!("{}h", secs / 3600),
        _ => format!("{}d", secs / 86_400),
    }
}

fn slip_summary(slip_id: &str) -> (String, bool) {
    let dir = config::ledgers_dir();
    let path = std::path::Path::new(&dir).join(format!("{slip_id}.jsonl"));
    let loaded = match ledger::load_ledger(&path) {
        Ok(loaded) => loaded,
        Err(error) => return (format!("no ledger for {slip_id}: {error}"), true),
    };
    let Some(slip) = ledger::fold(&loaded.events) else {
        return (format!("{slip_id} has a ledger but never opened"), true);
    };
    let mut lines = vec![
        format!("slip {} · {} · {:?}", slip.id, slip.workflow, slip.status),
        format!("engineer: {}", slip.engineer),
        format!("territory: {}", slip.repo),
        format!("request: {}", slip.request),
    ];
    if let Some(boundary) = &slip.cocked {
        lines.push(format!("COCKED at {boundary} — awaiting the controller"));
    }
    if let Some(boundary) = &slip.holding {
        lines.push(format!(
            "holding at {boundary} — cleared, awaiting continue"
        ));
    }
    if let Some(phase) = &slip.failed {
        lines.push(format!("FAILED at {phase} — awaiting disposition"));
    }
    if let Some(reason) = &slip.close_reason {
        if !reason.is_empty() {
            lines.push(format!("close reason: {reason}"));
        }
    }
    for phase in &slip.phases {
        let outcome = phase
            .outcome
            .as_ref()
            .map(|o| format!("{o:?}"))
            .unwrap_or_else(|| "open".to_string());
        // Pre-attribution ledgers fold to an empty flyer; stay quiet then.
        let flew = if phase.engineer.is_empty() {
            String::new()
        } else {
            format!(" · flown by {}", phase.engineer)
        };
        lines.push(format!(
            "phase {} ({}) · {outcome}{flew}",
            phase.phase, phase.owner
        ));
    }
    for section in &slip.sections {
        lines.push(format!("section {} · {}", section.section, section.summary));
    }
    lines.push(format!(
        "{} tokens · ${:.4} · {} tool call(s)",
        slip.tokens,
        slip.cost,
        slip.tool_trail.len()
    ));
    (lines.join("\n"), false)
}
