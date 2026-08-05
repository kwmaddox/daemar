//! Workflows are Rust: each flight is a plain function sequencing stages.
//! Stages name Roles; the roster seats them. The workflow owns the shape —
//! boundaries, clearances, what closes and what cocks.
//!
//! Flights return reports; they never print. Presentation belongs to the
//! interface — the CLI formats footers, the MCP tower formats tool results —
//! and stdout belongs to whoever owns the transport.

use std::fmt;
use std::path::{Path, PathBuf};

use ledger::{Kind, Lane, LedgerWriter, PhaseOutcome, SlipId, SlipOutcome};

use crate::config::Config;
use crate::engine::{run_stage, PinnedTerritory, StageOut, StageSpec};
use crate::pens::load_open_slip;
use crate::roster::{Role, ToolAccess};
use crate::sandbox::{self, CageMode, SystemRunner};
use crate::worktree;

pub const PLAN_TO_RESPOND: &str = "plan->respond";
pub const BUILD_TO_APPLY: &str = "build->apply";

/// What a completed flight hands its interface.
pub struct FlightReport {
    pub slip_id: String,
    pub text: String,
    pub tokens: u64,
    pub cost: f64,
    pub turns: usize,
    /// Set when the flight parked at a boundary awaiting clearance instead
    /// of closing.
    pub cocked_at: Option<&'static str>,
}

/// How a flight fails. Every variant is a different conversation with the
/// controller.
#[derive(Debug)]
pub enum FlightError {
    /// The flight never happened — bad territory, unmet guard — and no new
    /// events were minted. The message says why.
    Refused(String),
    /// Witnessed failure: a phase failed, the reason is on the ledger, and
    /// the slip stays OPEN for the controller's disposition.
    Failed { slip_id: String },
    /// The ledger itself could not be written: nothing to salvage, fail loud.
    Ledger(ledger::LedgerError),
}

impl fmt::Display for FlightError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FlightError::Refused(message) => write!(f, "{message}"),
            FlightError::Failed { slip_id } => {
                write!(f, "slip {slip_id} failed; reason on the ledger")
            }
            FlightError::Ledger(error) => write!(f, "ledger failure: {error}"),
        }
    }
}

impl From<ledger::LedgerError> for FlightError {
    fn from(error: ledger::LedgerError) -> Self {
        FlightError::Ledger(error)
    }
}

/// Validate a territory before any slip is minted: a bad path is a refusal,
/// not a flight.
fn canonical_territory(repo: &str) -> Result<PathBuf, FlightError> {
    let path = Path::new(repo);
    let canonical = path.canonicalize().map_err(|e| {
        FlightError::Refused(format!(
            "territory {} cannot be resolved: {e}",
            path.display()
        ))
    })?;
    if !canonical.is_dir() {
        return Err(FlightError::Refused(format!(
            "territory {} is not a directory",
            canonical.display()
        )));
    }
    Ok(canonical)
}

/// Pin a territory before minting: canonical path AND a resolvable HEAD.
/// A territory that cannot pin — not a repo, unborn HEAD — refuses the
/// flight while it still costs nothing. When the cage is on, docker and
/// the image preflight here too, for the same reason.
fn pinned_territory(
    config: &Config,
    repo: &str,
    access: ToolAccess,
) -> Result<PinnedTerritory, FlightError> {
    let source = canonical_territory(repo)?;
    let base = worktree::head(&source).map_err(|e| {
        FlightError::Refused(format!("territory {} cannot pin: {e}", source.display()))
    })?;
    // Write access preflights the cage UNCONDITIONALLY: write tools were
    // born caged and a builder never flies without one. Read-only seats
    // keep the DAEMAR_CAGE dial.
    if access == ToolAccess::ReadWrite || config.cage == CageMode::On {
        sandbox::preflight(&SystemRunner, &config.sandbox)
            .map_err(|e| FlightError::Refused(e.to_string()))?;
    }
    Ok(PinnedTerritory { source, base })
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

fn accepted(w: &LedgerWriter, out: StageOut) -> FlightReport {
    FlightReport {
        slip_id: w.slip_id().to_string(),
        text: out.text,
        tokens: out.tokens,
        cost: out.cost,
        turns: out.turns,
        cocked_at: None,
    }
}

fn witnessed_failure(w: &LedgerWriter) -> FlightError {
    FlightError::Failed {
        slip_id: w.slip_id().to_string(),
    }
}

/// The default territory: wherever the engineer is standing.
fn cwd_territory() -> String {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.canonicalize().ok())
        .map(|p| p.display().to_string())
        .unwrap_or_default()
}

/// The one-stage prompt workflow: respond, close accepted.
pub fn prompt_flight(config: &Config, request: &str) -> Result<FlightReport, FlightError> {
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
            Ok(accepted(&w, out))
        }
        None => Err(witnessed_failure(&w)),
    }
}

/// Stage one of the planned workflow: a GROUNDED plan — the planner reads
/// the territory with the scout's tools before planning — then the slip
/// cocks at the boundary and the flight ENDS. The strip waits on the board;
/// the ledger carries everything the respond stage will need.
pub fn plan_flight(
    config: &Config,
    request: &str,
    repo: &str,
) -> Result<FlightReport, FlightError> {
    let territory = pinned_territory(config, repo, ToolAccess::ReadOnly)?;
    let mut w = open_flight(
        config,
        "plan",
        request,
        &territory.source.display().to_string(),
    )?;
    let stage = StageSpec {
        phase: "plan",
        section: "plan.v1",
        user: request.to_string(),
        territory: Some(territory),
    };
    match run_stage(&mut w, config, Role::Planner, stage)? {
        Some(out) => {
            w.append(&Kind::ClearanceRequested {
                boundary: PLAN_TO_RESPOND.to_string(),
                by: "planner".to_string(),
            })?;
            let mut report = accepted(&w, out);
            report.cocked_at = Some(PLAN_TO_RESPOND);
            Ok(report)
        }
        None => Err(witnessed_failure(&w)),
    }
}

/// Read-only reconnaissance over a territory. The tower stays home; only
/// the tools visit the repo.
pub fn scout_flight(
    config: &Config,
    request: &str,
    repo: &str,
) -> Result<FlightReport, FlightError> {
    let territory = pinned_territory(config, repo, ToolAccess::ReadOnly)?;
    let mut w = open_flight(
        config,
        "scout",
        request,
        &territory.source.display().to_string(),
    )?;
    let stage = StageSpec {
        phase: "scout",
        section: "scout.v1",
        user: request.to_string(),
        territory: Some(territory),
    };
    match run_stage(&mut w, config, Role::Scout, stage)? {
        Some(out) => {
            close_accepted(&mut w)?;
            Ok(accepted(&w, out))
        }
        None => Err(witnessed_failure(&w)),
    }
}

/// The build workflow, phase 2 of the write era: ONE builder stage over a
/// pinned worktree, then a CODE-OWNED diff. The diff is its own code-lane
/// phase signed gate:diff — the model reports what it did; git testifies
/// to what actually changed. A nonempty diff cocks at build->apply and the
/// flight EXITS (apply is phase 3); an empty diff closes accepted as an
/// explicit no-op — nothing cocks for nothing.
pub fn build_flight(
    config: &Config,
    request: &str,
    repo: &str,
) -> Result<FlightReport, FlightError> {
    let territory = pinned_territory(config, repo, ToolAccess::ReadWrite)?;
    let base = territory.base.clone();
    let mut w = open_flight(
        config,
        "build",
        request,
        &territory.source.display().to_string(),
    )?;
    let stage = StageSpec {
        phase: "build",
        section: "build.v1",
        user: request.to_string(),
        territory: Some(territory),
    };
    let Some(out) = run_stage(&mut w, config, Role::Builder, stage)? else {
        return Err(witnessed_failure(&w));
    };
    let Some(wt) = out.worktree.clone() else {
        // A tooled stage without a worktree is a bug, not a flight.
        w.append(&Kind::Note {
            text: "build stage returned no worktree — engine bug".to_string(),
        })?;
        return Err(witnessed_failure(&w));
    };

    // The diff gate: a code-lane phase. Its failure is witnessed like any
    // other phase's; its verdict is the patch itself.
    w.append(&Kind::PhaseStarted {
        phase: "diff".to_string(),
        owner: "gate:diff".to_string(),
        lane: Lane::Code,
        engineer: config.engineer.clone(),
    })?;
    let patch = match worktree::diff_against_base(&wt, &base) {
        Ok(patch) => patch,
        Err(error) => {
            w.append(&Kind::Note {
                text: format!("diff gate failed: {error}"),
            })?;
            w.append(&Kind::PhaseEnded {
                phase: "diff".to_string(),
                outcome: PhaseOutcome::Error,
            })?;
            eprintln!("daemar: diff gate failed: {error}");
            return Err(witnessed_failure(&w));
        }
    };
    if patch.trim().is_empty() {
        w.append(&Kind::SectionWritten {
            section: "diff.v1".to_string(),
            by: "gate:diff".to_string(),
            summary: format!("no changes: base={base} worktree={}", wt.display()),
            body: String::new(),
        })?;
        w.append(&Kind::PhaseEnded {
            phase: "diff".to_string(),
            outcome: PhaseOutcome::Success,
        })?;
        close_accepted(&mut w)?;
        return Ok(accepted(&w, out));
    }
    w.append(&Kind::SectionWritten {
        section: "diff.v1".to_string(),
        by: "gate:diff".to_string(),
        summary: format!("base={base} worktree={}", wt.display()),
        body: patch,
    })?;
    w.append(&Kind::PhaseEnded {
        phase: "diff".to_string(),
        outcome: PhaseOutcome::Success,
    })?;
    w.append(&Kind::ClearanceRequested {
        boundary: BUILD_TO_APPLY.to_string(),
        by: "gate:diff".to_string(),
    })?;
    let mut report = accepted(&w, out);
    report.cocked_at = Some(BUILD_TO_APPLY);
    Ok(report)
}

/// Fly the stage after a granted boundary, context rebuilt purely from the
/// ledger: the printout. A fresh process, possibly a different airframe —
/// the ledger is the memory.
pub fn continue_flight(config: &Config, slip_id: &str) -> Result<FlightReport, FlightError> {
    let slip = load_open_slip(slip_id).map_err(FlightError::Refused)?;
    if let Some(boundary) = &slip.cocked {
        return Err(FlightError::Refused(format!(
            "{slip_id} is awaiting clearance at {boundary} — daemar grant {slip_id} first"
        )));
    }
    // Phase 2 ends at the cocked diff: a granted build->apply has no
    // machinery yet, and saying so beats a misleading plan->respond error.
    let apply_granted = slip.clearances.iter().any(|c| {
        c.boundary == BUILD_TO_APPLY
            && c.response
                .as_ref()
                .is_some_and(|r| r.verdict == ledger::ClearanceVerdict::Granted)
    });
    if apply_granted {
        return Err(FlightError::Refused(format!(
            "{slip_id} has {BUILD_TO_APPLY} granted — APPLY is phase 3 and is not built yet"
        )));
    }
    let granted = slip.clearances.iter().any(|c| {
        c.boundary == PLAN_TO_RESPOND
            && c.response
                .as_ref()
                .is_some_and(|r| r.verdict == ledger::ClearanceVerdict::Granted)
    });
    if !granted {
        return Err(FlightError::Refused(format!(
            "{slip_id} has no granted {PLAN_TO_RESPOND} clearance — nothing to continue"
        )));
    }
    if slip.phases.iter().any(|p| p.phase == "respond") {
        return Err(FlightError::Refused(format!(
            "{slip_id} already flew its respond phase"
        )));
    }
    let Some(plan_body) = slip
        .sections
        .iter()
        .rev()
        .find(|s| s.section == "plan.v1")
        .map(|s| s.body.clone())
    else {
        return Err(FlightError::Refused(format!(
            "{slip_id} has no plan.v1 section to continue from"
        )));
    };

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
            Ok(accepted(&w, out))
        }
        None => Err(witnessed_failure(&w)),
    }
}
