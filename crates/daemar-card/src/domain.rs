//! The Card domain: identities, provenance, and the closed, versioned
//! entry vocabulary (milestone Q1/Q6; conventions C6/C7).

use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::Error;

/// The current payload schema version for every S1 entry type. Accepted
/// payloads are never migrated: readers keep parsing every historical
/// version forever, and evolution appends a new version alongside.
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Daemar-assigned durable Card identity (`UUIDv7`). Opaque to consumers
/// and never derived from an external task key (S1-B1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardId(String);

impl CardId {
    pub(crate) fn generate() -> CardId {
        CardId(uuid::Uuid::now_v7().to_string())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for CardId {
    fn from(value: String) -> CardId {
        CardId(value)
    }
}

impl std::fmt::Display for CardId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Daemar-assigned entry identity (`UUIDv7`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryId(String);

impl EntryId {
    pub(crate) fn generate() -> EntryId {
        EntryId(uuid::Uuid::now_v7().to_string())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for EntryId {
    fn from(value: String) -> EntryId {
        EntryId(value)
    }
}

impl std::fmt::Display for EntryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Who claims to have produced an entry. Recorded verbatim; M1 does not
/// authenticate producers (milestone Q6).
#[derive(Debug, Clone)]
pub struct Producer {
    /// The producer's self-reported identity (e.g. `claude`, `codex`).
    pub id: String,
    /// What kind of actor the producer claims to be.
    pub kind: ProducerKind,
}

/// The closed set of producer kinds; parsed exactly once at the CLI
/// boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProducerKind {
    /// A coding agent (Claude, Codex, …).
    Agent,
    /// A human operator.
    Operator,
    /// The factory control plane itself.
    Factory,
}

impl FromStr for ProducerKind {
    type Err = Error;

    fn from_str(s: &str) -> Result<ProducerKind, Error> {
        match s {
            // ast-grep-ignore: no-string-literal-dispatch -- parse boundary, converts once
            "agent" => Ok(ProducerKind::Agent),
            // ast-grep-ignore: no-string-literal-dispatch -- parse boundary, converts once
            "operator" => Ok(ProducerKind::Operator),
            // ast-grep-ignore: no-string-literal-dispatch -- parse boundary, converts once
            "factory" => Ok(ProducerKind::Factory),
            other => Err(Error::UnknownProducerKind {
                requested: other.to_owned(),
            }),
        }
    }
}

impl std::fmt::Display for ProducerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            ProducerKind::Agent => "agent",
            ProducerKind::Operator => "operator",
            ProducerKind::Factory => "factory",
        };
        f.write_str(name)
    }
}

/// The closed S1 entry vocabulary (milestone Q1). Grows only when a later
/// slice's behavior demands a variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    /// The Card's opening entry, always at sequence 1.
    CardCreated,
    /// A workflow decision with its reason.
    Decision,
}

impl FromStr for EntryType {
    type Err = Error;

    fn from_str(s: &str) -> Result<EntryType, Error> {
        match s {
            // ast-grep-ignore: no-string-literal-dispatch -- parse boundary, converts once
            "card-created" => Ok(EntryType::CardCreated),
            // ast-grep-ignore: no-string-literal-dispatch -- parse boundary, converts once
            "decision" => Ok(EntryType::Decision),
            other => Err(Error::UnknownEntryType {
                requested: other.to_owned(),
            }),
        }
    }
}

impl std::fmt::Display for EntryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            EntryType::CardCreated => "card-created",
            EntryType::Decision => "decision",
        };
        f.write_str(name)
    }
}

/// A validated, typed, versioned entry payload. Constructing one is the
/// validation boundary: a `Payload` in hand is storable (S1-B8).
#[derive(Debug, Clone)]
pub enum Payload {
    /// `card-created` version 1.
    CardCreatedV1(CardCreatedV1),
    /// `decision` version 1.
    DecisionV1(DecisionV1),
}

impl Payload {
    /// Parses raw producer JSON against the claimed entry type and schema
    /// version. No partial acceptance: any defect rejects the whole entry.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownSchemaVersion`] for a version this build cannot
    /// validate; [`Error::MalformedPayload`] when the JSON does not parse
    /// as the claimed type.
    pub fn from_raw(
        entry_type: EntryType,
        schema_version: u32,
        raw: &str,
    ) -> Result<Payload, Error> {
        let malformed = |source| Error::MalformedPayload { entry_type, source };
        match (entry_type, schema_version) {
            (EntryType::CardCreated, CURRENT_SCHEMA_VERSION) => Ok(Payload::CardCreatedV1(
                serde_json::from_str(raw).map_err(malformed)?,
            )),
            (EntryType::Decision, CURRENT_SCHEMA_VERSION) => Ok(Payload::DecisionV1(
                serde_json::from_str(raw).map_err(malformed)?,
            )),
            (_, requested) => Err(Error::UnknownSchemaVersion {
                entry_type,
                requested,
            }),
        }
    }

    /// The entry type this payload belongs to.
    #[must_use]
    pub fn entry_type(&self) -> EntryType {
        match self {
            Payload::CardCreatedV1(_) => EntryType::CardCreated,
            Payload::DecisionV1(_) => EntryType::Decision,
        }
    }

    /// The schema version this payload was validated as.
    #[must_use]
    pub fn schema_version(&self) -> u32 {
        match self {
            Payload::CardCreatedV1(_) | Payload::DecisionV1(_) => CURRENT_SCHEMA_VERSION,
        }
    }

    /// The payload as a JSON value — the shape stored in and read from
    /// the record.
    ///
    /// # Errors
    ///
    /// [`Error::MalformedPayload`] if serialization fails (not expected
    /// for these field types; surfaced rather than fabricated away).
    pub fn to_json(&self) -> Result<serde_json::Value, Error> {
        let result = match self {
            Payload::CardCreatedV1(payload) => serde_json::to_value(payload),
            Payload::DecisionV1(payload) => serde_json::to_value(payload),
        };
        result.map_err(|source| Error::MalformedPayload {
            entry_type: self.entry_type(),
            source,
        })
    }
}

/// The `card-created` payload, version 1: the Card's opening metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CardCreatedV1 {
    /// Human-facing title of the task the Card accompanies.
    pub title: String,
    /// Optional external task key (e.g. `PER-90`); retained metadata,
    /// never identity (S1-B1/B2).
    pub task_key: Option<String>,
    /// Optional repository/workspace reference the Card points at.
    pub workspace: Option<String>,
}

/// The `decision` payload, version 1: a workflow decision and its reason.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionV1 {
    /// What was decided.
    pub summary: String,
    /// Why it was decided.
    pub reason: String,
}

/// One accepted, immutable Card entry with its complete envelope
/// (S1-B5/B11). `recorded_at` is server-assigned at acceptance.
#[derive(Debug, Clone)]
pub struct Entry {
    /// Daemar-assigned entry identity.
    pub entry_id: EntryId,
    /// The Card this entry belongs to.
    pub card_id: CardId,
    /// Per-Card monotonic sequence, assigned by the store; starts at 1,
    /// gapless (S1-B5/B6).
    pub sequence: u64,
    /// Who claimed to produce the entry.
    pub producer: Producer,
    /// Server-assigned acceptance time.
    pub recorded_at: time::OffsetDateTime,
    /// The validated, typed payload; entry type and schema version are
    /// derivable from it.
    pub payload: Payload,
}

/// One row of `card list`, from the cards table.
#[derive(Debug, Clone)]
pub struct CardSummary {
    /// Daemar-assigned Card identity.
    pub card_id: CardId,
    /// Human-facing title.
    pub title: String,
    /// Optional external task key.
    pub task_key: Option<String>,
    /// Optional repository/workspace reference.
    pub workspace: Option<String>,
    /// Server-assigned creation time.
    pub created_at: time::OffsetDateTime,
}

/// A request to open a Card for a real task (S1-B1…B4).
///
/// Overlaps [`CardCreatedV1`] deliberately (C10): this is the request
/// envelope — producer and retry token included — while `CardCreatedV1`
/// is the durable payload the store derives from it at sequence 1.
#[derive(Debug)]
pub struct CreateCard {
    /// Human-facing title of the task.
    pub title: String,
    /// Optional external task key; absent stays absent (S1-B2).
    pub task_key: Option<String>,
    /// Optional repository/workspace reference.
    pub workspace: Option<String>,
    /// Who is opening the Card.
    pub producer: Producer,
    /// Optional retry token (S1-B3/B4).
    pub idempotency_key: Option<String>,
}

/// A request to append one validated entry (S1-B5…B9).
#[derive(Debug)]
pub struct AppendEntry {
    /// The Card to append to.
    pub card_id: CardId,
    /// The validated payload (see [`Payload::from_raw`]).
    pub payload: Payload,
    /// Who is appending.
    pub producer: Producer,
    /// Optional retry token (S1-B7).
    pub idempotency_key: Option<String>,
}

/// What an accepted — or idempotently replayed — append returns (S1-B5/B7).
#[derive(Debug, Clone)]
pub struct Accepted {
    /// The entry's Daemar-assigned identity.
    pub entry_id: EntryId,
    /// The entry's per-Card sequence.
    pub sequence: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCategory;

    fn malformed_source() -> serde_json::Error {
        match serde_json::from_str::<serde_json::Value>("not json") {
            Err(source) => source,
            Ok(_) => unreachable!("`not json` must not parse"),
        }
    }

    /// S1-B12: every failure variant maps to its contract category.
    #[test]
    fn every_error_variant_maps_to_its_category() {
        let cases = [
            (
                Error::UnknownEntryType {
                    requested: "gizmo".to_owned(),
                },
                ErrorCategory::Validation,
            ),
            (
                Error::UnknownProducerKind {
                    requested: "ghost".to_owned(),
                },
                ErrorCategory::Validation,
            ),
            (
                Error::UnknownSchemaVersion {
                    entry_type: EntryType::Decision,
                    requested: 999,
                },
                ErrorCategory::Validation,
            ),
            (
                Error::MalformedPayload {
                    entry_type: EntryType::Decision,
                    source: malformed_source(),
                },
                ErrorCategory::Validation,
            ),
            (Error::MissingProducer, ErrorCategory::Validation),
            (
                Error::IdempotencyConflict {
                    key: "k".to_owned(),
                },
                ErrorCategory::Conflict,
            ),
            (
                Error::CardNotFound {
                    card_id: CardId::from("card".to_owned()),
                },
                ErrorCategory::Missing,
            ),
            (
                Error::Corrupt {
                    context: crate::error::StorageContext::ReadHistory,
                },
                ErrorCategory::Storage,
            ),
            (
                Error::Storage {
                    context: crate::error::StorageContext::Open,
                    source: sqlx::Error::RowNotFound,
                },
                ErrorCategory::Storage,
            ),
        ];
        for (error, category) in cases {
            assert_eq!(error.category(), category, "for {error:?}");
        }
    }

    /// The storage read path depends on `Display` -> `FromStr` round-tripping
    /// exactly, including the `factory` kind no behavior scenario uses.
    #[test]
    fn producer_kind_round_trips() {
        for kind in [
            ProducerKind::Agent,
            ProducerKind::Operator,
            ProducerKind::Factory,
        ] {
            let parsed = ProducerKind::from_str(&kind.to_string()).unwrap();
            assert_eq!(parsed, kind);
        }
        assert!(matches!(
            ProducerKind::from_str("ghost"),
            Err(Error::UnknownProducerKind { requested }) if requested == "ghost"
        ));
    }

    #[test]
    fn entry_type_round_trips() {
        for entry_type in [EntryType::CardCreated, EntryType::Decision] {
            let parsed = EntryType::from_str(&entry_type.to_string()).unwrap();
            assert_eq!(parsed, entry_type);
        }
        assert!(matches!(
            EntryType::from_str("gizmo"),
            Err(Error::UnknownEntryType { requested }) if requested == "gizmo"
        ));
    }

    /// S1-B8 below behavior granularity: input shapes the CLI scenarios
    /// cannot cheaply enumerate.
    #[test]
    fn from_raw_rejects_text_that_is_not_json() {
        assert!(matches!(
            Payload::from_raw(EntryType::Decision, CURRENT_SCHEMA_VERSION, "not json"),
            Err(Error::MalformedPayload {
                entry_type: EntryType::Decision,
                ..
            })
        ));
    }

    #[test]
    fn from_raw_rejects_unknown_fields() {
        let raw = r#"{"summary":"s","reason":"r","confidence":"high"}"#;
        assert!(matches!(
            Payload::from_raw(EntryType::Decision, CURRENT_SCHEMA_VERSION, raw),
            Err(Error::MalformedPayload { .. })
        ));
    }

    #[test]
    fn from_raw_rejects_wrong_json_shape() {
        assert!(matches!(
            Payload::from_raw(EntryType::Decision, CURRENT_SCHEMA_VERSION, "[1,2]"),
            Err(Error::MalformedPayload { .. })
        ));
    }

    #[test]
    fn from_raw_reports_the_requested_unknown_version() {
        assert!(matches!(
            Payload::from_raw(EntryType::Decision, 999, r#"{"summary":"s","reason":"r"}"#),
            Err(Error::UnknownSchemaVersion {
                entry_type: EntryType::Decision,
                requested: 999
            })
        ));
    }

    #[test]
    fn from_raw_parses_card_created_with_absent_references() {
        let raw = r#"{"title":"t","task_key":null,"workspace":null}"#;
        let payload =
            Payload::from_raw(EntryType::CardCreated, CURRENT_SCHEMA_VERSION, raw).unwrap();
        assert_eq!(payload.entry_type(), EntryType::CardCreated);
        assert_eq!(payload.schema_version(), CURRENT_SCHEMA_VERSION);
        match payload {
            Payload::CardCreatedV1(created) => {
                assert_eq!(created.title, "t");
                assert_eq!(created.task_key, None);
                assert_eq!(created.workspace, None);
            }
            Payload::DecisionV1(_) => panic!("parsed as the wrong variant"),
        }
    }
}
