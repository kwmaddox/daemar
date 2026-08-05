//! The ledger and the slip fold.
//!
//! The ledger is the append-only truth: one JSONL file per slip, every event
//! carrying a versioned `kind`. The slip is a fold over those events — it is
//! never written, only derived, so it cannot lie. Liveness and attention
//! (in flight, cocked) are derived states; no event ever asserts them.
//!
//! Type discipline (CONTEXT.md hard rule): strings cross exactly one boundary
//! — the serde edge — and are parsed once, there. Closed sets are enums, open
//! vocabularies are newtypes, unknown event kinds are an explicit variant,
//! and nothing is dropped silently: bad lines are counted and surfaced.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SCHEMA: &str = "ledger.v1";

// ── Identifiers (open vocabulary → newtype) ──────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SlipId(pub String);

impl fmt::Display for SlipId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ── Closed sets (→ enums; an exhaustive match is the change-impact analysis) ─

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lane {
    Engineer,
    Agent,
    Code,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseOutcome {
    Success,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlipOutcome {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearanceVerdict {
    Granted,
    Refused,
}

impl fmt::Display for Lane {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Lane::Engineer => "engineer",
            Lane::Agent => "agent",
            Lane::Code => "code",
        })
    }
}

impl fmt::Display for PhaseOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PhaseOutcome::Success => "success",
            PhaseOutcome::Error => "error",
        })
    }
}

impl fmt::Display for ClearanceVerdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ClearanceVerdict::Granted => "granted",
            ClearanceVerdict::Refused => "refused",
        })
    }
}

// ── Events ───────────────────────────────────────────────────────────────────

/// The known event kinds. A payload that does not parse — unknown kind OR a
/// known kind whose closed-set field carries an unknown value — becomes
/// `EventKind::Unknown`, explicitly, at the boundary. Old boards survive new
/// factories, and the tolerance is visible in the type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload")]
pub enum Kind {
    #[serde(rename = "slip_opened.v1")]
    SlipOpened {
        request: String,
        workflow: String,
        engineer: String,
        /// The territory: the repository this flight operates on. The tower
        /// stays home; the slip remembers where it was going, so a resumed
        /// flight lands in the right repo without being told.
        #[serde(default)]
        repo: String,
    },
    #[serde(rename = "phase_started.v1")]
    PhaseStarted {
        phase: String,
        owner: String,
        lane: Lane,
        /// Who flew this stage. The slip's engineer is permanently the
        /// opener; this field keeps the flyer when a later client continues
        /// the flight. Defaulted: pre-attribution ledgers fold cleanly.
        #[serde(default)]
        engineer: String,
    },
    #[serde(rename = "phase_ended.v1")]
    PhaseEnded {
        phase: String,
        outcome: PhaseOutcome,
    },
    #[serde(rename = "section_written.v1")]
    SectionWritten {
        section: String,
        by: String,
        #[serde(default)]
        summary: String,
        /// Full section content. The summary is the table line; the body is
        /// the disclosure. Empty is legal — some sections are their summary.
        #[serde(default)]
        body: String,
    },
    #[serde(rename = "clearance_requested.v1")]
    ClearanceRequested { boundary: String, by: String },
    #[serde(rename = "clearance_granted.v1")]
    ClearanceGranted { boundary: String, by: String },
    #[serde(rename = "clearance_refused.v1")]
    ClearanceRefused {
        boundary: String,
        by: String,
        #[serde(default)]
        reason: String,
    },
    #[serde(rename = "query_made.v1")]
    QueryMade { phase: String, query: String },
    /// Intent half of a model call — anything spanning real time gets a pair.
    /// Carries the EXACT prompts sent: the epistemic record of what this
    /// phase knew, written before the call so even a crashed flight keeps it.
    #[serde(rename = "model_requested.v1")]
    ModelRequested {
        phase: String,
        model: String,
        #[serde(default)]
        system: String,
        #[serde(default)]
        user: String,
    },
    #[serde(rename = "model_call.v1")]
    ModelCall {
        phase: String,
        model: String,
        /// Total tokens; the split rides beside it because pricing is
        /// asymmetric and a receipt needs its line items.
        tokens: u64,
        #[serde(default)]
        prompt_tokens: u64,
        /// Cache-hit subset of prompt_tokens, billed at the cached rate.
        #[serde(default)]
        cached_tokens: u64,
        #[serde(default)]
        completion_tokens: u64,
        /// USD as computed AT FLIGHT TIME from the registry — a frozen
        /// receipt, never recomputed from a later table.
        cost: f64,
    },
    /// One tool invocation, outcome included. Tools are tier-3 queries
    /// against the world: every read is logged (an unlogged read is a hole
    /// in the epistemic record), with a content-hash pointer instead of the
    /// bytes — what the agent saw, recoverable, without bloating the ledger.
    #[serde(rename = "tool_call.v1")]
    ToolCall {
        phase: String,
        tool: String,
        #[serde(default)]
        args: Value,
        ok: bool,
        #[serde(default)]
        summary: String,
        /// Content hash for reads; empty for tools with nothing to pin.
        #[serde(default)]
        hash: String,
    },
    #[serde(rename = "note.v1")]
    Note { text: String },
    #[serde(rename = "slip_closed.v1")]
    SlipClosed {
        outcome: SlipOutcome,
        #[serde(default)]
        reason: String,
        /// Who rendered the verdict. Doctrine: a process may close its own
        /// slip accepted; a failed flight is closed only by the controller
        /// (disposition) or a gate that carries a verdict.
        #[serde(default)]
        by: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum EventKind {
    Known(Kind),
    Unknown { kind: String, payload: Value },
}

impl EventKind {
    /// The wire form (kind string, payload) — for raw display and rewriting.
    pub fn wire(&self) -> (String, Value) {
        match self {
            EventKind::Known(kind) => {
                let v = serde_json::to_value(kind).expect("Kind serializes");
                (
                    v.get("kind")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    v.get("payload").cloned().unwrap_or(Value::Null),
                )
            }
            EventKind::Unknown { kind, payload } => (kind.clone(), payload.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub schema: String,
    pub slip_id: SlipId,
    pub seq: u64,
    pub ts: String,
    pub kind: EventKind,
}

/// The serde edge. Strings live here and stop here.
#[derive(Deserialize)]
struct WireEvent {
    #[serde(default)]
    schema: String,
    slip_id: SlipId,
    seq: u64,
    ts: String,
    kind: String,
    #[serde(default)]
    payload: Value,
}

impl Event {
    /// Parse one ledger line. This is the ONLY place a kind string is read.
    pub fn from_line(line: &str) -> Result<Self, serde_json::Error> {
        let wire: WireEvent = serde_json::from_str(line)?;
        let attempt = serde_json::json!({ "kind": wire.kind, "payload": wire.payload });
        let kind = match serde_json::from_value::<Kind>(attempt) {
            Ok(known) => EventKind::Known(known),
            Err(_) => EventKind::Unknown {
                kind: wire.kind,
                payload: wire.payload,
            },
        };
        Ok(Event {
            schema: wire.schema,
            slip_id: wire.slip_id,
            seq: wire.seq,
            ts: wire.ts,
            kind,
        })
    }
}

// ── Errors (every fallible seam owns its failure modes) ──────────────────────

#[derive(Debug)]
pub enum LedgerError {
    Io { path: PathBuf, source: io::Error },
}

impl fmt::Display for LedgerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LedgerError::Io { path, source } => {
                write!(f, "ledger io at {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for LedgerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LedgerError::Io { source, .. } => Some(source),
        }
    }
}

// ── The slip: a fold over one ledger ─────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    InFlight,
    Accepted,
    Rejected,
}

#[derive(Debug, Clone)]
pub struct PhaseRow {
    pub phase: String,
    pub owner: String,
    pub lane: Lane,
    /// The engineer that flew this stage; empty on pre-attribution ledgers.
    pub engineer: String,
    pub started: String,
    pub ended: Option<String>,
    pub outcome: Option<PhaseOutcome>,
}

/// One entry in the tool trail — what the agent did to the world, in order.
#[derive(Debug, Clone)]
pub struct ToolRow {
    pub phase: String,
    pub tool: String,
    pub ok: bool,
    pub summary: String,
    pub ts: String,
}

#[derive(Debug, Clone)]
pub struct SectionRow {
    pub section: String,
    pub by: String,
    pub summary: String,
    pub body: String,
    pub ts: String,
}

/// One model request as sent: the phase's realized context.
#[derive(Debug, Clone)]
pub struct ModelRequestRow {
    pub phase: String,
    pub model: String,
    pub system: String,
    pub user: String,
    pub ts: String,
}

#[derive(Debug, Clone)]
pub struct ClearanceResponse {
    pub verdict: ClearanceVerdict,
    pub by: String,
    pub ts: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct ClearanceRow {
    pub boundary: String,
    pub requested_by: String,
    pub requested_ts: String,
    pub response: Option<ClearanceResponse>,
}

/// The face plus everything the detail view discloses. Always derived.
#[derive(Debug, Clone)]
pub struct Slip {
    pub id: SlipId,
    pub request: String,
    pub workflow: String,
    pub engineer: String,
    /// The territory this flight operates on; empty on pre-territory ledgers.
    pub repo: String,
    pub status: Status,
    /// The boundary awaiting the controller, when the strip is cocked.
    pub cocked: Option<String>,
    /// The phase whose error is unwitnessed: last phase ended in error and
    /// nothing has happened since, on a slip still open. Awaiting disposition.
    pub failed: Option<String>,
    /// Cleared and holding: the latest clearance was granted and no phase has
    /// flown since — the flight is waiting for `continue`. Awaiting throttle.
    pub holding: Option<String>,
    pub current_phase: Option<String>,
    pub close_reason: Option<String>,
    pub phases: Vec<PhaseRow>,
    pub sections: Vec<SectionRow>,
    pub model_requests: Vec<ModelRequestRow>,
    pub tool_trail: Vec<ToolRow>,
    pub clearances: Vec<ClearanceRow>,
    pub tokens: u64,
    pub cost: f64,
    pub model_calls: u32,
    pub queries: u32,
    /// The airframe currently flying: the model most recently requested or
    /// completed.
    pub last_model: Option<String>,
    pub opened_ts: String,
    pub last_ts: String,
    pub event_count: usize,
}

/// Fold a ledger into its slip. `None` when no `slip_opened.v1` exists —
/// a ledger that never opened has no face to show.
pub fn fold(events: &[Event]) -> Option<Slip> {
    let mut slip: Option<Slip> = None;
    // Cleared-and-holding is tracked in EVENT ORDER, not by timestamp:
    // timestamps are second-granular display, and a grant and a phase start
    // can share a second. Sequence is the truth.
    let mut throttle_awaited: Option<String> = None;

    for event in events {
        let ts = event.ts.clone();
        if let Some(s) = slip.as_mut() {
            s.last_ts = ts.clone();
            s.event_count += 1;
        }
        let EventKind::Known(kind) = &event.kind else {
            continue;
        };

        match kind.clone() {
            Kind::SlipOpened {
                request,
                workflow,
                engineer,
                repo,
            } => {
                slip = Some(Slip {
                    id: event.slip_id.clone(),
                    request,
                    workflow,
                    engineer,
                    repo,
                    status: Status::InFlight,
                    cocked: None,
                    failed: None,
                    holding: None,
                    current_phase: None,
                    close_reason: None,
                    phases: Vec::new(),
                    sections: Vec::new(),
                    model_requests: Vec::new(),
                    tool_trail: Vec::new(),
                    clearances: Vec::new(),
                    tokens: 0,
                    cost: 0.0,
                    model_calls: 0,
                    queries: 0,
                    last_model: None,
                    opened_ts: ts.clone(),
                    last_ts: ts,
                    event_count: 1,
                });
            }
            other => {
                let Some(s) = slip.as_mut() else { continue };
                match other {
                    Kind::SlipOpened { .. } => unreachable!("handled above"),
                    Kind::PhaseStarted {
                        phase,
                        owner,
                        lane,
                        engineer,
                    } => {
                        throttle_awaited = None; // the throttle was pushed
                        s.current_phase = Some(phase.clone());
                        s.phases.push(PhaseRow {
                            phase,
                            owner,
                            lane,
                            engineer,
                            started: ts,
                            ended: None,
                            outcome: None,
                        });
                    }
                    Kind::PhaseEnded { phase, outcome } => {
                        if let Some(row) = s
                            .phases
                            .iter_mut()
                            .rev()
                            .find(|r| r.phase == phase && r.ended.is_none())
                        {
                            row.ended = Some(ts);
                            row.outcome = Some(outcome);
                        }
                        if s.current_phase.as_deref() == Some(phase.as_str()) {
                            s.current_phase = None;
                        }
                    }
                    Kind::SectionWritten {
                        section,
                        by,
                        summary,
                        body,
                    } => {
                        s.sections.push(SectionRow {
                            section,
                            by,
                            summary,
                            body,
                            ts,
                        });
                    }
                    Kind::ClearanceRequested { boundary, by } => {
                        s.clearances.push(ClearanceRow {
                            boundary,
                            requested_by: by,
                            requested_ts: ts,
                            response: None,
                        });
                    }
                    Kind::ClearanceGranted { boundary, by } => {
                        if answer_clearance(
                            s,
                            &boundary,
                            ClearanceVerdict::Granted,
                            by,
                            ts,
                            String::new(),
                        ) {
                            throttle_awaited = Some(boundary);
                        }
                    }
                    Kind::ClearanceRefused {
                        boundary,
                        by,
                        reason,
                    } => {
                        answer_clearance(s, &boundary, ClearanceVerdict::Refused, by, ts, reason);
                    }
                    Kind::QueryMade { .. } => s.queries += 1,
                    Kind::ModelRequested {
                        phase,
                        model,
                        system,
                        user,
                    } => {
                        s.last_model = Some(model.clone());
                        s.model_requests.push(ModelRequestRow {
                            phase,
                            model,
                            system,
                            user,
                            ts,
                        });
                    }
                    Kind::ModelCall {
                        model,
                        tokens,
                        cost,
                        ..
                    } => {
                        s.model_calls += 1;
                        s.tokens += tokens;
                        s.cost += cost;
                        s.last_model = Some(model);
                    }
                    Kind::ToolCall {
                        phase,
                        tool,
                        ok,
                        summary,
                        ..
                    } => {
                        s.tool_trail.push(ToolRow {
                            phase,
                            tool,
                            ok,
                            summary,
                            ts,
                        });
                    }
                    Kind::Note { .. } => {}
                    Kind::SlipClosed {
                        outcome, reason, ..
                    } => {
                        s.status = match outcome {
                            SlipOutcome::Accepted => Status::Accepted,
                            SlipOutcome::Rejected => Status::Rejected,
                        };
                        if !reason.is_empty() {
                            s.close_reason = Some(reason);
                        }
                    }
                }
            }
        }
    }

    // Attention states are derived, never asserted. Cocked: an unanswered
    // clearance on an open slip. Failed: an open slip whose most recent phase
    // ended in error — the flight cannot close itself rejected (failure must
    // be witnessed), so it waits here for the controller's disposition.
    if let Some(s) = slip.as_mut() {
        if s.status == Status::InFlight {
            s.cocked = s
                .clearances
                .iter()
                .rev()
                .find(|c| c.response.is_none())
                .map(|c| c.boundary.clone());
            s.failed = s
                .phases
                .last()
                .filter(|p| p.outcome == Some(PhaseOutcome::Error))
                .map(|p| p.phase.clone());
            // Holding: cleared for the next leg, nothing flown since — the
            // flight waits for `continue`.
            s.holding = if s.cocked.is_none() && s.failed.is_none() {
                throttle_awaited
            } else {
                None
            };
        } else {
            s.cocked = None;
            s.failed = None;
            s.holding = None;
        }
    }
    slip
}

/// Answer the newest open request at this boundary. Returns whether one
/// existed — an answer to nothing changes nothing.
fn answer_clearance(
    slip: &mut Slip,
    boundary: &str,
    verdict: ClearanceVerdict,
    by: String,
    ts: String,
    reason: String,
) -> bool {
    if let Some(row) = slip
        .clearances
        .iter_mut()
        .rev()
        .find(|c| c.boundary == boundary && c.response.is_none())
    {
        row.response = Some(ClearanceResponse {
            verdict,
            by,
            ts,
            reason,
        });
        true
    } else {
        false
    }
}

// ── Time ─────────────────────────────────────────────────────────────────────

/// Parse a ledger timestamp ("YYYY-MM-DDTHH:MM:SS…", UTC assumed) to epoch
/// seconds. Anything after the seconds — "Z", ".123Z" — is ignored. Ledger
/// writers emit RFC3339 UTC; this reads exactly that and nothing cleverer.
pub fn parse_ts(ts: &str) -> Option<u64> {
    let num = |r: std::ops::Range<usize>| ts.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, s) = (num(11..13)?, num(14..16)?, num(17..19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    // Howard Hinnant's days_from_civil.
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = y2 - era * 400;
    let doy = (153 * (if mo > 2 { mo - 3 } else { mo + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    u64::try_from(days * 86400 + h * 3600 + mi * 60 + s).ok()
}

/// The inverse of `parse_ts`: epoch seconds to "YYYY-MM-DDTHH:MM:SSZ".
/// Howard Hinnant's civil_from_days.
pub fn format_ts(epoch: u64) -> String {
    let days = (epoch / 86400) as i64;
    let secs = epoch % 86400;
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60
    )
}

// ── Writing ──────────────────────────────────────────────────────────────────

/// The append half. One writer per slip, one line per event, flushed as it
/// lands so readers (and the board's doorbell) see events while the flight
/// is still in the air. Sync and std-only: the core stays runtime-free.
pub struct LedgerWriter {
    file: fs::File,
    path: PathBuf,
    slip_id: SlipId,
    seq: u64,
}

impl LedgerWriter {
    /// Create `<dir>/<slip_id>.jsonl`. Fails loud if it already exists —
    /// a slip's ledger is born exactly once, appended forever, never reborn.
    pub fn create(dir: &Path, slip_id: SlipId) -> Result<Self, LedgerError> {
        fs::create_dir_all(dir).map_err(|source| LedgerError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = dir.join(format!("{slip_id}.jsonl"));
        let file = fs::OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(&path)
            .map_err(|source| LedgerError::Io {
                path: path.clone(),
                source,
            })?;
        Ok(LedgerWriter {
            file,
            path,
            slip_id,
            seq: 0,
        })
    }

    /// Append one event, stamped now, flushed. Returns the sequence number.
    pub fn append(&mut self, kind: &Kind) -> Result<u64, LedgerError> {
        use io::Write;
        self.seq += 1;
        let now = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let (kind_str, payload) = EventKind::Known(kind.clone()).wire();
        let line = serde_json::json!({
            "schema": SCHEMA,
            "slip_id": self.slip_id,
            "seq": self.seq,
            "ts": format_ts(now),
            "kind": kind_str,
            "payload": payload,
        });
        let io_err = |source| LedgerError::Io {
            path: self.path.clone(),
            source,
        };
        writeln!(self.file, "{line}").map_err(io_err)?;
        self.file.flush().map_err(io_err)?;
        Ok(self.seq)
    }

    pub fn slip_id(&self) -> &SlipId {
        &self.slip_id
    }

    /// Reopen an existing ledger for append — disposition, later phases.
    /// Fails loud if the ledger does not exist: resume never creates.
    pub fn resume(dir: &Path, slip_id: SlipId) -> Result<Self, LedgerError> {
        let path = dir.join(format!("{slip_id}.jsonl"));
        let existing = load_ledger(&path)?;
        let seq = existing.events.iter().map(|e| e.seq).max().unwrap_or(0);
        let file = fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .map_err(|source| LedgerError::Io {
                path: path.clone(),
                source,
            })?;
        Ok(LedgerWriter {
            file,
            path,
            slip_id,
            seq,
        })
    }
}

// ── Loading ──────────────────────────────────────────────────────────────────

/// One parsed ledger file. Bad lines are counted, never silently dropped:
/// the board must keep rendering a fleet when one writer misbehaves, but it
/// must also say so.
#[derive(Debug, Default)]
pub struct LedgerFile {
    pub events: Vec<Event>,
    /// 1-based line numbers that failed to parse as events.
    pub bad_lines: Vec<u64>,
}

pub fn load_ledger(path: &Path) -> Result<LedgerFile, LedgerError> {
    let text = fs::read_to_string(path).map_err(|source| LedgerError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut file = LedgerFile::default();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match Event::from_line(line) {
            Ok(event) => file.events.push(event),
            Err(_) => file.bad_lines.push(index as u64 + 1),
        }
    }
    file.events.sort_by_key(|e| e.seq);
    Ok(file)
}

#[derive(Debug)]
pub struct FoldedSlip {
    pub slip: Slip,
    pub events: Vec<Event>,
    pub bad_lines: Vec<u64>,
}

/// Everything a reader needs, with nothing hidden: the folded slips, plus
/// every file that could not be read at all.
#[derive(Debug, Default)]
pub struct LoadReport {
    pub slips: Vec<FoldedSlip>,
    pub skipped: Vec<(PathBuf, LedgerError)>,
}

impl LoadReport {
    pub fn bad_line_count(&self) -> usize {
        self.slips.iter().map(|s| s.bad_lines.len()).sum()
    }
}

/// Load every `*.jsonl` ledger in a directory, folded, newest activity first.
pub fn load_dir(dir: &Path) -> Result<LoadReport, LedgerError> {
    let entries = fs::read_dir(dir).map_err(|source| LedgerError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    let mut report = LoadReport::default();
    for entry in entries {
        let entry = entry.map_err(|source| LedgerError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        match load_ledger(&path) {
            Ok(file) => {
                if let Some(slip) = fold(&file.events) {
                    report.slips.push(FoldedSlip {
                        slip,
                        events: file.events,
                        bad_lines: file.bad_lines,
                    });
                }
            }
            Err(error) => report.skipped.push((path, error)),
        }
    }
    report
        .slips
        .sort_by(|a, b| b.slip.last_ts.cmp(&a.slip.last_ts));
    Ok(report)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn line(seq: u64, ts: &str, kind: &str, payload: &str) -> String {
        format!(
            r#"{{"schema":"ledger.v1","slip_id":"slip-1","seq":{seq},"ts":"{ts}","kind":"{kind}","payload":{payload}}}"#
        )
    }

    fn parse(lines: &[String]) -> Vec<Event> {
        lines
            .iter()
            .map(|l| Event::from_line(l).expect("parses"))
            .collect()
    }

    #[test]
    fn accepted_flight_folds_clean() {
        let events = parse(&[
            line(
                1,
                "t1",
                "slip_opened.v1",
                r#"{"request":"add /health","workflow":"simple","engineer":"kendall"}"#,
            ),
            line(
                2,
                "t2",
                "phase_started.v1",
                r#"{"phase":"plan","owner":"planner","lane":"agent"}"#,
            ),
            line(
                3,
                "t3",
                "model_requested.v1",
                r#"{"phase":"plan","model":"m","system":"be a planner","user":"plan it"}"#,
            ),
            line(
                4,
                "t4",
                "model_call.v1",
                r#"{"phase":"plan","model":"m","tokens":1000,"cost":0.02}"#,
            ),
            line(
                5,
                "t5",
                "section_written.v1",
                r#"{"section":"plan.v1","by":"planner"}"#,
            ),
            line(
                6,
                "t6",
                "phase_ended.v1",
                r#"{"phase":"plan","outcome":"success"}"#,
            ),
            line(
                7,
                "t7",
                "clearance_requested.v1",
                r#"{"boundary":"plan->build","by":"planner"}"#,
            ),
            line(
                8,
                "t8",
                "clearance_granted.v1",
                r#"{"boundary":"plan->build","by":"kendall"}"#,
            ),
            line(9, "t9", "slip_closed.v1", r#"{"outcome":"accepted"}"#),
        ]);
        let slip = fold(&events).expect("slip opened");
        assert_eq!(slip.status, Status::Accepted);
        assert_eq!(slip.cocked, None);
        assert_eq!(slip.current_phase, None);
        assert_eq!(slip.phases.len(), 1);
        assert_eq!(slip.phases[0].outcome, Some(PhaseOutcome::Success));
        assert_eq!(slip.phases[0].lane, Lane::Agent);
        assert_eq!(
            slip.phases[0].engineer, "",
            "pre-attribution wire folds to an empty flyer, not a parse failure"
        );
        assert_eq!(slip.sections.len(), 1);
        assert_eq!(slip.tokens, 1000);
        assert_eq!(slip.last_model.as_deref(), Some("m"));
        assert_eq!(slip.model_requests.len(), 1);
        assert_eq!(slip.model_requests[0].system, "be a planner");
        assert_eq!(slip.model_requests[0].user, "plan it");
        assert_eq!(slip.event_count, 9);
        let response = slip.clearances[0].response.as_ref().expect("answered");
        assert_eq!(response.verdict, ClearanceVerdict::Granted);
    }

    #[test]
    fn unanswered_clearance_cocks_the_strip() {
        let events = parse(&[
            line(
                1,
                "t1",
                "slip_opened.v1",
                r#"{"request":"r","workflow":"w","engineer":"e"}"#,
            ),
            line(
                2,
                "t2",
                "clearance_requested.v1",
                r#"{"boundary":"plan->build","by":"planner"}"#,
            ),
        ]);
        let slip = fold(&events).unwrap();
        assert_eq!(slip.status, Status::InFlight);
        assert_eq!(slip.cocked.as_deref(), Some("plan->build"));
    }

    #[test]
    fn closed_slips_are_never_cocked() {
        let events = parse(&[
            line(
                1,
                "t1",
                "slip_opened.v1",
                r#"{"request":"r","workflow":"w","engineer":"e"}"#,
            ),
            line(
                2,
                "t2",
                "clearance_requested.v1",
                r#"{"boundary":"ship","by":"reviewer"}"#,
            ),
            line(
                3,
                "t3",
                "slip_closed.v1",
                r#"{"outcome":"rejected","reason":"review failed"}"#,
            ),
        ]);
        let slip = fold(&events).unwrap();
        assert_eq!(slip.status, Status::Rejected);
        assert_eq!(slip.cocked, None);
        assert_eq!(slip.close_reason.as_deref(), Some("review failed"));
    }

    #[test]
    fn unknown_kinds_are_explicit_and_survive_the_fold() {
        let events = parse(&[
            line(
                1,
                "t1",
                "slip_opened.v1",
                r#"{"request":"r","workflow":"w","engineer":"e"}"#,
            ),
            line(2, "t2", "teleportation_requested.v9", r#"{"to":"moon"}"#),
        ]);
        assert!(
            matches!(&events[1].kind, EventKind::Unknown { kind, .. } if kind == "teleportation_requested.v9")
        );
        let slip = fold(&events).unwrap();
        assert_eq!(slip.status, Status::InFlight);
        assert_eq!(slip.event_count, 2); // counted in the ledger, opaque to the fold
    }

    #[test]
    fn unknown_closed_set_values_degrade_to_unknown_not_lies() {
        // A known kind whose closed-set field has an unknown value must not
        // half-parse into something wrong — it degrades to Unknown, whole.
        let event = Event::from_line(&line(
            1,
            "t1",
            "phase_ended.v1",
            r#"{"phase":"plan","outcome":"transcended"}"#,
        ))
        .unwrap();
        assert!(matches!(event.kind, EventKind::Unknown { .. }));
    }

    #[test]
    fn bad_lines_are_counted_never_silently_dropped() {
        let dir = std::env::temp_dir().join(format!("daemar-ledger-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("slip-x.jsonl");
        std::fs::write(
            &path,
            format!(
                "{}\nthis is not json\n{}\n",
                line(
                    1,
                    "t1",
                    "slip_opened.v1",
                    r#"{"request":"r","workflow":"w","engineer":"e"}"#
                ),
                line(2, "t2", "note.v1", r#"{"text":"fine"}"#),
            ),
        )
        .unwrap();
        let file = load_ledger(&path).unwrap();
        assert_eq!(file.events.len(), 2);
        assert_eq!(file.bad_lines, vec![2]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn format_and_parse_are_inverses() {
        for epoch in [0u64, 1_785_776_400, 86_399, 86_400, 4_102_444_800] {
            assert_eq!(parse_ts(&format_ts(epoch)), Some(epoch), "epoch {epoch}");
        }
        assert_eq!(format_ts(1_785_776_400), "2026-08-03T17:00:00Z");
    }

    #[test]
    fn writer_and_reader_agree_on_the_wire() {
        let dir = std::env::temp_dir().join(format!("daemar-writer-test-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let mut w = LedgerWriter::create(&dir, SlipId("slip-w".into())).unwrap();
        w.append(&Kind::SlipOpened {
            request: "r".into(),
            workflow: "prompt".into(),
            engineer: "kendall".into(),
            repo: String::new(),
        })
        .unwrap();
        w.append(&Kind::SectionWritten {
            section: "response.v1".into(),
            by: "responder".into(),
            summary: "short".into(),
            body: "the full text\nwith lines".into(),
        })
        .unwrap();
        w.append(&Kind::SlipClosed {
            outcome: SlipOutcome::Accepted,
            reason: String::new(),
            by: "daemar".into(),
        })
        .unwrap();

        // A second writer for the same slip must fail loud: born exactly once.
        assert!(LedgerWriter::create(&dir, SlipId("slip-w".into())).is_err());

        let file = load_ledger(&dir.join("slip-w.jsonl")).unwrap();
        assert_eq!(file.bad_lines.len(), 0);
        let slip = fold(&file.events).expect("folds");
        assert_eq!(slip.status, Status::Accepted);
        assert_eq!(slip.sections[0].body, "the full text\nwith lines");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn phase_engineers_ride_the_wire_and_survive_the_fold() {
        let dir = std::env::temp_dir().join(format!("daemar-flyer-test-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let mut w = LedgerWriter::create(&dir, SlipId("slip-flyer".into())).unwrap();
        w.append(&Kind::SlipOpened {
            request: "r".into(),
            workflow: "plan".into(),
            engineer: "mcp:moggy".into(),
            repo: String::new(),
        })
        .unwrap();
        w.append(&Kind::PhaseStarted {
            phase: "respond".into(),
            owner: "responder".into(),
            lane: Lane::Agent,
            engineer: "mcp:moghedien".into(),
        })
        .unwrap();

        let file = load_ledger(&dir.join("slip-flyer.jsonl")).unwrap();
        assert!(file.bad_lines.is_empty());
        let slip = fold(&file.events).unwrap();
        assert_eq!(slip.engineer, "mcp:moggy", "the slip belongs to its opener");
        assert_eq!(
            slip.phases[0].engineer, "mcp:moghedien",
            "the stage records its flyer"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_unwitnessed_error_derives_failed() {
        let events = parse(&[
            line(
                1,
                "t1",
                "slip_opened.v1",
                r#"{"request":"r","workflow":"w","engineer":"e"}"#,
            ),
            line(
                2,
                "t2",
                "phase_started.v1",
                r#"{"phase":"respond","owner":"o","lane":"agent"}"#,
            ),
            line(
                3,
                "t3",
                "phase_ended.v1",
                r#"{"phase":"respond","outcome":"error"}"#,
            ),
        ]);
        let slip = fold(&events).unwrap();
        assert_eq!(slip.status, Status::InFlight); // no one closed it — by design
        assert_eq!(slip.failed.as_deref(), Some("respond"));
    }

    #[test]
    fn disposition_clears_failed_and_a_retry_phase_clears_it_too() {
        let base = [
            line(
                1,
                "t1",
                "slip_opened.v1",
                r#"{"request":"r","workflow":"w","engineer":"e"}"#,
            ),
            line(
                2,
                "t2",
                "phase_started.v1",
                r#"{"phase":"respond","owner":"o","lane":"agent"}"#,
            ),
            line(
                3,
                "t3",
                "phase_ended.v1",
                r#"{"phase":"respond","outcome":"error"}"#,
            ),
        ];
        let disposed = parse(
            &base
                .iter()
                .cloned()
                .chain([line(
                    4,
                    "t4",
                    "slip_closed.v1",
                    r#"{"outcome":"rejected","reason":"disposed","by":"kendall"}"#,
                )])
                .collect::<Vec<_>>(),
        );
        let slip = fold(&disposed).unwrap();
        assert_eq!(slip.status, Status::Rejected);
        assert_eq!(slip.failed, None);

        let retried = parse(
            &base
                .iter()
                .cloned()
                .chain([line(
                    4,
                    "t4",
                    "phase_started.v1",
                    r#"{"phase":"respond","owner":"o","lane":"agent"}"#,
                )])
                .collect::<Vec<_>>(),
        );
        let slip = fold(&retried).unwrap();
        assert_eq!(slip.failed, None); // a new phase is progress, not a corpse
    }

    #[test]
    fn a_granted_clearance_with_nothing_flown_is_holding() {
        let base = [
            line(
                1,
                "2026-01-01T00:00:01Z",
                "slip_opened.v1",
                r#"{"request":"r","workflow":"plan","engineer":"e"}"#,
            ),
            line(
                2,
                "2026-01-01T00:00:02Z",
                "phase_started.v1",
                r#"{"phase":"plan","owner":"o","lane":"agent"}"#,
            ),
            line(
                3,
                "2026-01-01T00:00:03Z",
                "phase_ended.v1",
                r#"{"phase":"plan","outcome":"success"}"#,
            ),
            line(
                4,
                "2026-01-01T00:00:04Z",
                "clearance_requested.v1",
                r#"{"boundary":"plan->respond","by":"planner"}"#,
            ),
            line(
                5,
                "2026-01-01T00:00:05Z",
                "clearance_granted.v1",
                r#"{"boundary":"plan->respond","by":"kendall"}"#,
            ),
        ];
        let slip = fold(&parse(&base)).unwrap();
        assert_eq!(slip.cocked, None); // answered, so not cocked...
        assert_eq!(slip.holding.as_deref(), Some("plan->respond")); // ...but waiting for throttle

        // Pushing the throttle clears holding.
        let continued = parse(
            &base
                .iter()
                .cloned()
                .chain([line(
                    6,
                    "2026-01-01T00:00:06Z",
                    "phase_started.v1",
                    r#"{"phase":"respond","owner":"o","lane":"agent"}"#,
                )])
                .collect::<Vec<_>>(),
        );
        assert_eq!(fold(&continued).unwrap().holding, None);
    }

    #[test]
    fn resume_continues_the_sequence_and_never_creates() {
        let dir = std::env::temp_dir().join(format!("daemar-resume-test-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        assert!(LedgerWriter::resume(&dir.join("nope"), SlipId("ghost".into())).is_err());

        let mut w = LedgerWriter::create(&dir, SlipId("slip-r".into())).unwrap();
        w.append(&Kind::SlipOpened {
            request: "r".into(),
            workflow: "w".into(),
            engineer: "e".into(),
            repo: String::new(),
        })
        .unwrap();
        drop(w);

        let mut resumed = LedgerWriter::resume(&dir, SlipId("slip-r".into())).unwrap();
        let seq = resumed
            .append(&Kind::SlipClosed {
                outcome: SlipOutcome::Rejected,
                reason: "disposed".into(),
                by: "kendall".into(),
            })
            .unwrap();
        assert_eq!(seq, 2); // continued, not restarted
        let file = load_ledger(&dir.join("slip-r.jsonl")).unwrap();
        assert_eq!(fold(&file.events).unwrap().status, Status::Rejected);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn timestamps_parse_to_epoch_seconds() {
        assert_eq!(parse_ts("2026-08-03T17:00:00Z"), Some(1_785_776_400));
        assert_eq!(parse_ts("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_ts("2026-08-03T17:00:00.123Z"), Some(1_785_776_400));
        assert_eq!(parse_ts("not a timestamp"), None);
    }
}
