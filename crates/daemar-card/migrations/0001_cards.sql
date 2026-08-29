-- The Card record (PER-82): identity rows plus the append-only entry log.
-- The Card is the log; anything document-like is a projection built later.
-- Producers never write these tables directly — the application boundary
-- (Store) is the only supported writer.

CREATE TABLE cards (
    card_id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    task_key TEXT,
    workspace TEXT,
    created_at TEXT NOT NULL,
    -- Retry token for create (S1-B3/B4); fingerprint holds the canonical
    -- request JSON the key was first used with, for replay-vs-conflict.
    idempotency_key TEXT UNIQUE,
    fingerprint TEXT NOT NULL
) STRICT;

CREATE TABLE card_entries (
    entry_id TEXT PRIMARY KEY,
    card_id TEXT NOT NULL REFERENCES cards (card_id),
    -- Per-Card monotonic sequence assigned by the store (S1-B5/B6/B9).
    sequence INTEGER NOT NULL,
    entry_type TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    producer_id TEXT NOT NULL,
    producer_kind TEXT NOT NULL,
    recorded_at TEXT NOT NULL,
    payload TEXT NOT NULL,
    -- Retry token for append (S1-B7), scoped per Card; NULLs never collide.
    idempotency_key TEXT,
    fingerprint TEXT NOT NULL,
    UNIQUE (card_id, sequence),
    UNIQUE (card_id, idempotency_key)
) STRICT;
