//! The factory itself, as a library: every interface — the CLI today, the
//! MCP tower tomorrow — is a thin skin over these seams.
//!
//! - [`roster`] — who fills each seat: roles, agents, tool access
//! - [`config`] — the env edge, parsed once; per-role airframes; pricing
//! - [`engine`] — one stage, run as a turn loop, events on the ledger
//! - [`workflows`] — the flights: prompt, plan, scout, continue
//! - [`pens`] — the controller's writes: grant, refuse, dispose
//! - [`provider`] — the OpenAI-compatible chat seam
//! - [`tools`] — read-only territory tools, confined by construction
//! - [`registry`] — airframes.toml: real prices, never silent

pub mod config;
pub mod engine;
pub mod pens;
pub mod provider;
pub mod registry;
pub mod roster;
pub mod tools;
pub mod workflows;
