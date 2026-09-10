# PER-84 Task 2 repair-B handoff

Status: complete for the assigned storage/lib repair slice. The predecessor
handoff was not accepted; this packet repairs each diagnosed acceptance gap.
Only `crates/daemar-card/src/storage.rs` changed in this repair (the existing
`lib.rs` export remains unchanged). No protected feature/browser tests,
migrations, policy files, or later-task code changed.

## Repairs and evidence sites

- `compare_schema` now owns the complete deterministic precedence, including
  dirty-first and empty/uninitialized (`storage.rs:600-646`). Named private
  `ExpectedMigration`/`AppliedMigration` representations remove recurring
  unnamed tuple shapes (`:648-660`). `schema_comparison_precedence_covers_all_shapes`
  (`:911`) covers exact, uninitialized, nonempty Behind, Ahead, Diverged,
  checksum, dirty precedence, and earliest competing mismatch.
- `reader_connection_is_read_only_and_sees_live_writer_append` (`:1086`)
  uses a valid `created_at` write against a writable control, asserts the
  Reader's actual SQLite readonly error and no inserted value, then preserves
  live writer append visibility.
- `reader_entry_membership_distinguishes_card_and_entry_absence` (`:1205`)
  creates two real Cards and proves own-entry success, foreign-entry
  `EntryNotFound`, unknown-card `CardNotFound`, and missing-entry distinction.
- Empty, dirty, and checksum refusal tests (`:997`, `:1026`) snapshot actual
  main-file bytes, file length, and permission metadata after fixture
  checkpoint/quiescence and compare after Reader refusal. SQLite coordination
  files remain fixture-local and are not treated as main-file mutation.
- `queue_orders_cards_and_uses_latest_sequence_without_decoding_payload`
  (`:1144`) gives sequence 1 a later timestamp than sequence 2 and asserts
  sequence 2 wins; `queue_rejects_corrupt_selected_columns` (`:1176`) now
  rejects both entry `recorded_at` and Card `created_at` corruption while
  retaining corrupt-payload success coverage.
- C14 load-bearing repeated corruption data is named `CORRUPT_TIMESTAMP`
  (`:856`); fixture-only values remain scenario data under C14's explicit
  test-fixture exception.

## Test-first and command evidence

The predecessor's missing initial snapshot is explicitly preserved: it had no
saved initial-test/source evidence, so this handoff makes no retrospective TDD
claim. This repair captured the candidate source diff and initial compile-red
output before the final green reruns:

- `run2-task2-repair-b-source-before-green.diff` — SHA-256
  `74f8edd0d96e3434b08d45a49737e197b5fc3002a052b3d7022d2067edbb1038`.
- `run2-task2-repair-b-initial-compile.txt` — initial missing typed-helper
  compile red, exit 101, SHA-256
  `39eb87bb55a79a7f8a874aab1555f150ce34dba682d5faf4c0f6bdebb7c34356d`.
  `run2-task2-repair-b-initial.txt` records the attempted multi-filter command
  rejection (cargo CLI usage, exit 1), honestly retained rather than treated
  as a test result.
- `cargo fmt --all`: exit 0, `run2-task2-repair-b-fmt-final.txt`, SHA-256
  `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.
- Focused `cargo test --locked -p daemar-card --lib storage::store_seam_tests`:
  11/11 passed, exit 0, `run2-task2-repair-b-focused-final.txt`, SHA-256
  `03e66b0220231b1b46c61b20460684ba653067ddd7d023d41961515f71923c5d`.
- Strict `cargo clippy --locked --all-targets --all-features -- -D warnings`:
  exit 0, `run2-task2-repair-b-clippy-final.txt`, SHA-256
  `b701f1a779c143f0446fefaef03762fa24bda78d195a2bf54d0da406560ef8c4`.
- Escalated full `cargo test --locked -p daemar-card --lib`: 51/51 passed,
  exit 0, `run2-task2-repair-b-lib-full-final.txt`, SHA-256
  `7d8efdc92c559ae7ea1fa30bf4d6ce5e3095e4cd29784a58ede5f0a008749999`.
- Escalated unfiltered `just check`: repository lint/build/unit stages passed;
  documented successor boundary remains 119 scenarios, 57 passed / 62
  failed (413 steps, 351 passed / 62 failed), exit 101 solely because
  approved `serve` is still absent. Full output is
  `run2-task2-repair-b-just-check-final.txt`, SHA-256
  `94f30a1940a3b31881e7b79c6e5b6be3ba80d8077cb65bedccae33ff9f790569`.
  `git diff --check` passed.

## C1-C16 checklist

C1 native Error boundaries; C2 hand-written error/display and no new helper
crate; C3 typed schema/error payloads; C4 no production panic; C5 existing
reasoned suppressions only; C6 enum dispatch and one parse boundary; C7 typed
IDs/versions; C8 no blanket conversion; C9 no public API additions; C10
named private migration representations and existing QueueCard composition;
C11 no semantic clone changes; C12 no shared mutability/task; C13 no new
formatting/allocation issue; C14 named repeated load-bearing test fixture and
single-occurrence fixture literals; C15 no semantic bool parameter; C16
schema opening/verification/query ownership remains separated. Strict Clippy,
ast-grep, and the repository gate's completed stages found no concrete
convention violation in this repair.

Candidate source SHA-256: `storage.rs`
`7a020297c098ebc213d661ab0bfed0d79b00e5a2979d3db896024e6f6d074107`;
`lib.rs` unchanged candidate hash
`720c1963019e074499ace658befb5d1f00571b663ce01da263fe1f98ec8277f1`.

No Card escalation or commit was made. Root may inspect this handoff and the
candidate diff, then dispatch Task 3.
