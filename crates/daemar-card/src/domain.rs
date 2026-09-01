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
    /// A free-form record of what a stage did.
    StageEvent,
}

impl FromStr for EntryType {
    type Err = Error;

    fn from_str(s: &str) -> Result<EntryType, Error> {
        match s {
            // ast-grep-ignore: no-string-literal-dispatch -- parse boundary, converts once
            "card-created" => Ok(EntryType::CardCreated),
            // ast-grep-ignore: no-string-literal-dispatch -- parse boundary, converts once
            "decision" => Ok(EntryType::Decision),
            // ast-grep-ignore: no-string-literal-dispatch -- parse boundary, converts once
            "stage-event" => Ok(EntryType::StageEvent),
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
            EntryType::StageEvent => "stage-event",
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
    /// `stage-event` version 1.
    StageEventV1(StageEventV1),
}

impl Payload {
    /// Parses raw producer JSON against the claimed entry type and schema
    /// version. No partial acceptance: any defect rejects the whole entry.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownSchemaVersion`] for a version this build cannot
    /// validate; [`Error::BlankField`] when required text is empty or
    /// whitespace-only; [`Error::MalformedPayload`] when the JSON does not parse
    /// as the claimed type.
    pub fn from_raw(
        entry_type: EntryType,
        schema_version: u32,
        raw: &str,
    ) -> Result<Payload, Error> {
        let malformed = |source| Error::MalformedPayload { entry_type, source };
        let payload = match (entry_type, schema_version) {
            (EntryType::CardCreated, CURRENT_SCHEMA_VERSION) => {
                Payload::CardCreatedV1(serde_json::from_str(raw).map_err(malformed)?)
            }
            (EntryType::Decision, CURRENT_SCHEMA_VERSION) => {
                Payload::DecisionV1(serde_json::from_str(raw).map_err(malformed)?)
            }
            (EntryType::StageEvent, CURRENT_SCHEMA_VERSION) => {
                let envelope = parse_stage_event_envelope(raw, entry_type)?;
                Payload::StageEventV1(StageEventV1::validated(
                    envelope.stage,
                    envelope.summary,
                    envelope.payload,
                )?)
            }
            (_, requested) => {
                return Err(Error::UnknownSchemaVersion {
                    entry_type,
                    requested,
                })
            }
        };
        payload.validate()?;
        Ok(payload)
    }

    /// Constructs a stage-event payload from CLI ingest components, enforcing
    /// all stage-event validation rules.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownSchemaVersion`] if the version is not [`CURRENT_SCHEMA_VERSION`];
    /// [`Error::BlankField`] if stage or summary is empty/whitespace-only;
    /// [`Error::NotSingleLine`] if summary contains CR or LF;
    /// [`Error::MalformedPayload`] if `raw_payload` is present but not a JSON object
    /// or contains duplicate members; [`Error::DuplicateJsonMember`] if the payload
    /// contains the same member name twice at any nesting depth.
    pub fn stage_event_from_parts(
        schema_version: u32,
        stage: String,
        summary: String,
        raw_payload: Option<&str>,
    ) -> Result<Payload, Error> {
        if schema_version != CURRENT_SCHEMA_VERSION {
            return Err(Error::UnknownSchemaVersion {
                entry_type: EntryType::StageEvent,
                requested: schema_version,
            });
        }

        let payload = match raw_payload {
            Some(raw) => {
                reject_duplicate_members(raw)?;
                // Deserialize directly to Map - any non-object JSON yields a genuine error.
                let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(raw)
                    .map_err(|source| Error::MalformedPayload {
                        entry_type: EntryType::StageEvent,
                        source,
                    })?;
                Some(map)
            }
            None => None,
        };

        Ok(Payload::StageEventV1(StageEventV1::validated(
            stage, summary, payload,
        )?))
    }

    /// Required workflow content must not be blank (deep-review finding 2).
    /// Optional external references stay exempt.
    pub(crate) fn validate(&self) -> Result<(), Error> {
        match self {
            Payload::CardCreatedV1(created) => require_text(&created.title, "title"),
            Payload::DecisionV1(decision) => {
                require_text(&decision.summary, "decision summary")?;
                require_text(&decision.reason, "decision reason")
            }
            Payload::StageEventV1(event) => {
                validate_stage_event_fields(&event.stage, &event.summary)
            }
        }
    }

    /// The entry type this payload belongs to.
    #[must_use]
    pub fn entry_type(&self) -> EntryType {
        match self {
            Payload::CardCreatedV1(_) => EntryType::CardCreated,
            Payload::DecisionV1(_) => EntryType::Decision,
            Payload::StageEventV1(_) => EntryType::StageEvent,
        }
    }

    /// The schema version this payload was validated as.
    #[must_use]
    pub fn schema_version(&self) -> u32 {
        match self {
            Payload::CardCreatedV1(_) | Payload::DecisionV1(_) | Payload::StageEventV1(_) => {
                CURRENT_SCHEMA_VERSION
            }
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
            Payload::StageEventV1(payload) => serde_json::to_value(payload),
        };
        result.map_err(|source| Error::MalformedPayload {
            entry_type: self.entry_type(),
            source,
        })
    }

    /// Returns the entry-level members this payload contributes to a `card history`
    /// response. S1 types contribute the existing {\"payload\": ...} shape;
    /// stage-event contributes stage, summary, and optional payload at the entry level.
    ///
    /// # Errors
    ///
    /// [`Error::MalformedPayload`] if serialization fails.
    pub fn history_fields(&self) -> Result<serde_json::Map<String, serde_json::Value>, Error> {
        let mut map = serde_json::Map::new();
        match self {
            Payload::CardCreatedV1(_) | Payload::DecisionV1(_) => {
                map.insert("payload".to_owned(), self.to_json()?);
            }
            Payload::StageEventV1(event) => {
                map.insert(
                    "stage".to_owned(),
                    serde_json::Value::String(event.stage.clone()),
                );
                map.insert(
                    "summary".to_owned(),
                    serde_json::Value::String(event.summary.clone()),
                );
                if let Some(payload) = &event.payload {
                    let value = serde_json::to_value(payload).map_err(|source| {
                        Error::MalformedPayload {
                            entry_type: self.entry_type(),
                            source,
                        }
                    })?;
                    map.insert("payload".to_owned(), value);
                }
            }
        }
        Ok(map)
    }
}

/// R2: Traversal probe for finding duplicate JSON member names.
/// Walks any well-formed JSON and reports the first duplicate member name found at any depth.
/// The finding is carried in the success value, not in serde's error channel.
#[derive(Debug)]
struct DuplicateProbe {
    /// The name of the first duplicate member found, or None if no duplicates exist.
    duplicate_found: Option<String>,
}

impl<'de> serde::de::Deserialize<'de> for DuplicateProbe {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        deserializer.deserialize_any(DuplicateProbeVisitor)
    }
}

struct DuplicateProbeVisitor;

impl<'de> serde::de::Visitor<'de> for DuplicateProbeVisitor {
    type Value = DuplicateProbe;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("any JSON value")
    }

    fn visit_bool<E>(self, _v: bool) -> Result<DuplicateProbe, E>
    where
        E: serde::de::Error,
    {
        Ok(DuplicateProbe {
            duplicate_found: None,
        })
    }

    fn visit_i64<E>(self, _v: i64) -> Result<DuplicateProbe, E>
    where
        E: serde::de::Error,
    {
        Ok(DuplicateProbe {
            duplicate_found: None,
        })
    }

    fn visit_u64<E>(self, _v: u64) -> Result<DuplicateProbe, E>
    where
        E: serde::de::Error,
    {
        Ok(DuplicateProbe {
            duplicate_found: None,
        })
    }

    fn visit_f64<E>(self, _v: f64) -> Result<DuplicateProbe, E>
    where
        E: serde::de::Error,
    {
        Ok(DuplicateProbe {
            duplicate_found: None,
        })
    }

    fn visit_str<E>(self, _v: &str) -> Result<DuplicateProbe, E>
    where
        E: serde::de::Error,
    {
        Ok(DuplicateProbe {
            duplicate_found: None,
        })
    }

    fn visit_borrowed_str<E>(self, _v: &'de str) -> Result<DuplicateProbe, E>
    where
        E: serde::de::Error,
    {
        Ok(DuplicateProbe {
            duplicate_found: None,
        })
    }

    fn visit_string<E>(self, _v: String) -> Result<DuplicateProbe, E>
    where
        E: serde::de::Error,
    {
        Ok(DuplicateProbe {
            duplicate_found: None,
        })
    }

    fn visit_none<E>(self) -> Result<DuplicateProbe, E>
    where
        E: serde::de::Error,
    {
        Ok(DuplicateProbe {
            duplicate_found: None,
        })
    }

    fn visit_some<D>(self, deserializer: D) -> Result<DuplicateProbe, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        serde::de::Deserialize::deserialize(deserializer)
    }

    fn visit_unit<E>(self) -> Result<DuplicateProbe, E>
    where
        E: serde::de::Error,
    {
        Ok(DuplicateProbe {
            duplicate_found: None,
        })
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<DuplicateProbe, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        // Drain every element and check each for duplicates (MID: must drain all, not short-circuit)
        let mut finding: Option<String> = None;
        while let Some(probe) = seq.next_element::<DuplicateProbe>()? {
            if finding.is_none() {
                if let Some(name) = probe.duplicate_found {
                    finding = Some(name);
                }
            }
        }
        Ok(DuplicateProbe {
            duplicate_found: finding,
        })
    }

    fn visit_map<A>(self, mut map: A) -> Result<DuplicateProbe, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut seen_keys = std::collections::HashSet::new();
        let mut finding: Option<String> = None;

        while let Some((key, probe)) = map.next_entry::<String, DuplicateProbe>()? {
            // Must drain all entries even after finding a duplicate (MID: R3a requirement)
            if finding.is_none() {
                // Check if this key was already seen in this object (C11: avoid clone on non-duplicate path)
                if seen_keys.contains(&key) {
                    // Duplicate found at this level
                    finding = Some(key);
                } else {
                    seen_keys.insert(key);

                    // Check if a nested structure found a duplicate
                    if let Some(name) = probe.duplicate_found {
                        finding = Some(name);
                    }
                }
            }
        }

        Ok(DuplicateProbe {
            duplicate_found: finding,
        })
    }
}

/// Strict envelope structure for parsing stage-event validation (R5).
/// Used only to obtain genuine `serde_json::Error` values when envelope validation fails.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictEnvelope {
    stage: String,
    summary: String,
    #[serde(default)]
    payload: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Parse and validate stage-event envelope, rejecting explicit null payload (C16).
fn parse_stage_event_envelope(raw: &str, entry_type: EntryType) -> Result<StrictEnvelope, Error> {
    reject_duplicate_members(raw)?;

    // Parse to Value to check for explicit null payload (which must be rejected)
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|source| Error::MalformedPayload { entry_type, source })?;

    // Check for explicit null payload member (violates R5)
    if let serde_json::Value::Object(ref obj) = value {
        if let Some(serde_json::Value::Null) = obj.get("payload") {
            // Get a genuine error from trying to deserialize the actual null member value as a Map
            let null_str = serde_json::to_string(&serde_json::Value::Null)
                .unwrap_or_else(|_| "null".to_string());
            if let Err(source) =
                serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&null_str)
            {
                return Err(Error::MalformedPayload { entry_type, source });
            }
        }
    }

    // Attempt strict parse to catch unknown members and validate structure
    serde_json::from_value(value).map_err(|source| Error::MalformedPayload { entry_type, source })
}

/// Rejects JSON objects that carry the same member name twice at any nesting depth.
/// Malformed JSON is not this function's business; it returns Ok and lets
/// downstream parsing handle JSON validity.
fn reject_duplicate_members(raw: &str) -> Result<(), Error> {
    match serde_json::from_str::<DuplicateProbe>(raw) {
        Ok(probe) => {
            if let Some(name) = probe.duplicate_found {
                Err(Error::DuplicateJsonMember { name })
            } else {
                Ok(())
            }
        }
        Err(_) => {
            // Malformed JSON is not this function's business; return Ok
            // and let downstream parsing handle JSON validity.
            Ok(())
        }
    }
}

/// Rejects empty or whitespace-only required text (S1 blank-field rule).
pub(crate) fn require_text(value: &str, field: &'static str) -> Result<(), Error> {
    if value.trim().is_empty() {
        Err(Error::BlankField { field })
    } else {
        Ok(())
    }
}

/// Validate stage-event fields: stage and summary non-blank, summary single-line (C10/C14).
fn validate_stage_event_fields(stage: &str, summary: &str) -> Result<(), Error> {
    require_text(stage, "stage")?;
    require_text(summary, "summary")?;
    if summary.contains('\r') || summary.contains('\n') {
        return Err(Error::NotSingleLine { field: "summary" });
    }
    Ok(())
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

/// The `stage-event` payload, version 1: a free-form record of what a stage did.
#[derive(Debug, Clone, Serialize)]
pub struct StageEventV1 {
    /// Producer-chosen free-form stage name, non-blank, never judged.
    stage: String,
    /// One-line summary of what happened, non-blank, no CR or LF.
    summary: String,
    /// Optional producer-submitted JSON object payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<serde_json::Map<String, serde_json::Value>>,
}

impl StageEventV1 {
    /// Validates and constructs a `StageEventV1`, enforcing all field rules.
    ///
    /// # Errors
    ///
    /// [`Error::BlankField`] if stage or summary is empty/whitespace-only;
    /// [`Error::NotSingleLine`] if summary contains CR or LF.
    fn validated(
        stage: String,
        summary: String,
        payload: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<StageEventV1, Error> {
        validate_stage_event_fields(&stage, &summary)?;
        Ok(StageEventV1 {
            stage,
            summary,
            payload,
        })
    }
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
                Error::BlankField { field: "title" },
                ErrorCategory::Validation,
            ),
            (
                Error::NotAppendable {
                    entry_type: EntryType::CardCreated,
                },
                ErrorCategory::Validation,
            ),
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
            (
                Error::NotSingleLine { field: "summary" },
                ErrorCategory::Validation,
            ),
            (
                Error::DuplicateJsonMember {
                    name: "count".to_owned(),
                },
                ErrorCategory::Validation,
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
        for entry_type in [
            EntryType::CardCreated,
            EntryType::Decision,
            EntryType::StageEvent,
        ] {
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

    /// Deep review: required workflow content cannot be empty or
    /// whitespace-only; optional references stay exempt.
    #[test]
    fn from_raw_rejects_blank_required_fields() {
        let blank_decisions = [
            r#"{"summary":"","reason":"r"}"#,
            r#"{"summary":"   ","reason":"r"}"#,
            r#"{"summary":"s","reason":""}"#,
            r#"{"summary":"s","reason":"\t "}"#,
        ];
        for raw in blank_decisions {
            assert!(
                matches!(
                    Payload::from_raw(EntryType::Decision, CURRENT_SCHEMA_VERSION, raw),
                    Err(Error::BlankField { .. })
                ),
                "expected BlankField for {raw}"
            );
        }
        assert!(matches!(
            Payload::from_raw(
                EntryType::CardCreated,
                CURRENT_SCHEMA_VERSION,
                r#"{"title":"  ","task_key":null,"workspace":null}"#,
            ),
            Err(Error::BlankField { field: "title" })
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
            Payload::DecisionV1(_) | Payload::StageEventV1(_) => {
                panic!("parsed as the wrong variant")
            }
        }
    }

    // R1: Unit tests for stage-event duplicate detection and envelope validation

    /// T1: duplicate members at top level
    #[test]
    fn stage_event_from_parts_rejects_duplicate_at_top_level() {
        let result = Payload::stage_event_from_parts(
            CURRENT_SCHEMA_VERSION,
            "stage".to_string(),
            "summary".to_string(),
            Some(r#"{"count": 1, "count": 2}"#),
        );
        assert!(matches!(
            result,
            Err(Error::DuplicateJsonMember { name }) if name == "count"
        ));
    }

    /// T2: duplicate members nested deep
    #[test]
    fn stage_event_from_parts_rejects_duplicate_nested_deep() {
        let result = Payload::stage_event_from_parts(
            CURRENT_SCHEMA_VERSION,
            "stage".to_string(),
            "summary".to_string(),
            Some(r#"{"outer": {"mid": {"count": 1, "count": 2}}}"#),
        );
        assert!(matches!(
            result,
            Err(Error::DuplicateJsonMember { name }) if name == "count"
        ));
    }

    /// T3: duplicate inside an array element
    #[test]
    fn stage_event_from_parts_rejects_duplicate_in_array_element() {
        let result = Payload::stage_event_from_parts(
            CURRENT_SCHEMA_VERSION,
            "stage".to_string(),
            "summary".to_string(),
            Some(r#"{"items": [{"a": 1, "a": 2}]}"#),
        );
        assert!(matches!(
            result,
            Err(Error::DuplicateJsonMember { name }) if name == "a"
        ));
    }

    /// T4: duplicate in nested payload within an envelope
    #[test]
    fn from_raw_stage_event_rejects_duplicate_in_payload() {
        let envelope = r#"{"stage":"s","summary":"sum","payload":{"nested":{"dup":1,"dup":2}}}"#;
        let result = Payload::from_raw(EntryType::StageEvent, CURRENT_SCHEMA_VERSION, envelope);
        assert!(matches!(result, Err(Error::DuplicateJsonMember { .. })));
    }

    /// T5: duplicate at envelope level
    #[test]
    fn from_raw_stage_event_rejects_duplicate_envelope_member() {
        let envelope = r#"{"stage":"s","summary":"sum","summary":"sum2"}"#;
        let result = Payload::from_raw(EntryType::StageEvent, CURRENT_SCHEMA_VERSION, envelope);
        assert!(matches!(result, Err(Error::DuplicateJsonMember { .. })));
    }

    /// T6: non-object payloads (array, string, number, boolean, null)
    #[test]
    fn stage_event_from_parts_rejects_non_object_payloads() {
        let non_objects = [
            ("[1,2,3]", "array"),
            ("\"just a string\"", "string"),
            ("7", "number"),
            ("true", "boolean"),
            ("null", "null"),
        ];

        for (payload_str, desc) in non_objects {
            let result = Payload::stage_event_from_parts(
                CURRENT_SCHEMA_VERSION,
                "stage".to_string(),
                "summary".to_string(),
                Some(payload_str),
            );
            assert!(
                matches!(
                    result,
                    Err(Error::MalformedPayload {
                        entry_type: EntryType::StageEvent,
                        ..
                    })
                ),
                "expected MalformedPayload for {desc}"
            );
        }
    }

    /// T7: `from_raw` rejects unknown member in envelope
    #[test]
    fn from_raw_stage_event_rejects_unknown_envelope_member() {
        let envelope = r#"{"stage":"s","summary":"sum","duration":"10s"}"#;
        let result = Payload::from_raw(EntryType::StageEvent, CURRENT_SCHEMA_VERSION, envelope);
        assert!(matches!(result, Err(Error::MalformedPayload { .. })));
    }

    /// T8: `from_raw` rejects explicit null payload member
    #[test]
    fn from_raw_stage_event_rejects_explicit_null_payload() {
        let envelope = r#"{"stage":"s","summary":"sum","payload":null}"#;
        let result = Payload::from_raw(EntryType::StageEvent, CURRENT_SCHEMA_VERSION, envelope);
        assert!(matches!(result, Err(Error::MalformedPayload { .. })));

        // absent payload member is valid
        let envelope_valid = r#"{"stage":"s","summary":"sum"}"#;
        let result_valid = Payload::from_raw(
            EntryType::StageEvent,
            CURRENT_SCHEMA_VERSION,
            envelope_valid,
        );
        assert!(result_valid.is_ok());
    }

    /// T9: `from_raw` rejects non-string stage field
    #[test]
    fn from_raw_stage_event_rejects_non_string_stage() {
        let envelope = r#"{"stage":7,"summary":"sum"}"#;
        let result = Payload::from_raw(EntryType::StageEvent, CURRENT_SCHEMA_VERSION, envelope);
        assert!(matches!(result, Err(Error::MalformedPayload { .. })));
    }

    /// T10: summary with CR or LF in non-empty text rejects as `NotSingleLine`
    #[test]
    fn stage_event_from_parts_rejects_cr_or_lf_in_summary() {
        // text with embedded CR
        let result_cr = Payload::stage_event_from_parts(
            CURRENT_SCHEMA_VERSION,
            "stage".to_string(),
            "text\rwith cr".to_string(),
            None,
        );
        assert!(matches!(
            result_cr,
            Err(Error::NotSingleLine { field: "summary" })
        ));

        // text with embedded LF
        let result_lf = Payload::stage_event_from_parts(
            CURRENT_SCHEMA_VERSION,
            "stage".to_string(),
            "text\nwith lf".to_string(),
            None,
        );
        assert!(matches!(
            result_lf,
            Err(Error::NotSingleLine { field: "summary" })
        ));
    }

    /// T11: `history_fields` for a decision returns exactly `{"payload": ...}`
    #[test]
    fn history_fields_decision_format() {
        let payload = Payload::DecisionV1(DecisionV1 {
            summary: "decided".to_string(),
            reason: "because".to_string(),
        });
        let fields = payload.history_fields().unwrap();
        assert_eq!(fields.len(), 1, "decision should have exactly one key");
        assert!(
            fields.contains_key("payload"),
            "decision should have payload key"
        );
    }

    /// T12: `history_fields` for stage event without payload omits the payload key
    #[test]
    fn history_fields_stage_event_no_payload() {
        let payload = Payload::StageEventV1(StageEventV1 {
            stage: "test".to_string(),
            summary: "summary".to_string(),
            payload: None,
        });
        let fields = payload.history_fields().unwrap();
        assert!(
            fields.contains_key("stage"),
            "stage event should have stage key"
        );
        assert!(
            fields.contains_key("summary"),
            "stage event should have summary key"
        );
        assert!(
            !fields.contains_key("payload"),
            "stage event without payload should NOT have payload key"
        );
    }

    /// R2: Traversal probe accepts any well-formed JSON value
    #[test]
    fn duplicate_probe_traverses_well_formed_json() {
        let test_cases = [
            ("{}", "empty object"),
            ("{\"a\":1}", "simple object"),
            ("{\"outer\":{\"inner\":{\"deep\":true}}}", "nested object"),
            ("[1,2,3]", "array"),
            (r#"[{"a":1},{"b":2}]"#, "array of objects"),
            ("null", "null"),
            ("true", "boolean"),
            ("42", "number"),
            ("\"string\"", "string"),
            (
                r#"{"items":[{"x":1,"y":2},{"x":3,"y":4}],"nested":{"a":[true,false,null]}}"#,
                "complex mixed structure",
            ),
        ];

        for (json_str, desc) in test_cases {
            let result = serde_json::from_str::<DuplicateProbe>(json_str);
            assert!(
                result.is_ok(),
                "failed to traverse {}: {}",
                desc,
                result.unwrap_err()
            );
        }
    }

    /// T13: nested object payload is accepted and preserved through `stage_event_from_parts`
    #[test]
    fn stage_event_nested_payload_round_trip() {
        let payload_str = r#"{"outer":{"inner":{"value":42}}}"#;
        let result = Payload::stage_event_from_parts(
            CURRENT_SCHEMA_VERSION,
            "test".to_string(),
            "summary".to_string(),
            Some(payload_str),
        );
        assert!(result.is_ok());
        let payload = result.unwrap();
        let json = payload.to_json().unwrap();
        // Verify the payload portion is preserved in the serialized form
        if let serde_json::Value::Object(map) = json {
            assert!(map.contains_key("payload"));
            if let Some(payload_value) = map.get("payload") {
                let expected: serde_json::Value = serde_json::from_str(payload_str).unwrap();
                assert_eq!(payload_value, &expected);
            } else {
                panic!("payload key not found after assert");
            }
        } else {
            panic!("to_json should return an object");
        }
    }

    /// MID regression: duplicate followed by later sibling at top level must be detected
    /// even though the entry that follows the duplicate is not drained (short-circuit bug).
    #[test]
    fn stage_event_from_parts_duplicate_with_later_sibling_top_level() {
        let result = Payload::stage_event_from_parts(
            CURRENT_SCHEMA_VERSION,
            "stage".to_string(),
            "summary".to_string(),
            Some(r#"{"dup":1,"dup":2,"later":"field"}"#),
        );
        assert!(matches!(
            result,
            Err(Error::DuplicateJsonMember { name }) if name == "dup"
        ));
    }

    /// MID regression: nested duplicate with later sibling after the nested object
    /// must be detected even though siblings are not drained.
    #[test]
    fn stage_event_from_parts_duplicate_nested_with_later_sibling() {
        let result = Payload::stage_event_from_parts(
            CURRENT_SCHEMA_VERSION,
            "stage".to_string(),
            "summary".to_string(),
            Some(r#"{"nested":{"dup":1,"dup":2},"later":"field"}"#),
        );
        assert!(matches!(
            result,
            Err(Error::DuplicateJsonMember { name }) if name == "dup"
        ));
    }

    /// MID regression: duplicate inside array element followed by later elements
    /// and top-level member must be detected even though the rest is not drained.
    #[test]
    fn stage_event_from_parts_duplicate_array_with_later_members() {
        let result = Payload::stage_event_from_parts(
            CURRENT_SCHEMA_VERSION,
            "stage".to_string(),
            "summary".to_string(),
            Some(r#"{"items":[{"dup":1,"dup":2},{"other":3}],"later":"field"}"#),
        );
        assert!(matches!(
            result,
            Err(Error::DuplicateJsonMember { name }) if name == "dup"
        ));
    }
}
