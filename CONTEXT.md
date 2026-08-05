# daemar

Daes Dae'mar — the Game of Houses. A software factory: deterministic Rust owns
the workflow; models work inside bounded phases; one artifact — the **slip** —
carries each task through its whole flight. The engineer is a controller, not a
pilot: aircraft fly themselves; the controller sequences, separates, and
handles exceptions.

This document is the ubiquitous language. Every term below means exactly one
thing everywhere in this repo — code, docs, prompts, and UI. Change the meaning
here or don't change it at all.

## The core idea

A task's entire life is recorded in an append-only **ledger**. Everything else
— current state, phase context, the UI — is a **projection** of that ledger.
Ambiguity decreases monotonically as work flows inward: a loose prose request
enters the factory; a validated, typed contract crosses into execution. Checks
are earned, not speculated: structure is added when a real run demonstrates the
need, and never before.

Heritage, for the archaeologists: ATC flight progress strips (the slip, the
board, cocking, clearances), manufacturing travelers (the record rides with the
work), SSSF's "agent proposes, code disposes" (code owns the loop), and this
repo's own Wayfinder era (`archive/wayfinder-v0`: the run-record event
envelope, the all-problems-at-once preflight pattern — quarry, don't inherit).

## Glossary

**Ledger** — the append-only event log of one task's flight: every stage,
every clearance, every query, every annotation. The single source of truth.
Events have versioned kinds (`plan_section_written.v1`); anything spanning
real time gets an intent/outcome pair. _Avoid_: updating or deleting events;
a correction is a new event.

**Slip** — the canonical record of one task, derived by folding its ledger.
Face plus sections. The slip is always a projection — it cannot lie, because
it cannot be written. _Avoid_: any code path that mutates a slip directly.
A slip's engineer is permanently its **opener** — the slip belongs to who
opened it. Each phase additionally records the engineer that flew that stage
(`phase_started.v1`), so a flight continued by another client keeps its owner
while the audit keeps the flyer.

**Face** — the slip's frontmatter: id, request one-liner, current phase,
owner, status, clearance state, cost. Printed into every phase's context,
rendered on every board card. Stable schema forever, so one board renders
every workflow ever built.

**Section** — a typed, versioned unit of slip content: `request.v1`,
`plan.v1`, `review.v1`. Workflows extend the factory by adding section kinds,
never by touching the face.

**Printout** — the context assembled for a phase at entry: the face plus the
sections named in the phase's declaration. A pure function of
(ledger, declaration) — deterministic, reproducible byte-for-byte, at any
later date, for any model. The printout is the sector-local strip: the ledger
is infinite, the reader gets exactly what its job needs.

**Declaration** — what a phase states it requires, fixed at phase entry.
Defines the printout. Declarations evolve from evidence: a section every run
queries gets promoted into the declaration; a declared section nobody reads
gets demoted.

**Query** — the pull tier: a model asks the ledger for more than its printout,
through a mechanism, mid-phase. Every query and its result reference are
themselves appended to the ledger. This makes each phase's knowledge
auditable: not only what happened, but what the deciding agent knew, and when.
_Avoid_: any read path that bypasses the query mechanism; an unlogged read is
a hole in the epistemic record.

**Phase** — one bounded segment of a task's flight. A model (or code, or the
engineer) is autonomous within a phase and stopped at its boundary. Phases
default to failure; success is earned by a clean exit and a granted clearance.

**Clearance** — permission to cross a phase boundary. Granted by code
(required sections present and valid — reported all-problems-at-once, with
pointers, so a rejection is a correction prompt) or by the controller (an
approval stamp). Recorded on the ledger like everything else. Every grant and
refusal is signed: `by` names the grantor — the engineer, or a code gate
(`by: "gate:<name>"`). A boundary's policy is a dial, not a doctrine: human
stamp, code gate, or open — chosen per boundary per workflow, and demoted on
evidence (a gate that refuses nothing across hundreds of flights has proven
the check can relax). Management by exception, never by turnstile.

**Cocked** — a slip whose latest clearance request has no response: it needs
the controller. Always derived from the ledger, never asserted — the same rule
as liveness, where a missing terminator event means interrupted, and no event
ever claims "still running."

**Territory** — the repository a flight operates on, recorded on
`slip_opened` and remembered by the slip (a resumed flight lands in the
right repo because the ledger says so, not because anyone re-typed a flag).
The tower model: one factory, many territories — daemar stays home, its
agents visit. Tools are confined to the territory by construction.

**Board** — the observability front end: slip faces in bays, ordered by
attention; progressive disclosure from face to sections to raw ledger. The
controller's instrument. Built first, because every later piece of the factory
is born with a place to be seen.

**Controller** — the engineer. Manages by exception: watches the board, works
the cocked slips, grants or refuses clearances. Does not fly the aircraft.

**Role** — the seat a workflow stage requires: scout, planner, responder. A
stage names a role, never an agent — the stage owns how the seat is used
(phase name, section kind, task prompt); it never owns who sits in it. Roles
are a closed set (a Rust enum): adding one is a variant, and the compiler
walks you to every decision that must care.

**Agent** — who fills a seat: name (signed as `owner` and `by` on the
ledger), persona, airframe binding, tool access. Config defines who an agent
IS; the call site defines how it is USED — the decoupling inherited from
SSSF. _Avoid_: stage concerns leaking into an agent definition, or persona
text living at a call site.

**Roster** — the binding from role to agent. Today it is 1:1 and lives in
Rust; when multiple candidates per role exist it becomes data
(`roster.toml` beside `airframes.toml`) and bindings can be assigned per
flight — by policy, or someday by measured competence.

## The three-tier context economy

1. **Pushed always** — the face. On every printout, no exceptions.
2. **Pushed by declaration** — the sections a phase declared. Deterministic.
3. **Pulled on demand** — queries, logged as events.

Storage is unlimited; consumers are not. The human has glance-budget, the
model has a context window — so the paper strip's discipline survives at
projection time. The query log is the tuning instrument for tier 2: context
contracts evolve from measured demand, not taste.

Because the ledger is the memory, a model session is only a cache. Resuming a
task is re-printing it. Any model can pick up any slip at any boundary, which
is what makes the factory crash-tolerant, multi-model, and cheap to iterate.

## Hard rules

1. Append only. The slip is a fold. Nothing writes state directly.
2. Every read outside the printout goes through the query mechanism and lands
   on the ledger.
3. Liveness and attention (running, interrupted, cocked) are derived, never
   asserted.
4. Success is earned: phases default to fail; boundaries require clearance.
   And its mirror — success may be self-reported, failure must be witnessed:
   a process may close its slip accepted (gates check the claim), but never
   rejected. A failed flight stays open, in the attention bay, until the
   controller disposes it. The machine's verdict on its own failure is
   exactly the judgment we do not trust.
5. Checks are added when a real run shows the need. Not before.
6. Workflows are Rust. _Avoid_: workflow DSLs, graph engines, config-as-code
   control flow — the compiler is the first gate.
7. Rust discipline: strings cross exactly one boundary — the serde edge — and
   are parsed there, once. Closed sets are enums (the exhaustive match is the
   change-impact analysis); open vocabularies are newtypes; unknown wire data
   is an explicit variant, never a silent skip. Every fallible seam has its
   own error enum with real variants. _Avoid_: `Box<dyn Error>`, stringly
   interfaces, re-parsing past the boundary, dropping anything uncounted.

## Build order (dogfooding)

1. **The board.** Renders slips from fixture ledgers before any loop exists.
   The fixtures define the schema demand-side.
2. **The first loop.** Deliberately minimal: Rust, one model API behind a thin
   seam, no checks — the board and the controller's eyes are the check.
3. **Everything else, earned.** Clearances, section validators, more phases,
   more workflows — each added when a real run on the board demands it.
