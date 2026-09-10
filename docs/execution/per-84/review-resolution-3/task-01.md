# Task 01 — CE-M fail-closed delayed-refresh oracle

Execute as one fresh **gpt-5.6-luna** agent in
`/Users/kendall/code/github/daemar`. Implement this packet completely;
no plan review or further delegation. Root orchestrates and verifies
commands only. Do not perform a broad review or reopen CE-N.

## Inputs and ownership

Read AGENTS.md, CONTEXT.md, conventions.md; this folder's
`dispositions.md`; `docs/plans/per-84-ce-m-residue-plan.md`;
`docs/execution/per-84/contract.md`; and
`docs/execution/per-84/review-resolution-2/evidence/01/handoff.md`.
Use the TDD skill at `/Users/kendall/.agents/skills/tdd/SKILL.md` and its
test/mocking references. The pre-agreed seam is specified below. Do not
consult chat, memories, old plans, reviewer definitions or whole Card
history. This packet supplies the required narrowed advisory disposition.

Sole source ownership:
`crates/daemar-card/tests/behavior/console/mod.rs`.
Evidence writes belong under
`docs/execution/per-84/review-resolution-3/evidence/01/` (temporary
snapshots/log staging are permitted). Preserve the pre-existing dirty and
untracked tree. No production, browser, feature, dependency, policy,
migration, API, Card or commit changes. No unrelated cleanup.

## Required outcome

`assert_document_urls_same_origin` currently skips the None returned by
`meta_refresh_target`. None combines non-refresh metadata with refresh
syntax the extractor does not handle. Close that false-pass path:

- Non-meta or non-refresh metadata remains ignored by refresh handling.
- A recognized meta refresh must yield a target; otherwise the oracle
  fails with a diagnostic naming unsupported/unresolved refresh content.
- Extracted targets still pass through the unchanged `is_same_origin`.
- Keep the existing `;url=` extractor's supported case/whitespace/quote
  behavior. Do not implement additional refresh grammar. Comma and bare
  target forms deliberately fail closed, including their local versions.
- Missing/empty content on recognized refresh fails closed too.

Use a narrow private representation or control flow that distinguishes
non-refresh from extraction failure. If adding a closed domain state use
an enum per C6, not magic strings/bools or a redundant parallel type.
No library public surface changes or new dependencies are required.
Keep the helper at one abstraction level (C16). Document the conservative
fallback without claiming it implements the complete WHATWG algorithm.

The primary [WHATWG refresh algorithm](https://html.spec.whatwg.org/multipage/semantics.html#shared-declarative-refresh-steps)
supports the supplied counterexamples (steps 10 and 11; planner checked
2026-09-09). The approved choice here is fail-closed oracle handling,
not additional parsing or browser navigation simulation.

## Behavioral fixtures and sequence

Use complete isolated HTML documents parsed through `Document::parse`,
calling the same `assert_document_urls_same_origin` used for real response
bodies. A should-reject fixture catches the oracle panic and requires
`is_err()` outside the catch; do not catch parse/setup failures as proof.
Positive fixtures call the oracle normally. Wire them into existing
`check_same_origin_oracle_fixtures`, which the ordinary S3-B8 step calls;
the behavior target has no default Rust test harness.

Start with one foreign comma fixture, content
`30,url=https://foreign.example/`, before altering the oracle. Run the
focused scenario and save its actual result and before-repair source.
Then implement the small fail-closed repair and get that tracer green.
Next add `30;https://foreign.example/` and
`30,https://foreign.example/` as separate labeled should-reject documents.
Record their actual first results; they may already pass after the first
repair. No fake assertion, production mutation, rollback or invented red.

Pin the chosen fallback with should-reject local unsupported forms
`30,url=/cards/local` and `30;/cards/local`, plus recognized refresh with
missing content and empty content. These are conservative rejection
controls, not claims that the browser would navigate off-origin.

Retain all existing controls: foreign `30;url=https://foreign.example/`
rejects; local stylesheet, mixed-case/whitespace/quoted local refresh, and
non-refresh metadata containing a foreign URL pass. Keep the old fixtures
or preserve each exact input and expected outcome in a small labeled
table. Every check remains effective on real responses; do not bypass the
404 scan, URL attributes, one-stylesheet checks, or error-page assertions.
Do not alter the closed CE-N title/sink checks or response collection.

## Evidence and validation

Before editing, capture HEAD, `git status --short`, the owned source and
a manifest of current tracked/untracked source inputs sufficient to
prove only the admitted source changed. Preserve original evidence files.
Use a small evidence command runner created with apply_patch if useful:
each run records exact argv, cwd, complete stdout/stderr and real exit
status; it must preserve failing statuses through tee/pipelines. Do not
overwrite failure logs on retry. Save before/after snapshots and a
baseline-relative unified patch because the owned file is untracked.

Commands, with cwd repository root unless explicitly specified:

```text
cargo test -p daemar-card --test behavior -- --name '^The console is script-free and self-contained$'
cargo test -p daemar-card --test behavior -- --name 'Producer-controlled text is inert at every sink'
cargo test -p daemar-card --test behavior -- --name 'An unknown Card is a 404 page with the queue|A corrupt row is a 500 that fabricates nothing'
git diff --check
just check
just browser
```

Finally, from `/Users/kendall/code/github/daemar/browser`:

```text
./node_modules/.bin/tsc --noEmit
```

Verify focused commands actually select their intended scenarios (1 S3-B8,
11 hostile rows, 2 named error scenarios), not zero tests. Use behavior
`--help` if needed. Expected final full suites are 119 Cucumber scenarios
and 11 Playwright tests, subject to actual measured output. Run the full
test/static/conventions loop, fix owned formatting/lint/test diagnostics,
then earn all three final unfiltered green gates on the final candidate.
Formatting only the owned file is permitted; preserve unrelated files.
Assess every C1-C16 against the actual test-helper diff, marking justified
non-applicability rather than declaring all private/test code exempt.

For sandbox loopback, advisory DB lock, network or Chromium restrictions,
retain the failure and request normal command escalation; do not weaken
checks or substitute predecessor results. If a failure requires changing
an unowned source or expanding scope, report exact evidence to root.

Deliver `evidence/01/handoff.md` with actual commands/cwds/exits/counts,
initial-result provenance, final hashes, ownership comparison, conformance
assessment, and the baseline-relative patch/snapshot paths. Explicitly
state the fail-closed tradeoff and no complete parser/navigation claim.
Return for the orchestrator's CE-M-only re-verification. Do not spawn a
reviewer, append a Card, commit, or announce independent verification.
