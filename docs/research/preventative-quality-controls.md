# Preventative quality controls for agent-authored code — evidence pass

**Researched 2026-08-23.** Companion to `rust-quality-enforcement.md` (the
detective stack). Question: what are practitioners, harness builders, and
researchers **actually doing, with reported results**, to raise agent code
quality *before and during* generation — as opposed to post-hoc scanning?

The four controls previously captured in Linear PER-71 (codebase-as-prompt,
typed-design-first, gates-in-the-generating-agent's-loop, salience-designed
standards docs) were stated from prior knowledge. This pass treats them as
**hypotheses to confirm, refute, or refine** against sources, hunting for
disconfirming evidence, not just support.

Every claim carries an evidence-quality label:
**MEASURED** (eval/benchmark/controlled study) · **PRACTITIONER** (field
report from someone who deployed it) · **VENDOR** (maker's claim about their
own tool) · **THEORY** (reasoned, not yet borne out). Where something was
searched for and not found, that is said explicitly and distinguished from
"does not exist."

## Verdicts up front

1. **The central practical question — how to keep gates coupled to the
   generating agent — has a convergent, deployed answer: harness-level event
   hooks, not instructions.** Every system found that makes in-loop
   enforcement work reliably (aider, SWE-agent, Claude Code hooks, Cursor
   hooks, Factory.ai's platform) uses the same architecture: the *harness*
   runs the check deterministically on a lifecycle event and injects the
   failure into the generating agent's own context while it still holds
   working state. The model never decides whether the check runs; it only
   decides how to fix the failure. Kendall's field observation — "run these
   checks when done" gets lost; orchestrator-run checks decouple into
   detective controls — is corroborated by practitioner consensus and by
   instruction-capacity research, and the hook architecture is precisely the
   repair for it.
2. **Hypotheses 3 (gates in the loop) and 4 (salience-designed docs) are
   CONFIRMED — 4 with a sharpening: docs are the weakest control and decay
   within a session.** Hypothesis 1 (codebase-as-prompt) is **supported but
   only indirectly measured**. Hypothesis 2 (typed-design-first) is
   **plausible-with-adjacent-evidence: nothing directly measures pre-written
   type skeletons**, but type-constrained generation research and the
   spec-first tooling wave both point the same way.
3. **Rust-specific in-loop tooling exists but is young**: rust-analyzer/cargo
   MCP servers, a Claude Code Rust plugin with 16 on-edit hooks, and
   `conclaude` — a Rust CLI purpose-built to stop-gate Claude Code sessions
   behind `cargo fmt/clippy/test` — are all v0.1.x with minimal adoption. The
   *mechanisms* they package (hooks, LSP diagnostics) are mature; the
   packages are not. Building daemar's own thin gate wiring is lower-risk
   than adopting any of them wholesale.

---

## 1. The in-loop enforcement question (the central problem)

### 1.1 The failure mode is real and named

Kendall's experience — instruction-requested checks get lost — is not
idiosyncratic:

- Multiple reproducible reports of Claude Code ignoring its own instruction
  file: [issue #42863](https://github.com/anthropics/claude-code/issues/42863)
  ("rules are not reliably enforced — agent ignores its own instructions"),
  [issue #7777](https://github.com/anthropics/claude-code/issues/7777).
  **PRACTITIONER.**
- A practitioner-documented compliance decay curve within single sessions:
  ~95% compliance at messages 1–2, 60–80% by messages 3–5, 20–60% by messages
  6–10 ([TianPan summary of Siddhant Khare's
  data](https://tianpan.co/blog/2026-02-25-claude-md-agents-md-ai-coding-agent-instruction-files)).
  Informal but quantified. **PRACTITIONER.**
- The capacity mechanism is measured: the IFScale benchmark
  ([arXiv:2507.11538](https://arxiv.org/abs/2507.11538)) finds even frontier
  models reach only "68% accuracy at the max density of 500 instructions,"
  with bias toward earlier instructions and degradation as density rises.
  (Caveat: the benchmark task is keyword inclusion in report writing, not
  code style — transfer to coding instructions is plausible, not proven.)
  **MEASURED.**

An end-of-task instruction ("finally, run the checks") is a single
low-salience instruction competing at the point of maximum context pressure —
the structurally worst place for it. The evidence says: don't fight that
fight.

### 1.2 The deployed repair: deterministic hooks that inject failure into the generator's context

Verified against primary sources, in rough order of deployment maturity:

- **aider** (oldest deployed example): linting is **on by default** — "Aider
  will lint any files which it edits"; with `--auto-test`, the test suite runs
  after each edit. On non-zero exit "aider will try and fix any errors" — the
  command output is fed back to the *same model in the same conversation*,
  which attempts the fix and re-runs
  ([lint-test docs](https://aider.chat/docs/usage/lint-test.html)).
  **PRIMARY DOCS / PRACTITIONER-heritage** (years of field use).
- **SWE-agent** ([NeurIPS 2024 paper](https://proceedings.neurips.cc/paper_files/paper/2024/file/5a7c947568c1b1328ccc5230172e1e7c-Paper-Conference.pdf)):
  the agent-computer interface runs a linter *inside the edit action* —
  syntactically invalid edits are **rejected before they land**, with the
  error fed back to the model. Measured context: 51.7% of trajectories
  contained at least one failed edit, and recovery odds *decrease* as failed
  edits accumulate — evidence both that in-loop rejection is load-bearing and
  that fast first-failure feedback matters. **MEASURED.**
- **Claude Code hooks** ([official reference](https://code.claude.com/docs/en/hooks)):
  hooks "execute automatically at specific points in Claude Code's
  lifecycle" — the model cannot suppress them. The contract that matters
  here, verified per-event:
  - `PostToolUse` (after an edit): exit 2 "shows stderr to Claude; the tool
    already ran" — i.e. a gate script's failure output lands in the
    generating agent's transcript immediately after the edit that caused it.
  - `Stop`: exit 2 "prevents Claude from stopping, continues the
    conversation" — the completion gate. The agent cannot declare done while
    the gate is red; stderr becomes its next work item.
  - Practitioner consensus on the Stop-gate pattern
    ([fbakkensen](https://fbakkensen.github.io/ai/devtools/development/2026/03/27/quality-gates-for-coding-agents-how-stop-hooks-make-validation-mandatory.html),
    [zarar.dev](https://zarar.dev/agent-hooks-deterministic-guardrails-for-ai-generated-code/),
    [claudefa.st](https://claudefa.st/blog/tools/hooks/stop-hook-task-enforcement)):
    it works, with two hard-won caveats — (a) the `stop_hook_active` guard is
    mandatory or a permanently-red gate traps the agent in an infinite
    fix→stop→blocked loop; (b) "a Stop hook catches premature completion. It
    does not catch correctness bugs" — it is a completion gate, not a
    correctness gate. **PRIMARY DOCS + PRACTITIONER.**
- **Cursor hooks** ([docs](https://cursor.com/docs/hooks)): `afterFileEdit`
  runs linters/formatters on every agent edit; practitioners run lint/build
  verification per agent turn
  ([lirantal](https://lirantal.com/blog/cursor-stop-hook-lint-build-verification)).
  Same architecture, second major harness. **PRIMARY DOCS + PRACTITIONER.**
- **Factory.ai** ([Using Linters to Direct Agents](https://factory.ai/news/using-linters-to-direct-agents)):
  a commercial agent platform's stated architecture: "Agents generate code,
  get automatic feedback from linters, and self-learn/iterate until clean,"
  with linters wired "inside the agent toolchain" and lint-passing treated as
  the definition of done and the merge gate. Qualitative results only ("each
  rule you codify reduces review overhead"). **VENDOR/PRACTITIONER.**

**The pattern all five share** — and the answer to the
orchestrator-decoupling problem: the check runs on a *harness event* (edit
made, agent stopping), not on the model's initiative, and its failure output
is injected into the **generating agent's own context** while the generation
context is still warm. The orchestrator pattern Kendall found lacking differs
in exactly this one property: the failure lands in a different context than
the one that produced the code. The evidence supports the working hypothesis:
**the dividing line between preventative and detective is whether the failure
reaches the agent that still holds the working context.**

What was searched and not found: any credible report that
*instruction-requested* end-of-task checks are reliable in long sessions. The
absence is conspicuous; every practitioner source that discusses it at all
recommends moving the check into hooks.

### 1.3 Rust-specific in-loop tooling (young but real)

- **`conclaude`** ([lib.rs](https://lib.rs/crates/conclaude)): a Rust CLI
  built precisely for the stop-gate: its `.conclaude.yaml` runs
  `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`
  sequentially fail-fast on the Stop hook and "blocks session completion if
  any check fails." v0.1.1 (2025-09), self-described unstable, minimal
  adoption. Useful as an existence proof and design reference; not yet an
  adoption candidate. **PRIMARY (repo), immature.**
- **[zircote/rust-lsp](https://github.com/zircote/rust-lsp)**: Claude Code
  plugin — rust-analyzer LSP integration plus "16 automated hooks" (fmt,
  check, clippy, cargo-audit, cargo-deny, unsafe-code detection), all
  `afterWrite`, non-blocking (surface diagnostics rather than reject).
  v0.1.2, 12 stars. **PRIMARY (repo), immature.**
- **rust-analyzer / cargo MCP servers**
  ([Vaiz/rust-mcp-server](https://github.com/Vaiz/rust-mcp-server),
  [zeenix/rust-analyzer-mcp](https://github.com/zeenix/rust-analyzer-mcp)):
  expose diagnostics, clippy, cargo test to the agent as tools. These make
  checks *available*, which is weaker than making them *unavoidable* — the
  model still chooses to call them. **PRIMARY (repos).**
- Practitioner reports of the cargo-check-on-edit loop for Claude Code +
  Rust: "Claude Code sees compiler output after every edit and can
  immediately fix borrow checker errors in the next tool call"; stricter
  clippy configs are reported as *more* effective with agents because "more
  feedback results in better AI output"
  ([getbeam.dev](https://getbeam.dev/blog/claude-code-rust-development.html),
  [clauder-navi](https://www.clauder-navi.com/en/claude-rust)). Consistent
  with the detective-pass finding that agents respond to hard failure better
  than guidance; also note `cargo check` (3–5× faster than build) as the
  per-edit check, clippy at coarser grain. **PRACTITIONER, informal.**

Rust holds one structural advantage worth naming: the compiler itself is the
strongest in-loop critic in any mainstream language — borrow checker and type
errors are deterministic, local, and actionable, exactly the feedback shape
agents handle best. The L0–L2 gates extend that critic to style; the hook
architecture is how both reach the agent mid-generation.

### 1.4 Strands steering — the generalized architecture (added same day)

[Strands Agents](https://strandsagents.com/) (open-source, 7k+ stars,
Python/TypeScript, model-agnostic, shipped with a dedicated
[harness-sdk](https://github.com/strands-agents/harness-sdk)) names and
productizes the in-loop pattern as
[**steering**](https://strandsagents.com/docs/user-guide/concepts/plugins/steering/):
"modular prompting for complex agent tasks through context-aware guidance
that appears when relevant, rather than front-loading all instructions in
monolithic prompts." Their rationale is the same failure mode measured in
§1.1: "for tasks with 30+ steps, monolithic prompts become unwieldy: agents
ignore instructions, hallucinate behaviors, or fail to follow critical
procedures." **PRIMARY (vendor docs).**

Mechanism: **context providers** observe the agent and populate structured
state (built-in: a tool ledger of every call with inputs, timing, outcome);
**steering handlers** fire at two lifecycle points —

- **before a tool call**: `Proceed` / `Guide` (cancel the tool, inject
  corrective feedback into the agent's context) / `Interrupt` (pause for
  human input);
- **after a model response**: `Proceed` / `Guide` (discard the response and
  retry with injected guidance).

Handlers are rule-based or LLM-judged (`LLMSteeringHandler`: a narrow
plain-English system prompt judging one question at the moment it matters).

What this adds beyond §1.2's hook findings:

1. **Pre-execution rejection generalized to policy.** Hooks are
   post-hoc-in-loop (check after the edit lands). Steering's before-tool
   `Guide` bounces a proposed action *before it executes* — for daemar,
   structural checks against the *proposed* `.rs` content, rejected with the
   violated rule before the file lands. SWE-agent's measured in-edit
   rejection (§1.2) is this shape for syntax; steering generalizes it.
2. **Guide-and-retry on responses** — the discard/retry-with-guidance loop
   with the failure re-entering the same generation context.
3. **Just-in-time convention injection** — the direct answer to "how do we
   direct *specific coding-convention choices*": inject the one relevant
   convention at the moment of the relevant action ("post-it notes"), rather
   than only a front-loaded doc competing for salience session-wide.
   Front-of-context placement, tiny instruction count, maximal relevance —
   the §1.1/§2 salience findings working *for* the convention.

Adoption posture: Python/TS only — for a Rust-native factory this is a
**reference architecture** (context providers → handlers →
Proceed/Guide/Interrupt), not a dependency. Combined with the owned-harness
correction in §2, the convention-delivery design for daemar's harness is
three-tiered, all channels owned: (1) a first-class conventions file injected
at system-prompt level with authoritative framing; (2) deterministic steering
— pre-write structural checks, post-edit `cargo check`, stop-gate full chain;
(3) just-in-time convention injection for the judgment tier, rule-based where
deterministic, LLM-judged only where genuinely judgment.

---

## 2. Steering artifacts: what makes instruction files followed vs. ignored

Beyond the decay/capacity evidence in §1.1:

- **Anthropic's own harness deprioritizes the file by design**: Claude Code
  wraps CLAUDE.md content in a system reminder that it "may or may not be
  relevant to your tasks... You should not respond to this context unless it
  is highly relevant" ([HumanLayer's
  analysis](https://www.humanlayer.dev/blog/writing-a-good-claude-md), quoting
  the injected wrapper). An instruction file is fighting its own delivery
  mechanism. **PRIMARY (observable in the harness) via PRACTITIONER
  write-up.**
- **HumanLayer's field guidance** (same source): keep the file under 300
  lines (theirs is under 60); include only universally applicable
  WHAT/WHY/HOW; move everything else to referenced per-topic files the agent
  reads on demand (progressive disclosure); avoid code snippets (they go
  stale). They explicitly cite the IFScale capacity finding: ~150–200
  instructions followed with reasonable consistency, ~50 already spent by the
  harness's own system prompt — and note that overloading degrades compliance
  *uniformly across all instructions*, not just the new ones. **PRACTITIONER,
  citing MEASURED.**
- **Examples carry measurable weight**: a controlled ablation on in-context
  examples for code generation finds identifier naming in examples matters up
  to 30 percentage points
  ([arXiv:2508.06414](https://arxiv.org/pdf/2508.06414)) — supporting
  examples-over-prose, though it measures example *quality*, not
  examples-vs-rules head-to-head. **MEASURED, indirect.**
- What was searched and not found: a controlled study of rationale-bearing
  rules vs. bare directives for code style compliance. The
  "examples + rationale" design remains **THEORY** with practitioner
  convergence.

**Sharpened conclusion:** the standards doc is the *weakest* preventative
control, and its effectiveness decays within a session. The design
implications beyond "keep it short": (a) budget instructions, don't just
trim lines — every rule competes with ~50 harness-builtins; (b) per-task
injection of the few rules that matter for *this* task beats a global file
(front-of-context placement exploits the measured earlier-instruction bias);
(c) anything expressible as a gate should be deleted from the doc — spending
instruction budget on machine-checkable rules is waste twice over.

**Owned-harness correction (added same day, after review):** the wrapper
evidence above is a fact about *Claude Code's* delivery mechanism, not about
instruction files as a category — this section overgeneralized. A harness
that owns its injection path (daemar's factory) can deliver a first-class
conventions file at system-prompt level with authoritative framing — a
categorically stronger channel than a deprioritized context file. The
capacity-budget and session-decay findings still apply *within* that channel;
the wrapper penalty does not. Kendall's point, and it stands.

**Review-referent reframe (added same day, Kendall):** this section evaluates
the doc in one channel only — generation-side injection, the measured *worst
case* for instruction-following (long context, task pressure, competing
instructions). The doc's primary role is a different channel entirely: the
**durable review referent**, where the salience profile inverts to the
measured best case — a review agent gets a fresh context, one job, and the
doc + diff front-loaded. Specs teach the same lesson: weak as driving
artifacts for the same reasons, load-bearing as the contract reviewed
against (this repo's `specs/sandbox.md` produced the PER-50..61 adversarial
findings exactly this way — and the review sharpened the referent in
return). Design consequence: one canonical conventions file with stable
citable IDs (the `specs/sandbox.md` behavior-ID pattern applied to style);
review findings, gate-rule messages, and per-task injection excerpts all
cite C-IDs; the instruction budget above governs the excerpts, not the
referent. The weakest-*steering*-control verdict stands; it is not a verdict
on the artifact.

---

## 3. Constraint-first workflows: spec-first, typed-first, test-first

- **Spec-driven development is a deployed wave, not advocacy**:
  [GitHub Spec Kit](https://github.com/github/spec-kit) (MIT, ~111k stars by
  mid-2026) structures agent work as
  constitution → specify → plan → tasks → implement, with steps repeating
  "until convergence"; it supports 30+ agents. Its premise: "Define what to
  build before building it." Community reports claim 60–80% fewer rework
  cycles vs. prompt-driven development
  ([rywalker's evaluation write-up](https://rywalker.com/research/github-spec-kit)
  describes a 12-feature-cycle comparison on a production SaaS app) — treat
  the numbers as **PRACTITIONER-informal**; the adoption scale is the harder
  evidence. Amazon Kiro is the same shape productized. **PRIMARY (repo) +
  PRACTITIONER claims.**
- **Type constraints measurably improve generation** — at the decoding
  level: PLDI 2025's type-constrained decoding
  ([arXiv:2504.09246](https://arxiv.org/pdf/2504.09246)) "reduces compilation
  errors by more than half and significantly increases functional
  correctness" across model families by enforcing well-typedness during
  token selection. Formalized for TypeScript; **no Rust implementation was
  found** (searched), and the technique needs logit-level access unavailable
  against hosted frontier APIs. Its relevance here is as *mechanism
  evidence*: constraining generation with the type system improves output —
  which is what a pre-written Rust skeleton does at the workflow level,
  compiler-enforced. **MEASURED (adjacent).**
- **Direct evidence that pre-written type skeletons improve agent code
  quality: searched and not found.** No study measures "human/agent writes
  signatures and error types first, second agent fills in" against free-form
  generation. What exists: the type-constrained result above, the spec-first
  adoption wave, repository-context research showing models violate project
  conventions when the relevant types/APIs aren't in context
  ([arXiv:2406.11927](https://arxiv.org/html/2406.11927v3)), and the
  detective-pass finding (arXiv:2605.02741) whose authors' own recommendation
  is equipping agents with "explicit architectural foresight." Typed-first
  remains **THEORY with strong adjacent evidence and a deterministic
  enforcement path** (the compiler holds the skeleton).
- The volume-law authors' framing — the problem is "architectural complexity
  management," not generation — supports front-loading design decisions into
  artifacts the agent cannot drift from (specs, skeletons, budgets). No
  published follow-ups citing 2605.02741 with deployed results were found
  yet (young paper; searched). **THEORY from MEASURED premises.**

---

## 4. Codebase-as-prompt: the indirect evidence

No study directly tests "keep the corpus exemplary and agents will maintain
its standards." What is measured:

- Models "frequently hallucinate APIs... or generate implementations that
  violate project conventions when relevant repository context is
  unavailable" — and providing in-repo context/examples improves adherence
  ([repository-level generation studies:
  arXiv:2406.11927](https://arxiv.org/html/2406.11927v3),
  [RACG survey](https://arxiv.org/html/2510.04905v1)). The contrapositive of
  the hypothesis, measured. **MEASURED, indirect.**
- In-context example quality (naming) shifts outcomes by up to 30pp
  ([arXiv:2508.06414](https://arxiv.org/pdf/2508.06414)). Surrounding code
  *is* the agent's in-context example set. **MEASURED, indirect.**
- An industrial case study found one-shot in-context examples gave
  "smaller but consistent improvements," with fine-tuning giving the largest
  gains ([arXiv:2604.24678](https://arxiv.org/html/2604.24678v1)) — a
  reminder that context steering has real but bounded effect size.
  **MEASURED.**

Verdict: **supported, indirectly**. The practical consequence stands on its
own: in a small workspace (daemar today), the existing crates dominate the
agent's Rust context, so corpus hygiene has outsized leverage — and the
detective gates are what keep the corpus from eroding.

---

## 5. Briefly surveyed and parked

- **Constrained decoding for style enforcement**: real research
  (PLDI 2025 above), not practical for daemar — no Rust support found, and
  hosted frontier models expose no logits. Revisit only if running open
  weights locally. **Parked with cause.**
- **Fine-tune/LoRA on house style**: established industry pattern for teams
  (style transfer is listed among strong LoRA use cases; production systems
  combine retrieval + adapters). For a solo operator on hosted frontier
  models: requires a held-out eval set, ongoing retraining across model
  generations, and either open-weight hosting or provider fine-tuning tiers —
  poor ROI while gates + context controls remain uncashed. **Parked with
  cause.**

---

## 6. Hypothesis verdicts (PER-71's four controls)

| # | Hypothesis | Verdict | Basis |
|---|---|---|---|
| 1 | Codebase-as-prompt | **Supported, indirectly measured** | Repo-context studies measure the contrapositive (missing context → convention violations); example-quality ablations show in-context code carries heavy weight. No direct corpus-hygiene study exists. |
| 2 | Typed-design-first | **Plausible; direct evidence absent** | Nothing measures pre-written skeletons specifically (searched, not found). Adjacent: type-constrained decoding halves compile errors (MEASURED); spec-first tooling deployed at 100k-star scale; volume-law authors recommend architectural front-loading. Deterministic once the skeleton exists — the compiler enforces it. |
| 3 | Gates in the generating agent's loop | **CONFIRMED — strongest-evidenced control in this pass** | Convergent architecture across aider, SWE-agent (MEASURED), Claude Code hooks, Cursor hooks, Factory.ai. The refinement: it only works as a *harness event*, never as an instruction — which resolves, rather than contradicts, Kendall's field experience. |
| 4 | Salience-designed standards docs | **CONFIRMED, and demoted** | Capacity research + decay reports + harness-level deprioritization confirm the design constraints (short, universal, examples) and simultaneously establish docs as the weakest layer. Sharpening: per-task injection > global file; delete every gate-expressible rule from prose. |

The ivory-tower objection was half right: control 3 *as previously stated*
("agents run checks while working") is unreliable exactly as Kendall
observed. The deployed, evidenced form is narrower: **the harness runs the
checks; the agent only ever sees the failures.** That distinction is the
finding.

---

## 7. Recommendations for daemar

Mapped to the factory architecture (one-shot sandboxed runs; orchestrated
single-task workflow), deployment-evidenced items first:

1. **Adopt the hook architecture for the current interactive phase now**
   (deployment-evidenced): a `PostToolUse` hook running `cargo check`
   (fast path) on `*.rs` edits, and a `Stop` hook running the full gate
   chain — `cargo fmt --check && cargo clippy --all-targets -- -D warnings
   && cargo deny check && ast-grep scan && cargo test` — exiting 2 with the
   failure on stderr. Include the `stop_hook_active` guard. This is the
   PER-71 control-3 implementation, corrected from instruction to mechanism.
   Write our own thin scripts; `conclaude` and `zircote/rust-lsp` are
   references, not dependencies (both v0.1.x).
2. **Design the factory's Generation Stage with the same property**
   (evidence-backed principle, novel application): the sandboxed one-shot
   run's driver should run the gate chain *inside the run* and feed failures
   back to the generating agent within its session — or, if the loop lives
   outside the sandbox, the orchestrator must re-enter the *same* generation
   context with the failure, not open a fresh fixer context. The property to
   preserve is failure-reaches-warm-context; SWE-agent's measured result
   (recovery odds fall as failures accumulate) argues for the fastest
   possible first feedback, i.e. `cargo check` granularity.
3. **Keep the standards doc, demoted and budgeted** (evidence-refined):
   under ~60 lines of universally applicable judgment-tier content; per-task
   injection of the few task-relevant rules into the ticket/prompt itself;
   zero rules that a gate already enforces. This revises PER-71's "~150
   lines" guidance downward and adds the per-task injection requirement.
4. **Typed-design-first: adopt as a workflow convention with honest
   labeling** (plausible-theory tier): the skeleton is cheap, reviewable,
   and compiler-enforced once written; but treat it as an experiment —
   measure rework/review friction on tickets with and without skeletons
   rather than assuming the benefit. Spec Kit's constitution/specify/plan
   phase structure is worth mining for the factory's task-authoring format,
   without adopting the toolkit.
5. **Corpus hygiene as standing policy** (indirectly evidenced): the gates
   exist to keep the exemplar clean; treat any merged violation as a
   context-poisoning event, not just a defect.

---

## 8. Sources

- Claude Code hooks: [official reference](https://code.claude.com/docs/en/hooks)
- Stop-gate practice: [fbakkensen](https://fbakkensen.github.io/ai/devtools/development/2026/03/27/quality-gates-for-coding-agents-how-stop-hooks-make-validation-mandatory.html) ·
  [zarar.dev](https://zarar.dev/agent-hooks-deterministic-guardrails-for-ai-generated-code/) ·
  [claudefa.st](https://claudefa.st/blog/tools/hooks/stop-hook-task-enforcement) ·
  [codingwithroby](https://codingwithroby.substack.com/p/the-stop-hook-that-wont-let-claude)
- aider: [lint/test docs](https://aider.chat/docs/usage/lint-test.html)
- SWE-agent: [NeurIPS 2024 paper](https://proceedings.neurips.cc/paper_files/paper/2024/file/5a7c947568c1b1328ccc5230172e1e7c-Paper-Conference.pdf) ·
  [issue #560 (new-vs-preexisting lint errors)](https://github.com/SWE-agent/SWE-agent/issues/560)
- Cursor hooks: [docs](https://cursor.com/docs/hooks) ·
  [lirantal per-turn verification](https://lirantal.com/blog/cursor-stop-hook-lint-build-verification)
- Factory.ai: [Using Linters to Direct Agents](https://factory.ai/news/using-linters-to-direct-agents)
- Rust tooling: [conclaude](https://lib.rs/crates/conclaude) ·
  [zircote/rust-lsp](https://github.com/zircote/rust-lsp) ·
  [Vaiz/rust-mcp-server](https://github.com/Vaiz/rust-mcp-server) ·
  [zeenix/rust-analyzer-mcp](https://github.com/zeenix/rust-analyzer-mcp) ·
  practitioner loops: [getbeam.dev](https://getbeam.dev/blog/claude-code-rust-development.html) ·
  [clauder-navi](https://www.clauder-navi.com/en/claude-rust)
- Instruction capacity/compliance: [IFScale, arXiv:2507.11538](https://arxiv.org/abs/2507.11538) ·
  [HumanLayer, Writing a good CLAUDE.md](https://www.humanlayer.dev/blog/writing-a-good-claude-md) ·
  [claude-code #42863](https://github.com/anthropics/claude-code/issues/42863) ·
  [claude-code #7777](https://github.com/anthropics/claude-code/issues/7777) ·
  [TianPan compliance-decay summary](https://tianpan.co/blog/2026-02-25-claude-md-agents-md-ai-coding-agent-instruction-files)
- Spec-first: [GitHub Spec Kit](https://github.com/github/spec-kit) ·
  [rywalker evaluation](https://rywalker.com/research/github-spec-kit)
- Type constraints: [Type-Constrained Code Generation, PLDI 2025, arXiv:2504.09246](https://arxiv.org/pdf/2504.09246)
- Repo-context/example evidence: [arXiv:2406.11927](https://arxiv.org/html/2406.11927v3) ·
  [arXiv:2508.06414](https://arxiv.org/pdf/2508.06414) ·
  [arXiv:2604.24678](https://arxiv.org/html/2604.24678v1) ·
  [RACG survey, arXiv:2510.04905](https://arxiv.org/html/2510.04905v1)
- Volume law: [AI-Generated Smells, arXiv:2605.02741](https://arxiv.org/abs/2605.02741)
- Strands steering: [steering docs](https://strandsagents.com/docs/user-guide/concepts/plugins/steering/) ·
  [harness-sdk](https://github.com/strands-agents/harness-sdk) ·
  [strandsagents.com](https://strandsagents.com/)
