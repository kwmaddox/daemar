# PER-84 planning input: selected durable contract

Card: 01a0693e-bc16-7272-9ce9-a20f3b07f875
Task: PER-84 — S3: The operator reads Cards on a local Queue First console
Repository: /Users/kendall/code/github/daemar

## Disclosure manifest

This packet supplies Card identity, approved typed contract (sequence 38, amended at 46), and binding operator contract rulings (45). Sequence 38 historical review discussions are excluded; its PublishStartup classification/display amendment is retained explicitly. Old execution plans and plan reviews are not inputs. This packet contains no prior conversation or agent transcript. The external ticket is unavailable through current connectors: the accepted executable specification is crates/daemar-card/tests/features/console.feature and browser/tests/console.spec.ts. Do not claim to have read an external ticket. Identify any material missing scope information explicitly.

## Card sequence 38

## lib.rs

```rust
pub mod console;
pub use domain::{MigrationVersion, QueueCard, SchemaIncompatibility};
pub use storage::Reader;
```

## error.rs (additions; existing variants unchanged)

```rust
pub enum Error {
    // ...existing...

    /// The database file does not exist at `path`. `serve` never creates
    /// one (S3-B3).
    DatabaseMissing {
        /// The resolved path that was absent.
        path: PathBuf,
    },
    /// The database at `path` exists but its migration history is not
    /// exactly this build's embedded set (S3-B3). Verified read-only,
    /// never advanced.
    SchemaIncompatible {
        /// The resolved path that was refused.
        path: PathBuf,
        /// Why the history does not match.
        reason: SchemaIncompatibility,
    },
    /// The loopback socket could not be bound (S3-B1). Never a fallback
    /// to another port.
    Bind {
        /// The address that was requested.
        addr: console::LoopbackAddr,
        /// The bind failure.
        source: std::io::Error,
    },
    /// The startup line could not be written or flushed (S3-B1). Serving
    /// never begins after this failure; some or all of the line may
    /// already be visible on `out`, since emitted bytes cannot be
    /// retracted.
    PublishStartup {
        /// The write or flush failure.
        source: std::io::Error,
    },
    /// The HTTP server's accept loop failed after the startup line was
    /// published.
    Serve {
        /// The accept-loop failure.
        source: std::io::Error,
    },
    /// No entry with this ID is a member of this Card (S3-B6). An entry
    /// that exists under another Card is this variant, never an inspector.
    EntryNotFound {
        /// The Card in the request path.
        card_id: CardId,
        /// The entry that is not a member of it.
        entry_id: EntryId,
    },
}

pub enum StorageContext {
    // ...existing...
    /// Opening an existing database read-only.
    OpenReadOnly,
    /// Reading the applied-migration history.
    VerifySchema,
    /// Reading the queue with per-Card last activity.
    Queue,
    /// Reading one entry by Card ID and entry ID.
    ReadEntry,
}

pub enum ErrorCategory {
    // ...existing...
    /// The resource exists but cannot be used as it stands: a port
    /// already bound, a database whose schema does not match.
    Unavailable,
}
```

`category()`: `DatabaseMissing | EntryNotFound => Missing`; `SchemaIncompatible | Bind | Serve => Unavailable`.
`source()`: `Bind`, `PublishStartup`, and `Serve` return their io source;
the rest `None`.
`Display` (hand-written): `DatabaseMissing` → "no database at {path}"; `SchemaIncompatible` → "database {path} schema is {reason}"; `Bind` → "cannot bind {addr}"; `Serve` → "console server failed"; `EntryNotFound` → "no entry `{entry_id}` on Card `{card_id}`". `StorageContext` names: `open-read-only`, `verify-schema`, `queue`, `read-entry`. `ErrorCategory::Unavailable` → "unavailable".

## domain.rs (additions)

```rust
/// One queue row (S3-B4). Rows come in `card list` order (creation order,
/// `cards.rowid`); `last_activity` is the `recorded_at` of the Card's
/// highest-sequence entry, read in the same query. Composes
/// [`CardSummary`] rather than restating it (C10).
#[derive(Debug, Clone)]
pub struct QueueCard {
    /// The Card as `card list` returns it.
    pub card: CardSummary,
    /// `recorded_at` of the entry with the greatest sequence on this Card.
    pub last_activity: time::OffsetDateTime,
}

/// A migration's version number as sqlx records it (C7: identifies a
/// migration; std has no fitting type). `Display` prints the number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MigrationVersion(i64);

impl MigrationVersion {
    #[must_use]
    pub const fn new(version: i64) -> Self { ... }
    #[must_use]
    pub const fn get(self) -> i64 { ... }
}

/// How an existing database's migration history differs from this
/// build's embedded set (S3-B3). Any difference refuses the open.
///
/// Classification is deterministic, first mismatch wins, in this order:
/// 1. a dirty row (`success = false`) anywhere → `Dirty`;
/// 2. no bookkeeping table or zero applied rows → `Uninitialized`;
/// 3. walk embedded and applied in order, pairwise: differing version at
///    the same position → `Diverged`; same version, differing checksum →
///    `ChecksumMismatch`;
/// 4. all pairs equal and applied shorter → `Behind`; applied longer →
///    `Ahead` naming the first surplus version.
///
/// Domain payload enum (C2/C6): hand-written `Display`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaIncompatibility {
    /// No migration bookkeeping table or no applied rows.
    Uninitialized,
    /// A proper prefix of the embedded set is applied; `next_required` is
    /// the first embedded version not applied. `applied`/`required` are
    /// dimensionless counts (C7).
    Behind { applied: usize, required: usize, next_required: MigrationVersion },
    /// Every embedded version is applied and a further version is
    /// recorded that this build does not know.
    Ahead { unknown_version: MigrationVersion },
    /// At the same position the applied history names a different
    /// version than the embedded set (a gap or a reorder).
    Diverged { expected: MigrationVersion, found: MigrationVersion },
    /// An applied version has a different checksum from the embedded one.
    ChecksumMismatch { version: MigrationVersion },
    /// A migration is recorded as started but not successfully finished.
    Dirty { version: MigrationVersion },
}
```

## storage.rs (additions)

```rust
/// Read-only capability on one factory database (S3-B8, structural
/// proof): this type has no write method, and `console` handlers hold
/// only this type, so `Store`'s write API is unreachable from them.
///
/// The pool holds exactly one connection for the process lifetime
/// (`max_connections(1)`, `min_connections(1)`, `idle_timeout(None)`,
/// `max_lifetime(None)`), so the connection whose schema was verified at
/// open is the connection every read uses. If that connection breaks,
/// the pool reconnects without re-verification; a database replaced
/// under a running console is outside S3-B3 (Gherkin finding G1).
pub struct Reader {
    pool: SqlitePool,
}

impl Reader {
    /// Opens an existing, fully migrated database read-only.
    ///
    /// Invariants: `path` must exist as a file before connecting; the
    /// connection is `read_only(true)` and `create_if_missing(false)`;
    /// the applied-migration history is compared exactly against the
    /// embedded set and never applied; file permissions are not touched.
    /// SQLite may create `-wal`/`-shm` beside the file.
    ///
    /// # Errors
    ///
    /// [`Error::DatabaseMissing`], [`Error::SchemaIncompatible`], or
    /// [`Error::Storage`] with [`StorageContext::OpenReadOnly`] /
    /// [`StorageContext::VerifySchema`].
    pub async fn open_existing(path: &Path) -> Result<Reader, Error> { ... }

    /// The queue (S3-B4): every Card in creation order with its last
    /// activity, one query per call, nothing cached. Decodes only the
    /// `cards` columns and the highest-sequence `recorded_at`; entry
    /// payloads are not read, so a corrupt payload does not fail the queue
    /// (S3-B9). A corrupt `recorded_at` or `cards` column does.
    ///
    /// # Errors
    ///
    /// [`Error::Corrupt`] or [`Error::Storage`] with [`StorageContext::Queue`].
    pub async fn queue(&self) -> Result<Vec<QueueCard>, Error> { ... }

    /// All Cards in creation order (S1-B1). Moved from `Store`, which
    /// now delegates.
    ///
    /// # Errors
    ///
    /// [`Error::Corrupt`] or [`Error::Storage`].
    pub async fn list_cards(&self) -> Result<Vec<CardSummary>, Error> { ... }

    /// A Card's complete history in sequence order (S1-B11). Moved from
    /// `Store`, which now delegates.
    ///
    /// # Errors
    ///
    /// [`Error::CardNotFound`], [`Error::Corrupt`], or [`Error::Storage`].
    pub async fn history(
        &self,
        card_id: &CardId,
        filter: Option<EntryType>,
    ) -> Result<Vec<Entry>, Error> { ... }

    /// One entry by (Card ID, entry ID) (S3-B6). Membership is part of
    /// the lookup: `WHERE card_id = ?1 AND entry_id = ?2`.
    ///
    /// # Errors
    ///
    /// [`Error::CardNotFound`] when the Card is absent;
    /// [`Error::EntryNotFound`] when the entry is absent or belongs to
    /// another Card; [`Error::Corrupt`] or [`Error::Storage`] with
    /// [`StorageContext::ReadEntry`].
    pub async fn entry(&self, card_id: &CardId, entry_id: &EntryId) -> Result<Entry, Error> { ... }
}

// `Store`'s public surface is unchanged. Internally `Store` owns a
// `Reader` over its own pool and delegates `history` and `list_cards`
// to it, so each query text exists once (C10).
```

## console.rs

```rust
//! `card serve` (PER-84, S3-B1…B9): a loopback-only, read-only,
//! script-free HTML console over [`Reader`].

/// A loopback socket address (S3-B2). [`LoopbackAddr::v4`] and
/// [`LoopbackAddr::v6`] are the only constructors and [`Listener::bind`]
/// accepts nothing else, so no non-loopback bind path exists.
///
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopbackAddr {
    ip: LoopbackIp,
    port: Port,
}

/// A TCP port (C7, operator ruling at Card seq 37/38). Every `u16` is a
/// valid port; `Port::new(0)` requests an OS-assigned port, and the
/// bound listener reports the port assigned. `new`/`get` are the
/// representation conversion boundary, as `MigrationVersion`'s are.
/// `Display` prints the number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Port(u16);

impl Port {
    /// A port requested as `0` is assigned by the OS.
    pub const EPHEMERAL: Port = Port(0);
    #[must_use]
    pub const fn new(port: u16) -> Self { ... }
    #[must_use]
    pub const fn get(self) -> u16 { ... }
}

/// Which loopback interface. Private: constructed and observed only
/// through [`LoopbackAddr`] (C9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopbackIp {
    V4,
    V6,
}

impl LoopbackAddr {
    /// `127.0.0.1:port`.
    #[must_use]
    pub fn v4(port: Port) -> LoopbackAddr { ... }
    /// `[::1]:port`.
    #[must_use]
    pub fn v6(port: Port) -> LoopbackAddr { ... }
    /// The port as requested or, on a bound listener's address, as
    /// resolved.
    #[must_use]
    pub fn port(&self) -> Port { ... }
    /// The std address to bind.
    #[must_use]
    pub fn socket_addr(&self) -> SocketAddr { ... }
}

/// `127.0.0.1:{port}` or `[::1]:{port}`: the authority form used in the
/// startup URL and in the Host check.
impl std::fmt::Display for LoopbackAddr { ... }

/// Port a bare `card serve` binds (C14). Fixed so the operator's
/// bookmark survives restarts; `--port 0` is the ephemeral path. 7331
/// is chosen because it is outside the IANA well-known range, is not a
/// registered service port, and is not a default of the common local
/// development servers (3000, 5173, 8000, 8080), so a running dev
/// server does not collide with the console.
pub const DEFAULT_PORT: Port = Port::new(7331);

/// A bound loopback listener: the only value [`serve`] accepts.
#[derive(Debug)]
pub struct Listener {
    inner: tokio::net::TcpListener,
    bound: LoopbackAddr,
}

impl Listener {
    /// Binds `addr`. With port 0 the OS-assigned port is resolved and
    /// reported by [`Listener::bound`]. Never falls back to another port.
    ///
    /// # Errors
    ///
    /// [`Error::Bind`].
    pub async fn bind(addr: LoopbackAddr) -> Result<Listener, Error> { ... }
    /// The address actually bound, port resolved.
    #[must_use]
    pub fn bound(&self) -> LoopbackAddr { ... }
}

/// The startup record (S3-B1): what `card serve` publishes once, before
/// serving. The address is read from a bound [`Listener`], never supplied
/// as a free value, and the URL is derived from it, so the URL always
/// agrees with the record's stored address and that address is one the
/// OS actually assigned. That the same listener is then passed to
/// [`serve`] is the binary's flow, shown in `main.rs` below. `db_path`
/// and `source` use the `card db-path` vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Startup {
    bound: LoopbackAddr,
    db_path: PathBuf,
    source: ConfigSource,
}

/// Where the database path was resolved from (S1-B14 vocabulary). Moved
/// from the binary to the library so [`Startup`] can carry it typed (C6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    Flag,
    Env,
    DotEnv,
    Default,
}

impl Startup {
    /// Builds the record from the address `listener` actually bound
    /// (`listener.bound()`), so a port requested as `0` is recorded as
    /// the port assigned.
    #[must_use]
    pub fn from_listener(listener: &Listener, db_path: PathBuf, source: ConfigSource) -> Self { ... }
    /// `http://{bound}/`, derived.
    #[must_use]
    pub fn url(&self) -> String { ... }
    #[must_use]
    pub fn bound(&self) -> LoopbackAddr { ... }
    #[must_use]
    pub fn db_path(&self) -> &Path { ... }
    #[must_use]
    pub fn source(&self) -> ConfigSource { ... }
    /// The startup line: one JSON object `{url, db_path, source}`.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value { ... }
}

/// Publishes `startup` on `out` and flushes, so the operator holds the
/// URL before any request can be served (S3-B1). On success exactly one
/// line was written. The binary calls this with locked stdout and then
/// [`serve`]; a closed or failed stdout is reported, not ignored.
///
/// # Errors
///
/// [`Error::PublishStartup`] carrying the write or flush failure.
pub fn publish_startup(out: &mut impl std::io::Write, startup: &Startup) -> Result<(), Error> { ... }

/// Serves the console on `listener` until the accept loop fails.
///
/// Invariants (S3-B8): every handler receives `&Reader` and nothing
/// that can write; there is no cache, projection, fold, or background
/// task, every request reads storage afresh. (S3-B2): a request whose
/// `Host` is not the bound loopback authority (`127.0.0.1`, `localhost`,
/// or `[::1]`, each with the bound port) is answered `421` with an empty
/// body before routing. Any method other than `GET`/`HEAD` is `405` on
/// every path, before routing. (S3-B7): producer-controlled text is
/// rendered only as text nodes through Askama auto-escaping with the
/// `safe` filter unused; the only attribute values built from data are
/// `href`s interpolating [`CardId`] and [`EntryId`] and the
/// `data-card-id`/`data-entry-id`/`data-sequence` markers, never
/// producer text. (S3-B9): the path segment after `/cards/` is extracted
/// as text and converted to [`CardId`] infallibly, so a malformed ID is
/// a `404` page with the queue, never an extractor `400`.
///
/// Routes: `GET /` queue page; `GET /cards/{card_id}` Card page, with
/// `?entry={entry_id}` opening that entry's inspector;
/// `GET /static/console.css` the one stylesheet; anything else `404`.
///
/// # Errors
///
/// [`Error::Serve`].
pub async fn serve(listener: Listener, reader: Reader) -> Result<(), Error> { ... }
```

Private to `console` (listed for layout review, not API): `HostAuthority`
(the three accepted spellings, checked against the bound port); Axum
state `Arc<Reader>` (C12 reason at the site: Axum requires `Clone` state
shared across request tasks; `Reader` is read-only so sharing carries no
mutation); `QueuePage`, `CardPage`, `ErrorPage` Askama templates in
`crates/daemar-card/templates/`; `Page`, the outcome enum with this fixed
mapping (R2-S1): `CardNotFound` and `EntryNotFound` → `404` with the
queue; `Corrupt` and `Storage` → `500` with the queue; any error from
[`Reader::queue`] itself → a `500` error page saying storage failed,
with no queue and no Card content. An
error page carries no partial Card identity, stream, inspector, or
payload: a page is whole or it is the error page.

## main.rs

New subcommand `serve` with `--port <PORT>`: clap parses a `u16`
privately (default `DEFAULT_PORT.get()`), converted once with
`Port::new` at the boundary; no public `FromStr` on `Port`. Flow:
resolve config → `Reader::open_existing` →
`let listener = Listener::bind(LoopbackAddr::v4(Port::new(port)))` →
`let startup = Startup::from_listener(&listener, db_path, source)` →
`publish_startup(&mut stdout.lock(), &startup)` → `serve(listener, reader)`.
The same `listener` binding flows from bind to serve. The
binary's private `ConfigSource` is replaced by `console::ConfigSource`.
Every failure goes through `Failure::Domain`, so `unavailable` reaches
stderr through the existing JSON contract.


Binding amendment retained from sequence 38: Error::PublishStartup has category Unavailable and Display text "startup line could not be published".


## Card sequence 45

R1 accepted: split the StageSummary sink row from DecisionSummary in the hostile sink table; remove the payload sink only from the Stage and StageSummary rows; absence/equality assertions use member presence, not indexing that collapses absence to JSON null. The inspector renders the producer payload only and omits the payload element when the history entry has no payload member. Expected end state of the plan becomes all 119 cucumber scenarios green. R2 accepted as a public-surface change, approved by the operator in conversation: `impl std::fmt::Display for ConfigSource` with one spelling source, Flag -> flag, Env -> env, DotEnv -> dotenv, Default -> default, written directly to the formatter. R3 accepted: `datetime` attributes on time elements may carry typed server timestamps; producer text stays out of attributes. R4 accepted: card_page identity comes from the matching QueueCard of the same request's queue read, no second list query. R5 accepted: a small private RFC3339 formatter per crate is fine, no new public helper. R6 accepted: Askama render failure yields a fixed plain-text 500 after buffering; storage failures keep the HTML error page; no new error variant. R7 accepted: favicon is escalated only on an observed fetch-set failure with evidence. R8 accepted: the no-background-task fence means no application workflow, projection or polling tasks; SQLx pool maintenance is permitted. R9 accepted: the reader opens as a normal read-only live WAL reader, journal_mode unspecified, immutable false, and never reuses the writer's create/migrate/chmod path or falls back to writing.

## Card sequence 46

Amendment to skeleton revision 5. Added to the console module, next to `ConfigSource`:

```rust
/// The single spelling of each configuration source, used by the startup
/// line and by every error that names where a value came from.
///
/// Invariant: `Flag` renders `flag`, `Env` renders `env`, `DotEnv` renders
/// `dotenv`, `Default` renders `default`; nothing else in the crate spells
/// these names (C14).
impl std::fmt::Display for ConfigSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result;
}
```

The body writes each name directly to the formatter with `f.write_str`; no intermediate `String`, no `to_string` helper. Any `Startup` or error `Display` that names a source goes through this impl. Everything else in revision 5 stands unchanged.



