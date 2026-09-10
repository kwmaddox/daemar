# PER-84 review-resolution plan

Card `01a0693e-bc16-7272-9ce9-a20f3b07f875`; baseline HEAD `4c809b31264f6c147563c52604d6aea598487ccd` plus the admitted working tree. Factory input: `docs/execution/per-84/review-resolution/planner-input.md`. Dispositions and evidence: `docs/execution/per-84/review-resolution/dispositions.md`. Findings are guidance; the disposition ledger is the planner's bounded judgment. Preserve the approved contract at `docs/execution/per-84/contract.md` and selected governing decisions. No implementation was performed during planning.

## Dispatch and ownership

Execute these packets sequentially, each in a **fresh gpt-5.6-luna** context. No plan review, new broad review, review subagents, or root semantic review. Root records artifacts, dispatches, and verifies actual commands. Executor owns the implementation and ordinary check/fix loop. No commits or Card writes by these executors; root owns recording this repair's artifacts.

1. `docs/execution/per-84/review-resolution/01-storage-dependencies.md`: CE-E/H; owns storage.rs, crate Cargo.toml and narrow lockfile repair.
2. `docs/execution/per-84/review-resolution/02-http.md`: CE-C/F/G, HTTP parts of I, K/L; owns console.rs, private tests, and evidence-triggered minimal existing-template repairs.
3. `docs/execution/per-84/review-resolution/03-cli-final.md`: CE-A/B/J and CLI part of I; owns main.rs and focused binary tests, then explicitly receives predecessor repair-file ownership for final ordinary in-scope check/fix work.

All packets read `docs/execution/per-84/review-resolution/executor-common.md`, this plan, their exact named contract sections, and the ledger. Root dispatches the packet pathname and common input pathname directly; the packet is the task, not an invitation to recover intent from chat. Prior candidate evidence is `docs/execution/per-84/run2-review-ready.md` and `run2-final-repair-hashes.txt`. Preserve it, together with `admission.md`. New evidence is scoped below `docs/execution/per-84/review-resolution/evidence/01/`, `02/`, and `03/` respectively.

## Test and evidence discipline

The TDD skill `/Users/kendall/.agents/skills/tdd/SKILL.md`, `tests.md`, and `mocking.md` informed these packets. Use behavior tests and real storage, with the already approved private router/rendering and binary orchestration seams. Approval of these seams and behavior-preserving review repairs overrides generic public-only, no-internal-seam and no-refactoring restrictions; do not ask for their approval again.

Write each behavioral regression test before its corresponding behavior repair, run it, and preserve the actual initial result. CE-C should expose the missing header. Newly strengthened tests of already-correct behavior may pass initially; label that honestly. Minimal extraction of current behavior into an approved private injection boundary is authorized and does not require an invented red. Preserve before/after source and outputs for that extraction. CE-A/E/F/G/H are standards or behavior-preserving repairs; baseline/after verification suffices. Do not claim this repairs the missing historical test-first record. No forced broken implementation, artificial assert-false, fake older timestamp, or retrospective red claim.

## Completion and continuation

Each packet must finish its focused checks and full unfiltered `just check` with no failures before normal handoff. It must repair ordinary compile, formatting, Clippy, test, SQL fixture, dependency-lock, and runtime failures in its owned scope. A command failing because of sandbox network/socket/advisory-database restrictions must be retried through the documented permission mechanism, retaining the failure and rerun; an environment failure is not behavioral red or permission to skip a gate. If dependencies are already available, do not reinstall them.

After packet03's last code change, run complete `just check`, complete `just browser` (which builds the current binary), and `./node_modules/.bin/tsc --noEmit` with cwd `browser`. Capture full output and actual exit of each; a filtered result or partial output summary is insufficient. Require all gate stages completed and all admitted119 behavior scenarios and11 browser tests passing; additions may increase counts. Record actual library/binary test totals, not baseline totals copied from Card79. No skipped/ignored accepted scenarios, test retries, relaxed timeouts, or policy edits. Verify protected file hashes against admission/current baseline. Post-check source changes require affected checks plus the final three-command barrier again.

All Rust repairs include an executor-authored C1–C16 conformance checklist with exact changed sites and any resolutions. This is a bounded response to an existing review, with no new review stage; root does not turn the checklist into its own semantic finding lane. Keep reports explicitly executor-authored rather than claiming independent verification of their semantics.

If an actual failure requires changing approved public surface, protected acceptance files, migrations, dependency policy, or unrelated user changes, save the exact command/output, smallest reproducer, affected authority and proposed change, then report missing authority to root. Mere absence of historical red, a newly passing coverage test, or routine owned-code errors is not a missing-authority stop. A repeated repair failure requires the executor to diagnose from evidence and narrow its next attempt, not ask root for semantic hints. No new product scope is authorized.

Final output `evidence/03/handoff.md` identifies every CE disposition/repair and its evidence, retains CE-D's refutation and CE-I's historical limitation, explains CE-G's promotion, gives all command exits/log paths and candidate hashes, enumerates modified files, and states any remaining limitation. Full all-green gates and complete artifacts are the completion condition; no mandatory second independent review is added by this plan.
