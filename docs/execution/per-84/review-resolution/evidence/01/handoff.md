# Packet01 handoff — shared existence query and dependency cleanup

Executor scope was limited to CE-E and CE-H. No public API, acceptance
feature, browser file, migration, policy file, or unrelated source was edited.

## Implementation

- CE-E: `crates/daemar-card/src/storage.rs` now has one private
  `require_card_from_pool(&SqlitePool, &CardId, StorageContext)` helper owning
  the single `SELECT 1 FROM cards WHERE card_id = ?1` operation. `Reader::entry`
  uses `ReadEntry`, `history_from_pool` uses `ReadHistory`, and Store append's
  existing private `require_card` wrapper uses `AppendEntry`. CardNotFound is
  still returned before entry lookup; EntryNotFound remains distinct for an
  absent/foreign entry. No transaction boundary, pool capability, schema
  verification, or query semantics changed.
- CE-H: removed only the direct `hyper = "1"` declaration from
  `crates/daemar-card/Cargo.toml`. The narrow lockfile package-edge update
  removes `hyper` from the `daemar-card` package dependency list while the
  transitive `hyper` package remains through Axum and hyper-util. Evidence:
  `dependency-hyper-tree.log`.

## Initial and final verification

The initial library run in the restricted sandbox exited 101 because eight
loopback tests received OS `PermissionDenied`; 52 tests passed. The required
permission escalation rerun exited 0 with 60/60 tests. This is recorded in
`baseline-library-sandbox.log` and `baseline-library-escalated.log`.

The extraction was behavior-preserving, so no new test was added and no
artificial red was manufactured. Existing storage seams covered missing Cards,
foreign/absent entries, Store append, and history. Focused storage verification
passed 13/13 in `after-focused-storage.log`; full library verification passed
60/60 in `after-library.log`.

| Command | Exit | Complete log |
| --- | ---: | --- |
| `cargo test --locked -p daemar-card --lib` (restricted baseline) | 101 | `baseline-library-sandbox.log` |
| same (escalated baseline) | 0 | `baseline-library-escalated.log` |
| `cargo test --locked -p daemar-card --lib storage::` | 0 | `after-focused-storage.log` |
| `cargo test --locked -p daemar-card --lib` | 0 | `after-library.log` |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 | `after-clippy.log` |
| `git diff --check` | 0 | `after-diff-check.log` |
| `just check` (restricted sandbox) | 1 | `after-just-check.log` |
| `just check` (escalated) | 0 | `after-just-check-escalated.log` |
| `cargo tree --locked --offline -p daemar-card -i hyper` | 0 | `dependency-hyper-tree.log` |

The restricted `just check` failure was the Cargo advisory DB lock being on a
read-only path. Its log also reports an admitted, unowned unused suppression
in `docs/execution/per-84/run2-task3-continuation-console-before-tests.rs`; it
was not changed in this packet. The escalated complete check passed: 60
library tests, 6 binary tests, 119/119 behavior scenarios and 627/627 steps.

## C1–C16 executor checklist

- C1: unchanged; helper returns crate-native `Result<(), Error>`.
- C2: unchanged; no error-helper crate or derived error implementation added.
- C3: unchanged; `StorageContext` remains typed and is passed to the helper.
- C4: unchanged; no production panic or unchecked operation added.
- C5: unchanged; no suppression added.
- C6: unchanged; no string dispatch added.
- C7: unchanged; `CardId` and `StorageContext` remain typed at the helper seam.
- C8: unchanged; SQL errors retain the caller's context at the helper.
- C9: unchanged; helper is private and no public surface changed.
- C10: resolved at `storage.rs:572-588`: the repeated existence query has one
  text owner; the three callers retain their operation-specific contexts.
- C11: unchanged; the existing CardId clone is only for the typed error value.
- C12: unchanged; no shared mutability introduced.
- C13: unchanged; no buffering or publication path touched.
- C14: unchanged; no load-bearing literal introduced.
- C15: unchanged; no boolean parameter introduced.
- C16: unchanged; the helper is one storage operation at one abstraction level.

## Changed paths and hashes

Changed by this packet:

- `crates/daemar-card/src/storage.rs`
- `crates/daemar-card/Cargo.toml`
- `Cargo.lock`
- `docs/execution/per-84/review-resolution/evidence/01/` (this handoff and logs)

Final SHA256 hashes:

```text
1eb91c16b70dd6ffdcbc2296e1e93dc02cfaf5012785e0071da50a6d5c943d34  crates/daemar-card/src/storage.rs
4d5521a7e40dc5d29edd7adc234c2e4b6102556356116b3fef5ce3d11d494b9a  crates/daemar-card/Cargo.toml
5b5f355fda23e532b173c968dc229acd3afc77c867ef1c291e92c98cada9d73e  Cargo.lock
```

Initial hashes recorded before edits were:

```text
7a020297c098ebc213d661ab0bfed0d79b00e5a2979d3db896024e6f6d074107  crates/daemar-card/src/storage.rs
7e72430b1efb30a1a661061ec2b7206de2b11430cfebbad7580f9ef1aa52ec25  crates/daemar-card/Cargo.toml
1ad85ac771e8a38de03e61b35ce3d068d02145e069fdb5c116f4e13f2ba360df  Cargo.lock
```

## Protected-file verification

The packet did not edit protected files. Hashes match the supplied
`run2-final-repair-hashes.txt` for `console.rs`, both templates, and the three
prior final command logs. Existing unrelated dirty and untracked files remain
untouched. No commit or Card write was made.

## Limitations

The historical missing test-first evidence and the admitted unused suppression
reported by the repository gate remain outside this packet's ownership. No
new behavioral defect or dependency feature requirement was observed.
