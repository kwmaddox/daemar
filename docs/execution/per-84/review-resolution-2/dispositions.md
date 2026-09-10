# PER-84 CE-M/CE-N dispositions

Dedicated planner; 2026-09-09. Evidence refers to the inspected working
tree at HEAD `4c809b31264f6c147563c52604d6aea598487ccd`, before this
follow-up. This is a disposition of supplied advisory findings, not a
fresh broad review. No source changed during planning.

| Finding | Disposition | Concrete evidence and action |
| --- | --- | --- |
| CE-M | Accept narrowed finding; retain withdrawn limbs | `crates/daemar-card/tests/behavior/console/mod.rs:591-599` omits refresh content from URL attributes and meta from embedding bans; `:2272-2289` checks only those attributes. The same scan already gets an unknown-Card 404 at `:2209-2210`. `browser/tests/console.spec.ts:81-106` fetch-checks only three healthy routes. `crates/daemar-card/tests/features/console.feature:402-416` requires every response reference to be same-origin, and selected Card16 explicitly includes meta refresh; Card18 removes the extractor but records no refresh exclusion. Add delayed-refresh target handling to the existing Layer-1 origin oracle, with isolated positive/negative fixtures. |
| CE-N | Accept title-only finding; retain blanket-form withdrawal | `crates/daemar-card/tests/behavior/console/mod.rs:1910-1921` obtains only three healthy pages; `:1925-1933` lists the title's healthy sinks; `:2016-2021` assumes all responses are 200. `crates/daemar-card/templates/error.html:1` separately renders `card.title` under queue-row/title, while the accepted outline at `crates/daemar-card/tests/features/console.feature:327-337` requires every sink. Extend only the Title row with real unknown-Card 404 and corrupt-stream 500 responses and queue title assertions. |

No rejected survivor or promotion of withdrawn material is proposed.
Specifically, CE-M's 421 limb remains withdrawn because
`crates/daemar-card/src/console.rs:413-419` returns an empty body. The
zero-delay limb remains withdrawn on the supplied Card87 refutation;
`browser/tests/console.spec.ts:239-253` shows assertions after navigation,
but this planning pass has not reproduced a zero-delay timing result and
does not claim one. Delayed content is the bounded counterexample.

CE-N's other ten fields remain excluded from added error-page coverage:
`templates/error.html:1` renders only Card ID and title in its queue rows,
and the ID is the typed route/identity marker, not one of those hostile
outline fields. Card45/47 do not exclude the title sink. The chosen 500
fixture corrupts an entry payload through the existing
`console/mod.rs:2478-2492` fixture; accepted S3-B9 at
`console.feature:464-478` explicitly keeps the queue on that error page.
A queue-decoding failure deliberately omits the queue and cannot prove
this title sink.

One bounded fresh-Luna task is sufficient. The only admitted source edit
is console/mod.rs; focused oracle self-checks are invoked by existing
steps because Cargo.toml:20-22 disables the default test harness. All
existing status/sink assertions remain effective. No test feature,
browser, production, dependency, policy, migration, or public-surface
change is authorized. No current production defect or authority gap was
found. The source snapshots, commands and outputs from execution must
establish the resulting candidate; the predecessor's green run cannot
substitute for that evidence.

Plan: `docs/plans/per-84-error-page-test-resolution-plan.md`.
Direct dispatch: `docs/execution/per-84/review-resolution-2/task-01.md`.
No plan review, independent review, Card append, or commit is part of this
planning task. Subsequent re-verification is limited to these two claims
absent a production-surface change (Card88).
