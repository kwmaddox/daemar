//! The executor seam: where a stage's tool calls actually run.
//!
//! In-process is the fast path read-only seats have always flown. Sandboxed
//! sends each request across the wall as JSON and reads the outcome back.
//! The seam is invisible above it: the engine appends the same tool_call.v1
//! either way, and neither the ledger nor the fold learns which executor
//! ran a call.
//!
//! The write guard across the boundary (the phase-1 deferred question,
//! answered): the wall spawns a fresh in-guest process per request, so
//! the HOST retains the read-hash record — updated from each successful
//! read or mutation outcome — and refuses an edit of an unread file before
//! paying for a crossing. The expected hash rides to the guest in a field
//! the model never controls, and the in-guest executor re-verifies the
//! file's current bytes immediately before replacing: durable state outside
//! the blast radius, the authoritative check inside it. A successful
//! mutation advances the record to its post-image.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::roster::ToolAccess;
use crate::tools::{self, ToolContext, ToolOutcome};
use crate::wall::StageWall;

/// One tool request crossing the cage boundary. `expected_hash` is set by
/// the HOST from its read record — never from model-visible arguments.
#[derive(Debug, Serialize, Deserialize)]
pub struct ToolRequest {
    pub name: String,
    pub args: Value,
    /// Defaulted to ReadOnly: the cage boundary is a WIRE PROTOCOL, and a
    /// host old enough to omit this field is a read-era host — least
    /// privilege is also backward compatibility. (Learned live: a phase-1
    /// host against a phase-2 image broke every tool call.)
    #[serde(default = "ToolRequest::default_access")]
    pub access: ToolAccess,
    #[serde(default)]
    pub expected_hash: Option<String>,
}

impl ToolRequest {
    fn default_access() -> ToolAccess {
        ToolAccess::ReadOnly
    }
}

pub enum StageExecutor {
    InProcess {
        ctx: ToolContext,
        access: ToolAccess,
    },
    Sandboxed {
        wall: Box<dyn StageWall>,
        access: ToolAccess,
        /// path (as the model names it) -> hash the guard expects after the
        /// last successful read or mutation.
        read_hashes: HashMap<String, String>,
    },
}

fn arg_path(args: &Value) -> Option<String> {
    args.get("path").and_then(Value::as_str).map(str::to_string)
}

impl StageExecutor {
    pub fn execute(&mut self, name: &str, args: &Value) -> ToolOutcome {
        match self {
            StageExecutor::InProcess { ctx, access } => tools::execute(name, args, ctx, *access),
            StageExecutor::Sandboxed {
                wall,
                access,
                read_hashes,
            } => {
                // The host half of the guard: an edit with no recorded read
                // refuses before a crossing is paid for. The stale case is
                // the guest's to catch — its check is at mutation time.
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
                let outcome = match wall.send(&request_json) {
                    Ok(outcome_json) => match serde_json::from_str::<ToolOutcome>(&outcome_json) {
                        Ok(outcome) => outcome,
                        Err(error) => ToolOutcome {
                            content: format!("the sandbox returned an unreadable outcome: {error}"),
                            is_error: true,
                            hash: String::new(),
                            before_hash: None,
                        },
                    },
                    Err(detail) => ToolOutcome {
                        content: format!("sandbox failure: {detail}"),
                        is_error: true,
                        hash: String::new(),
                        before_hash: None,
                    },
                };
                // Record each successful read or mutation's reported image
                // so the next request is stamped against the world it made.
                if matches!(name, "read" | "edit" | "write")
                    && !outcome.is_error
                    && !outcome.hash.is_empty()
                {
                    if let Some(path) = arg_path(args) {
                        read_hashes.insert(path, outcome.hash.clone());
                    }
                }
                outcome
            }
        }
    }

    /// A dead wall cannot be trusted for another call; the stage must end
    /// as a witnessed failure. In-process execution has no such state.
    pub fn dead(&self) -> bool {
        match self {
            StageExecutor::InProcess { .. } => false,
            StageExecutor::Sandboxed { wall, .. } => wall.dead(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    use crate::wall::{Teardown, WallError};

    struct FakeWall {
        requests: Rc<RefCell<Vec<String>>>,
        outcomes: VecDeque<String>,
    }

    impl StageWall for FakeWall {
        fn id(&self) -> &str {
            "fake"
        }

        fn send(&mut self, request_json: &str) -> Result<String, String> {
            self.requests.borrow_mut().push(request_json.to_string());
            Ok(self.outcomes.pop_front().unwrap())
        }

        fn dead(&self) -> bool {
            false
        }

        fn terminate(self: Box<Self>) -> Result<Teardown, WallError> {
            Ok(Teardown::Removed)
        }
    }

    fn outcome(hash: &str) -> String {
        serde_json::to_string(&ToolOutcome {
            content: "ok".to_string(),
            is_error: false,
            hash: hash.to_string(),
            before_hash: None,
        })
        .unwrap()
    }

    fn sandboxed(outcomes: &[&str]) -> (StageExecutor, Rc<RefCell<Vec<String>>>) {
        let requests = Rc::new(RefCell::new(Vec::new()));
        let wall = FakeWall {
            requests: requests.clone(),
            outcomes: outcomes.iter().map(|hash| outcome(hash)).collect(),
        };
        (
            StageExecutor::Sandboxed {
                wall: Box::new(wall),
                access: ToolAccess::ReadWrite,
                read_hashes: HashMap::new(),
            },
            requests,
        )
    }

    #[test]
    fn unread_edits_are_refused_before_crossing_the_wall() {
        let (mut executor, requests) = sandboxed(&[]);
        let out = executor.execute("edit", &serde_json::json!({"path": "src/lib.rs"}));
        assert!(out.is_error);
        assert!(requests.borrow().is_empty());
    }

    #[test]
    fn successive_edits_use_the_previous_post_image_hash() {
        let (mut executor, requests) = sandboxed(&["read-hash", "first-post", "second-post"]);
        executor.execute("read", &serde_json::json!({"path": "src/lib.rs"}));
        let first = executor.execute("edit", &serde_json::json!({"path": "src/lib.rs"}));
        assert!(!first.is_error);
        executor.execute("edit", &serde_json::json!({"path": "src/lib.rs"}));

        let requests: Vec<ToolRequest> = requests
            .borrow()
            .iter()
            .map(|request| serde_json::from_str(request).unwrap())
            .collect();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[2].expected_hash.as_deref(), Some("first-post"));
    }

    #[test]
    fn a_write_seeds_the_record_for_an_immediate_edit() {
        let (mut executor, requests) = sandboxed(&["write-post", "edit-post"]);
        executor.execute("write", &serde_json::json!({"path": "src/new.rs"}));
        let out = executor.execute("edit", &serde_json::json!({"path": "src/new.rs"}));
        assert!(!out.is_error);

        let requests: Vec<ToolRequest> = requests
            .borrow()
            .iter()
            .map(|request| serde_json::from_str(request).unwrap())
            .collect();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].expected_hash.as_deref(), Some("write-post"));
    }
}
