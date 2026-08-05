//! The factory's runner.
//!
//!     daemar "<request>"              one-phase prompt workflow
//!     ... | daemar -                  request from stdin
//!     daemar plan "<request>"         plan phase, then cock at plan->respond
//!     daemar grant <slip-id>          controller clears the boundary
//!     daemar refuse <slip-id> [why]   controller refuses; slip closes rejected
//!     daemar continue <slip-id>       fly the next phase from the printout
//!     daemar dispose <slip-id> [why]  close a flight that could not close itself
//!
//! Exit-and-resume at boundaries, by ruling: a flight that hits a clearance
//! EXITS. The strip cocks on the board; `continue` later rebuilds context
//! purely from the ledger — the printout — in a fresh process, possibly on a
//! different airframe. The ledger is the memory; sessions are caches.
//!
//! Failure must be witnessed: on error the runner reports, ends the phase in
//! error, and leaves the slip OPEN for the controller's disposition.

use std::fmt;
use std::process::ExitCode;

use ledger::{Kind, Lane, LedgerWriter, PhaseOutcome, Slip, SlipId, SlipOutcome, Status};

mod provider;
mod registry;
mod tools;

use provider::Provider;
use registry::Registry;

const RESPOND_SYSTEM: &str = "You are daemar's responder. Answer the request directly and \
completely, in plain text. If a plan is provided, follow it.";
const PLAN_SYSTEM: &str = "You are daemar's planner. Produce a concise plan for answering the \
engineer's request: what the answer must cover, in what order, and what would make it \
complete. Do not answer the request itself. Plain text.";
const BOUNDARY: &str = "plan->respond";
const SCOUT_SYSTEM: &str = "You are daemar's scout: read-only reconnaissance over one \
repository (the territory). Use the tools to find where things live and how they connect. \
Never guess file contents — read them. When you have enough evidence, reply in plain text \
with your findings: what lives where (cite paths), how the pieces connect, and anything \
surprising. Be concise and concrete.";
/// Turn cap for tool loops: enough for real recon, finite by construction.
const MAX_TURNS: usize = 12;

// ── Config (env is the serde edge of a CLI; parsed once, here) ───────────────

struct Config {
    provider: Provider,
    plan_model: String,
    respond_model: String,
    scout_model: String,
    ledgers: String,
    airframes: String,
    engineer: String,
}

#[derive(Debug)]
enum ConfigError {
    Missing(&'static str),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Missing(name) => write!(
                f,
                "{name} is not set — cd into the repo so direnv decrypts secrets, \
                 or export it"
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

impl Config {
    fn from_env() -> Result<Self, ConfigError> {
        let default_model = env("DAEMAR_MODEL");
        let plan_model = env("DAEMAR_PLAN_MODEL")
            .or_else(|| default_model.clone())
            .ok_or(ConfigError::Missing("DAEMAR_PLAN_MODEL (or DAEMAR_MODEL)"))?;
        let respond_model = env("DAEMAR_RESPOND_MODEL")
            .or_else(|| default_model.clone())
            .ok_or(ConfigError::Missing(
                "DAEMAR_RESPOND_MODEL (or DAEMAR_MODEL)",
            ))?;
        let scout_model = env("DAEMAR_SCOUT_MODEL")
            .or_else(|| default_model.clone())
            .ok_or(ConfigError::Missing("DAEMAR_SCOUT_MODEL (or DAEMAR_MODEL)"))?;
        Ok(Config {
            provider: Provider {
                base_url: env("DAEMAR_BASE_URL")
                    .unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
                api_key: env("OPENAI_API_KEY").ok_or(ConfigError::Missing("OPENAI_API_KEY"))?,
            },
            plan_model,
            respond_model,
            scout_model,
            ledgers: env("DAEMAR_LEDGERS").unwrap_or_else(|| "ledgers".to_string()),
            airframes: env("DAEMAR_AIRFRAMES").unwrap_or_else(|| "airframes.toml".to_string()),
            engineer: engineer(),
        })
    }
}

fn engineer() -> String {
    env("USER").unwrap_or_else(|| "engineer".to_string())
}

fn ledgers_dir() -> String {
    env("DAEMAR_LEDGERS").unwrap_or_else(|| "ledgers".to_string())
}

/// The default territory: wherever the engineer is standing.
fn cwd_territory() -> String {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.canonicalize().ok())
        .map(|p| p.display().to_string())
        .unwrap_or_default()
}

// ── Pricing (resolved per airframe; not-priced is never silent) ──────────────

enum Pricing {
    Priced(registry::Price),
    Unregistered { model: String },
    RegistryBroken { detail: String },
}

impl Pricing {
    fn resolve(airframes: &str, model: &str) -> Pricing {
        match Registry::load(airframes.as_ref()) {
            Ok(registry) => match registry.price(model) {
                Some(price) => Pricing::Priced(price),
                None => Pricing::Unregistered {
                    model: model.to_string(),
                },
            },
            Err(error) => Pricing::RegistryBroken {
                detail: error.to_string(),
            },
        }
    }

    fn cost(&self, prompt_tokens: u64, cached_tokens: u64, completion_tokens: u64) -> f64 {
        match self {
            Pricing::Priced(price) => price.cost(prompt_tokens, cached_tokens, completion_tokens),
            Pricing::Unregistered { .. } | Pricing::RegistryBroken { .. } => 0.0,
        }
    }

    fn complaint(&self) -> Option<String> {
        match self {
            Pricing::Priced(_) => None,
            Pricing::Unregistered { model } => Some(format!(
                "airframe {model} is not in airframes.toml; cost unrecorded"
            )),
            Pricing::RegistryBroken { detail } => Some(format!(
                "airframe registry unreadable; cost unrecorded — {detail}"
            )),
        }
    }
}

// ── Dispatch ─────────────────────────────────────────────────────────────────

fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("dispose") => dispose(&args[1..]),
        Some("grant") => grant(&args[1..]),
        Some("refuse") => refuse(&args[1..]),
        Some("continue") => continue_flight(&args[1..]),
        Some("plan") => match read_request(&args[1..]) {
            Some(request) => plan_flight(&request),
            None => usage(),
        },
        Some("scout") => scout_dispatch(&args[1..]),
        _ => match read_request(&args) {
            Some(request) => prompt_flight(&request),
            None => usage(),
        },
    }
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: daemar \"<request>\"            (or ... | daemar -)\n       \
         daemar plan \"<request>\"\n       daemar scout [--repo <path>] \"<question>\"\n       \
         daemar grant|refuse|continue|dispose <slip-id> [\"<reason>\"]"
    );
    ExitCode::from(2)
}

/// The request from args, or stdin when the sole arg is '-'. None = usage.
fn read_request(args: &[String]) -> Option<String> {
    let request = if args.len() == 1 && args[0] == "-" {
        let mut buffer = String::new();
        use std::io::Read;
        std::io::stdin().read_to_string(&mut buffer).ok()?;
        buffer
    } else {
        args.join(" ")
    };
    if request.trim().is_empty() {
        None
    } else {
        Some(request)
    }
}

fn with_config<F>(flight: F) -> ExitCode
where
    F: FnOnce(&Config) -> Result<bool, ledger::LedgerError>,
{
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("daemar: {error}");
            return ExitCode::from(2);
        }
    };
    match flight(&config) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            // A ledger that cannot be written is a flight that cannot be
            // recorded: nothing to salvage, fail loud.
            eprintln!("daemar: ledger failure: {error}");
            ExitCode::FAILURE
        }
    }
}

// ── The phase engine: one agent phase, events in, reply out ──────────────────

struct PhaseSpec<'a> {
    phase: &'a str,
    owner: &'a str,
    section: &'a str,
    model: &'a str,
    system: &'a str,
    user: String,
}

/// Fly one agent phase, appending its whole event sequence. `Ok(Some)` on
/// success; `Ok(None)` when the model call failed — the phase is closed in
/// error and the slip is left open (failure must be witnessed).
fn agent_phase(
    w: &mut LedgerWriter,
    config: &Config,
    spec: &PhaseSpec,
) -> Result<Option<provider::ModelReply>, ledger::LedgerError> {
    let pricing = Pricing::resolve(&config.airframes, spec.model);
    if let Some(complaint) = pricing.complaint() {
        eprintln!("daemar: {complaint}");
    }
    w.append(&Kind::PhaseStarted {
        phase: spec.phase.to_string(),
        owner: spec.owner.to_string(),
        lane: Lane::Agent,
    })?;
    w.append(&Kind::ModelRequested {
        phase: spec.phase.to_string(),
        model: spec.model.to_string(),
        system: spec.system.to_string(),
        user: spec.user.clone(),
    })?;
    match config
        .provider
        .complete(spec.model, spec.system, &spec.user)
    {
        Ok(reply) => {
            let cost = pricing.cost(
                reply.prompt_tokens,
                reply.cached_tokens,
                reply.completion_tokens,
            );
            w.append(&Kind::ModelCall {
                phase: spec.phase.to_string(),
                model: spec.model.to_string(),
                tokens: reply.total_tokens,
                prompt_tokens: reply.prompt_tokens,
                cached_tokens: reply.cached_tokens,
                completion_tokens: reply.completion_tokens,
                cost,
            })?;
            if let Some(complaint) = pricing.complaint() {
                w.append(&Kind::Note { text: complaint })?;
            }
            w.append(&Kind::SectionWritten {
                section: spec.section.to_string(),
                by: spec.owner.to_string(),
                summary: summarize(&reply.text),
                body: reply.text.clone(),
            })?;
            w.append(&Kind::PhaseEnded {
                phase: spec.phase.to_string(),
                outcome: PhaseOutcome::Success,
            })?;
            Ok(Some(reply))
        }
        Err(error) => {
            let reason = error.to_string();
            w.append(&Kind::Note {
                text: format!("model call failed: {reason}"),
            })?;
            w.append(&Kind::PhaseEnded {
                phase: spec.phase.to_string(),
                outcome: PhaseOutcome::Error,
            })?;
            eprintln!("daemar: model call failed: {reason}");
            Ok(None)
        }
    }
}

fn failed_open_message(id: &SlipId) {
    eprintln!(
        "slip {id} · FAILED, left open for disposition · \
         daemar dispose {id} \"<reason>\" · board: /slip/{id}"
    );
}

// ── Workflows (workflows are Rust) ───────────────────────────────────────────

/// The one-phase prompt workflow: respond, close accepted.
fn prompt_flight(request: &str) -> ExitCode {
    with_config(|config| {
        let mut w = LedgerWriter::create(
            config.ledgers.as_ref(),
            SlipId(uuid::Uuid::now_v7().to_string()),
        )?;
        w.append(&Kind::SlipOpened {
            request: request.to_string(),
            workflow: "prompt".to_string(),
            repo: cwd_territory(),
            engineer: config.engineer.clone(),
        })?;
        let spec = PhaseSpec {
            phase: "respond",
            owner: "responder",
            section: "response.v1",
            model: &config.respond_model,
            system: RESPOND_SYSTEM,
            user: request.to_string(),
        };
        match agent_phase(&mut w, config, &spec)? {
            Some(reply) => {
                close_accepted(&mut w)?;
                println!("{}", reply.text.trim_end());
                eprintln!(
                    "\nslip {id} · accepted · {} tokens · board: /slip/{id}",
                    reply.total_tokens,
                    id = w.slip_id()
                );
                Ok(true)
            }
            None => {
                failed_open_message(w.slip_id());
                Ok(false)
            }
        }
    })
}

/// Phase one of the planned workflow: plan, then cock at the boundary and
/// EXIT. The strip waits on the board; the ledger carries everything the
/// respond phase will need.
fn plan_flight(request: &str) -> ExitCode {
    with_config(|config| {
        let mut w = LedgerWriter::create(
            config.ledgers.as_ref(),
            SlipId(uuid::Uuid::now_v7().to_string()),
        )?;
        w.append(&Kind::SlipOpened {
            request: request.to_string(),
            workflow: "plan".to_string(),
            repo: cwd_territory(),
            engineer: config.engineer.clone(),
        })?;
        let spec = PhaseSpec {
            phase: "plan",
            owner: "planner",
            section: "plan.v1",
            model: &config.plan_model,
            system: PLAN_SYSTEM,
            user: request.to_string(),
        };
        match agent_phase(&mut w, config, &spec)? {
            Some(reply) => {
                w.append(&Kind::ClearanceRequested {
                    boundary: BOUNDARY.to_string(),
                    by: "planner".to_string(),
                })?;
                println!("{}", reply.text.trim_end());
                eprintln!(
                    "\nslip {id} · COCKED at {BOUNDARY} · \
                     daemar grant {id}  |  daemar refuse {id} \"<reason>\" · board: /slip/{id}",
                    id = w.slip_id()
                );
                Ok(true)
            }
            None => {
                failed_open_message(w.slip_id());
                Ok(false)
            }
        }
    })
}

/// Fly the phase after a granted boundary, context rebuilt purely from the
/// ledger: the printout. A fresh process, possibly a different airframe —
/// the ledger is the memory.
fn continue_flight(args: &[String]) -> ExitCode {
    let Some(slip_id) = args.first().filter(|a| !a.trim().is_empty()) else {
        return usage();
    };
    let slip = match load_open_slip(slip_id) {
        Ok(slip) => slip,
        Err(message) => {
            eprintln!("daemar: {message}");
            return ExitCode::from(2);
        }
    };
    if let Some(boundary) = &slip.cocked {
        eprintln!(
            "daemar: {slip_id} is awaiting clearance at {boundary} — \
             daemar grant {slip_id} first"
        );
        return ExitCode::from(2);
    }
    let granted = slip.clearances.iter().any(|c| {
        c.boundary == BOUNDARY
            && c.response
                .as_ref()
                .is_some_and(|r| r.verdict == ledger::ClearanceVerdict::Granted)
    });
    if !granted {
        eprintln!("daemar: {slip_id} has no granted {BOUNDARY} clearance — nothing to continue");
        return ExitCode::from(2);
    }
    if slip.phases.iter().any(|p| p.phase == "respond") {
        eprintln!("daemar: {slip_id} already flew its respond phase");
        return ExitCode::from(2);
    }
    let Some(plan_body) = slip
        .sections
        .iter()
        .rev()
        .find(|s| s.section == "plan.v1")
        .map(|s| s.body.clone())
    else {
        eprintln!("daemar: {slip_id} has no plan.v1 section to continue from");
        return ExitCode::from(2);
    };

    with_config(|config| {
        let mut w = LedgerWriter::resume(config.ledgers.as_ref(), SlipId(slip_id.clone()))?;
        // The printout: this phase's declared context, assembled from the
        // ledger and nothing else. No process memory, no session.
        let printout = format!(
            "## Request\n\n{}\n\n## Plan (from the plan phase)\n\n{plan_body}\n\n\
             ## Task\n\nAnswer the request, following the plan.",
            slip.request
        );
        let spec = PhaseSpec {
            phase: "respond",
            owner: "responder",
            section: "response.v1",
            model: &config.respond_model,
            system: RESPOND_SYSTEM,
            user: printout,
        };
        match agent_phase(&mut w, config, &spec)? {
            Some(reply) => {
                close_accepted(&mut w)?;
                println!("{}", reply.text.trim_end());
                eprintln!(
                    "\nslip {id} · accepted · {} tokens · board: /slip/{id}",
                    reply.total_tokens,
                    id = w.slip_id()
                );
                Ok(true)
            }
            None => {
                failed_open_message(w.slip_id());
                Ok(false)
            }
        }
    })
}

// ── The scout: the factory learns to read ────────────────────────────────────

/// `daemar scout [--repo <path>] "<question>"` — parse the territory flag,
/// then fly.
fn scout_dispatch(args: &[String]) -> ExitCode {
    let (repo, rest): (String, &[String]) =
        if args.first().map(String::as_str) == Some("--repo") && args.len() >= 2 {
            (args[1].clone(), &args[2..])
        } else {
            (".".to_string(), args)
        };
    match read_request(rest) {
        Some(request) => scout_flight(&request, &repo),
        None => usage(),
    }
}

/// The first tool-using flight: a turn loop over read-only tools, confined
/// to the territory, every call on the ledger. The tower stays home; only
/// the tools visit the repo.
fn scout_flight(request: &str, repo: &str) -> ExitCode {
    let mut ctx = match tools::ToolContext::new(std::path::Path::new(repo)) {
        Ok(ctx) => ctx,
        Err(error) => {
            eprintln!("daemar: {error}");
            return ExitCode::from(2);
        }
    };
    let territory = match std::path::Path::new(repo).canonicalize() {
        Ok(p) => p.display().to_string(),
        Err(_) => repo.to_string(),
    };

    with_config(|config| {
        let mut w = LedgerWriter::create(
            config.ledgers.as_ref(),
            SlipId(uuid::Uuid::now_v7().to_string()),
        )?;
        w.append(&Kind::SlipOpened {
            request: request.to_string(),
            workflow: "scout".to_string(),
            engineer: config.engineer.clone(),
            repo: territory.clone(),
        })?;
        w.append(&Kind::PhaseStarted {
            phase: "scout".to_string(),
            owner: "scout".to_string(),
            lane: Lane::Agent,
        })?;

        let model = config.scout_model.clone();
        let pricing = Pricing::resolve(&config.airframes, &model);
        if let Some(complaint) = pricing.complaint() {
            eprintln!("daemar: {complaint}");
        }
        let specs = tools::specs();
        let mut messages = vec![
            serde_json::json!({ "role": "system", "content": SCOUT_SYSTEM }),
            serde_json::json!({ "role": "user", "content": request }),
        ];
        let mut priced_complaint_logged = false;

        for turn in 1..=MAX_TURNS {
            // Intent per turn: pairs with each model_call, keeps the silence
            // clock honest during long generations. Full prompts ride only on
            // turn 1 — later context is derivable from the trail.
            w.append(&Kind::ModelRequested {
                phase: "scout".to_string(),
                model: model.clone(),
                system: if turn == 1 {
                    SCOUT_SYSTEM.to_string()
                } else {
                    String::new()
                },
                user: if turn == 1 {
                    request.to_string()
                } else {
                    String::new()
                },
            })?;
            let out = match config.provider.chat(&model, &messages, Some(&specs)) {
                Ok(out) => out,
                Err(error) => {
                    let reason = error.to_string();
                    w.append(&Kind::Note {
                        text: format!("model call failed: {reason}"),
                    })?;
                    w.append(&Kind::PhaseEnded {
                        phase: "scout".to_string(),
                        outcome: PhaseOutcome::Error,
                    })?;
                    eprintln!("daemar: model call failed: {reason}");
                    failed_open_message(w.slip_id());
                    return Ok(false);
                }
            };
            let cost = pricing.cost(out.prompt_tokens, out.cached_tokens, out.completion_tokens);
            w.append(&Kind::ModelCall {
                phase: "scout".to_string(),
                model: model.clone(),
                tokens: out.total_tokens,
                prompt_tokens: out.prompt_tokens,
                cached_tokens: out.cached_tokens,
                completion_tokens: out.completion_tokens,
                cost,
            })?;
            if !priced_complaint_logged {
                if let Some(complaint) = pricing.complaint() {
                    w.append(&Kind::Note { text: complaint })?;
                    priced_complaint_logged = true;
                }
            }

            if !out.tool_calls.is_empty() {
                messages.push(out.assistant.clone());
                for call in &out.tool_calls {
                    let args: serde_json::Value =
                        serde_json::from_str(&call.arguments).unwrap_or(serde_json::Value::Null);
                    let outcome = if args.is_null() {
                        tools::ToolOutcome {
                            content: format!("{}: arguments were not valid JSON", call.name),
                            is_error: true,
                            hash: String::new(),
                        }
                    } else {
                        tools::execute(&call.name, &args, &mut ctx)
                    };
                    w.append(&Kind::ToolCall {
                        phase: "scout".to_string(),
                        tool: call.name.clone(),
                        args: args.clone(),
                        ok: !outcome.is_error,
                        summary: summarize(&outcome.content),
                        hash: outcome.hash.clone(),
                    })?;
                    messages.push(serde_json::json!({
                        "role": "tool",
                        "tool_call_id": call.id,
                        "content": outcome.content,
                    }));
                }
                continue;
            }

            if let Some(text) = out.text {
                w.append(&Kind::SectionWritten {
                    section: "scout.v1".to_string(),
                    by: "scout".to_string(),
                    summary: summarize(&text),
                    body: text.clone(),
                })?;
                w.append(&Kind::PhaseEnded {
                    phase: "scout".to_string(),
                    outcome: PhaseOutcome::Success,
                })?;
                close_accepted(&mut w)?;
                println!("{}", text.trim_end());
                eprintln!(
                    "\nslip {id} · accepted · {} tokens · {} turns · board: /slip/{id}",
                    out.total_tokens,
                    turn,
                    id = w.slip_id()
                );
                return Ok(true);
            }
        }

        // The cap is the cap: report it, leave the slip open, let the
        // controller decide.
        w.append(&Kind::Note {
            text: format!("turn cap ({MAX_TURNS}) reached without a report"),
        })?;
        w.append(&Kind::PhaseEnded {
            phase: "scout".to_string(),
            outcome: PhaseOutcome::Error,
        })?;
        eprintln!("daemar: scout hit the turn cap ({MAX_TURNS}) without reporting");
        failed_open_message(w.slip_id());
        Ok(false)
    })
}

fn close_accepted(w: &mut LedgerWriter) -> Result<(), ledger::LedgerError> {
    w.append(&Kind::SlipClosed {
        outcome: SlipOutcome::Accepted,
        reason: String::new(),
        by: "daemar".to_string(),
    })?;
    Ok(())
}

// ── The controller's pens ────────────────────────────────────────────────────

/// Grant the pending clearance: the strip un-cocks, the flight may continue.
fn grant(args: &[String]) -> ExitCode {
    controller_pen(args, |slip, _reason| {
        let Some(boundary) = slip.cocked.clone() else {
            return Err(format!("{} is not awaiting any clearance", slip.id));
        };
        Ok((
            vec![Kind::ClearanceGranted {
                boundary: boundary.clone(),
                by: engineer(),
            }],
            format!("cleared at {boundary} · daemar continue {}", slip.id),
        ))
    })
}

/// Refuse the pending clearance: a verdict-carrying rejection, so the slip
/// closes directly — attention was paid here, at the boundary.
fn refuse(args: &[String]) -> ExitCode {
    controller_pen(args, |slip, reason| {
        let Some(boundary) = slip.cocked.clone() else {
            return Err(format!("{} is not awaiting any clearance", slip.id));
        };
        let reason = if reason.is_empty() {
            "clearance refused".to_string()
        } else {
            reason
        };
        Ok((
            vec![
                Kind::ClearanceRefused {
                    boundary: boundary.clone(),
                    by: engineer(),
                    reason: reason.clone(),
                },
                Kind::SlipClosed {
                    outcome: SlipOutcome::Rejected,
                    reason,
                    by: engineer(),
                },
            ],
            format!("refused at {boundary} · slip closed rejected"),
        ))
    })
}

/// Close a flight that could not close itself. Ends any still-open phase as
/// an error first. Refuses already-closed slips: history is not re-litigated.
fn dispose(args: &[String]) -> ExitCode {
    controller_pen(args, |slip, reason| {
        let reason = if reason.is_empty() {
            "disposed by controller".to_string()
        } else {
            reason
        };
        let mut events = Vec::new();
        if let Some(open_phase) = slip.current_phase.clone() {
            events.push(Kind::PhaseEnded {
                phase: open_phase,
                outcome: PhaseOutcome::Error,
            });
        }
        events.push(Kind::SlipClosed {
            outcome: SlipOutcome::Rejected,
            reason: reason.clone(),
            by: engineer(),
        });
        Ok((events, format!("disposed by {} · {reason}", engineer())))
    })
}

/// Shared shape of every controller write: load the open slip, decide the
/// events, append them, say what happened.
fn controller_pen<F>(args: &[String], decide: F) -> ExitCode
where
    F: FnOnce(&Slip, String) -> Result<(Vec<Kind>, String), String>,
{
    let Some(slip_id) = args.first().filter(|a| !a.trim().is_empty()) else {
        return usage();
    };
    let reason = args[1..].join(" ");
    let slip = match load_open_slip(slip_id) {
        Ok(slip) => slip,
        Err(message) => {
            eprintln!("daemar: {message}");
            return ExitCode::from(2);
        }
    };
    let (events, message) = match decide(&slip, reason) {
        Ok(decided) => decided,
        Err(message) => {
            eprintln!("daemar: {message}");
            return ExitCode::from(2);
        }
    };
    let result = (|| -> Result<(), ledger::LedgerError> {
        let mut w = LedgerWriter::resume(ledgers_dir().as_ref(), SlipId(slip_id.clone()))?;
        for event in &events {
            w.append(event)?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            println!("slip {slip_id} · {message}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("daemar: ledger failure: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Load and fold a slip that must still be open.
fn load_open_slip(slip_id: &str) -> Result<Slip, String> {
    let dir = ledgers_dir();
    let path = std::path::Path::new(&dir).join(format!("{slip_id}.jsonl"));
    let loaded = ledger::load_ledger(&path).map_err(|e| e.to_string())?;
    let slip = ledger::fold(&loaded.events)
        .ok_or_else(|| format!("{slip_id} has a ledger but never opened"))?;
    if slip.status != Status::InFlight {
        return Err(format!(
            "{slip_id} is already closed ({:?}); history is not re-litigated",
            slip.status
        ));
    }
    Ok(slip)
}

/// The strip's table line: first line of the response, clipped.
fn summarize(text: &str) -> String {
    let first = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    if first.chars().count() <= 110 {
        first.to_string()
    } else {
        let clipped: String = first.chars().take(109).collect();
        format!("{clipped}…")
    }
}
