//! The crate's failure contract (C1/C2/C3): one explicit enum, hand-written
//! `Display`/`Error` impls, and a stable machine-facing category taxonomy.

use crate::domain::{CardId, EntryType};

/// Every way the Card store can refuse or fail (S1-B8/B12).
///
/// `category()` is the stable four-way taxonomy agents branch on; the
/// variants carry the typed detail.
#[derive(Debug)]
pub enum Error {
    /// The requested entry type is not in the closed M1 vocabulary.
    UnknownEntryType {
        /// The entry type string as the producer supplied it.
        requested: String,
    },
    /// The requested producer kind is not `agent`, `operator`, or `factory`.
    UnknownProducerKind {
        /// The producer kind string as the producer supplied it.
        requested: String,
    },
    /// The schema version is not one this build can validate for the type.
    UnknownSchemaVersion {
        /// The entry type the payload claimed to be.
        entry_type: EntryType,
        /// The version the producer supplied.
        requested: u32,
    },
    /// The payload JSON did not parse as the claimed type and version.
    MalformedPayload {
        /// The entry type the payload claimed to be.
        entry_type: EntryType,
        /// The underlying JSON error.
        source: serde_json::Error,
    },
    /// The request carried no producer identity or kind; every accepted
    /// entry requires provenance (S1-B5).
    MissingProducer,
    /// A required semantic field was empty or whitespace-only. Required
    /// workflow content and provenance cannot be blank; optional external
    /// references stay `Option` and are exempt.
    BlankField {
        /// Which required field was blank.
        field: &'static str,
    },
    /// The entry type is written by the store itself and cannot be
    /// appended by a producer (`card-created` exists only at sequence 1).
    NotAppendable {
        /// The reserved entry type the producer attempted to append.
        entry_type: EntryType,
    },
    /// An idempotency key was reused with different content (S1-B4).
    IdempotencyConflict {
        /// The reused key.
        key: String,
    },
    /// The addressed Card does not exist.
    CardNotFound {
        /// The identity that failed to resolve.
        card_id: CardId,
    },
    /// A stored value violated the store's own invariants — only possible
    /// through unsupported direct database writes.
    Corrupt {
        /// The operation that read the invalid value.
        context: StorageContext,
    },
    /// The underlying `SQLite` store failed.
    Storage {
        /// The operation that failed.
        context: StorageContext,
        /// The underlying database error.
        source: sqlx::Error,
    },
}

/// Which store operation an error belongs to, typed so callers and
/// reviewers can branch without parsing prose (C3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageContext {
    /// Opening the database and configuring WAL.
    Open,
    /// Running embedded migrations.
    Migrate,
    /// Creating a Card with its card-created entry.
    CreateCard,
    /// Appending an entry.
    AppendEntry,
    /// Reading a Card's history.
    ReadHistory,
    /// Listing Cards.
    ListCards,
}

/// The four-way failure taxonomy of the machine-facing contract (S1-B12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// The request was rejected before touching the record.
    Validation,
    /// An idempotency key was reused with different content.
    Conflict,
    /// The addressed Card does not exist.
    Missing,
    /// The store itself failed.
    Storage,
}

impl Error {
    /// The stable category agents branch on (S1-B12).
    #[must_use]
    pub fn category(&self) -> ErrorCategory {
        match self {
            Error::UnknownEntryType { .. }
            | Error::UnknownProducerKind { .. }
            | Error::UnknownSchemaVersion { .. }
            | Error::MalformedPayload { .. }
            | Error::MissingProducer
            | Error::BlankField { .. }
            | Error::NotAppendable { .. } => ErrorCategory::Validation,
            Error::IdempotencyConflict { .. } => ErrorCategory::Conflict,
            Error::CardNotFound { .. } => ErrorCategory::Missing,
            Error::Corrupt { .. } | Error::Storage { .. } => ErrorCategory::Storage,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::UnknownEntryType { requested } => {
                write!(f, "unknown entry type `{requested}`")
            }
            Error::UnknownProducerKind { requested } => {
                write!(
                    f,
                    "unknown producer kind `{requested}` (expected agent, operator, or factory)"
                )
            }
            Error::UnknownSchemaVersion {
                entry_type,
                requested,
            } => {
                write!(
                    f,
                    "unknown schema version {requested} for entry type `{entry_type}`"
                )
            }
            Error::MalformedPayload { entry_type, .. } => {
                write!(f, "payload does not parse as entry type `{entry_type}`")
            }
            Error::MissingProducer => {
                write!(f, "producer identity and kind are required on every entry")
            }
            Error::BlankField { field } => {
                write!(f, "{field} must not be empty or whitespace-only")
            }
            Error::NotAppendable { entry_type } => {
                write!(
                    f,
                    "entry type `{entry_type}` is written by the store and cannot be appended"
                )
            }
            Error::IdempotencyConflict { key } => {
                write!(
                    f,
                    "idempotency key `{key}` was already used with different content"
                )
            }
            Error::CardNotFound { card_id } => write!(f, "no Card with id `{card_id}`"),
            Error::Corrupt { context } => {
                write!(f, "stored data violated store invariants during {context}")
            }
            Error::Storage { context, .. } => write!(f, "storage failure during {context}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::MalformedPayload { source, .. } => Some(source),
            Error::Storage { source, .. } => Some(source),
            Error::UnknownEntryType { .. }
            | Error::UnknownProducerKind { .. }
            | Error::UnknownSchemaVersion { .. }
            | Error::MissingProducer
            | Error::BlankField { .. }
            | Error::NotAppendable { .. }
            | Error::IdempotencyConflict { .. }
            | Error::CardNotFound { .. }
            | Error::Corrupt { .. } => None,
        }
    }
}

impl std::fmt::Display for StorageContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            StorageContext::Open => "open",
            StorageContext::Migrate => "migrate",
            StorageContext::CreateCard => "create-card",
            StorageContext::AppendEntry => "append-entry",
            StorageContext::ReadHistory => "read-history",
            StorageContext::ListCards => "list-cards",
        };
        f.write_str(name)
    }
}

impl std::fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            ErrorCategory::Validation => "validation",
            ErrorCategory::Conflict => "conflict",
            ErrorCategory::Missing => "missing",
            ErrorCategory::Storage => "storage",
        };
        f.write_str(name)
    }
}
