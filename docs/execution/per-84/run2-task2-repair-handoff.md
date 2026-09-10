# PER-84 run2 Task2 repair handoff

Status: complete for the assigned storage/lib slice. The predecessor handoff
was not accepted; this repair adds the missing acceptance matrix and reruns
the required verification. No protected external behavior tests, migrations,
Task1 files, manifest/policy files, or later-task implementation were changed.

## Implemented and tested

- `Reader` remains the sole approved read capability, re-exported from `lib`.
- Read-only existing-file open verifies bookkeeping without migration,
  creation, chmod, journal-mode changes, writer fallback, or immutable mode.
- Store list/history delegate to shared Reader query helpers.
- Schema comparison has separately recognizable pure cases for exact,
  empty/uninitialized, dirty precedence, nonempty proper-prefix Behind, first
  surplus Ahead, same-position Diverged, checksum mismatch, and first mismatch
  when several differences compete.
- Real temporary SQLite tests cover missing-file noncreation, empty refusal,
  current-schema success, dirty/checksum refusal, actual Reader write refusal,
  normal reads, live append visibility, one-card queue creation order, latest
  sequence timestamp, corrupt payload ignored by queue, selected-column
  corruption rejection, and existing/unknown/foreign entry membership.
- Existing Store write/read and Task1 tests remain present and green under the
  full library run.

## Durable command evidence

All files below contain command output, cwd, and exit status. SHA-256 digests
are included to make the evidence packet inspectable:

- `run2-task2-repair-fmt.txt`: `cargo fmt --all`, exit 0,
  `6ec6572a1b05080b1d128d9168d404606438ce55af98404dc3de2518ebc700e3`.
- `run2-task2-repair-focused.txt`: focused storage tests, 11/11 passed, exit
  0, `470c4087c92b361eca4ad8929074556da26c81a6592a5f62177f61a8e55ca650`.
- `run2-task2-repair-clippy.txt`: strict Clippy (`--locked`, all targets,
  all features, `-D warnings`), exit 0,
  `d272f31b5bcebce39c0ad35ae8b344280829abcb1695ceceeb217351b70b5d69`.
- `run2-task2-repair-lib-full.txt`: escalated full library tests, 51/51
  passed, exit 0, `2b526ea530ded9434fd8e732a6c25759e05e81c4efcd6aed33116eb1eab9be9f`.
- `run2-task2-repair-just-check.txt`: escalated unfiltered gate, all lint,
  dependency, build, unit, and non-console behavior stages passed; expected
  successor absent-`serve` boundary is 57 scenarios passed / 62 failed (413
  steps, 351 passed / 62 failed), exit 101,
  `2866c3152eb47e7f7e12ef41fb8fb97859b10629432b5f42f10bc784df4dec9d`.

`git diff --check` passed. Candidate source hashes:

- `crates/daemar-card/src/storage.rs`:
  `aec7dbb8a763d7634b55fce01b23a0bfdf9f5491c91fbc9daa9173f4f25e7755`
- `crates/daemar-card/src/lib.rs`:
  `720c1963019e074499ace658befb5d1f00571b663ce01da263fe1f98ec8277f1`

The unprivileged baseline's seven loopback failures were environmental;
the escalated full library run passed all 51 tests. The gate's 62 failures are
the documented absent-`serve` successor scope, not Task2 storage failures.

## C1-C16 conformance checklist

C1 native crate error boundaries checked; C2 hand-written error/display
implementations preserved; C3 typed schema and storage error payloads checked;
C4 no production panic additions; C5 existing suppressions remain reasoned;
C6 enum dispatch preserved; C7 typed IDs/versions used; C8 no blanket error
conversion introduced; C9 no unapproved public item; C10 no duplicate domain
type; C11 ownership/borrowing follows existing store patterns; C12 no shared
mutable state or new async task; C13 no unnecessary formatting/allocation in
query/decoder paths; C14 no repeated fixture constants in new tests; C15
single authoritative schema/query ownership; C16 Reader verification,
storage errors, and Store write paths remain separate. Strict Clippy and the
repository ast-grep gate pass; no unresolved finding remains in this slice.

No Card escalation or commit was made. Root may independently inspect this
handoff and the candidate diff.
