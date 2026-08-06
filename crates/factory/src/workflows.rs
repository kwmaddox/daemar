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
pub const APPLY_TO_LAND: &str = "apply->land";

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
            summary: diff_receipt(&base, &wt),
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
        summary: diff_receipt(&base, &wt),
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

/// The diff receipt: diff.v1's summary is the CANONICAL machine carrier
/// of the stamped artifact's coordinates — versioned JSON, decoded once at
/// the ledger boundary. The materialization note stays audit-only prose.
fn diff_receipt(base: &str, worktree: &Path) -> String {
    serde_json::json!({
        "v": 1,
        "base": base,
        "worktree": worktree.display().to_string(),
    })
    .to_string()
}

struct DiffReceipt {
    base: String,
    worktree: PathBuf,
}

fn decode_diff_receipt(summary: &str) -> Option<DiffReceipt> {
    let v: serde_json::Value = serde_json::from_str(summary).ok()?;
    // A versioned receipt that never checks its version would happily
    // mis-read a future v2 (or a corrupted slip) as v1 coordinates.
    if v.get("v")?.as_u64()? != 1 {
        return None;
    }
    let base = v.get("base")?.as_str()?.to_string();
    let worktree = PathBuf::from(v.get("worktree")?.as_str()?);
    if !is_full_sha(&base) || !worktree.is_absolute() {
        return None;
    }
    Some(DiffReceipt { base, worktree })
}

/// Exactly forty hex characters — the only shape a stamped commit takes.
fn is_full_sha(s: &str) -> bool {
    s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// The park receipt: apply.v1's summary carries everything the land leg
/// needs — no note re-parsing.
fn park_receipt(base: &str, worktree: &Path, commit: &str, branch: &str) -> String {
    serde_json::json!({
        "v": 1,
        "base": base,
        "worktree": worktree.display().to_string(),
        "commit": commit,
        "branch": branch,
    })
    .to_string()
}

struct ParkReceipt {
    base: String,
    worktree: PathBuf,
    commit: String,
    branch: String,
}

fn decode_park_receipt(summary: &str) -> Option<ParkReceipt> {
    let v: serde_json::Value = serde_json::from_str(summary).ok()?;
    if v.get("v")?.as_u64()? != 1 {
        return None;
    }
    // The land leg removes worktrees and judges reachability from these
    // coordinates — a receipt is held to the same shape it was written in.
    let receipt = ParkReceipt {
        base: v.get("base")?.as_str()?.to_string(),
        worktree: PathBuf::from(v.get("worktree")?.as_str()?),
        commit: v.get("commit")?.as_str()?.to_string(),
        branch: v.get("branch")?.as_str()?.to_string(),
    };
    if !is_full_sha(&receipt.base)
        || !is_full_sha(&receipt.commit)
        || !receipt.worktree.is_absolute()
        || receipt.branch.is_empty()
    {
        return None;
    }
    Some(receipt)
}

/// A code leg's report: no model flew, no tokens burned.
fn code_report(slip_id: &str, text: String, cocked_at: Option<&'static str>) -> FlightReport {
    FlightReport {
        slip_id: slip_id.to_string(),
        text,
        tokens: 0,
        cost: 0.0,
        turns: 0,
        cocked_at,
    }
}

fn short_id(slip_id: &str) -> String {
    slip_id.chars().take(8).collect()
}

/// Fly the stage after a granted boundary, context rebuilt purely from the
/// ledger. Routing follows the fold's HOLDING projection — the granted
/// boundary nothing has flown since: plan->respond seats the responder;
/// build->apply and apply->land fly the deterministic gate legs, where no
/// model exists at all.
pub fn continue_flight(config: &Config, slip_id: &str) -> Result<FlightReport, FlightError> {
    let slip = load_open_slip(slip_id).map_err(FlightError::Refused)?;
    if let Some(boundary) = &slip.cocked {
        return Err(FlightError::Refused(format!(
            "{slip_id} is awaiting clearance at {boundary} — daemar grant {slip_id} first"
        )));
    }
    match slip.holding.as_deref() {
        Some(b) if b == PLAN_TO_RESPOND => respond_leg(config, slip_id, &slip),
        Some(b) if b == BUILD_TO_APPLY => apply_leg(config, slip_id, &slip),
        Some(b) if b == APPLY_TO_LAND => land_leg(config, slip_id, &slip),
        Some(other) => Err(FlightError::Refused(format!(
            "{slip_id} holds an unknown boundary '{other}' — this daemar does not fly it"
        ))),
        None => Err(FlightError::Refused(format!(
            "{slip_id} has no granted boundary awaiting throttle — nothing to continue"
        ))),
    }
}

/// The responder leg: unchanged behavior, now one arm of the routing.
fn respond_leg(
    config: &Config,
    slip_id: &str,
    slip: &ledger::Slip,
) -> Result<FlightReport, FlightError> {
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

/// The APPLY leg: deterministic, code-lane, gate:apply. Verifies the
/// stamped artifact byte-for-byte, commits it (parent = base by
/// construction), then the ruled hybrid ladder: clean-and-unmoved
/// fast-forwards and closes; anything else parks the commit on a branch —
/// a pure ref operation — and RE-COCKS at apply->land, because nothing
/// waits invisibly.
fn apply_leg(
    config: &Config,
    slip_id: &str,
    slip: &ledger::Slip,
) -> Result<FlightReport, FlightError> {
    // Malformed continuation state refuses BEFORE any phase begins.
    let Some(receipt) = slip
        .sections
        .iter()
        .rev()
        .find(|s| s.section == "diff.v1")
        .and_then(|s| decode_diff_receipt(&s.summary))
    else {
        return Err(FlightError::Refused(format!(
            "{slip_id} has no decodable diff.v1 receipt — cannot apply"
        )));
    };
    let diff_body = slip
        .sections
        .iter()
        .rev()
        .find(|s| s.section == "diff.v1")
        .map(|s| s.body.clone())
        .unwrap_or_default();
    let territory = PathBuf::from(&slip.repo);
    let granter = slip
        .clearances
        .iter()
        .rev()
        .find(|c| c.boundary == BUILD_TO_APPLY)
        .and_then(|c| c.response.as_ref())
        .map(|r| r.by.clone())
        .unwrap_or_else(|| "unknown".to_string());

    let mut w = LedgerWriter::resume(config.ledgers.as_ref(), SlipId(slip_id.to_string()))?;
    w.append(&Kind::PhaseStarted {
        phase: "apply".to_string(),
        owner: "gate:apply".to_string(),
        lane: Lane::Code,
        engineer: config.engineer.clone(),
    })?;
    let witness = |w: &mut LedgerWriter, reason: String| -> Result<(), ledger::LedgerError> {
        w.append(&Kind::Note {
            text: reason.clone(),
        })?;
        w.append(&Kind::PhaseEnded {
            phase: "apply".to_string(),
            outcome: PhaseOutcome::Error,
        })?;
        eprintln!("daemar: {reason}");
        Ok(())
    };

    // Post-grant defects are witnessed: the controller cleared an artifact
    // the world no longer honors, and the strip must say so.
    if worktree::head(&territory).is_err() {
        witness(
            &mut w,
            format!(
                "territory {} is no longer a repository",
                territory.display()
            ),
        )?;
        return Err(witnessed_failure(&w));
    }
    if !receipt.worktree.is_dir() {
        witness(
            &mut w,
            format!(
                "retained worktree {} is missing",
                receipt.worktree.display()
            ),
        )?;
        return Err(witnessed_failure(&w));
    }
    match worktree::head(&receipt.worktree) {
        Ok(head) if head == receipt.base => {}
        _ => {
            witness(
                &mut w,
                "retained worktree is not at the stamped base".to_string(),
            )?;
            return Err(witnessed_failure(&w));
        }
    }
    match worktree::diff_against_base(&receipt.worktree, &receipt.base) {
        Ok(patch) if patch.as_bytes() == diff_body.as_bytes() => {}
        Ok(_) => {
            witness(
                &mut w,
                "stamped diff mismatch — the worktree no longer matches what was cleared"
                    .to_string(),
            )?;
            return Err(witnessed_failure(&w));
        }
        Err(error) => {
            witness(&mut w, format!("diff verification failed: {error}"))?;
            return Err(witnessed_failure(&w));
        }
    }

    let message = format!(
        "slip {slip_id}: {}\n\ngranted-by: {granter}\nvia: daemar gate:apply",
        crate::engine::summarize(&slip.request)
    );
    let commit = match worktree::commit_all(&receipt.worktree, &message) {
        Ok(commit) => commit,
        Err(error) => {
            witness(&mut w, format!("apply commit failed: {error}"))?;
            return Err(witnessed_failure(&w));
        }
    };
    w.append(&Kind::Note {
        text: format!("apply commit {commit} (parent {})", receipt.base),
    })?;

    let dirty = match worktree::is_dirty(&territory) {
        Ok(dirty) => dirty,
        Err(error) => {
            witness(&mut w, format!("territory status failed: {error}"))?;
            return Err(witnessed_failure(&w));
        }
    };
    let head_now = match worktree::head(&territory) {
        Ok(head) => head,
        Err(error) => {
            witness(&mut w, format!("territory HEAD failed: {error}"))?;
            return Err(witnessed_failure(&w));
        }
    };

    if !dirty && head_now == receipt.base {
        if let Err(error) = worktree::merge_ff_only(&territory, &commit) {
            witness(
                &mut w,
                format!("fast-forward failed despite checks: {error}"),
            )?;
            return Err(witnessed_failure(&w));
        }
        w.append(&Kind::Note {
            text: format!("landed {commit} onto {}", territory.display()),
        })?;
        if let Err(error) = worktree::remove(&territory, &receipt.worktree) {
            witness(&mut w, format!("worktree removal failed: {error}"))?;
            return Err(witnessed_failure(&w));
        }
        w.append(&Kind::Note {
            text: format!("worktree removed: {}", receipt.worktree.display()),
        })?;
        w.append(&Kind::PhaseEnded {
            phase: "apply".to_string(),
            outcome: PhaseOutcome::Success,
        })?;
        w.append(&Kind::SlipClosed {
            outcome: SlipOutcome::Accepted,
            reason: format!("landed {commit}"),
            by: "gate:apply".to_string(),
        })?;
        return Ok(code_report(slip_id, format!("landed {commit}"), None));
    }

    // The park: pure ref creation, zero working-tree interaction. Reuse an
    // existing branch only when it already points at this exact commit.
    let branch = format!("daemar/slip-{}", short_id(slip_id));
    match worktree::branch_target(&territory, &branch) {
        Ok(None) => {
            if let Err(error) = worktree::branch_create(&territory, &branch, &commit) {
                witness(&mut w, format!("branch park failed: {error}"))?;
                return Err(witnessed_failure(&w));
            }
        }
        Ok(Some(existing)) if existing == commit => {
            w.append(&Kind::Note {
                text: format!("branch {branch} already parked at {commit}"),
            })?;
        }
        Ok(Some(existing)) => {
            witness(
                &mut w,
                format!("branch {branch} exists at {existing}, not {commit} — collision"),
            )?;
            return Err(witnessed_failure(&w));
        }
        Err(error) => {
            witness(&mut w, format!("branch inspection failed: {error}"))?;
            return Err(witnessed_failure(&w));
        }
    }
    let why = if dirty {
        "territory dirty"
    } else {
        "territory HEAD moved"
    };
    w.append(&Kind::SectionWritten {
        section: "apply.v1".to_string(),
        by: "gate:apply".to_string(),
        summary: park_receipt(&receipt.base, &receipt.worktree, &commit, &branch),
        body: format!(
            "{why}: commit {commit} parked on {branch}. Land it with daemar grant + \
             continue when the territory is ready, or merge the branch yourself — \
             continue will recognize the landing either way."
        ),
    })?;
    w.append(&Kind::PhaseEnded {
        phase: "apply".to_string(),
        outcome: PhaseOutcome::Success,
    })?;
    w.append(&Kind::ClearanceRequested {
        boundary: APPLY_TO_LAND.to_string(),
        by: "gate:apply".to_string(),
    })?;
    Ok(code_report(
        slip_id,
        format!("{why}: parked {commit} on {branch}"),
        Some(APPLY_TO_LAND),
    ))
}

/// The LAND leg: the parked commit's journey home. The factory cares that
/// the change is IN — reachable from the territory's HEAD — not who landed
/// it: a manual merge is recognized and closed. Stale or dirty conditions
/// REFUSE without events; the slip stays granted-and-holding, retryable.
fn land_leg(
    config: &Config,
    slip_id: &str,
    slip: &ledger::Slip,
) -> Result<FlightReport, FlightError> {
    let Some(receipt) = slip
        .sections
        .iter()
        .rev()
        .find(|s| s.section == "apply.v1")
        .and_then(|s| decode_park_receipt(&s.summary))
    else {
        return Err(FlightError::Refused(format!(
            "{slip_id} has no decodable apply.v1 receipt — cannot land"
        )));
    };
    let territory = PathBuf::from(&slip.repo);

    let landed_already = worktree::is_ancestor(&territory, &receipt.commit)
        .map_err(|e| FlightError::Refused(format!("cannot inspect territory: {e}")))?;
    let clean_and_unmoved = if landed_already {
        false
    } else {
        let dirty = worktree::is_dirty(&territory)
            .map_err(|e| FlightError::Refused(format!("cannot inspect territory: {e}")))?;
        let head = worktree::head(&territory)
            .map_err(|e| FlightError::Refused(format!("cannot inspect territory: {e}")))?;
        !dirty && head == receipt.base
    };
    if !landed_already && !clean_and_unmoved {
        return Err(FlightError::Refused(format!(
            "{slip_id} cannot land yet — territory dirty or moved; merge {} manually and \
             continue again, or continue once the territory is clean at the stamped base",
            receipt.branch
        )));
    }

    let mut w = LedgerWriter::resume(config.ledgers.as_ref(), SlipId(slip_id.to_string()))?;
    w.append(&Kind::PhaseStarted {
        phase: "land".to_string(),
        owner: "gate:land".to_string(),
        lane: Lane::Code,
        engineer: config.engineer.clone(),
    })?;
    let witness = |w: &mut LedgerWriter, reason: String| -> Result<(), ledger::LedgerError> {
        w.append(&Kind::Note {
            text: reason.clone(),
        })?;
        w.append(&Kind::PhaseEnded {
            phase: "land".to_string(),
            outcome: PhaseOutcome::Error,
        })?;
        eprintln!("daemar: {reason}");
        Ok(())
    };

    if landed_already {
        w.append(&Kind::Note {
            text: format!(
                "commit {} already reachable from territory HEAD — landed by hand",
                receipt.commit
            ),
        })?;
    } else {
        if let Err(error) = worktree::merge_ff_only(&territory, &receipt.commit) {
            witness(
                &mut w,
                format!("fast-forward failed despite checks: {error}"),
            )?;
            return Err(witnessed_failure(&w));
        }
        w.append(&Kind::Note {
            text: format!("landed {} onto {}", receipt.commit, territory.display()),
        })?;
    }
    if receipt.worktree.is_dir() {
        if let Err(error) = worktree::remove(&territory, &receipt.worktree) {
            witness(&mut w, format!("worktree removal failed: {error}"))?;
            return Err(witnessed_failure(&w));
        }
        w.append(&Kind::Note {
            text: format!("worktree removed: {}", receipt.worktree.display()),
        })?;
    }
    w.append(&Kind::PhaseEnded {
        phase: "land".to_string(),
        outcome: PhaseOutcome::Success,
    })?;
    w.append(&Kind::SlipClosed {
        outcome: SlipOutcome::Accepted,
        reason: format!("landed {}", receipt.commit),
        by: "gate:land".to_string(),
    })?;
    Ok(code_report(
        slip_id,
        format!("landed {}", receipt.commit),
        None,
    ))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn a_receipt_with_the_wrong_version_is_refused() {
        let good = diff_receipt(SHA, Path::new("/tmp/wt"));
        assert!(decode_diff_receipt(&good).is_some());
        let v2 = good.replace("\"v\":1", "\"v\":2");
        assert!(decode_diff_receipt(&v2).is_none());
        let good = park_receipt(SHA, Path::new("/tmp/wt"), SHA, "daemar/slip-x");
        assert!(decode_park_receipt(&good).is_some());
        let v2 = good.replace("\"v\":1", "\"v\":2");
        assert!(decode_park_receipt(&v2).is_none());
    }

    #[test]
    fn a_park_receipt_is_held_to_the_shape_it_was_written_in() {
        let wt = Path::new("/tmp/wt");
        for bad in [
            park_receipt("not-a-sha", wt, SHA, "daemar/slip-x"),
            park_receipt(SHA, wt, "HEAD", "daemar/slip-x"),
            park_receipt(SHA, Path::new("relative/wt"), SHA, "daemar/slip-x"),
            park_receipt(SHA, wt, SHA, ""),
        ] {
            assert!(decode_park_receipt(&bad).is_none(), "must refuse: {bad}");
        }
    }
}
