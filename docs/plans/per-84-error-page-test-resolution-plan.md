# PER-84: narrow error-page test resolution

Executor: one fresh `gpt-5.6-luna` context, dispatched directly with
`docs/execution/per-84/review-resolution-2/task-01.md`. No plan review.
Baseline inspected: working tree at HEAD
`4c809b31264f6c147563c52604d6aea598487ccd`, 2026-09-09. This plan is an
artifact of the dedicated resolution planner, not a new review or evidence
that its proposed changes have passed.

Authority is the supplied `planner-input.md`, `findings.json` (Card87/88),
selected `referents.json`, `../contract.md`, AGENTS.md, CONTEXT.md, and
conventions.md. Dispositions are beside the dispatch packet. Prior green
command evidence is `../review-resolution/completion.md` and its
`evidence/03/handoff.md`; preserve it as historical evidence. Root verifies
commands and orchestrates, without semantic code/test review (Card74).

## Accepted work and ownership

Accept CE-M only for the delayed foreign-origin meta refresh on the 404
error page. Accept CE-N only for the hostile title at the error template's
queue-row sink, covering an unknown-Card 404 and corrupt-stream 500 with a
readable queue. Do not promote the withdrawn 421, zero-delay, or all-fields
claims. No production defect has been established by this planning pass.

Only implementation-owned path:
`crates/daemar-card/tests/behavior/console/mod.rs`. Add evidence under
`docs/execution/per-84/review-resolution-2/evidence/01/`. All test helpers
and focused oracle self-checks can fit in the existing console harness;
no extra source file, dependency, feature, browser test, runner, policy,
migration, public API, or rendering edit is needed. Preserve the dirty
baseline and every existing acceptance assertion. Exact additive coverage
is expressly authorized by this planning input despite the predecessor's
acceptance-file protection.

The approved seams are the running CLI console's HTTP responses, parsed
DOM assertions at the existing S3-B7/S3-B8 steps, and isolated HTML inputs
to those same test oracles. The TDD skill's seam confirmation is already
satisfied by this explicit scope; do not ask the operator again. Follow
its small vertical slices and independent expectations. Working production
may immediately pass a newly added test. Do not invent historical red,
temporarily break a renderer, or weaken an assertion to obtain green.

## Slice 1: CE-M, delayed refresh origin check

`request_scan_pages` already obtains and status-checks the 404; retain its
five responses and all existing S3-B8 scans. Keep ordinary URL attribute
checks byte-for-byte in meaning. Extract their per-document assertion into
a small private helper only as needed to exercise the identical oracle on
isolated HTML and real HTTP bodies.

Add focused self-checks for a literal error document carrying
`<meta http-equiv="refresh" content="30;url=https://foreign.example/">`.
The strengthened origin oracle must reject it without waiting 30 seconds
or issuing network requests. Pair it with independent positive fixtures:
an ordinary error document with a local stylesheet, a same-origin delayed
refresh, and a non-refresh meta element. Include mixed-case refresh/URL
tokens, surrounding whitespace, and quoted target variants as small
literal cases. These validate the selected extraction, not a new browser
simulation. Do not claim comprehensive refresh-language coverage.

Use the existing `Document` HTML parser and `is_same_origin` URL resolver.
Recognize meta refresh independently of attribute order, with
ASCII-case-insensitive `http-equiv` value and URL token. Extract the
navigation target following the delay separator, allowing surrounding
ASCII whitespace and paired single/double target quotes, and apply the
same origin predicate used for the six existing URL attributes. Keep
delay-only reloads and non-refresh metadata distinct from URL-bearing
refresh content; do not silently skip a present target merely because
its URL token is mixed case. A small helper is enough; do not resurrect
the removed CSS/srcdoc/srcset/visibility machinery or ban all metadata.

Execute the negative fixture against the still-unstrengthened oracle
before adding meta handling and preserve the actual result. It should
demonstrate the reported false pass as a failed self-check expecting
rejection. Then implement only the test-oracle extension and run the same
fixture plus positive controls and the existing self-contained scenario.
If the observed initial result differs, preserve it and investigate only
this discrepancy. Do not report expected results as measurements.

Durable self-check wiring: this target has `harness = false`; an isolated
`#[test]` in console/mod.rs would not run under the normal gate. Use a
plain private self-check function called by the existing same-origin
step, before scanning real responses. To test an assertion's rejection,
use `catch_unwind` around construction and invocation on an isolated HTML
fixture; assert that the expected negative fixture panics and the positive
controls pass. Construct the DOM inside the closure, use the same helper
as the real scan, and keep the panic catch out of the real-response path.
Expected caught-panic diagnostics are fixture evidence, not production
failures; label them in the log/handoff. Do not suppress the panic hook
globally. Alternatively a non-panicking predicate shared by both paths
may avoid those diagnostics, provided all existing assertions remain
effective. Keep these self-checks small and in the admitted file.

## Slice 2: CE-N, title through both error responses

In `request_hostile_pages`, retain queue, Card, and inspector requests in
their original order. Only for `HostileField::Title`, append two responses:

1. GET `card_route(unknown_card_id())`, require exactly 404 and the existing
   Card-not-found error, with the known Card still present in its queue.
2. After saving all healthy responses, use the existing asynchronous
   `corrupt_payload(w, 1)` fixture on the primary Card, then GET its Card
   route. Require exactly 500, storage-failed error, and its readable queue.
   Sequence 1 exists in the title fixture; corruption touches the entry
   payload, which queue reading does not decode. Make the requesting step
   async to await this existing fixture. Do not corrupt title or timestamp,
   create a second fixture Card, or change production state outside this
   scenario's disposable database.

Assert neither error response has Card identity, stream/rows, inspector,
or payload. The first three responses must continue to have status 200.
For Title, require exactly five responses with ordered status expectations
`[200, 200, 200, 404, 500]`; all other hostile fields still require exactly
the original three successful responses. Replace the old blanket 200
assertion only with these stronger, exact page-specific expectations,
never with an allowed-status set or an unchecked error-page exemption.

Preserve every existing `hostile_sinks` tuple and assertion. Add the
error-page title checks for response indices 3 and 4, identifying the
queue row by the primary Card ID (`queue_row_for`) and checking its title
text equals the original hostile marker. Use the same DOM text/descendant
and whole-document safety assertions already used for the healthy pages;
the whole-document loop must now include both error responses. A small
private error-title assertion helper allows isolated oracle validation
without altering the other ten fields or their sink maps. No history CLI
read may be added after corruption to obtain the expected title: the
scenario already owns that marker and Card ID.

Validate the actual new error-title assertion on a literal escaped
queue-row/error fixture and its unescaped hostile-markup counterpart.
The escaped title must pass; the raw injected elements must be rejected.
Also require an absent title sink to fail, preventing vacuous success.
These are inert in-memory HTML documents, never template mutations.
Call the small private self-check from the title-only assertion branch so
it runs in the gate; the real responses remain uncaught. Initial controls
may already pass because the existing DOM primitives work. Record that
honestly, then run the full hostile outline and S3-B9 scenarios.

## Verification, evidence, and stop condition

The dispatch packet gives commands and evidence capture requirements.
Keep complete outputs and actual exits for initial fixture observations,
focused runs, any environmental failures/retries, and final gates.
Snapshot the owned source before each slice and after completion; record
HEAD/status and hashes of every candidate source/template/test/dependency
and policy file at admission/final, including untracked files. Preserve
the entire admitted candidate snapshot in a unique temporary directory.
Prove that every file outside the single owned test harness path stayed
unchanged, apart from this packet's evidence artifacts.

Finish with `git diff --check`, unfiltered `just check`, unfiltered
`just browser`, and `./node_modules/.bin/tsc --noEmit` from browser, all
green. Record measured scenario/test counts, not assumed predecessor
counts. Check the Rust test delta against C1-C16 in the executor handoff;
this is not an independent review. No review agents or plan-review stage
are authorized now. The later designated review is limited to CE-M/CE-N
per Card88 unless the production surface changes.

If a new real HTTP assertion fails, preserve its source, response and
command evidence, localize whether the failure is a test-fixture mistake
or production behavior, and repair only owned test mistakes. A proven
production defect requires explicit scoped ownership transfer from the
orchestrator/operator before any production edit. Report that concrete
dependency; do not preemptively expand scope. No such gap is presently
established. No Card writes, commits, or external messages.
