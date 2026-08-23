# Quint — primary-source research notes

**Retrieved:** 2026-08-10. All claims below are cited to primary sources (official
docs at `quint.sh`, the GitHub repository, GitHub API metadata, npm registry,
release notes). Where prior knowledge conflicted with a source, the source won —
see "Corrections to common assumptions" at the end.

---

## Summary

Quint is an executable specification language with TLA+ semantics, a
programming-style syntax, a static type system, and a CLI/LSP toolchain. It is
Apache-2.0 licensed and actively developed.

Two facts materially change the picture from what older write-ups say:

1. **Quint is no longer an Informal Systems project.** On 2026-04-16 it spun out
   into its own company, **Quint Co**, with Gabriela Moreira as CEO. The GitHub
   org moved from `informalsystems/quint` to `quint-co/quint` (old URLs
   redirect), and the docs domain moved from `quint-lang.org` to `quint.sh`
   (301).
2. **There is a first-party, supported Rust conformance path.** `quint-connect`
   (launched December 2025) is a model-based-testing crate that generates traces
   from a `.qnt` spec and replays them against a Rust implementation via a
   `Driver`/`State` trait pair, asserting state equivalence after every step.
   This is the most relevant tool for the daemar use case and it did not exist in
   most secondary material.

Maturity verdict: actively maintained, pre-1.0, monthly-ish releases through
2026-03, then a quieter Q2 that coincides with the corporate spin-out. Healthy
adoption (≈130k npm downloads/month, ~1.6k stars, ~19 named protocol
specifications). The main technical limits are the usual ones: verification is
*bounded* (Apalache) or *state-space-limited* (TLC), the simulator only samples,
and temporal/liveness properties are only partially supported.

---

## 1. What Quint is, precisely

Quint is described by the repository as "An executable specification language
with delightful tooling based on the temporal logic of actions (TLA)"
([repo description](https://github.com/quint-co/quint)). The docs site frames it
as turning "your system's intended behavior into checkable, verifiable evidence"
([quint.sh](https://quint.sh/)).

**Relationship to TLA+.** The docs' FAQ entry
["How does Quint compare to TLA+?"](https://quint.sh/faq) states the differences
directly:

- *Syntax*: "Quint uses programming-style syntax inspired by TypeScript and
  functional languages. TLA+ uses mathematical and LaTeX-style notation."
- *Types*: Quint has a type system (and an effect system); "TLA+ is purposely
  untyped so that designers can focus on algorithmic concepts."
- *Semantics*: Quint has TLA+-style semantics but "intentionally restricts you to
  a fragment of TLA. In practice, we have not encountered any limitations in
  expressing the systems we like to express."
- *Not PlusCal-like*: "Quint does not use TLA+ as an intermediate language
  (except for model checking)" — i.e. TLA+ is a compile target only when handing
  off to Apalache/TLC, not the authoring layer.
- *Tooling*: "Quint is built with developer tooling first: near-instant feedback
  in the VS Code extension, a REPL, a CLI for random simulation, testing, and
  model checking."

**Who builds it.** Originally Informal Systems. As of
[2026-04-16](https://quint.sh/posts/new_era), Quint spun out into its own
company: "Quint is becoming its own company, spinning out of Informal Systems to
make trust a first-class property of how systems are designed and built." Named
leadership: Gabriela Moreira (CEO), Zarko Milosevic (CTO), Josef Widder (Chief
Scientist), Arianne Flemming (COO). The GitHub org `quint-co` was
[created 2026-05-07](https://github.com/quint-co) with homepage `https://quint.sh/`.

**License.** Apache-2.0, per the
[repo license metadata](https://github.com/quint-co/quint/blob/main/LICENSE) and
the docs site footer.

---

## 2. Maturity and maintenance as of 2026-08-10

| Signal | Value | Source |
|---|---|---|
| Latest release | `v0.32.0`, 2026-03-31 | [releases](https://github.com/quint-co/quint/releases) |
| Latest commit to `main` | 2026-07-20 | GitHub API `repos/quint-co/quint/commits` |
| Stars | 1,597 | GitHub API `repos/quint-co/quint` |
| Open issues + PRs | 241 total, 15 open PRs | GitHub API |
| Issues opened / closed, last 90 days | 13 / 13 | GitHub search API |
| npm downloads, last month | 130,068 | [npm API](https://api.npmjs.org/downloads/point/last-month/@informalsystems/quint) |
| npm latest | `@informalsystems/quint@0.32.0` | [npm registry](https://registry.npmjs.org/@informalsystems/quint) |

**Release cadence, trailing 12 months** (from the releases API): v0.26.0
(2025-07-14), v0.27.0 (2025-08-29), v0.28.0 (2025-09-16), v0.29.0 (2025-09-26),
v0.29.1 (2025-11-11), v0.30.0 (2026-01-19), v0.31.0 (2026-02-27), v0.32.0
(2026-03-31). That is roughly monthly minor releases with real content —
`v0.32.0` added the `leadsTo` temporal operator, fixed fairness-operator effect
errors, fixed TLC parallel-invocation races, and patched several dependency CVEs
([v0.32.0 notes](https://github.com/quint-co/quint/releases/tag/v0.32.0)).

**Caveat on the recent gap.** No tagged release since 2026-03-31 — a ~4-month
gap against a previously monthly cadence. Commits continued through 2026-07-20,
but Q2 activity is thin and dominated by the org move, docs/Netlify plumbing, and
new AI-agent "skills" (`skills/quint-lang`, `skills/quint-modeling`, added
2026-06-30, [#1993](https://github.com/quint-co/quint/pull/1993)). The most
natural reading is that engineering attention went into the spin-out rather than
into abandonment: the company blog published four posts between 2026-04 and
2026-07 ([quint.sh/posts](https://quint.sh/posts)), including a 2026-07-07 case
study, and the spin-out post explicitly names "Quint Language, Connect, and LLM
Kit" as products going forward. I could not find a primary source confirming a
release is imminent — flagged in Open Questions.

**Investment signals.** The spin-out post positions verification as the company's
whole thesis: "In the age of AI-generated software, verification is no longer
optional, it is becoming the central challenge"
([new_era](https://quint.sh/posts/new_era)). Contributor concentration is a risk:
`bugarela` (Moreira) has 2,920 commits versus 556 for the next contributor
(GitHub contributors API).

---

## 3. Toolchain and workflows

Subcommands, from the [CLI reference](https://quint.sh/docs/quint):

- **`quint parse`** — parses and resolves imports, emits IR as JSON.
- **`quint typecheck`** — parse, then infer types *and effects*, emitting type
  and effect maps. The effect system is what distinguishes read-only from
  state-updating expressions.
- **`quint compile`** — lowers to JSON or **TLA+** (module flattening). This is
  the hand-off to the TLA+ ecosystem.
- **`quint repl`** — the default subcommand; interactive evaluation.
- **`quint run`** — **random simulation**. Samples executions and checks
  invariants at each state. Emits traces (`--out-itf`).
- **`quint test`** — runs unit tests, i.e. `run` definitions written inside the
  spec, with configurable sample limits.
- **`quint verify`** — **model checking**, Apalache by default, TLC via
  `--backend=tlc`.
- **`quint docs`** — documentation generation.
- **`quint lint`, `quint indent`** — listed in the reference but **not yet
  implemented**.

Notable flags: `--mbt` (experimental model-based-testing metadata),
`--out-itf`, `--backend` (`typescript` | `rust` | `apalache` | `tlc`),
`--max-steps`, `--seed`, `--n-traces`, `--invariant` / `--invariants`,
`--witnesses`.

### What each actually checks

**`quint run` (simulator).** Per the
[simulator docs](https://quint.sh/docs/simulator): it "generates samples of
executions valid for the model and check invariants at each state of those
executions." Crucially it is **not verification** — an OK result means only that
"it could not find a violation in the explored executions. **There might still be
an issue**." It checks **invariants only; temporal properties are not
supported**, and counterexamples are not minimal ("there might exist another
counter-example that is smaller"). The docs recommend simulating first because
starting with model checking gives "longer feedback cycles."

**`quint verify`.** Per [checking-properties](https://quint.sh/docs/checking-properties)
and [model-checkers](https://quint.sh/docs/model-checkers):

- *Apalache* (default): symbolic **bounded** model checker; compiles the model to
  SMT constraints discharged by Z3. "Integrated with Quint tooling, and will be
  automatically downloaded and invoked." Requires `--max-steps` (default 10).
  Handles large numeric ranges well. Supports invariants **and inductive
  invariants**; temporal properties have only **"partial support"**. It "cannot
  verify arbitrary execution lengths" — results are bounded by the step count.
- *TLC* (`--backend=tlc`): explicit-state; "enumerates all possible states and
  individually checks each one." It **can check executions of any length** and
  **supports invariants and temporal properties**, but it "can become
  computationally intractable for large systems" and cannot draw from infinite
  integer sets — you must constrain them (`Set(1, 2, 100, 999).oneOf()`).

**Invariants vs. temporal properties.** The docs are unambiguous about the
recommended path: "It is much easier to write and check safety properties, which
can be written as invariants," and liveness properties "require temporal formulas"
and are more complex, so start with safety. Inductive invariants sidestep state
explosion but require "expertise and creative insight" to construct.

For daemar's stated use case — phase transitions, clearance grants/refusals,
append-only ledger monotonicity — nearly everything of interest is a **safety
invariant**, which is the well-supported path. Only claims like "every cocked
slip is eventually granted or refused" are liveness, and those land in the
partial-support zone.

---

## 4. Installation, runtime requirements, editor support, CI

From [getting-started](https://quint.sh/docs/getting-started):

- **Node.js/npm** for the primary install: `npm i @informalsystems/quint -g`.
  (Note the npm scope is still `@informalsystems` despite the org move.)
- **JDK ≥ 17** for model checking: "The model checker requires Java Development
  Kit >= 17," with Eclipse Temurin or Zulu OpenJDK 17 recommended. This applies
  to both Apalache and TLC; Apalache's distribution is auto-downloaded.
- Alternative installs: **Homebrew** (`brew install quint`), **Nix**
  (`nix shell "github:NixOS/nixpkgs#quint"`), and **direct binaries** from GitHub
  Releases.
- **Editors:** official VS Code extension (Marketplace); LSP support covers Vim,
  Neovim, and Emacs; **Helix has built-in Quint support** with no configuration.

**CI-friendliness.** Everything is a non-interactive CLI subcommand with exit
codes, `--seed` for reproducibility, and machine-readable output (`--out-itf`
JSON, IR JSON from `parse`). Practically: `parse`/`typecheck`/`run`/`test` need
only Node; add a JDK 17 step for `verify`. One caveat visible in the release
notes: TLC had a bug "failing when multiple instances run in parallel," fixed in
v0.32.0 via per-invocation JVM tmpdir isolation
([#1949](https://github.com/quint-co/quint/pull/1949),
[#1974](https://github.com/quint-co/quint/pull/1974)) — relevant if you fan out
verification jobs on CI. Pin to ≥ 0.32.0.

---

## 5. Known limitations

Sourced from the docs and the issue tracker, not editorializing:

1. **Simulation is sampling, not proof.** An OK from `quint run` is explicitly
   "could not find a violation in the explored executions"
   ([simulator](https://quint.sh/docs/simulator)).
2. **Apalache verification is bounded.** Results are "incomplete verification
   results" obtained "by solving constraints up to a given length"; unbounded
   correctness needs an inductive invariant
   ([checking-properties](https://quint.sh/docs/checking-properties)).
3. **TLC hits state explosion** and cannot enumerate infinite integer sets
   ([model-checkers](https://quint.sh/docs/model-checkers)).
4. **Temporal properties are second-class**: unsupported in the simulator, "partial
   support" in Apalache, supported in TLC. The language itself is still evolving
   here — the most-discussed open issue (15 comments) is
   [#1236 "Extend the language to be able to convert an action to a temporal
   formula"](https://github.com/quint-co/quint/issues/1236), tagged
   `language design`.
5. **The Rust simulator backend is incomplete.** The
   [evaluator README](https://github.com/quint-co/quint/blob/main/evaluator/README.md)
   states it is "currently used for simulations under a feature flag
   (`quint run --backend=rust`). The default is still the Typescript simulator,"
   with an explicit unfinished checklist: `--seed`, `--mbt`, `--witnesses`,
   `--verbosity` support, plus using it for `quint test` and the REPL before the
   TypeScript evaluator can be deprecated. **Note the interaction: `--mbt` is not
   yet supported on the fast Rust backend**, so model-based-testing trace
   generation currently runs on the slower TypeScript simulator.
6. **`lint` and `indent` are unimplemented** ([CLI reference](https://quint.sh/docs/quint)).
7. **Open bug/UX themes** in the tracker: Apalache rewriter errors on polymorphic
   types ([#1398](https://github.com/quint-co/quint/issues/1398)), parser error
   quality ([#916](https://github.com/quint-co/quint/issues/916),
   [#1468](https://github.com/quint-co/quint/issues/1468)), unclear type-error
   messages ([#1361](https://github.com/quint-co/quint/issues/1361)), name
   resolution errors under `verify`
   ([#1800](https://github.com/quint-co/quint/issues/1800)), and typechecker
   soundness gaps around quantified type variables in vars/consts
   ([#789](https://github.com/quint-co/quint/issues/789), labeled `impact-high`).

---

## 6. Interop with implementation code — the Rust conformance story

This is the strongest part of the answer, and it is better than expected.

### ITF (Informal Trace Format)

Traces are emitted as **ITF** JSON via `--out-itf`. Under `--mbt`, `quint run`
additionally records, per state, `mbt::actionTaken` (which model action fired)
and `mbt::nondetPicks` (the nondeterministic choices made)
([model-based-testing](https://quint.sh/docs/model-based-testing)). Those two
fields are what make a trace *replayable* rather than merely *observable* — you
know which transition to drive and with what arguments.

`itf-rs` (<https://github.com/informalsystems/itf-rs>) deserializes ITF into
native Rust types via serde; the docs call this "super easy."

### Quint Connect — the supported path

Per the docs: "In December 2025, we launched
[Quint Connect](https://github.com/quint-co/quint-connect), a library for
Model-Based Testing in Rust," described as the modern replacement for the older
manual ITF-parsing approach.

From the [quint-connect README](https://github.com/quint-co/quint-connect):
"A model-based testing framework for Rust that connects Quint specifications with
Rust applications." It provides automatic trace generation, **automatic state
validation** ("verifies that implementation state matches specification state"),
declarative macros, and "clear diffs when implementation diverges from
specification." Crate: [`quint-connect`](https://crates.io/crates/quint-connect),
Apache-2.0, requires Rust ≥ 1.70 and `quint` on `PATH`.

The shape you write:

```rust
#[derive(Eq, PartialEq, Deserialize, Debug)]
struct MyState { /* fields matching your Quint variables */ }

impl State<MyDriver> for MyState {
    fn from_driver(driver: &MyDriver) -> Result<Self> { todo!() }
}

impl Driver for MyDriver {
    type State = MyState;
    fn step(&mut self, step: &Step) -> Result {
        switch!(step {
            init => self.init(),
            MyAction(param1, param2?) => { self.my_action(param1, param2); }
        })
    }
}

#[quint_test(spec = "spec.qnt", test = "myTest")]   // replay one named `run`
fn my_test() -> impl Driver { MyDriver::default() }

#[quint_run(spec = "spec.qnt")]                      // replay many simulated traces
fn simulation() -> impl Driver { MyDriver::default() }
```

Tests execute under plain `cargo test`; `QUINT_VERBOSE=1` surfaces each step's
action and nondeterministic choices.

**Documented constraint worth knowing up front:** every alternative inside the
spec's `step` action must be a *named* action. An anonymous `all { ... }` block
directly inside `any { ... }` will make Quint Connect "fail to run steps"; the
README's fix is to hoist it into a named action. Quint sum types serialize as
records with `tag` and `value` fields, which is what your Rust `Deserialize`
structs must match.

### The other direction: trace validation

The docs also describe **trace validation** as the inverse technique — capture
logs from a running system and ask whether those observed execution sequences
conform to the spec ("the other way around"). For an event-sourced append-only
ledger this is unusually well-matched: the ledger *is already* a trace. However,
I found no first-party tool that turns arbitrary application logs into
spec-checkable traces — the docs present it as a technique, not a shipped
command. Treat this as hand-built (see Open Questions).

### Codegen

**No spec-to-implementation codegen exists.** Nothing in the docs, the CLI
reference, or the repo suggests generating Rust from a Quint spec. `compile` only
targets JSON IR and TLA+. Conformance is achieved by *testing*, not by
*generation*.

### Assessment for daemar

Concretely available off the shelf: write invariants over the ledger fold
(phase-transition legality, "closed slips are never cocked", clearance
grant/refuse exclusivity), check them with `quint run` continuously and
`quint verify` in CI, then bind the spec to the Rust implementation with
`quint-connect` — a `Driver` mapping Quint actions to daemar's event-append
operations and a `State` projecting the fold into the spec's variable shape.
That gives mechanical, `cargo test`-enforced conformance.

What would have to be hand-built: (a) validating *real production ledgers* against
the spec (trace validation has no shipped tooling); (b) any guarantee that the
spec's action set stays in sync with the Rust event enum — nothing detects a new
Rust event variant that the spec never modeled, so that drift check is yours to
write; (c) `--mbt` trace generation is stuck on the TypeScript simulator until
the Rust evaluator reaches parity, which bounds trace volume.

---

## 7. Comparisons the docs themselves make

All from the [FAQ](https://quint.sh/faq); see §1 for the TLA+ detail.

- **vs. TLA+**: programming-style syntax vs. mathematical notation; typed vs.
  deliberately untyped; developer-tooling-first; a deliberate fragment of TLA.
- **vs. PlusCal**: PlusCal compiles to TLA+ as its authoring layer; Quint "does
  not use TLA+ as an intermediate language (except for model checking)."
- **vs. Alloy**: Alloy centers on "sets and relations"; "Quint is natively
  time-oriented, making it a better fit for concurrent and distributed systems."
- **vs. proof assistants (Coq, Lean, Isabelle)**: those need substantial manual
  proof effort, whereas "Quint brings value and increases confidence from the
  very beginning" through automation.

The FAQ does not compare Quint to P, and I found no first-party comparison to it.

---

## 8. Real-world usage evidence

From the docs' own [use-cases page](https://quint.sh/docs/use-cases), which links
a repository (and often a blog post or talk) for each. Nineteen named
specifications:

| Project | Organization |
|---|---|
| Alpenglow | Anza Research |
| ChonkyBFT | Matter Labs |
| CometBFT Gossip (Flood) | CometBFT / Informal |
| CometBFT Mempool | CometBFT / Informal |
| HotShot Epoch-change | Espresso |
| Interchain Security | Cosmos Hub / Informal |
| Jellyfish Merkle Tree | Left Curve |
| Malachite | Informal / Circle |
| Minimmit | commonware |
| MonadBFT | Category Labs |
| Mysticeti-C | Mysten Labs |
| Namada | Heliax |
| Neutron DeX | Neutron |
| Neutron Drop | Neutron |
| Neutron Liquidity Pool Migration | Neutron |
| Tendermint Consensus | Tendermint |
| Timewave Vault | Neutron |
| Universally Composable Security | IOG |
| ZKSync Governance | Matter Labs |

Notably, **Interchain Security** and **Neutron Liquidity Pool Migration** are
cited specifically as *model-based testing* deployments, not just specifications
— evidence the MBT path is exercised in production settings.

Additional first-party evidence of bugs actually caught: "It Told Me No: How
Quint Helped Us Fix a Long-Standing Race Condition" (2026-07-07,
[/posts/racecondition](https://quint.sh/posts/racecondition)) and "Catching a
DeFi Exploit with Quint: Modeling the Balancer Upscale Bug" (2026-06-24,
[/posts/balancerbug](https://quint.sh/posts/balancerbug)).

**Domain skew, stated plainly:** essentially every named user is a blockchain or
consensus-protocol project. That is the community Quint grew in. It is not
evidence Quint is unsuitable elsewhere — consensus protocols are harder
state machines than an orchestration ledger — but there is no primary-source
example of Quint applied to a non-distributed-systems workflow engine.

---

## Adjacent tooling (first-party, `quint-co` org)

| Repo | Purpose | Last push | Stars |
|---|---|---|---|
| [quint](https://github.com/quint-co/quint) | Language, CLI, LSP, VS Code ext, Rust evaluator | 2026-07-20 | 1597 |
| [quint-connect](https://github.com/quint-co/quint-connect) | Rust model-based testing | 2026-05-25 | 75 |
| [quint-llm-kit](https://github.com/quint-co/quint-llm-kit) | "Agents and tools for using Quint with LLMs" | 2026-07-01 | 83 |
| [choreo](https://github.com/quint-co/choreo) | "Choreograph distributed protocols in Quint" | 2026-06-23 | 15 |
| [quint-trace-explorer](https://github.com/quint-co/quint-trace-explorer) | TUI for exploring ITF traces | 2026-01-30 | 11 |
| [quint-sandbox](https://github.com/quint-co/quint-sandbox) | Demo/tutorial material | 2025-07-08 | 4 |

The main repo also ships `skills/quint-lang` and `skills/quint-modeling` —
Claude-Code-style agent skills, installable across coding agents via
`skills/install.sh` ([#1993](https://github.com/quint-co/quint/pull/1993),
[#1994](https://github.com/quint-co/quint/pull/1994), June–July 2026). Directly
usable if daemar wants an agent authoring or maintaining its own specs.

---

## Corrections to common assumptions

Recorded because the brief asked for conflicts to be surfaced:

1. "Quint is an Informal Systems project" — **outdated.** It spun out into Quint
   Co on 2026-04-16 ([/posts/new_era](https://quint.sh/posts/new_era)).
2. "The repo is `informalsystems/quint`" — **outdated.** It is `quint-co/quint`;
   the old path 301-redirects. The npm scope `@informalsystems/quint` has *not*
   moved, and doc pages still link the old GitHub org in places.
3. "Docs live at `quint-lang.org`" — **outdated.** 301 to `https://quint.sh/`.
4. "Rust interop means hand-parsing ITF with `itf-rs`" — **superseded** by
   `quint-connect` (December 2025), which the docs now name as the modern
   approach.

---

## Open questions / caveats

Things I could not settle from primary sources — flagged rather than filled in:

- **Is a release imminent?** No tagged release since 2026-03-31. Commits continue
  and the blog is active, but I found no roadmap, milestone, or statement
  addressing the gap. Cadence risk is real but unquantified.
- **Funding and commercial model.** The spin-out post names no funding round and
  does not state what stays open source long-term or what will be commercial. The
  language and Connect are Apache-2.0 *today*; I found no explicit governance or
  relicensing commitment either way.
- **`quint-connect` maturity.** ~82 commits, 75 stars, last push 2026-05-25. I did
  not verify its crates.io version history, release cadence, or whether any of the
  named production users in §8 use it (versus older hand-rolled ITF harnesses).
  Its README is the only substantial documentation I located; there is no
  first-party tutorial on quint.sh beyond the model-based-testing page's mention.
- **Trace validation tooling.** Described as a technique in the docs; I found no
  shipped command, library, or worked example implementing it. Whether
  `quint-connect` can consume externally-produced traces (as opposed to ones it
  generates) is unresolved and would need a source-code read.
- **`quint test` semantics.** The CLI reference is terse. I inferred that it
  executes `run` definitions written in the spec; I did not find a dedicated docs
  page spelling out its assertion model and failure reporting.
- **Effect system.** Referenced by `typecheck` and by the FAQ, and there is a
  `docs/choreo/custom-effects-extensions` page, but I did not read the effect
  system's reference documentation, so I cannot describe its rules precisely.
- **Performance numbers.** No benchmarks retrieved. The Rust evaluator is
  motivated by performance ("Quint Deserves Rust", 2025-02-18,
  [/posts/quint_deserves_rust](https://quint.sh/posts/quint_deserves_rust)) but I
  found no published speedup figures or state-space scale limits.
- **Non-blockchain adoption.** No primary-source example of Quint used outside
  distributed-systems/blockchain protocols.
