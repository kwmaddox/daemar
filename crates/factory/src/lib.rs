//! The factory itself, as a library: every interface — the CLI today, the
//! MCP tower tomorrow — is a thin skin over these seams.
//!
//! - [`roster`] — who fills each seat: roles, agents, tool access
//! - [`config`] — the env edge, parsed once; per-role airframes; pricing
//! - [`engine`] — one stage, run as a turn loop, events on the ledger
//! - [`workflows`] — the flights: prompt, plan, scout, build, continue
//! - [`pens`] — the controller's writes: grant, refuse, dispose
//! - [`provider`] — the OpenAI Responses API seam, stateless by doctrine
//! - [`tools`] — territory tools, capability-gated: reads for every tooled
//!   seat, hash-guarded edit/write for the builder, no delete — structurally
//! - [`worktree`] — pinned detached worktrees: stages never fly the live tree
//! - [`sandbox`] — the cage: per-stage Docker containment for tool execution
//! - [`executor`] — the seam picking where a stage's tools actually run
//! - [`registry`] — airframes.toml: real prices, never silent

pub mod config;
pub mod engine;
pub mod executor;
pub mod pens;
pub mod provider;
pub mod registry;
pub mod roster;
pub mod sandbox;
pub mod tools;
pub mod workflows;
pub mod worktree;
