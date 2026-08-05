//! Workflows are Rust: each flight is a plain function sequencing stages.
//! Stages name Roles; the roster seats them. The workflow owns the shape —
//! boundaries, clearances, what closes and what cocks.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ledger::{Kind, LedgerWriter, SlipId, SlipOutcome};

use crate::config::{cwd_territory, with_config, Config};
use crate::engine::{run_stage, StageOut, StageSpec};
use crate::pens::load_open_slip;
use crate::roster::Role;

pub const BOUNDARY: &str = "plan->respond";

/// Validate a territory before any slip is minted: a bad path is a usage
/// error, not a flight.
fn canonical_territory(repo: &str) -> Result<PathBuf, String> {
    let path = Path::new(repo);
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("territory {} cannot be resolved: {e}", path.display()))?;
    if !canonical.is_dir() {
        return Err(format!(
            "territory {} is not a directory",
            canonical.display()
        ));
    }
    Ok(canonical)
}

fn open_flight(
    config: &Config,
    workflow: &str,
    request: &str,
    territory: &str,
) -> Result<LedgerWriter, ledger::LedgerError> {
    let mut w = LedgerWriter::create(
        config.ledgers.as_ref(),
        SlipId(uuid::Uuid::now_v7().to_string()),
    )?;
    w.append(&Kind::SlipOpened {
        request: request.to_string(),
        workflow: workflow.to_string(),
        engineer: config.engineer.clone(),
        repo: territory.to_string(),
    })?;
    Ok(w)
}

fn close_accepted(w: &mut LedgerWriter) -> Result<(), ledger::LedgerError> {
    w.append(&Kind::SlipClosed {
        outcome: SlipOutcome::Accepted,
        reason: String::new(),
        by: "daemar".to_string(),
    })?;
    Ok(())
}

fn failed_open_message(id: &SlipId) {
    eprintln!(
        "slip {id} · FAILED, left open for disposition · \
         daemar dispose {id} \"<reason>\" · board: /slip/{id}"
    );
}

fn accepted_message(id: &SlipId, out: &StageOut) {
    eprintln!(
        "\nslip {id} · accepted · {} tokens · ${:.4} · {} turn(s) · board: /slip/{id}",
        out.tokens, out.cost, out.turns
    );
}

/// The one-stage prompt workflow: respond, close accepted.
pub fn prompt_flight(request: &str) -> ExitCode {
    with_config(|config| {
        let mut w = open_flight(config, "prompt", request, &cwd_territory())?;
        let stage = StageSpec {
            phase: "respond",
            section: "response.v1",
            user: request.to_string(),
            territory: None,
        };
        match run_stage(&mut w, config, Role::Responder, stage)? {
            Some(out) => {
                close_accepted(&mut w)?;
                println!("{}", out.text.trim_end());
                accepted_message(w.slip_id(), &out);
                Ok(true)
            }
            None => {
                failed_open_message(w.slip_id());
                Ok(false)
            }
        }
    })
}

/// Stage one of the planned workflow: a GROUNDED plan — the planner reads
/// the territory with the scout's tools before planning — then the slip
/// cocks at the boundary and the process EXITS. The strip waits on the
/// board; the ledger carries everything the respond stage will need.
pub fn plan_flight(request: &str, repo: &str) -> ExitCode {
    let territory = match canonical_territory(repo) {
        Ok(t) => t,
        Err(error) => {
            eprintln!("daemar: {error}");
            return ExitCode::from(2);
        }
    };
    with_config(|config| {
        let mut w = open_flight(config, "plan", request, &territory.display().to_string())?;
        let stage = StageSpec {
            phase: "plan",
            section: "plan.v1",
            user: request.to_string(),
            territory: Some(territory.clone()),
        };
        match run_stage(&mut w, config, Role::Planner, stage)? {
            Some(out) => {
                w.append(&Kind::ClearanceRequested {
                    boundary: BOUNDARY.to_string(),
                    by: "planner".to_string(),
                })?;
                println!("{}", out.text.trim_end());
                eprintln!(
                    "\nslip {id} · COCKED at {BOUNDARY} · {} tokens · ${:.4} · {} turn(s) · \
                     daemar grant {id}  |  daemar refuse {id} \"<reason>\" · board: /slip/{id}",
                    out.tokens,
                    out.cost,
                    out.turns,
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

/// Read-only reconnaissance over a territory. The tower stays home; only
/// the tools visit the repo.
pub fn scout_flight(request: &str, repo: &str) -> ExitCode {
    let territory = match canonical_territory(repo) {
        Ok(t) => t,
        Err(error) => {
            eprintln!("daemar: {error}");
            return ExitCode::from(2);
        }
    };
    with_config(|config| {
        let mut w = open_flight(config, "scout", request, &territory.display().to_string())?;
        let stage = StageSpec {
            phase: "scout",
            section: "scout.v1",
            user: request.to_string(),
            territory: Some(territory.clone()),
        };
        match run_stage(&mut w, config, Role::Scout, stage)? {
            Some(out) => {
                close_accepted(&mut w)?;
                println!("{}", out.text.trim_end());
                accepted_message(w.slip_id(), &out);
                Ok(true)
            }
            None => {
                failed_open_message(w.slip_id());
                Ok(false)
            }
        }
    })
}

/// Fly the stage after a granted boundary, context rebuilt purely from the
/// ledger: the printout. A fresh process, possibly a different airframe —
/// the ledger is the memory.
pub fn continue_flight(slip_id: &str) -> ExitCode {
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
        let mut w = LedgerWriter::resume(config.ledgers.as_ref(), SlipId(slip_id.to_string()))?;
        // The printout: this stage's declared context, assembled from the
        // ledger and nothing else. No process memory, no session.
        let printout = format!(
            "## Request\n\n{}\n\n## Plan (from the plan phase)\n\n{plan_body}\n\n\
             ## Task\n\nAnswer the request, following the plan.",
            slip.request
        );
        let stage = StageSpec {
            phase: "respond",
            section: "response.v1",
            user: printout,
            territory: None,
        };
        match run_stage(&mut w, config, Role::Responder, stage)? {
            Some(out) => {
                close_accepted(&mut w)?;
                println!("{}", out.text.trim_end());
                accepted_message(w.slip_id(), &out);
                Ok(true)
            }
            None => {
                failed_open_message(w.slip_id());
                Ok(false)
            }
        }
    })
}
