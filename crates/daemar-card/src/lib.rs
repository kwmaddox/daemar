//! Daemar Card store: durable, append-only workflow records for factory
//! tasks (PER-82).
//!
//! The Card is the log, not a document: a durable identity plus its
//! ordered, immutable entry sequence in one factory-owned `SQLite`
//! database. This crate is the service-shaped core; the `card` CLI (and
//! later frontends such as an MCP server) are thin clients over it.
//! Behavior is specified executably in `tests/features/` (S1-B1…S1-B14).

mod domain;
mod error;
mod storage;

pub use domain::{
    Accepted, AppendEntry, CardCreatedV1, CardId, CardSummary, CreateCard, DecisionV1, Entry,
    EntryId, EntryType, Payload, Producer, ProducerKind, CURRENT_SCHEMA_VERSION,
};
pub use error::{Error, ErrorCategory, StorageContext};
pub use storage::Store;
