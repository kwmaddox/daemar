# Deterministic enforcement of strict Rust for agent-authored code

**Researched 2026-08-23.** Primary sources throughout; every load-bearing claim
links to the source that owns it. Question: how much of the "agents turn Rust
into weakly-typed Python" failure class — anyhow everywhere, `Box<dyn Error>`,
unwraps, stringly-typed domain data, string-wrapper error enums — can be caught
by *deterministic, build-failing* gates today, and with what stack?

## Verdict

1. **The frontier has moved, but on a different axis than hoped.** A real
   2025–2026 wave of "AI slop" linters exists (aislop, antislop, grain,
   agent-slop-lint, desloppify) plus academic work formalizing LLM code smells.
   All of it targets **surface slop** — placeholder comments, stubs, hedging,
   dead code, narrative comments — via regex/tree-sitter, or (aislop) wraps
   clippy for its Rust coverage. **None of it detects Rust type-system smells**
   (anyhow use, `Box<dyn Error>`, stringly typing, weak error enums). For the
   type-level policy, the deterministic frontier is still Clippy + cargo-deny +
   custom lints — but that frontier is far stronger than its defaults, and
   Clippy itself is actively growing in exactly this direction (profile-scoped
   disallow lists merged June 2026, reverted for process, re-landing).
2. **Roughly 70% of the stated policy is mechanically enforceable today with
   configuration only** (hours of work, no custom code): the anyhow ban is
   *total* via cargo-deny + clippy `disallowed-*`; unwrap/expect/panic bans are
   built-in restriction lints with first-class test exemptions.
3. The remainder splits into **cheap structural rules** (ast-grep for
   `Box<dyn Error>` in signatures), **feasible custom lints** (Dylint for
   string-wrapper error variants), and a **genuinely non-deterministic
   residue** (newtype judgment, enum-vs-string design) where the strongest
   deterministic proxy is a diff-size budget — supported by 2026 research
   finding code volume is "a near-perfect predictor of structural degradation."

## 1. The deterministic frontier, tool by tool

### 1.1 Clippy restriction lints — the workhorse

The `restriction` group is explicitly designed for opt-in prohibition and is
not meant to be enabled wholesale
([Clippy lints book](https://doc.rust-lang.org/clippy/lints.html)). The lints
mapping to this policy (each verified against the
[lint configuration reference](https://doc.rust-lang.org/clippy/lint_configuration.html)
or the [lint index](https://rust-lang.github.io/rust-clippy/master/index.html)):

| Policy item | Lint(s) |
|---|---|
| no `unwrap()` / `expect()` | `clippy::unwrap_used`, `clippy::expect_used` |
| no `panic!` / `todo!` / `unimplemented!` / `dbg!` | `clippy::panic`, `clippy::todo`, `clippy::unimplemented`, `clippy::dbg_macro` |
| no panicking indexing/slicing | `clippy::indexing_slicing`, `clippy::string_slice` |
| no silent lossy casts | `clippy::as_conversions` (strict) or pedantic cast lints |
| no future-proofed-away match arms | `clippy::wildcard_enum_match_arm` |
| no error-context destruction | `clippy::map_err_ignore` |
| no unjustified suppressions | `clippy::allow_attributes` (force `#[expect]`), `clippy::allow_attributes_without_reason` |

**Test scoping is first-class**: `clippy.toml` supports
`allow-unwrap-in-tests`, `allow-expect-in-tests`, `allow-panic-in-tests`,
`allow-dbg-in-tests`, `allow-print-in-tests`, `allow-indexing-slicing-in-tests`
(all default `false`; they exempt `#[test]` functions and `#[cfg(test)]` code)
plus `allow-unwrap-types` for types like `LockResult`
([lint configuration](https://doc.rust-lang.org/clippy/lint_configuration.html)).

**Known hole:** the `allow-*-in-tests` options do **not** cover integration
tests in `tests/`, examples, or benches — those targets aren't `cfg(test)`.
Open feature request
[rust-clippy#13981](https://github.com/rust-lang/rust-clippy/issues/13981).
Workaround: a file-level `#![allow(clippy::unwrap_used, clippy::expect_used,
clippy::panic)]` preamble in each `tests/*.rs` (one line, auditable, and
`allow_attributes_without_reason` forces it to carry a reason). This matters
directly for daemar: the battery lives in `crates/sandbox/tests/battery.rs`.

### 1.2 Clippy `disallowed-*` — the policy engine

`clippy.toml` supports `disallowed-types`, `disallowed-methods`, and
`disallowed-macros`, each taking `path`, optional `reason`, and optional
`replacement`
([lint configuration](https://doc.rust-lang.org/clippy/lint_configuration.html)):

```toml
# clippy.toml — the anyhow ban, spelled out
[[disallowed-types]]
path = "anyhow::Error"
reason = "explicit error enums only; the failure surface is documentation (see specs standards)"

[[disallowed-macros]]
path = "anyhow::anyhow"
reason = "explicit error enums only"
[[disallowed-macros]]
path = "anyhow::bail"
reason = "explicit error enums only"
[[disallowed-macros]]
path = "anyhow::ensure"
reason = "explicit error enums only"
```

The corresponding lints (`clippy::disallowed_types` etc.) are `warn` by
default once configured; deny them in the lint table to make them
build-failing. Limits, stated honestly:

- **Paths, not generic instantiations.** `disallowed-types` bans a type path
  (e.g. all of `anyhow::Error`); it cannot express "`Box<T>` only when `T` is
  `dyn Error`". The `Box<dyn Error>` ban therefore needs ast-grep or Dylint
  (§1.4, §1.5).
- The Clippy configuration file is documented as **unstable** ("may be
  deprecated in the future") — a live but low-probability risk given how
  widely it is used ([configuration](https://doc.rust-lang.org/clippy/configuration.html)).
- **Frontier movement, in our favor:** profile-scoped disallow lists
  (`[disallowed-methods-profiles.*]` activated per-item via
  `#[clippy::disallowed_profile(...)]`) were merged 2026-06-09 and reverted
  2026-06-15 pending formal process; a follow-up PR is open
  ([rust-clippy#15779](https://github.com/rust-lang/rust-clippy/pull/15779)).
  When this lands, lib-vs-bin scoping of disallow lists becomes native.

### 1.3 Cargo `[lints]` / `[workspace.lints]` — cross-crate enforcement

Stable since Cargo 1.74. Workspace root declares `[workspace.lints.rust]` /
`[workspace.lints.clippy]`; **each member must explicitly opt in** with
`[lints] workspace = true` — inheritance is never implicit
([Cargo workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html),
[manifest `[lints]`](https://doc.rust-lang.org/cargo/reference/manifest.html#the-lints-section)).
Group-vs-individual precedence is controlled by `priority` (lower = applied
first, overridden by higher):

```toml
[workspace.lints.clippy]
all = { level = "deny", priority = -1 }
pedantic = { level = "warn", priority = -1 }
# individual overrides at default priority 0:
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
todo = "deny"
unimplemented = "deny"
dbg_macro = "deny"
indexing_slicing = "deny"
string_slice = "deny"
wildcard_enum_match_arm = "deny"
map_err_ignore = "deny"
allow_attributes_without_reason = "deny"
disallowed_types = "deny"
disallowed_methods = "deny"
disallowed_macros = "deny"
```

**Scoping is per-package only** — no per-target (lib/bin/test) granularity in
the manifest ([manifest reference](https://doc.rust-lang.org/cargo/reference/manifest.html#the-lints-section)).
Context scoping is therefore achieved by: (a) the `allow-*-in-tests` options,
(b) file-level `#![allow]`/`#![expect]` preambles in `tests/`, and (c) if a
binary crate ever legitimately needs looser rules, its own `[lints]` table —
though under this policy binaries stay strict.

### 1.4 cargo-deny — banning anyhow at the dependency graph

The `bans` check denies listed crates with a reason and fails CI, with
optional `wrappers` exceptions; it also covers advisories, licenses, and
sources ([cargo-deny book](https://embarkstudios.github.io/cargo-deny/checks/bans/index.html)).
Banning `anyhow` (and `eyre`, `color-eyre`) here is the **strongest possible
enforcement**: the crate cannot appear in the graph at all, so no agent can
"just add it to Cargo.toml" — the clippy disallow list then exists only as a
belt-and-suspenders and a better error message. Adjacent (not adopted now):
[cackle / cargo-acl](https://github.com/cackle-rs/cackle) applies
capability-based policies (fs/net/process) to dependencies.

### 1.5 Dylint — custom compiler-backed lints

[Dylint](https://github.com/trailofbits/dylint) (Trail of Bits, actively
maintained) runs lints from dynamic libraries with full access to the same
HIR/type information Clippy has; `cargo dylint new` scaffolds a working lint
library ([README](https://github.com/trailofbits/dylint/blob/master/README.md)).
This is the deterministic path to the smells nothing off-the-shelf catches:

- **"thiserror costume" detector**: enum implements/derives `Error` and every
  variant's fields are only `String`/`&str` → deny. Type-level, precisely
  expressible against HIR; not expressible in Clippy config.
- **foreign-error-in-API detector**: `pub fn` in a library crate returns
  `Result<_, E>` where `E` is not defined in the crate → deny.
- Their [example collections](https://github.com/trailofbits/dylint/tree/master/examples)
  already include relevant restriction-style lints: `env_literal`
  (stringly-typed environment access), `misleading_variable_name`,
  `non_local_effect_before_unhandled_error`, `try_io_result`.

Cost: real but bounded — a lint crate pinned to a nightly toolchain component,
maintained like any dev-dependency. This is days, not hours, and is the
correct home for policy that needs type information.

### 1.6 ast-grep / semgrep — structural rules without the compiler

[ast-grep](https://github.com/ast-grep/ast-grep) has built-in Rust support
(tree-sitter based), YAML rule files, and a `scan` mode with non-zero exit for
CI. It expresses what `disallowed-types` cannot, syntactically:

```yaml
# sgconfig rule: no Box<dyn Error> in non-test code
id: no-box-dyn-error
language: rust
rule:
  any:
    - pattern: Box<dyn Error>
    - pattern: Box<dyn std::error::Error>
    - pattern: Box<dyn std::error::Error + Send + Sync>
files: ["crates/**/src/**/*.rs"]   # excludes tests/ by path
```

Caveat: syntactic means alias-blind (`type BoxError = Box<dyn Error>` is
caught at the alias definition, uses of the alias are not). That is acceptable
as a first gate; Dylint is the semantic escalation. Semgrep also has GA Rust
support with Pro rules
([semgrep announcement](https://semgrep.dev/products/product-updates/rust-ga-support-and-swift-beta-support/)),
but its Rust rule packs target security, not idiom; ast-grep is lighter for
self-authored structural policy and installs via `cargo install ast-grep`.

### 1.7 The 2025–2026 "AI slop" tool wave — what it actually covers

Verified against the repos, not the marketing:

- [aislop](https://github.com/scanaislop/aislop) (572★, active, MIT): 50+
  deterministic rules, 10 languages, CI/GitHub Actions, 0–100 score, no LLM at
  runtime. **For Rust it orchestrates clippy/cargo-fmt** plus generic AST/regex
  rules (narrative comments, swallowed errors, dead code, todo stubs). No
  type-system rules beyond what clippy provides.
- [antislop](https://github.com/skew202/antislop) (early, 9★): five slop
  categories — placeholders, deferrals, hedging, stubs, comment noise — hybrid
  tree-sitter/regex, Rust among full-AST languages. **No Rust-idiom rules.**
- [grain](https://github.com/mmartoccia/grain) (33★): regex-based, **Python
  only** today; notable for its agent-loop design (JSON output, work queues
  for agentic repair).
- Academic: "AI-Generated Smells" (Zhu, Tsantalis, Rigby, May 2026,
  [arXiv:2605.02741](https://arxiv.org/abs/2605.02741)) formalizes the
  "machine signature" of agent code and finds a **Volume-Quality Inverse
  Law** — code volume near-perfectly predicts structural degradation — and
  that neither functional correctness nor detailed prompting mitigates it.
  ICSE 2026 NIER's "LLM code smells" line
  ([SpecDetect4AI](https://conf.researchr.org/details/icse-2026/icse-2026-nier/37/Specification-and-Detection-of-LLM-Code-Smells))
  addresses smells in code *that calls LLMs*, not LLM-authored code — a
  different problem despite the name.

**Conclusion for this layer:** worth one optional slot in CI for surface slop
(comment noise, stubs) — it is orthogonal to, and no substitute for, the
type-level stack. The research finding that survives into our design is the
volume law: a **maximum-diff-size gate** is a deterministic, cheap,
evidence-backed proxy for the bloat/coupling smell no linter can see.

### 1.8 Prior art on strict profiles

The common pattern in strict codebases: `all = deny` + `pedantic = warn` at
group priority −1, specific restriction lints denied individually, and
`#[expect(..., reason)]` (stabilized Rust 1.81) as the only suppression form —
e.g. [Rust Project Primer's lints chapter](https://rustprojectprimer.com/checks/lints.html)
and curated configs like
[Schwartz, "Your Clippy Config Should Be Stricter"](https://emschwartz.me/your-clippy-config-should-be-stricter/).
No off-the-shelf profile encodes a full anyhow ban — that part is always
project `clippy.toml` + cargo-deny.

## 2. Policy coverage matrix

| Policy item | Deterministic today? | Mechanism | What slips through |
|---|---|---|---|
| No anyhow anywhere | **Total** | cargo-deny ban + `disallowed-types/macros` | nothing (can't be in the graph) |
| No `Box<dyn Error>` outside tests | **High** | ast-grep rule (path-scoped); Dylint for semantic version | type aliases at use-sites (caught at definition) |
| No unwrap/expect/panic/todo outside tests | **Total** for unit/`cfg(test)`; file-preamble needed for `tests/` | restriction lints + `allow-*-in-tests` + tests/ preambles ([#13981](https://github.com/rust-lang/rust-clippy/issues/13981)) | nothing, given preambles |
| String-wrapper error variants | **Feasible, custom** | Dylint lint (days of work) | until written: everything |
| Enums over string matching | **Partial** | `wildcard_enum_match_arm`; Dylint `env_literal`; ast-grep heuristics for match-on-literal | legitimate string parsing vs. stringly domain logic is judgment |
| Newtypes over bare `String` | **Partial, project-specific** | Dylint/ast-grep rules for named patterns (`*_id`, `kind`, `status` as `String`) | general "should have been a type" judgment |
| Gratuitous clones | **Partial** | `redundant_clone` (nursery, imperfect), pedantic `needless_pass_by_value` | borrow-restructuring judgment |
| Suppression hygiene | **Total** | `allow_attributes` + `allow_attributes_without_reason` + `#[expect]` | — |
| Bloat/coupling (volume law) | **Proxy only** | diff-size budget gate in the factory | design quality itself |

## 3. Recommended stack for daemar, by adoption cost

1. **Layer 0 — configuration only (hours).** Workspace lint table as in §1.3
   with `[lints] workspace = true` in both crates; `clippy.toml` with the
   anyhow disallow entries (§1.2), `allow-unwrap-in-tests = true`,
   `allow-expect-in-tests = true`, `allow-panic-in-tests = true`; reasoned
   `#![allow]` preambles in `tests/*.rs`; CI gate
   `cargo clippy --all-targets --all-features -- -D warnings`.
2. **Layer 1 — cargo-deny (an hour).** `deny.toml` banning `anyhow`, `eyre`;
   advisories/licenses checks come free. CI gate `cargo deny check`.
3. **Layer 2 — ast-grep rule pack (a day).** `no-box-dyn-error` (§1.6),
   match-on-string-literal heuristic, project-named `String`-field rules.
   Versioned in-repo; CI gate `ast-grep scan`.
4. **Layer 3 — Dylint lint crate (days, when Layer 2 noise justifies it).**
   The thiserror-costume and foreign-error-in-API lints; adopt `env_literal`
   and `misleading_variable_name` from Trail of Bits' collection.
5. **Layer 4 — factory-level gates (with the workflow, not before).**
   Diff-size budget per change (the volume-law proxy); optionally aislop for
   surface slop. These belong to the validation-stage design, alongside fmt
   and test gates.

Layers 0–1 close the anyhow/unwrap/panic classes completely and are the
priority; nothing in them blocks or is blocked by the sandbox dial-in work.

## 4. Residual gaps (the honest list)

- **Type-design judgment** — when three `bool`s should be a state enum, when
  a `String` deserves a newtype in general. Strongest mitigation: a short
  standards document stating the error-design and newtype conventions agents
  must follow (needed anyway so error enums are stylistically uniform), plus
  review; the deterministic rules above shrink the review surface to actual
  judgment calls.
- **Error taxonomy quality** — a well-formed enum with wrong granularity
  passes every gate. Mitigation: the foreign-error-in-API Dylint lint bounds
  the worst case; review owns the rest.
- **Architectural bloat/coupling** — per
  [arXiv:2605.02741](https://arxiv.org/abs/2605.02741), scales with volume and
  resists prompting. Mitigation: diff-size budgets (deterministic) + review.
- **Clippy config instability** — the `clippy.toml` format is formally
  unstable ([configuration](https://doc.rust-lang.org/clippy/configuration.html));
  pin toolchain versions (already daemar practice for substrates) and treat
  lint-toolchain bumps like substrate bumps: deliberate, tested events.

## 5. Sources

- Clippy: [configuration](https://doc.rust-lang.org/clippy/configuration.html) ·
  [lint configuration reference](https://doc.rust-lang.org/clippy/lint_configuration.html) ·
  [lints book](https://doc.rust-lang.org/clippy/lints.html) ·
  [lint index](https://rust-lang.github.io/rust-clippy/master/index.html) ·
  [PR #15779 (profiles, merged→reverted)](https://github.com/rust-lang/rust-clippy/pull/15779) ·
  [issue #13981 (tests/ gap)](https://github.com/rust-lang/rust-clippy/issues/13981)
- Cargo: [manifest `[lints]`](https://doc.rust-lang.org/cargo/reference/manifest.html#the-lints-section) ·
  [workspaces / `[workspace.lints]`](https://doc.rust-lang.org/cargo/reference/workspaces.html)
- cargo-deny: [bans check](https://embarkstudios.github.io/cargo-deny/checks/bans/index.html)
- Dylint: [repo](https://github.com/trailofbits/dylint) ·
  [examples](https://github.com/trailofbits/dylint/tree/master/examples)
- ast-grep: [repo](https://github.com/ast-grep/ast-grep) ·
  semgrep: [Rust GA](https://semgrep.dev/products/product-updates/rust-ga-support-and-swift-beta-support/)
- Slop tools: [aislop](https://github.com/scanaislop/aislop) ·
  [antislop](https://github.com/skew202/antislop) ·
  [grain](https://github.com/mmartoccia/grain) ·
  [agent-slop-lint](https://github.com/JordanGunn/agent-slop-lint)
- Research: [AI-Generated Smells, arXiv:2605.02741](https://arxiv.org/abs/2605.02741) ·
  [LLM code smells / SpecDetect4AI, ICSE 2026 NIER](https://conf.researchr.org/details/icse-2026/icse-2026-nier/37/Specification-and-Detection-of-LLM-Code-Smells)
- Prior art: [Rust Project Primer, lints](https://rustprojectprimer.com/checks/lints.html) ·
  [Schwartz, stricter Clippy config](https://emschwartz.me/your-clippy-config-should-be-stricter/)
