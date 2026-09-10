# PER-84 Task 1 restoration handoff

Status: DONE

Restoration followed `docs/execution/per-84/task-1-restore-dispatch.md` and Card decision63 exactly.

Restored byte-for-byte from `/private/tmp/per84-execution-GGJtBp/admission.tar.gz` (archive SHA-256 `bbfd967bad23f986aec41ccbeb8370d3d035ce892b4158680c4454ab2c2a7ae6`):

- `crates/daemar-card/src/domain.rs`
- `crates/daemar-card/src/error.rs`
- `crates/daemar-card/src/lib.rs`
- `crates/daemar-card/Cargo.toml`
- `Cargo.lock`

Deleted only the rejected executor output `crates/daemar-card/src/console.rs`; it was absent from the admission archive.

Verification:

- All five restored targets compare byte-for-byte equal to their archive entries.
- `crates/daemar-card/src/console.rs` is absent.
- `deny.toml` remains SHA-256 `1dc7f379b94a00de399bc7d4dd8df21cad2290be2cdba2e981a6d41562e62d9d`.
- `git diff --check` passes.
- `cargo test --locked --offline -p daemar-card --lib` passes: 31 tests, 0 failed.

No implementation, Card write, review, commit, or other deletion was performed.
