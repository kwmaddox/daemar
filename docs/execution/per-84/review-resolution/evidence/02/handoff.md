# Packet02 handoff — HTTP conformance and meaningful failure/absence tests

Executor scope was limited to `crates/daemar-card/src/console.rs` and this
packet's evidence directory. No template repair was needed. Protected
behavior/features/browser files were not edited; no dependency, migration,
policy, instruction, or public API changes were made.

## Implementation by disposition

- CE-C: the existing outer method guard now returns `405`, `Allow: GET, HEAD`,
  and an empty body for rejected methods on root, Card, unknown, and CSS paths.
  Foreign-Host plus unsafe-method precedence remains empty `421`; no OPTIONS or
  write route was added.
- CE-I: replaced the placeholder storage tests with real temporary SQLite
  fixtures. A valid Reader is opened before a separate writable SQLx
  connection corrupts `cards.created_at` (queue failure) or one target
  `card_entries.payload` (stream failure). Accepted Host reaches `500`, while
  foreign Host remains empty `421`. Queue and stream assertions pin the
  required retained/absent markers. No query-count claim is made.
- CE-K: `render_failure_returns_fixed_plain_text_500` is asynchronous and now
  asserts exact `500`, `text/plain; charset=utf-8`, exact body
  `console rendering failed`, and no sentinel leakage by exact-body comparison.
- CE-L HEAD: one immutable real fixture compares GET/HEAD for queue, selected
  Card inspector, CSS, missing route, and missing Card. Status/content type
  match and every HEAD body is empty; GET controls are nonempty on content
  routes.
- CE-L payload absence: a typed stage event without optional payload is selected
  by its actual returned entry ID and asserts stage/summary/inspector with no
  payload element. A second typed stage event with an independent marker is a
  positive payload-element control.
- CE-F: the `Arc::new(reader)` state introduction now documents at the site that
  Axum requires Clone state for concurrent request tasks and that read-only
  Reader sharing carries no mutation.
- CE-G: `EntryView.schema_version` now derives from
  `entry.payload.schema_version()` instead of a chosen literal.

The only production response behavior changed was the standards-required
`Allow` header. All other changes are tests or the CE-F/CE-G behavior-preserving
repairs above.

## Initial and after results

The six named baseline tests all passed initially, but their original
assertions were insufficient; complete outputs are in the six
`baseline-*.log` files. The first post-edit focused compile attempt exposed
the missing `ConnectOptions` import and the restricted full console run
exposed only loopback `PermissionDenied`; both are retained in evidence. The
owned import was fixed, then the focused suite passed 13/13 with escalation.

Required focused cases, each run independently after repair, all exited 0:

| Case | Complete log |
|---|---|
| `unsafe_method_unknown_route_returns_405` | `after-unsafe_method_unknown_route_returns_405.log` |
| `unsafe_method_css_returns_405` | `after-unsafe_method_css_returns_405.log` |
| `host_rejection_precedes_unreadable_storage` | `after-host_rejection_precedes_unreadable_storage.log` |
| `queue_failure_has_no_queue_or_card` | `after-queue_failure_has_no_queue_or_card.log` |
| `stream_failure_has_queue_without_partial_card` | `after-stream_failure_has_queue_without_partial_card.log` |
| `render_failure_returns_fixed_plain_text_500` | `after-render_failure_returns_fixed_plain_text_500.log` |

The full private console suite passed 13/13 in
`after-console-escalated.log`. The complete library suite passed 64/64 in
`after-library-2.log`. The repository behavior gate passed 119/119 scenarios
and 627/627 steps in `after-just-check-2.log`; the same log also records 6/6
binary tests. Clippy, diff check, and formatting all exited 0 in their final
logs.

## Commands and exits

All logs contain the full command output, working-directory execution, and an
explicit `COMMAND_EXIT` line.

| Command | Exit | Log |
|---|---:|---|
| `cargo test --locked -p daemar-card --lib console::web::tests` (restricted) | 101 | `after-test-edit-console-2.log` |
| same (escalated) | 0 | `after-console-escalated.log` |
| each required focused test (six commands) | 0 | `after-<case>.log` |
| `cargo test --locked -p daemar-card --lib` | 0 | `after-library-2.log` |
| `cargo clippy --all-targets --all-features -- -D warnings` | 0 | `after-clippy-2.log` |
| `git diff --check` | 0 | `after-diff-check-2.log` |
| `just check` (restricted first attempt) | 101 | `after-just-check.log` |
| `just check` (escalated final) | 0 | `after-just-check-2.log` |

The restricted failures were environmental/owned repair-loop observations:
loopback bind permission for the first focused run, and the first Clippy/gate
run's owned unused import. No test or gate was filtered or skipped.

## C1–C16 executor checklist

- C1: unchanged; no fallible public signature or foreign error boundary changed.
- C2: unchanged; no error-helper crate or derived error implementation added.
- C3: unchanged; no error payload was stringified or retyped.
- C4: unchanged in production; existing panic-capable setup remains test-only.
- C5: unchanged; no lint suppression was added.
- C6: unchanged; no string dispatch was introduced.
- C7: unchanged; fixture values remain private tests and public domain types are retained.
- C8: unchanged; production storage/render errors retain existing typed paths.
- C9: unchanged; router/rendering seams remain private and no public API changed.
- C10: unchanged in this packet; shared storage query repair belongs to Packet01.
- C11: unchanged; no unnecessary clone or ownership boundary was introduced.
- C12: addressed at `console.rs:535-537`: the Arc state reason is documented at its introduction, with read-only sharing and no mutation.
- C13: unchanged; startup publication and buffering contracts were not altered.
- C14: addressed at `console.rs:337`: the entry inspector now uses the named domain schema-version accessor rather than an unnamed literal.
- C15: unchanged; no boolean parameter was added.
- C16: unchanged; the tests and small fixture helpers each stay at their abstraction level.

## Changed paths and hashes

Changed by this packet:

- `crates/daemar-card/src/console.rs`
- `docs/execution/per-84/review-resolution/evidence/02/` logs, snapshots,
  hashes, and this handoff

No templates changed. No dependency changes occurred. The owned source hash
before edits was recorded in `baseline-state.txt`:

```text
2620b57f9982e1df54d8dfb8d75edc00c7097ead21b39c20a7f7d2e4ba5d7d96  crates/daemar-card/src/console.rs
```

Final hashes are in `final-source-hashes.txt`:

```text
d2c6ade32004d2dff0f5ebe473e992587b5f38499a304fd9b273be2d62f3f537  crates/daemar-card/src/console.rs
9ab7796562b40de277cb140d9e13647e915d6e06c82ac3e352c00578ad8e2219  crates/daemar-card/templates/queue.html
e410217046de6e2389d3598115413d9eb366da82ee06a5f338a565e48867d936  crates/daemar-card/templates/error.html
```

## Protected files and limitations

`protected-current-hashes.txt` matches the admission hashes for
`browser/tests/console.spec.ts`, `crates/daemar-card/tests/behavior/console/{http,mod}.rs`,
and `crates/daemar-card/tests/features/console.feature`. The existing unrelated
dirty and untracked working-tree files were preserved. No commit or Card write
was made.

The historical missing test-first evidence remains a limitation as required by
the plan: the baseline cases were already green because they were weak tests,
and no artificial red was manufactured. CE-D remains refuted and untouched.
Packet03 owns the final browser/TypeScript gate and any explicitly transferred
CLI/final repair scope.
