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
    },
    #[serde(rename = "phase_started.v1")]
    PhaseStarted {
        phase: String,
        owner: String,
        lane: Lane,
    },
    #[serde(rename = "phase_ended.v1")]
    PhaseEnded { phase: String, outcome: PhaseOutcome },
    #[serde(rename = "section_written.v1")]
    SectionWritten {
        section: String,
        by: String,
        #[serde(default)]
        summary: String,
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
    #[serde(rename = "model_requested.v1")]
    ModelRequested { phase: String, model: String },
    #[serde(rename = "model_call.v1")]
    ModelCall {
        phase: String,
        model: String,
        tokens: u64,
        cost: f64,
    },
    #[serde(rename = "note.v1")]
    Note { text: String },
    #[serde(rename = "slip_closed.v1")]
    SlipClosed {
        outcome: SlipOutcome,
        #[serde(default)]
        reason: String,
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
                    v.get("kind").and_then(Value::as_str).unwrap_or_default().to_string(),
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
            Err(_) => EventKind::Unknown { kind: wire.kind, payload: wire.payload },
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
    pub started: String,
    pub ended: Option<String>,
    pub outcome: Option<PhaseOutcome>,
}

#[derive(Debug, Clone)]
pub struct SectionRow {
    pub section: String,
    pub by: String,
    pub summary: String,
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
    pub status: Status,
    /// The boundary awaiting the controller, when the strip is cocked.
    pub cocked: Option<String>,
    pub current_phase: Option<String>,
    pub close_reason: Option<String>,
    pub phases: Vec<PhaseRow>,
    pub sections: Vec<SectionRow>,
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

    for event in events {
        let ts = event.ts.clone();
        if let Some(s) = slip.as_mut() {
            s.last_ts = ts.clone();
            s.event_count += 1;
        }
        let EventKind::Known(kind) = &event.kind else { continue };

        match kind.clone() {
            Kind::SlipOpened { request, workflow, engineer } => {
                slip = Some(Slip {
                    id: event.slip_id.clone(),
                    request,
                    workflow,
                    engineer,
                    status: Status::InFlight,
                    cocked: None,
                    current_phase: None,
                    close_reason: None,
                    phases: Vec::new(),
                    sections: Vec::new(),
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
                    Kind::PhaseStarted { phase, owner, lane } => {
                        s.current_phase = Some(phase.clone());
                        s.phases.push(PhaseRow {
                            phase,
                            owner,
                            lane,
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
                    Kind::SectionWritten { section, by, summary } => {
                        s.sections.push(SectionRow { section, by, summary, ts });
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
                        answer_clearance(s, &boundary, ClearanceVerdict::Granted, by, ts, String::new());
                    }
                    Kind::ClearanceRefused { boundary, by, reason } => {
                        answer_clearance(s, &boundary, ClearanceVerdict::Refused, by, ts, reason);
                    }
                    Kind::QueryMade { .. } => s.queries += 1,
                    Kind::ModelRequested { model, .. } => {
                        s.last_model = Some(model);
                    }
                    Kind::ModelCall { model, tokens, cost, .. } => {
                        s.model_calls += 1;
                        s.tokens += tokens;
                        s.cost += cost;
                        s.last_model = Some(model);
                    }
                    Kind::Note { .. } => {}
                    Kind::SlipClosed { outcome, reason } => {
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

    // Cocked is derived, never asserted: an unanswered clearance on a slip
    // that is still in flight.
    if let Some(s) = slip.as_mut() {
        s.cocked = if s.status == Status::InFlight {
            s.clearances
                .iter()
                .rev()
                .find(|c| c.response.is_none())
                .map(|c| c.boundary.clone())
        } else {
            None
        };
    }
    slip
}

fn answer_clearance(
    slip: &mut Slip,
    boundary: &str,
    verdict: ClearanceVerdict,
    by: String,
    ts: String,
    reason: String,
) {
    if let Some(row) = slip
        .clearances
        .iter_mut()
        .rev()
        .find(|c| c.boundary == boundary && c.response.is_none())
    {
        row.response = Some(ClearanceResponse { verdict, by, ts, reason });
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
    let text = fs::read_to_string(path)
        .map_err(|source| LedgerError::Io { path: path.to_path_buf(), source })?;
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
    let entries = fs::read_dir(dir)
        .map_err(|source| LedgerError::Io { path: dir.to_path_buf(), source })?;
    let mut report = LoadReport::default();
    for entry in entries {
        let entry = entry.map_err(|source| LedgerError::Io { path: dir.to_path_buf(), source })?;
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
    report.slips.sort_by(|a, b| b.slip.last_ts.cmp(&a.slip.last_ts));
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
        lines.iter().map(|l| Event::from_line(l).expect("parses")).collect()
    }

    #[test]
    fn accepted_flight_folds_clean() {
        let events = parse(&[
            line(1, "t1", "slip_opened.v1", r#"{"request":"add /health","workflow":"simple","engineer":"kendall"}"#),
            line(2, "t2", "phase_started.v1", r#"{"phase":"plan","owner":"planner","lane":"agent"}"#),
            line(3, "t3", "model_requested.v1", r#"{"phase":"plan","model":"m"}"#),
            line(4, "t4", "model_call.v1", r#"{"phase":"plan","model":"m","tokens":1000,"cost":0.02}"#),
            line(5, "t5", "section_written.v1", r#"{"section":"plan.v1","by":"planner"}"#),
            line(6, "t6", "phase_ended.v1", r#"{"phase":"plan","outcome":"success"}"#),
            line(7, "t7", "clearance_requested.v1", r#"{"boundary":"plan->build","by":"planner"}"#),
            line(8, "t8", "clearance_granted.v1", r#"{"boundary":"plan->build","by":"kendall"}"#),
            line(9, "t9", "slip_closed.v1", r#"{"outcome":"accepted"}"#),
        ]);
        let slip = fold(&events).expect("slip opened");
        assert_eq!(slip.status, Status::Accepted);
        assert_eq!(slip.cocked, None);
        assert_eq!(slip.current_phase, None);
        assert_eq!(slip.phases.len(), 1);
        assert_eq!(slip.phases[0].outcome, Some(PhaseOutcome::Success));
        assert_eq!(slip.phases[0].lane, Lane::Agent);
        assert_eq!(slip.sections.len(), 1);
        assert_eq!(slip.tokens, 1000);
        assert_eq!(slip.last_model.as_deref(), Some("m"));
        assert_eq!(slip.event_count, 9);
        let response = slip.clearances[0].response.as_ref().expect("answered");
        assert_eq!(response.verdict, ClearanceVerdict::Granted);
    }

    #[test]
    fn unanswered_clearance_cocks_the_strip() {
        let events = parse(&[
            line(1, "t1", "slip_opened.v1", r#"{"request":"r","workflow":"w","engineer":"e"}"#),
            line(2, "t2", "clearance_requested.v1", r#"{"boundary":"plan->build","by":"planner"}"#),
        ]);
        let slip = fold(&events).unwrap();
        assert_eq!(slip.status, Status::InFlight);
        assert_eq!(slip.cocked.as_deref(), Some("plan->build"));
    }

    #[test]
    fn closed_slips_are_never_cocked() {
        let events = parse(&[
            line(1, "t1", "slip_opened.v1", r#"{"request":"r","workflow":"w","engineer":"e"}"#),
            line(2, "t2", "clearance_requested.v1", r#"{"boundary":"ship","by":"reviewer"}"#),
            line(3, "t3", "slip_closed.v1", r#"{"outcome":"rejected","reason":"review failed"}"#),
        ]);
        let slip = fold(&events).unwrap();
        assert_eq!(slip.status, Status::Rejected);
        assert_eq!(slip.cocked, None);
        assert_eq!(slip.close_reason.as_deref(), Some("review failed"));
    }

    #[test]
    fn unknown_kinds_are_explicit_and_survive_the_fold() {
        let events = parse(&[
            line(1, "t1", "slip_opened.v1", r#"{"request":"r","workflow":"w","engineer":"e"}"#),
            line(2, "t2", "teleportation_requested.v9", r#"{"to":"moon"}"#),
        ]);
        assert!(matches!(&events[1].kind, EventKind::Unknown { kind, .. } if kind == "teleportation_requested.v9"));
        let slip = fold(&events).unwrap();
        assert_eq!(slip.status, Status::InFlight);
        assert_eq!(slip.event_count, 2); // counted in the ledger, opaque to the fold
    }

    #[test]
    fn unknown_closed_set_values_degrade_to_unknown_not_lies() {
        // A known kind whose closed-set field has an unknown value must not
        // half-parse into something wrong — it degrades to Unknown, whole.
        let event = Event::from_line(&line(
            1, "t1", "phase_ended.v1", r#"{"phase":"plan","outcome":"transcended"}"#,
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
                line(1, "t1", "slip_opened.v1", r#"{"request":"r","workflow":"w","engineer":"e"}"#),
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
    fn timestamps_parse_to_epoch_seconds() {
        assert_eq!(parse_ts("2026-08-03T17:00:00Z"), Some(1_785_776_400));
        assert_eq!(parse_ts("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_ts("2026-08-03T17:00:00.123Z"), Some(1_785_776_400));
        assert_eq!(parse_ts("not a timestamp"), None);
    }
}
