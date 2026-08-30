# M1 — Dogfood the Card

**Status:** Accepted for story planning  
**Date:** 2026-08-26  
**Linear project:** Daemar  
**Priority, estimates, dates, and story order:** Deliberately unset

## Objective

Claude and Codex participating in normal Daemar development can append
structured workflow information to durable Cards, and an operator can inspect
those Cards through a local Queue First console.

This milestone dogfoods the Card before Daemar runs or instruments coding
agents. The agents in the existing development workflow are the first Card
producers. The resulting real data will inform the first agent-execution and
detailed-observability slice.

The milestone is successful when a real Daemar task can be handed between
Claude, Codex, and an operator using its Card as the durable workflow record,
without reconstructing the work from chat transcripts or terminal history.

## Product boundary

M1 builds the first usable factory work log. It does not build the factory
execution engine.

```text
Claude / Codex / operator
          │
          │ agent-friendly Card commands
          ▼
  append-only Card entries ──► SQLite ──► Card projections
                                      └──► Queue First HTMX console
```

There is no Execution Trace capture in this milestone. Model messages, tool
calls, stdout, stderr, token usage, timing, retries, and process supervision
enter scope with the first agent-execution slice.

## Desired behaviors

### 1. Create and identify a Card

- An operator or agent can create a Card for a real Task.
- Daemar assigns the Card its durable identity.
- A Card can retain an optional external task key and repository/workspace
  reference without using either as its identity.
- Creating the same Card twice with the same idempotency key produces one Card.
- The created Card immediately appears in the local queue.

### 2. Append rather than edit history

- Claude, Codex, an operator, or a later factory process can append a typed Card
  entry through a supported application boundary; no producer writes SQLite
  directly.
- Every accepted entry receives a per-Card monotonic sequence assigned by
  Daemar.
- An accepted entry cannot be updated or deleted. Corrections are new entries
  that explicitly supersede or amend earlier information.
- Retrying an append with the same idempotency key does not duplicate the
  entry.
- Concurrent accepted appends have one durable order.
- A malformed or invalid entry is rejected without a partial append or partial
  projection update.

### 3. Record enough workflow structure to dogfood

The initial vocabulary must express at least:

- Task/Card creation and metadata amendment;
- Attempt start and terminal disposition;
- Stage execution start and structured conclusion;
- workflow decision with actor and reason;
- Reported result, including reported changes or artifacts;
- reported evidence, including reported check outcomes;
- Task disposition or handoff.

Entry envelopes carry a schema version, entry ID, Card ID, Card sequence,
producer identity and kind, server-assigned timestamp, entry type, and typed
payload. Attempt and Stage-execution references are present when the entry
belongs to them.

The exact payload schema and transition table are story-planning decisions.
The distinction between Reported result and Verified evidence is not: M1
records agent claims with provenance and never silently promotes them to
independently verified facts.

### 4. Read Cards from agent workflows

- A local CLI gives Claude and Codex a stable, scriptable boundary to create,
  append, and read Cards.
- Machine-facing success output is structured and includes assigned IDs and
  sequence numbers.
- Machine-facing failures are structured enough for an agent to distinguish
  validation, sequence conflict, missing Card, and storage failure.
- A producer can read the current narrow Frontmatter projection.
- A producer can read the ordered Card history and select entries by Stage or
  entry type.
- M1 does not automatically materialize a Stage contract or inject Card data
  into an agent context. Operators can provide the Card identity to the agents
  they are already using.

### 5. Rebuild current state from history

- Frontmatter, Card-list summaries, current Attempt, and current Stage are
  projections derived only from Card entries.
- Restarting Daemar reconstructs the same projections from the same entries.
- A projection can be discarded and rebuilt without changing the Card.
- An unfinished Stage remains unfinished after a writer disappears; silence is
  not inferred to mean success or failure.
- Missing optional values remain unknown rather than becoming fabricated
  defaults.

### 6. Triage Cards in Queue First

- The left pane lists Cards with task key/title, current state, active Stage,
  last activity, and attention state when known.
- Selecting a Card exposes its identity, current Attempt, ordered Stage rail,
  and structured workflow history.
- Selecting a Stage scopes the visible entries to that Stage execution without
  losing the surrounding Card identity.
- Selecting an entry reveals its complete structured payload, producer,
  timestamp, sequence, and amendment/supersession relationship.
- Reported results and evidence are visibly labeled as reported.
- New Card entries appear through HTMX polling without a full-page reload.
- Reloading or closing the browser does not alter factory state.
- The console is read-only in M1; Card writes happen through the application
  boundary used by agents and operators.

### 7. Preserve local, inspectable operation

- SQLite is authoritative for Card entries and uses WAL mode.
- Daemar binds locally and requires no hosted service.
- HTMX is served locally rather than fetched from a CDN.
- Schema migrations are explicit and repeatable.
- The database location is discoverable to the operator, while direct database
  writes remain unsupported.
- Restart after a normal shutdown or process termination does not lose an
  acknowledged Card entry.

## Prototype findings

The throwaway prototype lives at
[`prototypes/card-console`](../../prototypes/card-console/README.md). It is a
design record, not production code.

### Findings accepted into M1

1. **Queue First is the preferred outer hierarchy.** Persistent Card triage on
   the left gives the operator the best orientation before drill-down.
2. **The Stage rail is the primary drill-down seam.** A Card is easier to
   understand when activity is scoped to a selected Stage rather than shown as
   one undifferentiated history.
3. **Summary first, detail on selection.** Rows should stay human-scale; full
   payloads and provenance belong in a selected-entry inspector.
4. **Authority must remain legible.** Card workflow state stays visible while
   inspecting detail. Later Trace observations must not impersonate a Card
   outcome.
5. **Failures must remain visible after later success.** Append-only history
   must not rewrite an earlier reported failure into a clean narrative.
6. **Real density remains unvalidated.** The prototype establishes hierarchy,
   not final spacing, copy, grouping, or information density. Dogfood data is
   required before those decisions harden.
7. **The prototype overreaches M1 in its Trace inspector.** Invocation,
   stdout/stderr, timing, usage, correlation, retry lineage, and raw Trace
   payloads are retained as design input for the first agent slice, not scope
   for this milestone.

### Prototype decisions that are not yet product commitments

- exact color system and typography;
- precise pane widths and responsive behavior;
- whether entry detail remains beside the history or moves to a drawer;
- the final grouping and density of queue metadata;
- the fixture Stage names and entry copy;
- model/tool observability fields shown in the future Trace inspector.

## Acceptance scenarios

These are milestone-level behaviors. Story plans should allocate automated
proof without requiring one story to implement every scenario.

1. **Real dogfood handoff:** Claude starts a Stage execution, appends decisions
   and a Reported result, and Codex later reads the same ordered Card and appends
   the next handoff without chat-history reconstruction.
2. **Idempotent retry:** an agent repeats an append after losing the response;
   the Card contains one entry and returns the original assigned sequence.
3. **Concurrent writers:** two producers append concurrently; both accepted
   entries have distinct sequences and every reader observes the same order.
4. **Correction without mutation:** an agent corrects a reported path or
   outcome by appending an amendment; both the original claim and correction
   remain inspectable.
5. **Reported is not verified:** an agent reports that a check passed; CLI and
   UI preserve the producer and display the result as reported, never verified.
6. **Projection rebuild:** disposable projections are removed and rebuilt; the
   Card list, Frontmatter, Attempt, and Stage state are unchanged.
7. **Restart durability:** Daemar is restarted after acknowledging entries;
   every acknowledged entry and its order remain visible.
8. **Polling without duplication:** the console receives new entries while
   open and shows each exactly once; reload yields the same history.
9. **Invalid append:** an unknown entry version, missing required provenance,
   or invalid lifecycle reference is rejected without changing history or
   projections.
10. **Unfinished work stays honest:** an agent stops writing during a Stage;
    the Card remains in progress rather than inventing a terminal outcome.

## Tooling decisions

- Rust workspace
- SQLite in WAL mode
- SQLx for migrations and database access
- Axum for the local HTTP server
- Askama for server-side templates
- HTMX for interaction and incremental updates
- Tokio as the async runtime
- locally vendored HTMX and plain CSS
- polling rather than WebSockets for the first slice

These choices are implementation inputs, not domain language, and therefore do
not belong in `CONTEXT.md`.

## Explicitly deferred

- Execution Trace storage and projections;
- model messages or provider-supplied reasoning content;
- tool-call capture, stdout, stderr, and raw process logs;
- token, latency, throughput, or currency metrics;
- coding-agent adapters and provider integrations;
- launching, supervising, stopping, or recovering agents;
- retries inferred from operational activity;
- independent Git, filesystem, artifact, or check observation;
- automated Stage transitions or workflow orchestration;
- Stage-contract materialization and automatic context delivery;
- Card queries initiated by a running agent beyond the M1 CLI read filters;
- Linear synchronization;
- scheduling, prioritization, estimates, or target dates;
- authentication, remote access, and multiple users;
- Nar'baha integration;
- plugin architecture, WebSockets, or observability export.

## Inputs for story planning

The following are capability seams, not pre-created stories and not an implied
priority order:

- typed Card, Attempt, Stage-execution, entry-envelope, and error contracts;
- SQLite migrations and atomic append/idempotency behavior;
- lifecycle validation and amendment semantics;
- rebuildable Frontmatter and Card-summary projections;
- agent-facing CLI create/append/read boundary;
- local Axum/Askama application shell and polling fragments;
- Queue First Card list and selection behavior;
- Stage-scoped history and structured entry inspector;
- dogfood workflow documentation for Claude and Codex;
- end-to-end behavioral and recovery test harness.

Every story derived from this milestone should have one coherent behavioral
outcome, include its accepted tests, state what remains out of scope, and avoid
speculative public surface. Stories may be split further when their behavior
can be delivered and reviewed independently.

## Questions story planning must resolve

1. What is the smallest closed set of M1 Card-entry payload variants?
2. Which lifecycle contradictions must Daemar reject, and which should remain
   appendable reports for later human or factory adjudication?
3. What stable JSON shape should the CLI accept and return?
4. How do operators provide Card identity to Claude and Codex during the
   dogfood phase?
5. Which fields belong in Frontmatter before real Stage contracts exist?
6. How are producer identities represented without pretending M1 authenticates
   them?
7. How much raw structured payload should the first entry inspector expose by
   default?
8. Where does local state live, and how does an operator select a different
   database without encouraging direct database manipulation?

These questions should be settled while planning the stories that own the
relevant behavior. They do not justify expanding M1 into agent execution or
detailed observability.

## Discovery sources

- [Observability and frontend first-slice synthesis](../research/observability-frontend-first-slice.md)
- [Super Simple Software Factory source audit](../research/sssf-observability-frontend-current.md)
- [DeepSeek Harness source audit](../research/deepseek-harness-observability-frontend-current.md)
- [Queue First prototype](../../prototypes/card-console/README.md)
- [Daemar domain language](../../CONTEXT.md)
