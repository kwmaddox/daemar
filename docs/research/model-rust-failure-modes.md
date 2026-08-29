# Model-Rust failure modes — evidence pass for the conventions referent

**Researched 2026-08-23.** Feeds PER-72 (`conventions.md`). Question: what do
LLMs and coding agents *systematically* get wrong in Rust **above the
compile-error tier** — code that passes `cargo check` and default Clippy but
is poor Rust? Scope rules (Kendall's): not exhaustive, no restating the Rust
book, only failure modes with evidence of *recurrence in model output*.
Settled policy (native error handling, no anyhow/eyre/thiserror, newtypes,
enums over strings) is not re-litigated here; frequency evidence for those
patterns is collected to strengthen the conventions.

Evidence labels: **MEASURED** (study/benchmark) · **PRACTITIONER** (field
report) · **VENDOR** (maker's claim) · **THEORY** (reasoned inference).
"Searched and not found" is stated where it applies.

## Verdicts up front

1. **The best-measured failure mode is panic-path density**: 57.1% of
   *successfully compiling* LLM crypto samples called `unwrap()`/`expect()`
   on fallible operations; direct-LLM translation shows "elevated" runtime
   panic risks vs. human baseline. Already fully gated by L0 — the referent's
   job is to state the rule once and cite the gate.
2. **The best-measured *ungated* failure mode is redundant/duplicative
   code**: direct LLM generation scored "extreme" on redundant code per KLOC
   (worst of all categories vs. human baseline), converging with the
   volume-quality law and practitioner reports of near-duplicate types.
   This tier — duplication, dead `pub` surface, borrow-checker-avoidance
   clones, unjustified shared-state reach — is where conventions carry the
   load, because no gate in L0–L2 catches design-level redundancy.
3. **A real prior-art ecosystem exists** (searched, FOUND): agent-facing
   Rust rule sets are published in the wild — most notably `rust-skills`
   (265 rules, 26 categories, wrong/right pairs, progressive loading).
   Mineable for form and coverage; not adoptable wholesale (it encodes
   "thiserror for libs, anyhow for apps," the opposite of settled daemar
   policy).
4. **Idiomaticity gaps are largely a solved-by-gates problem**: studies use
   Clippy-warning counts as the idiomaticity metric (one pipeline cut
   warnings 7×), and daemar's L0 runs `pedantic = warn` under `-D warnings`
   in CI — so the loop-vs-iterator / needless-allocation tier is mostly
   mechanical. The referent should spend its budget above that tier.

---

## 1. Ranked catalog (by evidence strength)

### F1. Panic paths in compiling code — `unwrap`/`expect`/indexing

- **Evidence:** In an empirical evaluation of LLM-generated cryptographic
  Rust (Gemini 2.5 Pro, GPT-4o, DeepSeek Coder), **57.1% of successfully
  compiled samples used `unwrap()`/`expect()` on cryptographic operations**
  (CWE-252), the dominant defect class in code that compiled
  ([arXiv:2604.27001](https://arxiv.org/html/2604.27001v1)). **MEASURED.**
  Direct-LLM C→Rust translation (TranslationGym) shows *elevated* runtime
  panic risks per KLOC vs. human-written baseline
  ([arXiv:2602.00840](https://arxiv.org/html/2602.00840)). **MEASURED.**
  Practitioner shorthand for model output: "noob code with `.clone()` and
  `.unwrap()`" (kornel, [URLO thread](https://users.rust-lang.org/t/using-ai-to-generate-rust-code/128758)).
  **PRACTITIONER.**
- **Recurrence signal:** strongest of any failure mode — measured across
  models and tasks.
- **Gate coverage:** **CAUGHT** — L0 denies `unwrap_used`, `expect_used`,
  `panic`, `indexing_slicing`, `string_slice` (PER-66), with test
  exemptions. Referent states the rule once, annotated "enforced: L0."

### F2. Redundant, duplicative, and dead code

- **Evidence:** In the C→Rust quality study, the direct-LLM pipeline scored
  **"extreme" on redundant code per 1000 lines** — its worst category
  relative to human-written Rust ([arXiv:2602.00840](https://arxiv.org/html/2602.00840)).
  **MEASURED.** The volume-quality law: code volume near-perfectly predicts
  structural degradation in agent code; prompting does not mitigate
  ([arXiv:2605.02741](https://arxiv.org/abs/2605.02741)). **MEASURED
  (language-agnostic).** Practitioners report "poorly designed structs,
  nearly duplicate enums" generated without planning (kornel, URLO), and
  agents that "invariably change unrelated code for no reason" (parasyte,
  URLO). **PRACTITIONER.**
- **Recurrence signal:** strong — measured worst-category plus independent
  field reports.
- **Gate coverage:** **PARTIAL.** rustc `dead_code` fires only on private
  items; speculative `pub` surface, near-duplicate types, and unrequested
  scope growth pass every mechanical gate. → **Convention candidates CC1,
  CC2** (§2); volume itself is PER-70's diff budget and PER-71's
  small-slice discipline.

### F3. Borrow-checker avoidance: `.clone()` (and `Rc`/`Arc`) as the escape hatch

- **Evidence:** "Clone to satisfy the borrow checker" is a named community
  anti-pattern predating LLMs
  ([Rust Design Patterns](https://rust-unofficial.github.io/patterns/anti_patterns/borrow_clone.html)).
  Ownership/borrowing violations are the dominant class of model Rust
  *compile* errors — "over 40% of compilation errors in generated code"
  ([Strand-Rust-Coder tech report](https://huggingface.co/blog/Fortytwo-Network/strand-rust-coder-tech-report),
  **VENDOR**, consistent with borrow-fix resolution rates ~74% in repair
  studies, [arXiv:2308.05177](https://arxiv.org/pdf/2308.05177) **MEASURED**);
  the code that *does* compile is disproportionately the code that routed
  around ownership via clone/`Arc` — this inference is **THEORY from
  MEASURED premises** (no study directly counts avoidance-clones; searched,
  not found). Direct field report: "noob code with `.clone()`" (kornel,
  URLO). **PRACTITIONER.**
- **Recurrence signal:** moderate-strong; the mechanism is well-attested,
  the frequency is not directly measured.
- **Gate coverage:** **PARTIAL/UNCAUGHT.** `clippy::redundant_clone` is
  nursery-tier and documented as imperfect; it cannot judge whether a
  *compiling* clone reflects intended ownership semantics. → **Convention
  candidate CC3.**

### F4. Unjustified shared-state reach: `Arc<Mutex<_>>` where ownership redesign fits

- **Evidence:** Practitioner post-mortems of AI-flavored/naive Rust:
  a service "spending 78% of CPU time in atomic operations" from clean,
  compiling `Arc` sharing
  ([Jason Li](https://medium.com/@Jason__Li/this-rust-arc-pattern-silently-killed-our-performance-af91d7c039a8));
  the `Arc<Mutex<T>>`-first habit called out as an architectural default to
  unlearn ([TheOpinionatedDev](https://medium.com/@theopinionatedev/stop-using-arc-mutex-t-theres-a-better-architectural-pattern-cc4d26b89a3e));
  review-tool vendors list "overuse of `Arc<Mutex<T>>`" among what the
  compiler won't catch ([kodus](https://kodus.io/en/rust-code-review-practices-and-ai-tools/)).
  **PRACTITIONER/VENDOR** — LLM-specific frequency not measured (searched,
  not found); mechanism matches F3 (sharing as the ownership escape hatch).
- **Recurrence signal:** moderate; indirect but convergent.
- **Gate coverage:** **PARTIAL.** `clippy::await_holding_lock` catches the
  async footgun (irrelevant to daemar today — no async); the design-level
  reach for shared mutability is uncatchable mechanically. → **Convention
  candidate CC4.**

### F5. Non-idiomatic construct choice: loops over iterators, `format!` over `write!`, collect-then-iterate

- **Evidence:** Studies use Clippy-warning density as the idiomaticity
  metric; an idiomatic-translation pipeline produced "up to 7× fewer Clippy
  warnings" ([SACTOR, arXiv:2503.12511](https://arxiv.org/pdf/2503.12511)).
  **MEASURED (proxy).** General models "fail to internalize... zero-cost
  abstractions, explicit trait bounds, and pervasive use of pattern matching
  and iterators"; concrete example: `Display` impls via `format!` temporary
  strings instead of `write!`
  ([Strand tech report](https://huggingface.co/blog/Fortytwo-Network/strand-rust-coder-tech-report)).
  **VENDOR.**
- **Recurrence signal:** strong at the proxy level.
- **Gate coverage:** **LARGELY CAUGHT** — the relevant lints live in
  `clippy::all`/`pedantic` (`needless_range_loop`, `format_push_string`,
  `useless_conversion`, `needless_pass_by_value`, …) and daemar's CI runs
  pedantic warns under `-D warnings` (PER-66), which hard-fails them. The
  uncaught remainder (allocation strategy as design, e.g. buffering a whole
  stream where incremental writing fits) → folded into **CC5**.

### F6. Error-information destruction in compiling code

- **Evidence:** the CWE-252 unchecked-return pattern above (F1's study)
  plus the settled-policy tier: anyhow-style type erasure and
  string-wrapper enums destroy the machine-checkable failure surface
  (established in `docs/research/rust-quality-enforcement.md` and PER-66/67
  rationale). Rust-specific model frequency for `let _ =` discarding:
  not separately measured (searched, not found); generic "swallowed errors"
  appear in AI-slop rule sets (aislop, prior pass). **MEASURED-adjacent +
  PRACTITIONER.**
- **Gate coverage:** **CAUGHT** for the big classes (cargo-deny anyhow ban;
  `map_err_ignore`; `disallowed_types`; future string-wrapper Dylint in
  PER-69). Judgment remainder (taxonomy granularity, `source()` chaining
  discipline) is already assigned to the referent's error convention.

### F7. Stringly-typed domain data

- **Evidence base:** carried from the two prior research passes (parent
  corpus): repository-context studies show convention violations when types
  aren't in context; the pattern is the founding complaint behind this whole
  effort. LLM-specific frequency counts: not found as a dedicated
  measurement. **PRACTITIONER consensus + settled policy.**
- **Gate coverage:** **PARTIAL** (PER-68 ast-grep field-name heuristics) →
  the newtype/enum decision procedure is the referent's core judgment-tier
  content (already in PER-72's design constraints).

### Below the tier (noted, excluded from the referent)

- **API hallucination** — 41.3% of compile *failures* in the crypto study;
  dominant model failure overall but the compiler owns it entirely.
- **Unrelated-code churn / scope creep** — real (parasyte, URLO;
  volume law) but owned by PER-70's diff budget and PER-71's small-slice
  discipline, not by a style convention.
- **Over-abstraction (premature generics/traits)** — plausible, frequently
  claimed in general AI-slop discourse, but Rust-specific recurrence
  evidence is thin (searched; only indirect practitioner notes found).
  Candidate only if Kendall's review experience corroborates. **THEORY.**
  *Resolution (2026-08-23):* Kendall does not observe this as a Rust trend
  (sees it in Python, where his review hours are concentrated); combined
  with the thin evidence and the difficulty of qualifying/quantifying it,
  **excluded from the referent**. Re-propose only with Rust-specific
  recurrence evidence AND an adjudicable statement.

## 2. Candidate conventions (for collaborative review — candidates, not decisions)

Falsifiable one-liners in the `specs/sandbox.md` behavior-ID style, covering
the **uncaught remainder** only (gate-enforced rules enter the referent with
an "enforced: L0/L1/L2" annotation instead). Settled-policy conventions
(native error handling, newtypes, enums-over-strings) are assumed present
and not restated here.

- **CC1 (pub earns its place).** Every `pub` item has a current caller or a
  documented external consumer; speculative API surface is a defect.
  *Rationale: models over-produce surface (F2) and rustc's dead-code lint
  cannot see across `pub`.*
- **CC2 (no near-duplicate types).** Before defining a type, the author
  extends or reuses an existing type whose meaning overlaps; two types with
  overlapping fields and purpose are a defect unless the divergence is
  documented. *Rationale: measured "extreme" redundancy tier (F2) shows up
  as parallel structs/enums that later fork behavior.*
- **CC3 (clones carry semantics).** A `.clone()` exists only where an
  independent copy is semantically required; a clone whose purpose is to
  satisfy the borrow checker is a design defect — restructure ownership
  instead. *Rationale: the canonical model escape hatch (F3); mechanically
  undetectable because it compiles cleanly.*
- **CC4 (shared mutability is justified, not default).** Introducing
  `Rc`/`Arc`/`Mutex`/`RwLock` requires a stated reason why single ownership
  or message passing does not fit, recorded at the introduction site.
  *Rationale: sharing is F3's sibling escape hatch (F4) and silently taxes
  performance and design.*
- **CC5 (write through, don't buffer up).** Where an API supports writing or
  streaming directly (`write!` to a formatter, iterator chains to a sink),
  building intermediate `String`s/`Vec`s instead is a defect. *Rationale:
  the measured idiomaticity gap's design-level remainder (F5) that pedantic
  lints can't fully see.*
- **CC6 (functions state their failure surface — settled policy anchor).**
  Placeholder for the native-error-handling convention block (PER-72 design
  constraints); listed to keep numbering stable during drafting.

## 3. Prior art: agent-facing Rust conventions in the wild (FOUND)

- **[rust-skills](https://github.com/leonardomso/rust-skills)** — 265 rules,
  26 categories, each rule = imperative one-liner + "why it matters" +
  Bad/Good pair + cross-links; consumed progressively (index file → targeted
  rule pulls). 438★, current to Rust 1.96. Directly covers clone abuse
  ("own-borrow-over-clone"), unwrap abuse, allocation, dyn-vs-generic.
  **Mine for form and coverage checklist; do not adopt wholesale** — it
  encodes "thiserror for libs, anyhow for apps," the opposite of daemar
  policy, confirming that community rule packs embed exactly the idiom
  Kendall overrides deliberately.
- Practitioner CLAUDE.md templates for Rust exist (e.g.
  [minimaxir's gist](https://gist.github.com/minimaxir/23ee55a83633ac0b6b92de635291ad80),
  "Never `.unwrap()` in library code" tier) — thin, but they corroborate
  which rules practitioners reach for first. **PRACTITIONER.**
- A dedicated *measured* study of conventions-file effectiveness for Rust
  specifically: **searched, not found** (consistent with the prior
  preventative-controls pass).

## 4. Sources

- [Code Quality Analysis of Translations from C to Rust, arXiv:2602.00840](https://arxiv.org/html/2602.00840) —
  18-category taxonomy; redundant-code "extreme" tier; panic-risk elevation; Clippy blind spots
- [Empirical Security Evaluation of LLM-Generated Cryptographic Rust, arXiv:2604.27001](https://arxiv.org/html/2604.27001v1) —
  57.1% unwrap/expect in compiled samples; 41.3% API hallucination; "successful compilation may provide false assurance"
- [SACTOR, arXiv:2503.12511](https://arxiv.org/pdf/2503.12511) — Clippy-warning count as idiomaticity metric; 7× reduction
- [Fixing Rust Compilation Errors using LLMs, arXiv:2308.05177](https://arxiv.org/pdf/2308.05177) — borrow-checker repair difficulty
- [AI-Generated Smells, arXiv:2605.02741](https://arxiv.org/abs/2605.02741) — volume-quality inverse law (carried from prior pass)
- [Strand-Rust-Coder tech report](https://huggingface.co/blog/Fortytwo-Network/strand-rust-coder-tech-report) —
  40% ownership-error share; format!/write! example; idiom-internalization claim
- [Using AI to generate Rust code — users.rust-lang.org](https://users.rust-lang.org/t/using-ai-to-generate-rust-code/128758) —
  practitioner catalog (kornel, parasyte, Redglyph, ZiCog)
- [Clone to satisfy the borrow checker — Rust Design Patterns](https://rust-unofficial.github.io/patterns/anti_patterns/borrow_clone.html)
- Arc/Mutex practitioner reports: [Jason Li](https://medium.com/@Jason__Li/this-rust-arc-pattern-silently-killed-our-performance-af91d7c039a8) ·
  [TheOpinionatedDev](https://medium.com/@theopinionatedev/stop-using-arc-mutex-t-theres-a-better-architectural-pattern-cc4d26b89a3e) ·
  [kodus review guide](https://kodus.io/en/rust-code-review-practices-and-ai-tools/)
- Prior art: [rust-skills](https://github.com/leonardomso/rust-skills) ·
  [minimaxir Rust CLAUDE.md](https://gist.github.com/minimaxir/23ee55a83633ac0b6b92de635291ad80)
