# PER-84 Task 1 license repair handoff

## Scope and authority

Executed `docs/execution/per-84/task-1-license-dispatch.md` as a clean-context
bounded policy repair. The only policy change is the operator ruling at Card
sequence 61: allow `BSD-3-Clause` specifically for `matchit 0.8.4` via `axum
0.8.9`. No code, tests, dependencies, lockfile, or Card entries were changed.

## Exact diff

```diff
--- a/deny.toml
+++ b/deny.toml
@@ -12,6 +12,8 @@ allow = [
     "Apache-2.0",
     "MIT",
     "Unicode-3.0",
+    # 2026-09-08: matchit 0.8.4 (via axum 0.8.9) — BSD-3-Clause dependency license.
+    "BSD-3-Clause",
     # 2026-08-28: foldhash (via hashbrown ← sqlx) — permissive, OSI-approved.
     "Zlib",
 ]
```

## Evidence

- Manifest/lock inspection confirmed `axum = { version = "0.8", default-features = false }`, resolved `axum v0.8.9`, and `matchit v0.8.4`.
- Before edit: `cargo deny check licenses` failed because `matchit v0.8.4` declares `MIT AND BSD-3-Clause`, and `BSD-3-Clause` was not explicitly allowed. Full output: [`task-1-license-pre.txt`](task-1-license-pre.txt).
- After edit: `cargo deny check licenses` passed with `licenses ok`. Full output: [`task-1-license-post.txt`](task-1-license-post.txt).
- First normal `just check` run passed all five structural checks but failed before the full gate because the advisory database lock was on a read-only path. Full output: [`task-1-license-just-check.txt`](task-1-license-just-check.txt).
- Escalated `just check` ran the full gate: advisory, bans, licenses, and sources passed; unit tests passed (`39 passed`); behavior suite reported the known missing-`serve` intermediate state (`119 scenarios: 57 passed, 62 failed`; `413 steps: 351 passed, 62 failed`). Full output: [`task-1-license-just-check-escalated.txt`](task-1-license-just-check-escalated.txt).

## Result

Task 1 license policy repair is complete. The repository-wide gate remains red
only on the pre-existing absent-`serve` behavior failures; this task did not
attempt Task 2 or any implementation repair.
