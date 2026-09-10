# Fresh Luna task 01: PER-84 error-page test strengthening

You are the sole executor, model `gpt-5.6-luna`, in a fresh context.
Work in `/Users/kendall/code/github/daemar`. Execute directly: the operator
explicitly omitted plan review. Root orchestrates and verifies commands;
do not ask it for semantic code/test steering. No subagents, independent
review, Card writes, commits, or messages to external people/services.

Read these factory inputs and the implicated current repository referents:

- `docs/plans/per-84-error-page-test-resolution-plan.md` in full: this is
  the executable plan, including both fixture designs and stop condition.
- `docs/execution/per-84/review-resolution-2/planner-input.md`,
  `dispositions.md`, `findings.json`, and selected `referents.json`.
- `docs/execution/per-84/contract.md`, AGENTS.md, CONTEXT.md,
  conventions.md, and the TDD skill at
  `/Users/kendall/.agents/skills/tdd/SKILL.md` with its linked tests and
  mocking guidance. Its accepted seams are already specified in the plan.
- `docs/execution/per-84/review-resolution/completion.md` and
  `docs/execution/per-84/review-resolution/evidence/03/handoff.md` only
  for predecessor status/limitations; do not read old plans or reviews.
- The console behavior harness, DOM/HTTP helpers, relevant S3-B7/B8/B9
  feature text, error template, implicated console response paths,
  browser fetch/error scenarios, Cargo test-runner declaration and gate
  recipes as needed. No memories, chat, old failed plans, reviewer
  definitions, or broad review.

Own only `crates/daemar-card/tests/behavior/console/mod.rs` and new evidence
under `docs/execution/per-84/review-resolution-2/evidence/01/`. All current
dirty/untracked candidate files belong to the admitted baseline. No
production template/source edits or new test/source files. Use apply_patch
for edits. Additions to this exact acceptance harness are authorized;
weakening or removing accepted assertions is not.

## Execute

1. Record baseline HEAD, git status, and hashes. Snapshot the complete
   admitted candidate source (including untracked console, template and
   browser files), dependencies, tests and policy into a unique directory
   made with `mktemp -d`; preserve it, its path and manifest. Use explicit
   admitted paths, not a recursive snapshot of the entire workspace or
   build/dependency directories. Keep a separate before copy of the owned
   mod.rs in evidence. Do not overwrite predecessor evidence.
2. CE-M first: follow Slice 1 of the plan. Share the actual per-document
   origin assertion with inert literal fixture checks; run the delayed
   foreign-refresh negative fixture before adding detection. Preserve its
   actual outcome, then extend the oracle and prove the specified positive
   and negative controls plus the real self-contained scenario. Keep every
   ordinary attribute and existing scan assertion. Self-checks must execute
   in the normal cucumber gate, not sit in unused `#[test]` functions.
3. Snapshot the CE-M result. CE-N next: follow Slice 2 exactly. Append real
   title-only 404/500 responses after the original three; await the existing
   sequence-1 corrupt-payload fixture only after the healthy requests. Pin
   exact statuses, queue row identity/title text and absence of partial
   Card content. Preserve all old sink/status assertions. Validate the same
   new error-title oracle using independent escaped/raw/missing-title HTML
   fixtures; run the hostile outline and S3-B9 cases. Record immediate green
   honestly where production already satisfies the new assertions.
4. Run final checks below and write evidence/01/handoff.md, final source
   copy, hashes and outside-ownership equality proof. Document CE-M/CE-N
   implementation, actual initial/final observations, C1-C16 checklist,
   changed paths, full log/exit paths and remaining limitations. Historical
   missing evidence stays historical. No independent closure claim or
   reconstruction of old red evidence.

Useful commands, all from repository root unless stated:

```sh
git rev-parse HEAD
git status --short
cargo test -p daemar-card --test behavior -- --help
cargo test -p daemar-card --test behavior -- --name 'The console is script-free and self-contained'
cargo test -p daemar-card --test behavior -- --name 'Producer-controlled text is inert at every sink'
cargo test -p daemar-card --test behavior -- --name 'An unknown Card is a 404 page with the queue|A corrupt row is a 500 that fabricates nothing'
cargo fmt --all --check
git diff --check
just check
just browser
```

Confirm the installed cucumber CLI's `--name` semantics through help before
using focused commands; if filtering differs, use its documented equivalent
and record the exact command. A focused run with zero selected scenarios is
not success. If formatting is required, format only the owned mod.rs and
verify no other file changed; do not run a blanket rewrite on the dirty tree.
Final browser TypeScript check, cwd
`/Users/kendall/code/github/daemar/browser`:

```sh
./node_modules/.bin/tsc --noEmit
```

Capture complete stdout/stderr and actual command exit for each attempted
test/gate, including failures and retries, in distinct files. A logged
pipeline must preserve the tested command's exit, not only tee's exit.
Use a small apply_patch-created evidence runner if needed; log command,
cwd and `COMMAND_EXIT`, and return that exit. Do not erase failed logs.
Sandbox bind/network/environment errors require the normal escalation
mechanism and a separately logged retry; never count them as product red.
Use absolute evidence paths for the browser cwd.

Final acceptance requires all of `just check`, `just browser`, and local
`tsc --noEmit` green for this candidate, plus diff hygiene and an exact
outside-ownership comparison to admission. Report measured test counts.
The predecessor reported 119 scenarios/11 browser tests; those are context,
not a license to skip or claim this run. Root will independently verify
commands without conducting semantic review.

If new real HTTP coverage establishes a production failure, preserve its
full response and command evidence and report the precise production
ownership needed. Fix test-fixture/oracle mistakes within your file, but
do not alter production without explicit scoped ownership transfer. Stop
for that concrete dependency only; there is no current authority gap.
When done return the handoff path, changed-path list, results/log paths,
and any real blocker. Subsequent designated re-verification is CE-M/CE-N
only absent production-surface changes (Card88).
