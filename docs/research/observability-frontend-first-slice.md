# Daemar observability and frontend: first-slice decision

**Date:** 2026-08-26  
**Inputs:** [SSSF source audit](sssf-observability-frontend-current.md) and
[DeepSeek Harness source audit](deepseek-harness-observability-frontend-current.md)

## Decision

Build a local, read-mostly **task-card console**.

- Take the outer information hierarchy from Super Simple Software Factory:
  task work, ordered stages, stage status, validation evidence, and progressive
  drill-down.
- Take the inner activity presentation and durable-stream discipline from
  DeepSeek Harness: sequenced events, readable model/tool cards, explicit
  retries and terminal errors, replayable projections, and honest missing
  metrics.
- Keep two explicitly different streams: the Card is durable workflow truth;
  an Execution Trace is detailed operational evidence for the frontend. Add the
  factory concern neither system makes strong enough: a task's trusted produced
  changes and validation outcome must be first-class, not buried in transcript
  prose or generic JSON.

Do not build a chat application, a generic observability platform, or a
pluginized UI framework. Do not let the Execution Trace become a second workflow
record. Do not put Nar'baha or virtualization lifecycle into this slice.
Daemar is currently learning what the factory needs.

## The questions the first slice must answer

An operator should be able to answer these without opening a terminal or
reading a transcript like a novel:

1. What Card is the factory working on?
2. Which attempt and Stage is active, and is it still making progress?
3. What is the agent or deterministic code doing right now?
4. If work failed or retried, where and why?
5. What changed, and what evidence says the result is acceptable?
6. How long and how many reported tokens did it consume?
7. Can I stop it?

## Product hierarchy

Neither upstream hierarchy should be copied literally. SSSF makes an ADW
session the outer unit; DeepSeek makes one agent session the durable unit.
Daemar's outer unit is the Card: one durable workflow record for one Task,
across every attempt and Stage. Each Stage execution can have a separate,
high-detail Execution Trace referenced by the Card.

```text
Task
├── Card
│   ├── Stage lifecycle and structured output
│   ├── Workflow decisions and validation evidence
│   ├── Changes and artifacts
│   └── References to Execution Traces
└── Execution Trace per Stage execution
    ├── Model steps and visible messages
    ├── Tool / command calls and output
    ├── Retries, timings, and usage
    └── Operational errors
```

The Card also exposes narrow Frontmatter, provides each Stage the information
declared by its Stage contract, and answers explicit Card queries for additional
history. These are three views over one record, not three stores.

The first UI does not need a separate page for every level. It joins Card entries
and Trace events at the Stage execution. The model must retain the distinction
so later retries, parallel runs, agents, and gates do not need to be
reverse-engineered from a flat transcript.

## First screen

Use a two-pane layout with a compact Stage rail inside the selected Card:

```text
Cards                         Selected Card
------------------------      ------------------------------------------
● PER-123  implementing       PER-123 · attempt 2 · 03:41        [Stop]
✓ PER-122  accepted           Plan  >  Build*  >  Test  >  Review
× PER-121  failed
                              Activity
                              Agent: investigating the failing test
                              ▸ read  src/worker.rs
                              ▸ bash  cargo test       failed · 12s
                              ↻ retry 2/3              previous error kept

                              Changes                 Evidence
                              M src/worker.rs          test: pending
                              A tests/worker.rs        review: pending

                              3 model steps · 18k reported tokens · 03:41
```

### Left pane: Card triage

Show:

- external task key and title;
- latest attempt status and active Stage;
- elapsed time and last meaningful activity;
- accepted, failed, cancelled, or needs-attention state;
- multiple run attempts only when they exist.

Do not add miniature decorative telemetry, archive flows, advanced filters, or
project analytics until the Card population creates that pressure.

### Right pane: Card understanding

Keep these visible:

- Card identity, current attempt, status, elapsed time, and Stop;
- ordered Stage rail with the active or failed Stage obvious;
- chronological activity cards for assistant messages, tool calls,
  deterministic commands, retries, warnings, and terminal errors;
- compact result and validation summaries;
- reported model, step count, token buckets, model time, tool time, and wall
  time when available.

Cards are human-scale by default. Exact arguments, output, timestamps, raw
payload, and correlation IDs belong behind disclosure controls. Reasoning is
collapsed by default. A later success must not erase an earlier failure or
retry.

The frontend should make provenance legible without making the interface noisy:
Stage status, decisions, structured output, changes, and checks come from the
Card; the live activity feed, detailed messages, tool cards, output, and metrics
come from the selected Execution Trace. A green-looking Trace never implies an
accepted Stage, and a missing Trace never erases the Card's outcome.

### Result detail

Make result inspection a first-class section:

- changed paths and added/modified/deleted status;
- unified diff where available;
- commits and retained artifacts;
- deterministic check outcomes with their evidence;
- final acceptance or rejection reason.

Agent claims can explain intent, but Git/filesystem facts and check results
decide what happened.

## Two streams, two kinds of authority

### Card: durable workflow record

The Card contains only information relevant to advancing, evaluating, or
understanding the Task as factory work. Current status, Frontmatter, attempt
totals, Stage progress, and list summaries are projections from Card entries.

Every Card entry needs at least a schema version, entry ID, Task/Card ID,
per-Card monotonic sequence, producing Stage or control-plane identity,
timestamp, and typed payload.

Start with these Card-entry families:

| Family | Durable workflow facts |
|---|---|
| Task/Card | created; task metadata amended; terminal disposition |
| Attempt | started, cancellation requested, finished; terminal outcome |
| Stage | declared, started, finished; contract/version, owner, structured output, outcome |
| Context | Stage input materialized; Card sequence and selected entry/artifact references |
| Decision | accepted, rejected, routed, retried, or escalated; reason and actor |
| Check | verdict and item-level evidence relevant to acceptance |
| Result | changes, commits, artifacts, handoff data, and acceptance outcome |
| Trace reference | Execution Trace identity, Stage execution, completeness, and optional digest |

Frontmatter should be a rebuildable projection at a known Card sequence. Stage
input should record the Stage contract version and Card sequence from which it
was materialized. A Card query should record the query and selected references,
not duplicate the selected history.

### Execution Trace: detailed operational record

An Execution Trace contains the high-volume detail needed to watch and diagnose
one Stage execution:

- visible model messages and provider-supplied reasoning content or summaries;
- tool and command calls with arguments, results, exit status, and duration;
- stdout, stderr, logs, warnings, and operational errors;
- model-step boundaries, retries, token buckets, latency, and throughput;
- subagent or child-operation activity when it exists.

Every Trace event needs its own schema version, Trace ID, Stage-execution ID,
per-Trace monotonic sequence, timestamp, producer, correlations, and typed
payload. A quiet or disconnected trace never implies completion. Gaps are shown
as observability gaps and repaired from persisted Trace events when possible.

The Trace can be higher-volume, more provider-specific, more sensitive, and
shorter-lived than the Card. Hidden chain-of-thought must not be promised;
display only reasoning content or summaries a provider actually supplies and
that policy permits retaining.

### Promotion between streams

Trace events do not become Card entries automatically. The Stage controller
validates a Stage's structured conclusion and appends the workflow-relevant
result to the Card, including a reference to its Execution Trace. A tool call,
token count, or assistant message belongs only to the Trace unless a workflow
decision explicitly promotes a fact derived from it.

The Card remains valid if detailed Trace retention expires. The frontend then
shows that the detailed trace is unavailable rather than treating workflow
history as missing. Conversely, a complete Trace cannot manufacture a missing
Stage outcome on the Card.

## Persistence and transport

The two logical streams do not require two databases:

- Use SQLite in WAL mode with separate `card_entries` and `trace_events`
  append paths and independent sequences.
- Maintain disposable Card projections for Frontmatter, Card-list summaries,
  and current Stage/attempt state. Rebuild them only from Card entries.
- Build transcript/tool/metrics projections only from Trace events.
- Put large stdout, stderr, diffs, and other evidence in owned artifact files;
  reference them from the appropriate stream with size and digest.
- Let Stage contracts and Card queries use a Card reader. They do not read the
  Execution Trace implicitly.
- Poll both `cards/{id}/entries?after_seq=N` and the active
  `traces/{id}/events?after_seq=N` about twice per second, then perform a final
  drain. WebSockets are unnecessary for the first local slice.
- Detect sequence gaps independently and resynchronize without silently
  displaying partial history.
- Make Stop the only first-slice mutation, sent with an idempotency key.
- Bind locally. Authentication and remote multi-user access are later work.

Do not dual-write JSONL and SQLite while claiming both are authoritative. The
Card is authoritative for workflow history; the Execution Trace is authoritative
for retained operational observation. Their authority does not overlap.

## Borrow now

| Source | Component | Why it earns first-slice scope |
|---|---|---|
| SSSF | Run list -> phase progress -> phase drill-down | Fast triage followed by evidence-local diagnosis |
| SSSF | Phase as the join point for intent, activity, output, and checks | Prevents information from scattering across screens |
| SSSF | Normalized tool call rather than raw provider fragments | One real action becomes one readable record |
| SSSF | SQLite WAL plus monotonic cursor reads | Simple local live/history path |
| DeepSeek | Append-only sequenced ledger and derived projections | Replay and UI evolution without competing truth |
| DeepSeek | Baseline plus ordered deltas with gap recovery | A reconnect cannot invent or omit history |
| DeepSeek | Inline tool cards, retries, errors, and interrupted output | Makes the trajectory readable without hiding failure |
| DeepSeek | Explicit missing token/timing values | Avoids fabricated zeroes or precision |
| Daemar | One Task, one append-only Card | Keeps all attempts and Stages in one durable traveling work record |
| Daemar | Separate Execution Trace | Supplies deep live diagnosis without polluting workflow history |
| Daemar | Frontmatter + Stage contract + Card query | Gives Stages progressive disclosure without full-history context injection |
| Daemar | First-class changes, artifacts, and check evidence | The operator ultimately decides whether work is acceptable |

## Defer deliberately

- time-scaled multi-agent swimlanes until real concurrency needs them;
- DeepSeek's trajectory inspector, zooming timeline, and virtualization;
- pluginized UI slots and third-party frontend composition;
- recursive subagent navigation;
- full prompt/config viewers and raw reasoning as primary UX;
- context-pressure estimation;
- currency cost until provider/model/price-time/cache provenance exists;
- OpenTelemetry or any outbound observability exporter;
- queue steering, forking, plan approval, goal management, and workflow editing;
- archive inboxes, themes, localization, and settings screens;
- Nar'baha machine, network, or cleanup telemetry.

Preserve timestamps, parent IDs, typed payloads, and artifact references so the
valuable deferred views can be built from real demand rather than a migration.

## First-slice acceptance tests

1. Reloading a Card reconstructs the same ordered workflow decisions, Stage
   outcomes, changes, and check evidence across every attempt.
2. Disconnecting and reconnecting during an attempt produces no missing or
   duplicate retained Trace events.
3. A sequence gap in either stream forces that stream to resynchronize.
4. Failed calls and retries remain visible in the Execution Trace after eventual
   success, without becoming Card entries merely because they occurred.
5. Closing the browser does not cancel work; Stop visibly progresses from
   requested to a terminal outcome.
6. A crashed worker is recovered as interrupted rather than left running
   forever.
7. Missing Trace metrics render as not reported, never zero.
8. Secrets do not appear in Card entries, Trace events, frames, errors, or
   retained argument/output snippets.
9. Changes and checks come from trusted factory observation, not assistant prose.
10. The same Card and UI work regardless of which coding-agent harness produced
    the normalized Trace events.
11. Expiring an Execution Trace does not change the Card or its workflow
    projections; the frontend reports that detailed observation is unavailable.

## Bottom line

SSSF supplies the factory-oriented skeleton. DeepSeek supplies the more robust
event and activity-view mechanics. Daemar's first frontend should combine them
into a small operator console whose primary unit is the Card and whose final
question is not “what did the agent say?” but “what happened, what changed, and
why should I accept it?”
