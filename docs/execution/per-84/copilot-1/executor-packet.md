# PR37 Copilot bounded Luna fixes

Read dispositions.md and feedback.json beside this file, AGENTS.md, CONTEXT.md, conventions.md, docs/execution/per-84/contract.md, relevant source/tests. Fresh gpt-5.6-luna executor, no chat/memory/review definitions/agents. Root no semantic hints.

Own only crates/daemar-card/src/{storage,error,main,console}.rs plus docs/execution/per-84/copilot-1/evidence/01/ and handoff.md. Preserve unrelated dirty AGENTS.md/.pi/agents/PER83 docs, public API/error variants, migrations, acceptance features/browser assertions, manifests, justfile, historical evidence. No commits/Card/GitHub writes. Implement CP01-03 only, do not implement rejected CP04/summaryCP05.

1. Add queue_rejects_card_without_entries near existing queue tests. Healthy Card plus second Card whose entries alone are deleted via fixture writer, identity retained; assert Reader.queue Error::Corrupt Queue. Capture genuine initial result. An all-entryless case may use same setup.
2. Add entryless_card_queue_failure_has_no_queue_or_card using existing web fixtures/writable connection. Remove Card entries, request / and Card URL; assert500/storage failed/no queue/Card/stream/inspector/payload. Preserve existing complete-read pattern.
3. CP01 left join retains every Card; decode joined timestamp Option<OffsetDateTime>, rejectNone with existing Error::Corrupt Queue. Preserve actual malformed timestamp decode errors, onequery, rowidorder, highestseq, payloadindependence, no newpub/schema.
4. CP02 update error.rs top and ErrorCategory docs to five categories; main.rs module category description adds unavailable. No behavior change.
5. CP03 private constant in web::tests with deterministic router fixture rationale (not bound listener); derive valid and hostile Host authorities while preserving exact original spellings, malformed forms, methods and assertions. No production-default or other fixture refactor.

Read TDD skill/references for meaningful regression tests at existing seams; preserve actual before/after outputs and source snapshots, no invented historical red. Loop through ordinary owned compile/test/static/conventions failures until green. Required checks: named regressions, lib queue_, lib console::, full lib, full bin card, fmtcheck, strictClippy alltargets, gitdiffcheck, unfiltered just check, just browser, browser-local ./node_modules/.bin/tsc --noEmit. Use --locked for Cargo checks; no dependencieschange. Escalate environment permissions as needed; poll sessions to completion. Save COMPLETE logs/cwd/exits including failed attempts. No retrospective claims. Final handoff actualresults CPIDs, sourcehashes, scopeverification and C1-C16 checklist with sites/reasons. Root verifies commands, commits/pushes/posts/resolves.

