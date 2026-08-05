//! The executor seam: where a stage's tool calls actually run.
//!
//! In-process is the fast path scouts have always flown. Caged sends each
//! request across a container boundary as JSON and reads the outcome back.
//! The seam is invisible above it: the engine appends the same tool_call.v1
//! either way, and neither the ledger nor the fold learns which executor
//! ran a call.
//!
//! Phase-2 note: docker exec spawns a fresh process per request, so in-cage
//! ToolContext state (read_hashes) does not persist across calls. Write
//! tools will need the guard state carried host-side or a persistent
//! executor protocol — decided when the hands are built, not before.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::sandbox::Cage;
use crate::tools::{self, ToolContext, ToolOutcome};

/// One tool request crossing the cage boundary.
#[derive(Debug, Serialize, Deserialize)]
pub struct ToolRequest {
    pub name: String,
    pub args: Value,
}

pub enum StageExecutor<'r> {
    InProcess(ToolContext),
    Caged(Cage<'r>),
}

impl StageExecutor<'_> {
    pub fn execute(&mut self, name: &str, args: &Value) -> ToolOutcome {
        match self {
            StageExecutor::InProcess(ctx) => tools::execute(name, args, ctx),
            StageExecutor::Caged(cage) => {
                let request = ToolRequest {
                    name: name.to_string(),
                    args: args.clone(),
                };
                let request_json =
                    serde_json::to_string(&request).expect("a tool request serializes");
                match cage.execute(&request_json) {
                    Ok(outcome_json) => match serde_json::from_str::<ToolOutcome>(&outcome_json) {
                        Ok(outcome) => outcome,
                        Err(error) => ToolOutcome {
                            content: format!("cage returned an unreadable outcome: {error}"),
                            is_error: true,
                            hash: String::new(),
                        },
                    },
                    Err(detail) => ToolOutcome {
                        content: format!("cage failure: {detail}"),
                        is_error: true,
                        hash: String::new(),
                    },
                }
            }
        }
    }

    /// A dead cage cannot be trusted for another call; the stage must end
    /// as a witnessed failure. In-process execution has no such state.
    pub fn dead(&self) -> bool {
        match self {
            StageExecutor::InProcess(_) => false,
            StageExecutor::Caged(cage) => cage.dead,
        }
    }
}
