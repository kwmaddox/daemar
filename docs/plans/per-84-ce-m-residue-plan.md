# PER-84 CE-M delayed-refresh residue

Executor: one fresh **gpt-5.6-luna** agent, directly dispatched from
`docs/execution/per-84/review-resolution-3/task-01.md`. No plan review.
Root verifies commands and orchestrates; it does not perform semantic
code/test review. Review after execution is limited to the CE-M residue.

Disposition and rationale are in
`docs/execution/per-84/review-resolution-3/dispositions.md`.
Choose the review's fail-closed alternative. Retain the present supported
refresh extraction and assert that every recognized refresh yields a
target before applying the existing same-origin check. Do not treat an
unsupported refresh as absent metadata. Retain non-refresh success.

The one accepted test seam is the existing parsed-Document origin oracle,
`assert_document_urls_same_origin`, as exercised by
`check_same_origin_oracle_fixtures` from the ordinary S3-B8 step. Fixtures
are complete isolated DOM inputs whose expected accept/reject outcome is
independent of extractor internals. Cargo's behavior target is
`harness = false`; helper `#[test]` functions alone would not run.
This seam is already authorized by planner-input and the predecessor;
the TDD skill requires no additional confirmation for it.

1. Admit the dirty baseline, preserve owned-source before snapshot and
   hashes/status including untracked source, and establish complete log
   recording. Add one comma-separator foreign fixture through the seam;
   run the focused S3-B8 scenario and retain its actual initial result.
2. Make the smallest fail-closed repair in the owned file, keeping metadata
   classification separate from optional extraction failure. Re-run the
   tracer. No production changes or parser expansion.
3. Add the omitted-prefix foreign fixture and the combined comma/bare
   foreign fixture through the same seam. Preserve their first actual
   results even if already green. Pin the chosen conservative fallback
   with same-origin unsupported comma and bare forms, plus missing/empty
   refresh content; these must reject. All existing positive/negative
   controls remain. Do not manufacture red or remove the repair to obtain it.
4. Run focused regression scenarios, static checks and a C1-C16 conformance
   assessment of the actual owned diff; resolve in-scope diagnostics.
   Final unfiltered `just check`, `just browser`, and browser-local
   `./node_modules/.bin/tsc --noEmit` must all pass with complete command,
   cwd, output and exit evidence. Preserve failed attempts separately.
5. Hand off before/final source, baseline-relative patch and scope/hash
   verification, test counts/results, C-ID assessment, and remaining
   limitations. No source outside console/mod.rs, Card writes, commits,
   agents, plan review or broader review.

The disposition is bounded: a defect in the test instrument, not a
demonstrated production meta-refresh issue. This plan does not reopen
CE-N, 421 or zero-delay behavior, and makes no complete refresh-parser or
runtime navigation claim. Existing Layer-1/Layer-2 scope remains intact.
