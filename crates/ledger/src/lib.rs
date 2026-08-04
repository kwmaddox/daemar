//! The ledger and the slip fold.
//!
//! The ledger is the append-only truth: one JSONL file per slip, every event
//! carrying a versioned `kind`. The slip is a fold over those events — it is
//! never written, only derived, so it cannot lie. Liveness and attention
//! (in flight, cocked) are derived states; no event ever asserts them.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SCHEMA: &str = "ledger.v1";

/// One appended event. `kind` stays a string in the envelope so a ledger
/// written by a newer factory still loads; `typed()` gives the known view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub schema: String,
    pub slip_id: String,
    pub seq: u64,
    pub ts: String,
    pub kind: String,
    #[serde(default)]
    pub payload: Value,
}

/// The known event kinds. An unknown kind is not an error — it renders raw
/// and the fold skips it — so old boards survive new factories.
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
        /// engineer | agent | code — the swim lane, named to avoid the envelope's `kind`.
        lane: String,
    },
    #[serde(rename = "phase_ended.v1")]
    PhaseEnded { phase: String, outcome: String },
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
        outcome: String,
        #[serde(default)]
        reason: String,
    },
}

impl Event {
    pub fn typed(&self) -> Option<Kind> {
        serde_json::from_value(serde_json::json!({
            "kind": self.kind,
            "payload": self.payload,
        }))
        .ok()
    }
}

// ── The slip: a fold over one ledger ─────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Status {
    InFlight,
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Serialize)]
pub struct PhaseRow {
    pub phase: String,
    pub owner: String,
    pub lane: String,
    pub started: String,
    pub ended: Option<String>,
    pub outcome: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SectionRow {
    pub section: String,
    pub by: String,
    pub summary: String,
    pub ts: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClearanceRow {
    pub boundary: String,
    pub requested_by: String,
    pub requested_ts: String,
    /// (granted | refused, by, ts) once answered.
    pub response: Option<(String, String, String)>,
    #[serde(default)]
    pub reason: String,
}

/// The face plus everything the detail view discloses. Always derived.
#[derive(Debug, Clone, Serialize)]
pub struct Slip {
    pub id: String,
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
        let Some(kind) = event.typed() else { continue };

        match kind {
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
                    opened_ts: ts.clone(),
                    last_ts: ts,
                    event_count: 1,
                });
            }
            _ => {
                let Some(s) = slip.as_mut() else { continue };
                match kind {
                    Kind::SlipOpened { .. } => unreachable!(),
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
                            reason: String::new(),
                        });
                    }
                    Kind::ClearanceGranted { boundary, by } => {
                        answer_clearance(s, &boundary, "granted", by, ts, String::new());
                    }
                    Kind::ClearanceRefused { boundary, by, reason } => {
                        answer_clearance(s, &boundary, "refused", by, ts, reason);
                    }
                    Kind::QueryMade { .. } => s.queries += 1,
                    Kind::ModelCall { tokens, cost, .. } => {
                        s.model_calls += 1;
                        s.tokens += tokens;
                        s.cost += cost;
                    }
                    Kind::Note { .. } => {}
                    Kind::SlipClosed { outcome, reason } => {
                        s.status = if outcome == "accepted" {
                            Status::Accepted
                        } else {
                            Status::Rejected
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
    verdict: &str,
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
        row.response = Some((verdict.to_string(), by, ts));
        row.reason = reason;
    }
}

// ── Loading ──────────────────────────────────────────────────────────────────

/// Read one ledger file. Malformed lines are skipped, not fatal: the board
/// must keep rendering a fleet even when one writer misbehaves.
pub fn load_ledger(path: &Path) -> io::Result<Vec<Event>> {
    let text = fs::read_to_string(path)?;
    let mut events: Vec<Event> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    events.sort_by_key(|e| e.seq);
    Ok(events)
}

/// Load every `*.jsonl` ledger in a directory, folded, newest activity first.
pub fn load_dir(dir: &Path) -> io::Result<Vec<(Slip, Vec<Event>)>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let events = load_ledger(&path)?;
        if let Some(slip) = fold(&events) {
            out.push((slip, events));
        }
    }
    out.sort_by(|a, b| b.0.last_ts.cmp(&a.0.last_ts));
    Ok(out)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(seq: u64, ts: &str, kind: &str, payload: Value) -> Event {
        Event {
            schema: SCHEMA.to_string(),
            slip_id: "slip-1".to_string(),
            seq,
            ts: ts.to_string(),
            kind: kind.to_string(),
            payload,
        }
    }

    #[test]
    fn accepted_flight_folds_clean() {
        let events = vec![
            ev(1, "t1", "slip_opened.v1", serde_json::json!({
                "request": "add /health", "workflow": "simple", "engineer": "kendall"})),
            ev(2, "t2", "phase_started.v1", serde_json::json!({
                "phase": "plan", "owner": "planner", "lane": "agent"})),
            ev(3, "t3", "model_call.v1", serde_json::json!({
                "phase": "plan", "model": "m", "tokens": 1000, "cost": 0.02})),
            ev(4, "t4", "section_written.v1", serde_json::json!({
                "section": "plan.v1", "by": "planner"})),
            ev(5, "t5", "phase_ended.v1", serde_json::json!({
                "phase": "plan", "outcome": "success"})),
            ev(6, "t6", "clearance_requested.v1", serde_json::json!({
                "boundary": "plan->build", "by": "planner"})),
            ev(7, "t7", "clearance_granted.v1", serde_json::json!({
                "boundary": "plan->build", "by": "kendall"})),
            ev(8, "t8", "slip_closed.v1", serde_json::json!({"outcome": "accepted"})),
        ];
        let slip = fold(&events).expect("slip opened");
        assert_eq!(slip.status, Status::Accepted);
        assert_eq!(slip.cocked, None);
        assert_eq!(slip.current_phase, None);
        assert_eq!(slip.phases.len(), 1);
        assert_eq!(slip.phases[0].outcome.as_deref(), Some("success"));
        assert_eq!(slip.sections.len(), 1);
        assert_eq!(slip.tokens, 1000);
        assert_eq!(slip.event_count, 8);
    }

    #[test]
    fn unanswered_clearance_cocks_the_strip() {
        let events = vec![
            ev(1, "t1", "slip_opened.v1", serde_json::json!({
                "request": "r", "workflow": "w", "engineer": "e"})),
            ev(2, "t2", "clearance_requested.v1", serde_json::json!({
                "boundary": "plan->build", "by": "planner"})),
        ];
        let slip = fold(&events).unwrap();
        assert_eq!(slip.status, Status::InFlight);
        assert_eq!(slip.cocked.as_deref(), Some("plan->build"));
    }

    #[test]
    fn closed_slips_are_never_cocked() {
        let events = vec![
            ev(1, "t1", "slip_opened.v1", serde_json::json!({
                "request": "r", "workflow": "w", "engineer": "e"})),
            ev(2, "t2", "clearance_requested.v1", serde_json::json!({
                "boundary": "ship", "by": "reviewer"})),
            ev(3, "t3", "slip_closed.v1", serde_json::json!({
                "outcome": "rejected", "reason": "review failed"})),
        ];
        let slip = fold(&events).unwrap();
        assert_eq!(slip.status, Status::Rejected);
        assert_eq!(slip.cocked, None);
        assert_eq!(slip.close_reason.as_deref(), Some("review failed"));
    }

    #[test]
    fn unknown_kinds_survive_without_breaking_the_fold() {
        let events = vec![
            ev(1, "t1", "slip_opened.v1", serde_json::json!({
                "request": "r", "workflow": "w", "engineer": "e"})),
            ev(2, "t2", "teleportation_requested.v9", serde_json::json!({"to": "moon"})),
        ];
        let slip = fold(&events).unwrap();
        assert_eq!(slip.status, Status::InFlight);
        assert_eq!(slip.event_count, 2); // counted in the ledger, ignored by the fold
    }
}
