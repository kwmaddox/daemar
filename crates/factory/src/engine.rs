//! The phase engine: one agent stage, run as a turn loop, events on the
//! ledger, seat filled from the roster.
//!
//! A stage is one bounded phase of a workflow. It names a Role (the seat)
//! and carries only stage concerns — phase name, the section kind it must
//! produce, the task prompt, the pinned territory. Who fills the seat
//! (persona, airframe, tool access) is the roster's business. Toolless
//! agents simply finish in one turn; tooled agents loop over a DETACHED
//! WORKTREE of the territory — never the live checkout — with every call
//! logged with its epistemic pointer, optionally executing inside the cage.
//!
//! Failure must be witnessed: on provider error, turn-cap exhaustion, or a
//! cage that cannot vouch for itself (including an unproven teardown), the
//! engine records the reason, ends the phase in error, and returns None —
//! the slip stays open for the controller's disposition. A teardown failure
//! fails the phase even when the model already reported: no success section
//! rides above an unproven cage.

use std::path::{Path, PathBuf};

use ledger::{Kind, Lane, LedgerWriter, PhaseOutcome};

use crate::config::{Config, Pricing};
use crate::executor::StageExecutor;
use crate::roster::{self, AgentDef, Role, ToolAccess};
use crate::tools::{self, ToolContext};
use crate::wall::{Teardown, WallMode};
use crate::worktree;

/// Bounds pathology — loops and runaway spend — never task scope. High turn
/// consumption is a soft signal that a task may be doing too much and could be
/// split, not a requirement to split. Builder gets 48: roughly twice the
/// measured honest completion (23 turns) of a chore-grade multi-file task.
fn turn_cap(role: Role) -> usize {
    match role {
        Role::Scout => 12,
        Role::Planner => 12,
        Role::Responder => 12,
        Role::Builder => 48,
    }
}

fn turn_cap_exhausted_note(completed_turns: usize, cap: usize) -> String {
    format!("turn cap reached: completed_turns={completed_turns} cap={cap} without a report")
}

/// A territory validated and pinned BEFORE the slip was minted: the
/// canonical source checkout and the exact commit the stage will see.
pub struct PinnedTerritory {
    pub source: PathBuf,
    pub base: String,
}

/// Stage concerns only. The seat is named at the call site; everything about
/// its occupant comes from the roster.
pub struct StageSpec {
    pub phase: &'static str,
    pub section: &'static str,
    pub user: String,
    /// Required when the seated agent has tool access; the stage flies over
    /// a detached worktree of it.
    pub territory: Option<PinnedTerritory>,
}

/// What a completed stage hands back to its workflow.
pub struct StageOut {
    pub text: String,
    pub tokens: u64,
    pub cost: f64,
    pub turns: usize,
    /// The stage's retained worktree, when it flew one — the artifact a
    /// build workflow diffs and the apply era will consume.
    pub worktree: Option<PathBuf>,
}

/// How the turn loop ended, before teardown has its say.
enum LoopEnd {
    Report {
        text: String,
        turns: usize,
    },
    /// The reason is already on the ledger as a note; the phase ends in
    /// error after teardown runs.
    Failure,
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

    // Tooled seats fly over a pinned worktree, in-process or walled. Any
    // preparation failure after the phase started is witnessed, not thrown.
    let mut executor: Option<StageExecutor> = None;
    let mut stage_worktree: Option<PathBuf> = None;
    if agent.tools != ToolAccess::None {
        let Some(territory) = stage.territory.as_ref() else {
            return witness(
                w,
                &stage,
                format!(
                    "stage {} seated a tooled agent with no territory — workflow bug",
                    stage.phase
                ),
            );
        };
        let dest = Path::new(&config.worktrees)
            .join(w.slip_id().to_string())
            .join(stage.phase);
        let wt = match worktree::add_detached(&territory.source, &territory.base, &dest) {
            Ok(wt) => wt,
            Err(error) => return witness(w, &stage, format!("worktree failed: {error}")),
        };
        w.append(&Kind::Note {
            text: format!(
                "worktree materialized: phase={} base={} path={}",
                stage.phase,
                territory.base,
                wt.display()
            ),
        })?;
        stage_worktree = Some(wt.clone());
        // Write access cages unconditionally — the backstop behind the
        // workflow preflight; DAEMAR_CAGE remains the read-only seats' dial.
        let effective_cage = if agent.tools == ToolAccess::ReadWrite {
            WallMode::On
        } else {
            config.cage
        };
        executor = Some(match effective_cage {
            WallMode::Off => match ToolContext::new(&wt) {
                Ok(ctx) => StageExecutor::InProcess {
                    ctx,
                    access: agent.tools,
                },
                Err(error) => return witness(w, &stage, error),
            },
            WallMode::On => {
                let hint = format!("{}-{}", short(&w.slip_id().to_string()), stage.phase);
                match config.wall.open(&config.sandbox, &wt, agent.tools, &hint) {
                    Ok(wall) => {
                        w.append(&Kind::Note {
                            text: format!(
                                "sandbox started: phase={} wall={} sandbox_id={} image={}",
                                stage.phase,
                                config.wall.wall_name(),
                                wall.id(),
                                config.sandbox.image
                            ),
                        })?;
                        StageExecutor::Sandboxed {
                            wall,
                            access: agent.tools,
                            read_hashes: std::collections::HashMap::new(),
                        }
                    }
                    Err(error) => return witness(w, &stage, format!("sandbox failed: {error}")),
                }
            }
        });
    }
    let specs = executor.as_ref().map(|_| tools::specs(agent.tools));

    let mut flight_tokens = 0u64;
    let mut flight_cost = 0.0f64;
    let end = fly_loop(
        w,
        config,
        role,
        &agent,
        &model,
        &pricing,
        &stage,
        &mut executor,
        specs,
        &mut flight_tokens,
        &mut flight_cost,
    )?;

    // Teardown runs on EVERY path out of the loop. A phase is not finalized
    // until the cage is proven gone.
    let teardown_clean = match executor {
        Some(StageExecutor::Sandboxed { wall, .. }) => {
            let sandbox_id = wall.id().to_string();
            match wall.terminate() {
                Ok(proof) => {
                    let how = match proof {
                        Teardown::Removed => "",
                        // Something other than us removed it — harmless for
                        // containment (it is gone), but witnessed.
                        Teardown::AlreadyGone => " (was already gone)",
                    };
                    w.append(&Kind::Note {
                        text: format!(
                            "sandbox torn down{how}: phase={} sandbox_id={sandbox_id}",
                            stage.phase
                        ),
                    })?;
                    true
                }
                Err(error) => {
                    w.append(&Kind::Note {
                        text: format!(
                            "sandbox teardown FAILED: phase={} sandbox_id={sandbox_id}: {error}",
                            stage.phase
                        ),
                    })?;
                    eprintln!("daemar: sandbox teardown failed: {error}");
                    false
                }
            }
        }
        _ => true,
    };

    match (end, teardown_clean) {
        (LoopEnd::Report { text, turns }, true) => {
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
            Ok(Some(StageOut {
                text,
                tokens: flight_tokens,
                cost: flight_cost,
                turns,
                worktree: stage_worktree,
            }))
        }
        (LoopEnd::Report { .. }, false) | (LoopEnd::Failure, _) => {
            w.append(&Kind::PhaseEnded {
                phase: stage.phase.to_string(),
                outcome: PhaseOutcome::Error,
            })?;
            Ok(None)
        }
    }
}

/// Witness a preparation failure: note, phase ended in error, slip open.
fn witness(
    w: &mut LedgerWriter,
    stage: &StageSpec,
    reason: String,
) -> Result<Option<StageOut>, ledger::LedgerError> {
    w.append(&Kind::Note {
        text: reason.clone(),
    })?;
    w.append(&Kind::PhaseEnded {
        phase: stage.phase.to_string(),
        outcome: PhaseOutcome::Error,
    })?;
    eprintln!("daemar: {reason}");
    Ok(None)
}

fn short(slip_id: &str) -> String {
    slip_id.chars().take(8).collect()
}

/// The turn loop, ended by a report, a witnessed failure, or the cap. It
/// appends notes and calls but never PhaseEnded or the section — those wait
/// for teardown's verdict.
#[allow(clippy::too_many_arguments)] // the stage's full context, deliberately explicit
fn fly_loop(
    w: &mut LedgerWriter,
    config: &Config,
    role: Role,
    agent: &AgentDef,
    model: &str,
    pricing: &Pricing,
    stage: &StageSpec,
    executor: &mut Option<StageExecutor>,
    specs: Option<serde_json::Value>,
    flight_tokens: &mut u64,
    flight_cost: &mut f64,
) -> Result<LoopEnd, ledger::LedgerError> {
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
    let turn_cap = turn_cap(role);

    for turn in 1..=turn_cap {
        // Intent per turn: pairs with each model_call and keeps the silence
        // clock honest during long generations. Full prompts ride only on
        // turn 1 — later context is derivable from the tool trail.
        w.append(&Kind::ModelRequested {
            phase: stage.phase.to_string(),
            model: model.to_string(),
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
            model,
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
                eprintln!("daemar: model call failed: {reason}");
                return Ok(LoopEnd::Failure);
            }
        };
        let cost = pricing.cost(out.prompt_tokens, out.cached_tokens, out.completion_tokens);
        w.append(&Kind::ModelCall {
            phase: stage.phase.to_string(),
            model: model.to_string(),
            tokens: out.total_tokens,
            prompt_tokens: out.prompt_tokens,
            cached_tokens: out.cached_tokens,
            completion_tokens: out.completion_tokens,
            cost,
        })?;
        *flight_tokens += out.total_tokens;
        *flight_cost += cost;
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
            let Some(executor) = executor.as_mut() else {
                for call in &out.tool_calls {
                    w.append(&Kind::ToolCall {
                        phase: stage.phase.to_string(),
                        tool: call.name.clone(),
                        args: serde_json::from_str(&call.arguments)
                            .unwrap_or(serde_json::Value::Null),
                        ok: false,
                        summary: "refused: this agent has no tools".to_string(),
                        hash: String::new(),
                        before_hash: None,
                    })?;
                }
                let reason = format!(
                    "{} requested tools but holds none — provider misbehavior",
                    agent.name
                );
                w.append(&Kind::Note {
                    text: reason.clone(),
                })?;
                eprintln!("daemar: {reason}");
                return Ok(LoopEnd::Failure);
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
                        before_hash: None,
                    }
                } else {
                    executor.execute(&call.name, &args)
                };
                w.append(&Kind::ToolCall {
                    phase: stage.phase.to_string(),
                    tool: call.name.clone(),
                    args: args.clone(),
                    ok: !outcome.is_error,
                    summary: summarize(&outcome.content),
                    hash: outcome.hash.clone(),
                    before_hash: outcome.before_hash.clone(),
                })?;
                input.push(serde_json::json!({
                    "type": "function_call_output",
                    "call_id": call.id,
                    "output": outcome.content,
                }));
                // A dead cage cannot vouch for another call: the failed
                // outcome is on the trail; end the stage as witnessed.
                if executor.dead() {
                    let reason = "cage failed mid-stage — the flight cannot continue".to_string();
                    w.append(&Kind::Note {
                        text: reason.clone(),
                    })?;
                    eprintln!("daemar: {reason}");
                    return Ok(LoopEnd::Failure);
                }
            }
            continue;
        }

        if let Some(text) = out.text {
            return Ok(LoopEnd::Report { text, turns: turn });
        }
    }

    // The cap is the cap: report it, leave the slip open, let the
    // controller decide.
    w.append(&Kind::Note {
        text: turn_cap_exhausted_note(turn_cap, turn_cap),
    })?;
    eprintln!(
        "daemar: {} hit the turn cap ({turn_cap}) without reporting",
        agent.name
    );
    Ok(LoopEnd::Failure)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_caps_are_role_specific() {
        assert_eq!(turn_cap(Role::Scout), 12);
        assert_eq!(turn_cap(Role::Planner), 12);
        assert_eq!(turn_cap(Role::Responder), 12);
        assert_eq!(turn_cap(Role::Builder), 48);
    }

    #[test]
    fn exhaustion_note_records_completed_turns_and_cap() {
        assert_eq!(
            turn_cap_exhausted_note(48, 48),
            "turn cap reached: completed_turns=48 cap=48 without a report"
        );
    }
}
