//! The executor seam: where a stage's tool calls actually run.
//!
//! In-process is the fast path read-only seats have always flown. Caged
//! sends each request across a container boundary as JSON and reads the
//! outcome back. The seam is invisible above it: the engine appends the
//! same tool_call.v1 either way, and neither the ledger nor the fold learns
//! which executor ran a call.
//!
//! The write guard across the boundary (the phase-1 deferred question,
//! answered): docker exec spawns a fresh in-cage process per request, so
//! the HOST retains the read-hash record — built only from successful read
//! outcomes — and refuses an edit of an unread file before paying for an
//! exec. The expected hash rides to the cage in a field the model never
//! controls, and the in-cage executor re-verifies the file's current bytes
//! immediately before replacing: durable state outside the blast radius,
//! the authoritative check inside it. A successful edit does NOT advance
//! the record — the next edit of that file demands a fresh read.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::roster::ToolAccess;
use crate::sandbox::Cage;
use crate::tools::{self, ToolContext, ToolOutcome};

/// One tool request crossing the cage boundary. `expected_hash` is set by
/// the HOST from its read record — never from model-visible arguments.
#[derive(Debug, Serialize, Deserialize)]
pub struct ToolRequest {
    pub name: String,
    pub args: Value,
    pub access: ToolAccess,
    #[serde(default)]
    pub expected_hash: Option<String>,
}

pub enum StageExecutor<'r> {
    InProcess {
        ctx: ToolContext,
        access: ToolAccess,
    },
    Caged {
        cage: Cage<'r>,
        access: ToolAccess,
        /// path (as the model names it) -> hash of the last successful read.
        read_hashes: HashMap<String, String>,
    },
}

fn arg_path(args: &Value) -> Option<String> {
    args.get("path").and_then(Value::as_str).map(str::to_string)
}

impl StageExecutor<'_> {
    pub fn execute(&mut self, name: &str, args: &Value) -> ToolOutcome {
        match self {
            StageExecutor::InProcess { ctx, access } => tools::execute(name, args, ctx, *access),
            StageExecutor::Caged {
                cage,
                access,
                read_hashes,
            } => {
                // The host half of the guard: an edit with no recorded read
                // refuses before a docker exec is paid for. The stale case
                // is the cage's to catch — its check is at mutation time.
                let expected_hash = match (name, arg_path(args)) {
                    ("edit", Some(path)) => match read_hashes.get(&path) {
                        Some(hash) => Some(hash.clone()),
                        None => {
                            return ToolOutcome {
                                content: format!(
                                    "edit: '{path}' has not been read — read it first"
                                ),
                                is_error: true,
                                hash: String::new(),
                                before_hash: None,
                            }
                        }
                    },
                    _ => None,
                };
                let request = ToolRequest {
                    name: name.to_string(),
                    args: args.clone(),
                    access: *access,
                    expected_hash,
                };
                let request_json =
                    serde_json::to_string(&request).expect("a tool request serializes");
                let outcome = match cage.execute(&request_json) {
                    Ok(outcome_json) => match serde_json::from_str::<ToolOutcome>(&outcome_json) {
                        Ok(outcome) => outcome,
                        Err(error) => ToolOutcome {
                            content: format!("cage returned an unreadable outcome: {error}"),
                            is_error: true,
                            hash: String::new(),
                            before_hash: None,
                        },
                    },
                    Err(detail) => ToolOutcome {
                        content: format!("cage failure: {detail}"),
                        is_error: true,
                        hash: String::new(),
                        before_hash: None,
                    },
                };
                // Record successful reads; never advance on a mutation.
                if name == "read" && !outcome.is_error && !outcome.hash.is_empty() {
                    if let Some(path) = arg_path(args) {
                        read_hashes.insert(path, outcome.hash.clone());
                    }
                }
                outcome
            }
        }
    }

    /// A dead cage cannot be trusted for another call; the stage must end
    /// as a witnessed failure. In-process execution has no such state.
    pub fn dead(&self) -> bool {
        match self {
            StageExecutor::InProcess { .. } => false,
            StageExecutor::Caged { cage, .. } => cage.dead,
        }
    }
}
