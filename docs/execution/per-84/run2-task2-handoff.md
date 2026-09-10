# PER-84 run2 Task 2 handoff

Status: complete for the assigned storage/lib slice.

Implemented in `crates/daemar-card/src/storage.rs` and `src/lib.rs`:

- Added the approved public `Reader` capability and re-export.
- `Reader::open_existing` checks for an existing file, opens read-only with a
  one-connection pool, and verifies the embedded SQLx migration history without
  creating bookkeeping tables or running migrations.
- Schema precedence is dirty, uninitialized, pairwise version/checksum,
  behind, then ahead. Missing files map to `DatabaseMissing`; SQL/query errors
  retain `OpenReadOnly` or `VerifySchema` context.
- Queue reads Cards in `rowid` order and highest-sequence `recorded_at` without
  decoding payloads. List/history query ownership is shared through Reader and
  Store delegates to the same implementation. Entry lookup constrains both
  CardId and EntryId and distinguishes CardNotFound from EntryNotFound.
- Added a real temporary migrated-store Reader/list/history/queue test.

Evidence:

- `cargo test --locked -p daemar-card --lib reader_opens_migrated_store_and_delegates_reads`: exit 0, 1 passed.
- `cargo clippy --locked -p daemar-card --all-targets --all-features -- -D warnings`: exit 0.
- `git diff --check`: exit 0.
- Full library test command was run; the seven pre-existing loopback tests fail
  in this restricted environment with `Operation not permitted` while the
  predecessor 36 non-loopback tests pass. This is the documented Task 1
  environmental boundary, not a Task 2 failure.
- Source hashes at handoff:
  - `storage.rs`: `eccb59a002d8be655ae539df91944a12bd4a45cca6f77a246198f15607535b5b`
  - `lib.rs`: `720c1963019e074499ace658befb5d1f00571b663ce01da263fe1f98ec8277f1`

No CLI, HTTP, migration, manifest, policy, or external acceptance files were
changed by this task. No Card escalation was needed; no commit was made.
