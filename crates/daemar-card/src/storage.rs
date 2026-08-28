//! The SQLite-backed Card store: append-only writes, per-Card gapless
//! sequences, idempotent retries, and one durable order under concurrent
//! producers (S1-B3/B4/B6/B7/B9/B13).

use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow};
use sqlx::{Row, SqlitePool};

use crate::domain::{
    Accepted, AppendEntry, CardCreatedV1, CardId, CardSummary, CreateCard, Entry, EntryId,
    EntryType, Payload, Producer, ProducerKind, CURRENT_SCHEMA_VERSION,
};
use crate::error::{Error, StorageContext};

/// How long a writer waits on `SQLite`'s write lock before failing.
/// Concurrent producers are the norm (S1-B9) and each append is one small
/// statement, so seconds of patience buys one durable order without a
/// coordinator.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Handle on one factory database. Producers reach `SQLite` only through
/// this type; it has no update or delete surface (S1-B10), and never will.
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    /// Opens (creating if absent) the database at `path`: WAL mode,
    /// foreign keys on, embedded migrations run. An acknowledged write
    /// survives process termination (S1-B13).
    ///
    /// # Errors
    ///
    /// [`Error::Storage`] with [`StorageContext::Open`] or
    /// [`StorageContext::Migrate`].
    pub async fn open(path: &Path) -> Result<Store, Error> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(BUSY_TIMEOUT)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .map_err(|source| Error::Storage {
                context: StorageContext::Open,
                source,
            })?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|source| Error::Storage {
                context: StorageContext::Migrate,
                source: sqlx::Error::from(source),
            })?;
        restrict_database_permissions(path)?;
        Ok(Store { pool })
    }

    /// Creates a Card and appends its card-created entry at sequence 1,
    /// atomically. Replaying an idempotency key with identical fingerprint
    /// returns the original identity; reuse with different fingerprint is a
    /// conflict (S1-B1…B4).
    ///
    /// # Errors
    ///
    /// [`Error::BlankField`], [`Error::IdempotencyConflict`],
    /// [`Error::MalformedPayload`], or [`Error::Storage`].
    pub async fn create_card(&self, request: CreateCard) -> Result<CardId, Error> {
        let context = StorageContext::CreateCard;
        crate::domain::require_text(&request.producer.id, "producer identity")?;
        crate::domain::require_text(&request.title, "title")?;
        let created = CardCreatedV1 {
            title: request.title,
            task_key: request.task_key,
            workspace: request.workspace,
        };
        let payload_value =
            serde_json::to_value(&created).map_err(|source| Error::MalformedPayload {
                entry_type: EntryType::CardCreated,
                source,
            })?;
        let payload_text = payload_value.to_string();
        let fingerprint = canonical_fingerprint(
            EntryType::CardCreated,
            CURRENT_SCHEMA_VERSION,
            &payload_value,
            &request.producer,
        );

        if let Some(key) = &request.idempotency_key {
            if let Some(card_id) = self.replay_create(key, &fingerprint).await? {
                return Ok(card_id);
            }
        }

        let card_id = CardId::generate();
        let inserted = self
            .insert_card(
                &card_id,
                &created,
                &payload_text,
                &request.producer,
                request.idempotency_key.as_deref(),
                &fingerprint,
            )
            .await;
        match inserted {
            Ok(()) => Ok(card_id),
            Err(source) if is_unique_violation(&source) => match &request.idempotency_key {
                Some(key) => self
                    .replay_create(key, &fingerprint)
                    .await?
                    .ok_or(Error::Storage { context, source }),
                None => Err(Error::Storage { context, source }),
            },
            Err(source) => Err(Error::Storage { context, source }),
        }
    }

    /// Appends one validated entry. Sequences are per-Card, gapless, and
    /// assigned here in a single atomic statement, so concurrent appends
    /// serialize into one durable order (S1-B5/B6/B9). Rejection leaves
    /// the record untouched (S1-B8).
    ///
    /// # Errors
    ///
    /// [`Error::NotAppendable`], [`Error::BlankField`],
    /// [`Error::CardNotFound`], [`Error::IdempotencyConflict`],
    /// [`Error::MalformedPayload`], [`Error::Corrupt`], or
    /// [`Error::Storage`].
    pub async fn append(&self, request: AppendEntry) -> Result<Accepted, Error> {
        let context = StorageContext::AppendEntry;
        // card-created exists only at sequence 1, written by create_card;
        // a second creation fact would be a contradiction in the record
        // (deep-review finding 1). Defended at this seam so every future
        // frontend inherits the rule.
        if matches!(request.payload, Payload::CardCreatedV1(_)) {
            return Err(Error::NotAppendable {
                entry_type: EntryType::CardCreated,
            });
        }
        crate::domain::require_text(&request.producer.id, "producer identity")?;
        request.payload.validate()?;
        self.require_card(&request.card_id, context).await?;
        let payload_value = request.payload.to_json()?;
        let payload_text = payload_value.to_string();
        let fingerprint = canonical_fingerprint(
            request.payload.entry_type(),
            request.payload.schema_version(),
            &payload_value,
            &request.producer,
        );

        if let Some(key) = &request.idempotency_key {
            if let Some(accepted) = self
                .replay_append(&request.card_id, key, &fingerprint)
                .await?
            {
                return Ok(accepted);
            }
        }

        let inserted = self
            .insert_entry(&request, &payload_text, &fingerprint)
            .await;
        match inserted {
            Ok((entry_id, raw_sequence)) => Ok(Accepted {
                entry_id,
                sequence: u64::try_from(raw_sequence)
                    .map_err(|_negative| Error::Corrupt { context })?,
            }),
            Err(source) if is_unique_violation(&source) => match &request.idempotency_key {
                Some(key) => self
                    .replay_append(&request.card_id, key, &fingerprint)
                    .await?
                    .ok_or(Error::Storage { context, source }),
                None => Err(Error::Storage { context, source }),
            },
            Err(source) => Err(Error::Storage { context, source }),
        }
    }

    /// The Card's ordered history, optionally filtered by entry type
    /// (S1-B11).
    ///
    /// # Errors
    ///
    /// [`Error::CardNotFound`], [`Error::Corrupt`], or [`Error::Storage`].
    pub async fn history(
        &self,
        card_id: &CardId,
        filter: Option<EntryType>,
    ) -> Result<Vec<Entry>, Error> {
        let context = StorageContext::ReadHistory;
        self.require_card(card_id, context).await?;
        let base = "SELECT entry_id, card_id, sequence, entry_type, schema_version, \
                    producer_id, producer_kind, recorded_at, payload \
                    FROM card_entries WHERE card_id = ?1";
        let rows = match filter {
            Some(entry_type) => {
                sqlx::query(&format!("{base} AND entry_type = ?2 ORDER BY sequence"))
                    .bind(card_id.as_str())
                    .bind(entry_type.to_string())
                    .fetch_all(&self.pool)
                    .await
            }
            None => {
                sqlx::query(&format!("{base} ORDER BY sequence"))
                    .bind(card_id.as_str())
                    .fetch_all(&self.pool)
                    .await
            }
        }
        .map_err(|source| Error::Storage { context, source })?;
        rows.iter()
            .map(|row| entry_from_row(row, context))
            .collect()
    }

    /// All Cards in creation order (S1-B1).
    ///
    /// # Errors
    ///
    /// [`Error::Storage`].
    pub async fn list_cards(&self) -> Result<Vec<CardSummary>, Error> {
        let context = StorageContext::ListCards;
        let rows = sqlx::query(
            "SELECT card_id, title, task_key, workspace, created_at FROM cards ORDER BY rowid",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|source| Error::Storage { context, source })?;
        rows.iter()
            .map(|row| {
                Ok(CardSummary {
                    card_id: CardId::from(column::<String>(row, "card_id", context)?),
                    title: column(row, "title", context)?,
                    task_key: column(row, "task_key", context)?,
                    workspace: column(row, "workspace", context)?,
                    created_at: column(row, "created_at", context)?,
                })
            })
            .collect()
    }

    async fn require_card(&self, card_id: &CardId, context: StorageContext) -> Result<(), Error> {
        let found = sqlx::query("SELECT 1 FROM cards WHERE card_id = ?1")
            .bind(card_id.as_str())
            .fetch_optional(&self.pool)
            .await
            .map_err(|source| Error::Storage { context, source })?;
        match found {
            Some(_) => Ok(()),
            None => Err(Error::CardNotFound {
                card_id: card_id.clone(),
            }),
        }
    }

    /// Looks up a prior create with this key: identical fingerprint replays
    /// the original Card ID, different fingerprint is a conflict (S1-B3/B4).
    async fn replay_create(&self, key: &str, fingerprint: &str) -> Result<Option<CardId>, Error> {
        let context = StorageContext::CreateCard;
        let row = sqlx::query("SELECT card_id, fingerprint FROM cards WHERE idempotency_key = ?1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|source| Error::Storage { context, source })?;
        match row {
            None => Ok(None),
            Some(row) => {
                let stored: String = column(&row, "fingerprint", context)?;
                if stored == fingerprint {
                    Ok(Some(CardId::from(column::<String>(
                        &row, "card_id", context,
                    )?)))
                } else {
                    Err(Error::IdempotencyConflict {
                        key: key.to_owned(),
                    })
                }
            }
        }
    }

    /// Looks up a prior append with this key on this Card (S1-B7).
    async fn replay_append(
        &self,
        card_id: &CardId,
        key: &str,
        fingerprint: &str,
    ) -> Result<Option<Accepted>, Error> {
        let context = StorageContext::AppendEntry;
        let row = sqlx::query(
            "SELECT entry_id, sequence, fingerprint FROM card_entries \
             WHERE card_id = ?1 AND idempotency_key = ?2",
        )
        .bind(card_id.as_str())
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|source| Error::Storage { context, source })?;
        match row {
            None => Ok(None),
            Some(row) => {
                let stored: String = column(&row, "fingerprint", context)?;
                if stored == fingerprint {
                    Ok(Some(Accepted {
                        entry_id: EntryId::from(column::<String>(&row, "entry_id", context)?),
                        sequence: sequence_from_row(&row, context)?,
                    }))
                } else {
                    Err(Error::IdempotencyConflict {
                        key: key.to_owned(),
                    })
                }
            }
        }
    }

    async fn insert_card(
        &self,
        card_id: &CardId,
        created: &CardCreatedV1,
        payload_text: &str,
        producer: &Producer,
        idempotency_key: Option<&str>,
        fingerprint: &str,
    ) -> Result<(), sqlx::Error> {
        let now = time::OffsetDateTime::now_utc();
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO cards (card_id, title, task_key, workspace, created_at, \
             idempotency_key, fingerprint) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(card_id.as_str())
        .bind(&created.title)
        .bind(&created.task_key)
        .bind(&created.workspace)
        .bind(now)
        .bind(idempotency_key)
        .bind(fingerprint)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO card_entries (entry_id, card_id, sequence, entry_type, \
             schema_version, producer_id, producer_kind, recorded_at, payload, \
             idempotency_key, fingerprint) VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9)",
        )
        .bind(EntryId::generate().as_str())
        .bind(card_id.as_str())
        .bind(EntryType::CardCreated.to_string())
        .bind(CURRENT_SCHEMA_VERSION)
        .bind(&producer.id)
        .bind(producer.kind.to_string())
        .bind(now)
        .bind(payload_text)
        .bind(fingerprint)
        .execute(&mut *tx)
        .await?;
        tx.commit().await
    }

    /// One atomic statement: the sequence subquery and the insert commit
    /// together, so sequences stay gapless under concurrency (S1-B9).
    async fn insert_entry(
        &self,
        request: &AppendEntry,
        payload_text: &str,
        fingerprint: &str,
    ) -> Result<(EntryId, i64), sqlx::Error> {
        let entry_id = EntryId::generate();
        let row = sqlx::query(
            "INSERT INTO card_entries (entry_id, card_id, sequence, entry_type, \
             schema_version, producer_id, producer_kind, recorded_at, payload, \
             idempotency_key, fingerprint) \
             VALUES (?1, ?2, \
             (SELECT COALESCE(MAX(sequence), 0) + 1 FROM card_entries WHERE card_id = ?2), \
             ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
             RETURNING sequence",
        )
        .bind(entry_id.as_str())
        .bind(request.card_id.as_str())
        .bind(request.payload.entry_type().to_string())
        .bind(request.payload.schema_version())
        .bind(&request.producer.id)
        .bind(request.producer.kind.to_string())
        .bind(time::OffsetDateTime::now_utc())
        .bind(payload_text)
        .bind(&request.idempotency_key)
        .bind(fingerprint)
        .fetch_one(&self.pool)
        .await?;
        let raw: i64 = row.try_get("sequence")?;
        Ok((entry_id, raw))
    }
}

/// Owner-only mode for the durable Card record and its `WAL`/`SHM`
/// side files: M1 doesn't authenticate producers, but that grants
/// nothing to unrelated local OS users (deep-review finding 4).
#[cfg(unix)]
const DB_FILE_MODE: u32 = 0o600;

/// The Card log is factory-owned: whatever the operator's umask, the
/// database and its side files end up owner-only. Applies on every open
/// so pre-existing permissive artifacts are also repaired.
#[cfg(unix)]
fn restrict_database_permissions(path: &Path) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt;
    let mut side_files = std::path::PathBuf::from(path).into_os_string();
    let wal = {
        let mut name = side_files.clone();
        name.push("-wal");
        std::path::PathBuf::from(name)
    };
    side_files.push("-shm");
    let shm = std::path::PathBuf::from(side_files);
    for file in [path, wal.as_path(), shm.as_path()] {
        if file.exists() {
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(DB_FILE_MODE)).map_err(
                |source| Error::Storage {
                    context: StorageContext::Open,
                    source: sqlx::Error::Io(source),
                },
            )?;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn restrict_database_permissions(_path: &Path) -> Result<(), Error> {
    // Non-Unix targets have no mode bits to normalize; access control is
    // the platform ACL's concern until a Windows slice takes it up.
    Ok(())
}

/// The canonical JSON string used for idempotency comparison: identical
/// requests replay, different ones conflict (S1-B4/B7).
fn canonical_fingerprint(
    entry_type: EntryType,
    schema_version: u32,
    payload_value: &serde_json::Value,
    producer: &Producer,
) -> String {
    serde_json::json!({
        "entry_type": entry_type.to_string(),
        "schema_version": schema_version,
        "producer_id": producer.id,
        "producer_kind": producer.kind.to_string(),
        "payload": payload_value,
    })
    .to_string()
}

/// `SQLite` extended result codes for uniqueness violations, as reported by
/// `DatabaseError::code` (externally defined by `SQLite`).
const SQLITE_CONSTRAINT_UNIQUE: &str = "2067";
const SQLITE_CONSTRAINT_PRIMARYKEY: &str = "1555";

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
        .is_some_and(|code| {
            code == SQLITE_CONSTRAINT_UNIQUE || code == SQLITE_CONSTRAINT_PRIMARYKEY
        })
}

fn column<'r, T>(row: &'r SqliteRow, name: &str, context: StorageContext) -> Result<T, Error>
where
    T: sqlx::Decode<'r, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite>,
{
    row.try_get(name)
        .map_err(|source| Error::Storage { context, source })
}

fn sequence_from_row(row: &SqliteRow, context: StorageContext) -> Result<u64, Error> {
    let raw: i64 = column(row, "sequence", context)?;
    u64::try_from(raw).map_err(|_negative| Error::Corrupt { context })
}

fn entry_from_row(row: &SqliteRow, context: StorageContext) -> Result<Entry, Error> {
    let entry_type_text: String = column(row, "entry_type", context)?;
    let entry_type =
        EntryType::from_str(&entry_type_text).map_err(|_unknown| Error::Corrupt { context })?;
    let schema_version_raw: i64 = column(row, "schema_version", context)?;
    let schema_version =
        u32::try_from(schema_version_raw).map_err(|_negative| Error::Corrupt { context })?;
    let payload_text: String = column(row, "payload", context)?;
    // A stored payload that no longer parses is store corruption, not a
    // producer mistake — an agent must never be told to repair its
    // request over it (deep-review finding 5).
    let payload = Payload::from_raw(entry_type, schema_version, &payload_text)
        .map_err(|_unreadable| Error::Corrupt { context })?;
    let kind_text: String = column(row, "producer_kind", context)?;
    let kind = ProducerKind::from_str(&kind_text).map_err(|_unknown| Error::Corrupt { context })?;
    Ok(Entry {
        entry_id: EntryId::from(column::<String>(row, "entry_id", context)?),
        card_id: CardId::from(column::<String>(row, "card_id", context)?),
        sequence: sequence_from_row(row, context)?,
        producer: Producer {
            id: column(row, "producer_id", context)?,
            kind,
        },
        recorded_at: column(row, "recorded_at", context)?,
        payload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::DecisionV1;

    fn decision(summary: &str, reason: &str) -> Payload {
        Payload::DecisionV1(DecisionV1 {
            summary: summary.to_owned(),
            reason: reason.to_owned(),
        })
    }

    fn producer(id: &str, kind: ProducerKind) -> Producer {
        Producer {
            id: id.to_owned(),
            kind,
        }
    }

    fn fingerprint(payload: &Payload, producer: &Producer) -> String {
        canonical_fingerprint(
            payload.entry_type(),
            payload.schema_version(),
            &payload.to_json().unwrap(),
            producer,
        )
    }

    /// Idempotency correctness hangs on this property: logically identical
    /// requests must fingerprint equal across calls (S1-B3/B7), or replays
    /// would silently become conflicts.
    #[test]
    fn identical_requests_fingerprint_equal() {
        let claude = producer("claude", ProducerKind::Agent);
        let first = fingerprint(&decision("s", "r"), &claude);
        let second = fingerprint(&decision("s", "r"), &claude);
        assert_eq!(first, second);
    }

    /// The discrimination half (S1-B4): any differing field must change
    /// the fingerprint, or conflicts would silently replay.
    #[test]
    fn each_differing_field_changes_the_fingerprint() {
        let claude = producer("claude", ProducerKind::Agent);
        let base = fingerprint(&decision("s", "r"), &claude);
        let variants = [
            fingerprint(&decision("other", "r"), &claude),
            fingerprint(&decision("s", "other"), &claude),
            fingerprint(&decision("s", "r"), &producer("codex", ProducerKind::Agent)),
            fingerprint(
                &decision("s", "r"),
                &producer("claude", ProducerKind::Operator),
            ),
        ];
        for variant in variants {
            assert_ne!(base, variant);
        }
    }
}

#[cfg(test)]
mod store_seam_tests {
    use super::*;
    use crate::domain::DecisionV1;
    use crate::error::ErrorCategory;

    async fn open_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let store = Store::open(&dir.path().join("daemar.db"))
            .await
            .expect("open store");
        (dir, store)
    }

    fn operator(id: &str) -> Producer {
        Producer {
            id: id.to_owned(),
            kind: ProducerKind::Operator,
        }
    }

    async fn open_card(store: &Store) -> CardId {
        store
            .create_card(CreateCard {
                title: "Store-seam fixture".to_owned(),
                task_key: None,
                workspace: None,
                producer: operator("test-operator"),
                idempotency_key: None,
            })
            .await
            .expect("create card")
    }

    fn decision_payload() -> Payload {
        Payload::DecisionV1(DecisionV1 {
            summary: "s".to_owned(),
            reason: "r".to_owned(),
        })
    }

    /// Deep-review finding 1, store seam: the reserved card-created type
    /// is refused even by callers that bypass the CLI.
    #[tokio::test]
    async fn store_refuses_appending_card_created() {
        let (_dir, store) = open_store().await;
        let card_id = open_card(&store).await;
        let refused = store
            .append(AppendEntry {
                card_id: card_id.clone(),
                payload: Payload::CardCreatedV1(CardCreatedV1 {
                    title: "a second creation fact".to_owned(),
                    task_key: None,
                    workspace: None,
                }),
                producer: operator("test-operator"),
                idempotency_key: None,
            })
            .await;
        assert!(matches!(
            refused,
            Err(Error::NotAppendable {
                entry_type: EntryType::CardCreated
            })
        ));
        let history = store.history(&card_id, None).await.expect("history");
        assert_eq!(history.len(), 1, "the record must be untouched");
    }

    /// Deep-review finding 2, store seam: blank provenance and blank
    /// required content are refused below the CLI.
    #[tokio::test]
    async fn store_refuses_blank_required_fields() {
        let (_dir, store) = open_store().await;
        let card_id = open_card(&store).await;
        let blank_producer = store
            .append(AppendEntry {
                card_id: card_id.clone(),
                payload: decision_payload(),
                producer: operator("   "),
                idempotency_key: None,
            })
            .await;
        assert!(matches!(
            blank_producer,
            Err(Error::BlankField {
                field: "producer identity"
            })
        ));
        let blank_reason = store
            .append(AppendEntry {
                card_id: card_id.clone(),
                payload: Payload::DecisionV1(DecisionV1 {
                    summary: "s".to_owned(),
                    reason: " ".to_owned(),
                }),
                producer: operator("test-operator"),
                idempotency_key: None,
            })
            .await;
        assert!(matches!(blank_reason, Err(Error::BlankField { .. })));
        let blank_title = store
            .create_card(CreateCard {
                title: "\t".to_owned(),
                task_key: None,
                workspace: None,
                producer: operator("test-operator"),
                idempotency_key: None,
            })
            .await;
        assert!(matches!(
            blank_title,
            Err(Error::BlankField { field: "title" })
        ));
    }

    /// Deep-review finding 5: a stored payload that no longer parses is
    /// store corruption (category storage), never producer validation.
    #[tokio::test]
    async fn corrupted_stored_payload_reads_as_corrupt() {
        let (_dir, store) = open_store().await;
        let card_id = open_card(&store).await;
        store
            .append(AppendEntry {
                card_id: card_id.clone(),
                payload: decision_payload(),
                producer: operator("test-operator"),
                idempotency_key: None,
            })
            .await
            .expect("append");
        // Simulated corruption: tests may write the pool directly; the
        // unsupported-writer rule binds producers, not this simulation.
        sqlx::query("UPDATE card_entries SET payload = '}{' WHERE sequence = 2")
            .execute(&store.pool)
            .await
            .expect("corrupt the stored payload");
        let read = store.history(&card_id, None).await;
        match read {
            Err(error) => {
                assert!(matches!(error, Error::Corrupt { .. }), "got {error:?}");
                assert_eq!(error.category(), ErrorCategory::Storage);
            }
            Ok(entries) => panic!("corruption must not read cleanly: {entries:?}"),
        }
    }
}
