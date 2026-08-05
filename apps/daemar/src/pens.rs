//! The controller's pens: human writes on the ledger — grant, refuse,
//! dispose. No model calls here; just signed judgment.

use std::process::ExitCode;

use ledger::{Kind, LedgerWriter, PhaseOutcome, Slip, SlipId, SlipOutcome, Status};

use crate::config::{engineer, ledgers_dir};

/// Grant the pending clearance: the strip un-cocks, the flight may continue.
pub fn grant(args: &[String]) -> ExitCode {
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
pub fn refuse(args: &[String]) -> ExitCode {
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
pub fn dispose(args: &[String]) -> ExitCode {
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
        return crate::usage();
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
pub fn load_open_slip(slip_id: &str) -> Result<Slip, String> {
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
