//! The crate's failure contract (C1/C2/C3): one explicit enum, hand-written
//! `Display`/`Error` impls, and a stable machine-facing category taxonomy.

use crate::domain::{CardId, EntryId, EntryType, SchemaIncompatibility};
use std::path::PathBuf;

/// Every way the Card store can refuse or fail (S1-B8/B12).
///
/// `category()` is the stable five-way taxonomy agents branch on; the
/// variants carry the typed detail.
#[derive(Debug)]
pub enum Error {
    /// The requested database file is absent.
    DatabaseMissing {
        /// The absent path.
        path: PathBuf,
    },
    /// The database migration history does not match this build.
    SchemaIncompatible {
        /// The refused path.
        path: PathBuf,
        /// The mismatch reason.
        reason: SchemaIncompatibility,
    },
    /// Binding the loopback socket failed.
    Bind {
        /// The requested address.
        addr: crate::console::LoopbackAddr,
        /// The operating-system failure.
        source: std::io::Error,
    },
    /// Publishing the startup line failed.
    PublishStartup {
        /// The write or flush failure.
        source: std::io::Error,
    },
    /// The accept loop failed.
    Serve {
        /// The operating-system failure.
        source: std::io::Error,
    },
    /// The entry is not a member of the addressed Card.
    EntryNotFound {
        /// The addressed Card.
        card_id: CardId,
        /// The missing entry.
        entry_id: EntryId,
    },
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
    /// A summary field contained a CR or LF; rejected whole, never normalized.
    NotSingleLine {
        /// Which field contained the line break.
        field: &'static str,
    },
    /// A JSON object carried the same member name twice at any nesting depth.
    DuplicateJsonMember {
        /// The offending member name.
        name: String,
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
    /// Opening an existing database read-only.
    OpenReadOnly,
    /// Verifying migration history.
    VerifySchema,
    /// Reading the queue.
    Queue,
    /// Reading one entry.
    ReadEntry,
}

/// The five-way failure taxonomy of the machine-facing contract (S1-B12).
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
    /// An unavailable external resource.
    Unavailable,
}

impl Error {
    /// The stable category agents branch on (S1-B12).
    #[must_use]
    pub fn category(&self) -> ErrorCategory {
        match self {
            Error::DatabaseMissing { .. }
            | Error::EntryNotFound { .. }
            | Error::CardNotFound { .. } => ErrorCategory::Missing,
            Error::SchemaIncompatible { .. }
            | Error::Bind { .. }
            | Error::PublishStartup { .. }
            | Error::Serve { .. } => ErrorCategory::Unavailable,
            Error::UnknownEntryType { .. }
            | Error::UnknownProducerKind { .. }
            | Error::UnknownSchemaVersion { .. }
            | Error::MalformedPayload { .. }
            | Error::MissingProducer
            | Error::BlankField { .. }
            | Error::NotAppendable { .. }
            | Error::NotSingleLine { .. }
            | Error::DuplicateJsonMember { .. } => ErrorCategory::Validation,
            Error::IdempotencyConflict { .. } => ErrorCategory::Conflict,
            Error::Corrupt { .. } | Error::Storage { .. } => ErrorCategory::Storage,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::DatabaseMissing { path } => write!(f, "no database at {}", path.display()),
            Error::SchemaIncompatible { path, reason } => {
                write!(f, "database {} schema is {reason}", path.display())
            }
            Error::Bind { addr, .. } => write!(f, "cannot bind {addr}"),
            Error::PublishStartup { .. } => f.write_str("startup line could not be published"),
            Error::Serve { .. } => f.write_str("console server failed"),
            Error::EntryNotFound { card_id, entry_id } => {
                write!(f, "no entry `{entry_id}` on Card `{card_id}`")
            }
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
            Error::NotSingleLine { field } => {
                write!(f, "{field} contains a line break (CR or LF)")
            }
            Error::DuplicateJsonMember { name } => {
                write!(
                    f,
                    "json object has duplicate member `{name}` at some nesting depth"
                )
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Bind { source, .. }
            | Error::PublishStartup { source }
            | Error::Serve { source } => Some(source),
            Error::MalformedPayload { source, .. } => Some(source),
            Error::Storage { source, .. } => Some(source),
            Error::DatabaseMissing { .. }
            | Error::SchemaIncompatible { .. }
            | Error::EntryNotFound { .. }
            | Error::UnknownEntryType { .. }
            | Error::UnknownProducerKind { .. }
            | Error::UnknownSchemaVersion { .. }
            | Error::MissingProducer
            | Error::BlankField { .. }
            | Error::NotAppendable { .. }
            | Error::IdempotencyConflict { .. }
            | Error::CardNotFound { .. }
            | Error::Corrupt { .. }
            | Error::NotSingleLine { .. }
            | Error::DuplicateJsonMember { .. } => None,
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
            StorageContext::OpenReadOnly => "open-read-only",
            StorageContext::VerifySchema => "verify-schema",
            StorageContext::Queue => "queue",
            StorageContext::ReadEntry => "read-entry",
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
            ErrorCategory::Unavailable => "unavailable",
        };
        f.write_str(name)
    }
}

#[cfg(test)]
mod restart_b_tests {
    use std::error::Error as _;
    use std::io;
    use std::path::PathBuf;

    use super::{Error, ErrorCategory, StorageContext};
    use crate::console::LoopbackAddr;
    use crate::domain::{CardId, EntryId, EntryType, MigrationVersion, SchemaIncompatibility};

    // Stable port used only for the independent bind-error display fixture.
    const BIND_ERROR_FIXTURE_PORT: u16 = 7331;

    #[test]
    fn new_error_categories_displays_and_sources_are_typed() {
        let path = PathBuf::from("cards.db");
        let missing = Error::DatabaseMissing { path: path.clone() };
        assert_eq!(missing.category(), ErrorCategory::Missing);
        assert_eq!(missing.to_string(), "no database at cards.db");

        let incompatible = Error::SchemaIncompatible {
            path,
            reason: SchemaIncompatibility::Uninitialized,
        };
        assert_eq!(incompatible.category(), ErrorCategory::Unavailable);
        assert_eq!(
            incompatible.to_string(),
            "database cards.db schema is uninitialized"
        );

        assert_eq!(StorageContext::OpenReadOnly.to_string(), "open-read-only");
        assert_eq!(StorageContext::VerifySchema.to_string(), "verify-schema");
        assert_eq!(StorageContext::Queue.to_string(), "queue");
        assert_eq!(StorageContext::ReadEntry.to_string(), "read-entry");
        assert_eq!(ErrorCategory::Unavailable.to_string(), "unavailable");

        let source = io::Error::new(io::ErrorKind::PermissionDenied, "bind sentinel");
        let bind = Error::Bind {
            addr: LoopbackAddr::v4(crate::console::Port::new(BIND_ERROR_FIXTURE_PORT)),
            source,
        };
        assert_eq!(bind.category(), ErrorCategory::Unavailable);
        assert_eq!(bind.to_string(), "cannot bind 127.0.0.1:7331");
        assert_eq!(bind.source().unwrap().to_string(), "bind sentinel");

        let serve = Error::Serve {
            source: io::Error::other("serve sentinel"),
        };
        assert_eq!(serve.category(), ErrorCategory::Unavailable);
        assert_eq!(serve.to_string(), "console server failed");
        assert_eq!(serve.source().unwrap().to_string(), "serve sentinel");

        let entry = Error::EntryNotFound {
            card_id: CardId::from("card-1".to_owned()),
            entry_id: EntryId::from("entry-1".to_owned()),
        };
        assert_eq!(entry.category(), ErrorCategory::Missing);
        assert_eq!(entry.to_string(), "no entry `entry-1` on Card `card-1`");
        assert_eq!(EntryType::Decision.to_string(), "decision");
        assert_eq!(MigrationVersion::new(1).to_string(), "1");
    }
}
