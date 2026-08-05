//! The phase engine: one agent stage, run as a turn loop, events on the
//! ledger, seat filled from the roster.
//!
//! A stage is one bounded phase of a workflow. It names a Role (the seat)
//! and carries only stage concerns — phase name, the section kind it must
//! produce, the task prompt, the territory. Who fills the seat (persona,
//! airframe, tool access) is the roster's business. Toolless agents simply
//! finish in one turn; tooled agents loop, every call logged with its
//! epistemic pointer.
//!
//! Failure must be witnessed: on provider error or turn-cap exhaustion the
//! engine records the reason, ends the phase in error, and returns None —
//! the slip stays open for the controller's disposition.

use std::path::PathBuf;

use ledger::{Kind, Lane, LedgerWriter, PhaseOutcome};

use crate::config::{Config, Pricing};
use crate::roster::{self, Role, ToolAccess};
use crate::tools;

/// Turn cap for tool loops: enough for real recon, finite by construction.
const MAX_TURNS: usize = 12;

/// Stage concerns only. The seat is named at the call site; everything about
/// its occupant comes from the roster.
pub struct StageSpec {
    pub phase: &'static str,
    pub section: &'static str,
    pub user: String,
    /// Required when the seated agent has tool access; the tools are
    /// confined to it.
    pub territory: Option<PathBuf>,
}

/// What a completed stage hands back to its workflow.
pub struct StageOut {
    pub text: String,
    pub tokens: u64,
    pub cost: f64,
    pub turns: usize,
}

/// Fly one stage. `Ok(Some)` on success — the section is written and the
/// phase closed green. `Ok(None)` on witnessed failure — the reason is on
/// the ledger, the phase closed in error, the slip left open.
pub fn run_stage(
    w: &mut LedgerWriter,
    config: &Config,
    role: Role,
    stage: StageSpec,
) -> Result<Option<StageOut>, ledger::LedgerError> {
    let agent = roster::agent(role);
    let model = config.model_for(role).to_string();
    let pricing = Pricing::resolve(&config.airframes, &model);
    if let Some(complaint) = pricing.complaint() {
        eprintln!("daemar: {complaint}");
    }

    w.append(&Kind::PhaseStarted {
        phase: stage.phase.to_string(),
        owner: agent.name.to_string(),
        lane: Lane::Agent,
        engineer: config.engineer.clone(),
    })?;

    // Tool context, when the seat carries tools. A bad territory is a
    // witnessed failure, not a crash.
    let mut ctx = match agent.tools {
        ToolAccess::None => None,
        ToolAccess::ReadOnly => {
            let Some(territory) = stage.territory.as_deref() else {
                w.append(&Kind::Note {
                    text: format!(
                        "stage {} seated a tooled agent with no territory — workflow bug",
                        stage.phase
                    ),
                })?;
                w.append(&Kind::PhaseEnded {
                    phase: stage.phase.to_string(),
                    outcome: PhaseOutcome::Error,
                })?;
                return Ok(None);
            };
            match tools::ToolContext::new(territory) {
                Ok(ctx) => Some(ctx),
                Err(error) => {
                    w.append(&Kind::Note {
                        text: error.clone(),
                    })?;
                    w.append(&Kind::PhaseEnded {
                        phase: stage.phase.to_string(),
                        outcome: PhaseOutcome::Error,
                    })?;
                    eprintln!("daemar: {error}");
                    return Ok(None);
                }
            }
        }
    };
    let specs = ctx.as_ref().map(|_| tools::specs());

    let mut flight_tokens = 0u64;
    let mut flight_cost = 0.0f64;
    // The Responses input array, resent whole every turn (store:false).
    // The system prompt rides separately as `instructions`; on the ledger,
    // ModelRequested.system is the instructions and ModelRequested.user is
    // this initial input item — the epistemic record's mapping.
    let mut input = vec![serde_json::json!({
        "type": "message",
        "role": "user",
        "content": [{ "type": "input_text", "text": stage.user }],
    })];
    let mut complaint_logged = false;

    for turn in 1..=MAX_TURNS {
        // Intent per turn: pairs with each model_call and keeps the silence
        // clock honest during long generations. Full prompts ride only on
        // turn 1 — later context is derivable from the tool trail.
        w.append(&Kind::ModelRequested {
            phase: stage.phase.to_string(),
            model: model.clone(),
            system: if turn == 1 {
                agent.system.to_string()
            } else {
                String::new()
            },
            user: if turn == 1 {
                stage.user.clone()
            } else {
                String::new()
            },
        })?;
        let out = match config.provider.respond(
            &model,
            agent.system,
            &input,
            specs.as_ref(),
            config.effort_for(role),
        ) {
            Ok(out) => out,
            Err(error) => {
                let reason = error.to_string();
                w.append(&Kind::Note {
                    text: format!("model call failed: {reason}"),
                })?;
                w.append(&Kind::PhaseEnded {
                    phase: stage.phase.to_string(),
                    outcome: PhaseOutcome::Error,
                })?;
                eprintln!("daemar: model call failed: {reason}");
                return Ok(None);
            }
        };
        let cost = pricing.cost(out.prompt_tokens, out.cached_tokens, out.completion_tokens);
        w.append(&Kind::ModelCall {
            phase: stage.phase.to_string(),
            model: model.clone(),
            tokens: out.total_tokens,
            prompt_tokens: out.prompt_tokens,
            cached_tokens: out.cached_tokens,
            completion_tokens: out.completion_tokens,
            cost,
        })?;
        flight_tokens += out.total_tokens;
        flight_cost += cost;
        if !complaint_logged {
            if let Some(complaint) = pricing.complaint() {
                w.append(&Kind::Note { text: complaint })?;
                complaint_logged = true;
            }
        }

        if !out.tool_calls.is_empty() {
            // A toolless seat was advertised no tools; a provider that sends
            // tool calls anyway is misbehaving. Witness it, don't pay turns
            // conversing with it.
            let Some(ctx) = ctx.as_mut() else {
                for call in &out.tool_calls {
                    w.append(&Kind::ToolCall {
                        phase: stage.phase.to_string(),
                        tool: call.name.clone(),
                        args: serde_json::from_str(&call.arguments)
                            .unwrap_or(serde_json::Value::Null),
                        ok: false,
                        summary: "refused: this agent has no tools".to_string(),
                        hash: String::new(),
                    })?;
                }
                let reason = format!(
                    "{} requested tools but holds none — provider misbehavior",
                    agent.name
                );
                w.append(&Kind::Note {
                    text: reason.clone(),
                })?;
                w.append(&Kind::PhaseEnded {
                    phase: stage.phase.to_string(),
                    outcome: PhaseOutcome::Error,
                })?;
                eprintln!("daemar: {reason}");
                return Ok(None);
            };
            // Stateless replay: the turn's reasoning and function_call
            // items ride back in the next input, or the reasoning thread
            // breaks. This is the state chat-completions never made us keep.
            input.extend(out.continuation.iter().cloned());
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
                    tools::execute(&call.name, &args, ctx)
                };
                w.append(&Kind::ToolCall {
                    phase: stage.phase.to_string(),
                    tool: call.name.clone(),
                    args: args.clone(),
                    ok: !outcome.is_error,
                    summary: summarize(&outcome.content),
                    hash: outcome.hash.clone(),
                })?;
                input.push(serde_json::json!({
                    "type": "function_call_output",
                    "call_id": call.id,
                    "output": outcome.content,
                }));
            }
            continue;
        }

        if let Some(text) = out.text {
            w.append(&Kind::SectionWritten {
                section: stage.section.to_string(),
                by: agent.name.to_string(),
                summary: summarize(&text),
                body: text.clone(),
            })?;
            w.append(&Kind::PhaseEnded {
                phase: stage.phase.to_string(),
                outcome: PhaseOutcome::Success,
            })?;
            return Ok(Some(StageOut {
                text,
                tokens: flight_tokens,
                cost: flight_cost,
                turns: turn,
            }));
        }
    }

    // The cap is the cap: report it, leave the slip open, let the
    // controller decide.
    w.append(&Kind::Note {
        text: format!("turn cap ({MAX_TURNS}) reached without a report"),
    })?;
    w.append(&Kind::PhaseEnded {
        phase: stage.phase.to_string(),
        outcome: PhaseOutcome::Error,
    })?;
    eprintln!(
        "daemar: {} hit the turn cap ({MAX_TURNS}) without reporting",
        agent.name
    );
    Ok(None)
}

/// The strip's table line: first line of the text, clipped.
pub fn summarize(text: &str) -> String {
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
