# DeepSeek Harness observability and frontend: current implementation and a Daemar first slice

**Snapshot date:** 2026-08-26  
**Pinned upstream:** [`deepseek-ai/deepseek-harness` `dsh-v0.1.1-rc.2` / `b150a551b8d465e31e418e1b2eaf5e79bbb7d28e`](https://github.com/deepseek-ai/deepseek-harness/tree/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e) (commit date 2026-08-21)  
**Question:** What does DeepSeek Harness actually implement for its frontend and observability, and which parts belong in Daemar's first slice?

## Executive conclusion

DeepSeek Harness's most valuable idea for Daemar is **an append-only, sequenced event log as the durable truth, with transcript, progress, metrics, hierarchy, and artifacts built as disposable projections of that log**. This lets the same facts support a simple chat-like run view now and a denser diagnostic view later without introducing a second account of what happened. The browser receives an initial history/projection cut, then monotonic live events and projection updates; reconnect and replay return to the durable authority.

The first Daemar UI should borrow that spine, but not DeepSeek Harness's breadth. A useful first slice is one local operator console with:

1. a run list;
2. one run detail containing status, elapsed time, agent/phase progress, and a live transcript of model messages and tool/command calls;
3. explicit inline failure/retry/cancellation records;
4. produced-file and changeset/diff inspection;
5. a small metrics strip: model, turns/steps, input/cache/output tokens where reported, LLM/tool time, and run wall time;
6. Stop as the only mutating operator control.

That slice needs durable events and replay-safe read models before it needs React cleverness. Defer DeepSeek's plugin-composed UI, three-column inspector, trajectory timeline, virtualized paging, context-pricing heuristics, OpenTelemetry export, complex subagent navigation, approvals, queue steering, workflow controls, themes, and settings. They solve scale and extensibility problems Daemar does not yet have.

DeepSeek Harness does **not** currently provide a cost dashboard in the inspected release. It reports token buckets, cache hit ratio, context pressure, timing, latency, and throughput. Its model catalog contains cost metadata in places, but the web client does not turn that into per-run currency accounting. Daemar should not infer or display dollar cost until provider, model version, cached-token pricing, retries, and price-time provenance are explicit.

## Identity and evidence standard

There is no material identity ambiguity after checking first-party sources. DeepSeek's official product page names **DeepSeek Harness**, links `https://github.com/deepseek-ai/deepseek-harness`, and gives `npx @deepseek-ai/dsh web` as the quick start ([official product page](https://www.deepseek.com/harness/en/)). The repository identifies itself as an open-source harness developed by DeepSeek AI and warns that it is a rapidly changing developer preview ([pinned README](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/README.md#L1-L23)).

This report treats pinned source, package documentation kept beside that source, and repository end-to-end tests as implementation evidence. It does not treat the product-page phrase “everything is a plugin” as proof that a particular screen exists. The pinned application was inspected but not launched, and its upstream tests were not re-executed in Daemar's workspace. Therefore “implemented” below means that the pinned code and test suite contain the behavior, not that this research independently certified the release binary.

## The implemented data model

### One session is one agent's durable ledger

The core `Session` is an append-only log of JSON-serializable, contiguously sequenced events. The model-visible message history is derived from it rather than stored as a second structure. Core events include:

- `turn/start` and `turn/end`, with completed, aborted, blocked, error, max-token, and crash-interrupted outcomes;
- `step/start` and `step/end`, where a step is one model request plus requested tools;
- `user/message`, raw `assistant/chunk`, and assembled `assistant/message`;
- `tool/call` with the model's raw argument JSON and `tool/result` with a paired call id, structured error identity, and optional tool-owned presentation metadata;
- whole-list `todo/write` progress snapshots;
- `request/header` and `request/context`, including provider, model, request envelope, and advertised context window.

The assistant's provider-reported token usage is carried on the assembled assistant message. A cancelled stream can still finalize the visible prefix with `interrupted: true`. Tool result `meta` can preserve a result-time contextual diff so replay reproduces the same card ([session event types](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/core/session/src/types.ts#L230-L313)).

Plugins extend the ledger with events for retries, compaction, approvals, goals, agent presets, inbox changes, subagent descriptors, team work, and workflow runs. The build generates a known-event catalog and refuses required event types it cannot interpret, avoiding a plausible-looking replay with missing semantics ([known event catalog](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/core/session/src/known-event-types.ts#L8-L69)).

### Persistence is lossless and local

The default persistence backend stores one logical JSONL transcript per session, compressed as concatenated Zstandard frames by default. It retains raw chunks, contiguous sequence numbers, session lineage, cwd, selected agent preset, and delegation depth. Writes batch but remain append-only and `fsync`; crash recovery preserves a valid tail and synthesizes explicit interrupted tool/step/turn closers instead of silently pretending completion. The backend explicitly has one live-writer ownership and no deletion API ([JSONL persistence contract](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/session/session-persistence-jsonl/README.md)).

This is local-first behavior, not merely branding: DeepSeek's data statement says prompts, model output, tool calls, attachments, paths, results, and runtime logs are stored on the user's device by default ([official data-processing statement](https://www.deepseek.com/harness/en/data-processing/)).

### Projections are derived read models, not another truth

Host-side projection units fold the ledger into whole values such as title, todo list, token usage, context pressure, context composition, goals, permissions, and full-session statistics. A projection snapshot has an `asOfSeq`; live `session/projection` frames are higher-sequence-wins. The projection registry drives every registered unit from each committed session event and can checkpoint fold state to avoid replaying an entire cold log ([projection subsystem](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/docs/subsystems/session-projection.md)).

That distinction is important for Daemar. The durable fact should be “tool call X ended with error Y at sequence N,” not “the dashboard error count is 4.” Counts, current status, progress summaries, and UI cards are recomputable views.

## Frontend and live-update architecture

The shipped app is a TypeScript/Vite/React 18 browser client. Its object runtime uses Zustand and Immer. Cordis supplies the plugin lifecycle, and a typed slot registry composes screens and controls. The app is split into many client packages rather than one component tree ([web app package](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/apps/web/package.json), [client runtime package](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/client/runtime/package.json), [slot registry](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/client/ui-slots/README.md)).

The transport is HTTP POST for unary commands/responses and two downlink-only WebSockets for session/multiplexed events and host events. A connection is ready only after both sockets and a host-description handshake succeed. Losing either stream tears down the connection generation and rebuilds both; the ordinary web transport has no SSE fallback ([connection contract](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/client/connection/README.md#L5-L13)).

On the browser side, `SessionRuntime` owns a shared contiguous event window, history paging, session/workspace list mirrors, and projection stores. The initial history tail seeds projections; live session events update transcript definitions, while live projection frames update higher-sequence-wins cells. Incremental list frames that arrive during a baseline request are replayed over the response rather than lost. The client also carries authoritative transient snapshots for queue, pending interactions, and jobs where those facts do not belong to a completed transcript ([client runtime](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/client/runtime/README.md)).

The Daemar lesson is not “use exactly two WebSockets.” It is:

- baseline plus ordered deltas;
- every durable event has `run_id`, monotonic `seq`, timestamp, and typed payload;
- a reconnect can ask “events after sequence N” or replace from a consistent snapshot;
- transient liveness is visibly distinct from durable outcomes;
- the UI detects a gap and resynchronizes instead of guessing.

For a first local console, one WebSocket carrying typed run events plus ordinary HTTP commands is sufficient.

## Implemented screens and components

### Shell, run/session browser, and transcript

The shell is a three-column frame: collapsible sidebar, conversation, and optional details panel. The details panel starts closed and its geometry is intentionally transient. The sidebar delegates workspace/session grouping and search to a separate package; subagent sessions are omitted from the ordinary list and reached through parent lineage ([layout](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/client/ui-layout/README.md), [sidebar](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/client/ui-sidebar/README.md)).

The default Chat view renders durable user and assistant messages, streaming text, collapsed reasoning, context-injection disclosures, tool calls/results, compaction markers, todo progress, queued messages, retries, terminal errors, and per-turn footers. Tool calls are paired and topologically assembled by the client runtime, then dispatched to specialized cards for shell, read, diff, search, web, write/edit, todo, question, and code-dispatch results; unknown tools retain a generic fallback ([conversation implementation](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/client/ui-conversation/README.md), [tool presentation](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/client/ui-tool/README.md)).

### Diagnostic trajectory

Trajectory is a separate conversation tab, not the chat transcript with debug rows toggled on. It renders a turn-aware ledger of User, Assistant, Tool, and nested Subtool records. Selection opens an inspector for token usage, duration, input, output, and timing. A timeline overview distinguishes time-to-first-token from decoding, supports interval selection, zoom, and pan, and virtualizes long histories while preserving streaming tail-follow semantics. Running records deliberately show no invented duration ([trajectory package](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/client/ui-trajectory/README.md#L1-L17)). Repository browser tests assert bounded mounted-row counts while loading older pages and streaming, but those tests were not rerun for this report ([trajectory E2E](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/apps/web/tests/trajectory-virtualization.e2e.ts#L174-L336)).

This is impressive but not a first-slice requirement. Daemar can preserve the event detail necessary to add it later without building it now.

### Run and agent hierarchy

DeepSeek's primary durable unit is the session/agent. Subagents have parent lineage, mode (`continuable` or one-shot), title/label, running state, accumulated tokens, active duration, and recursively browsable child catalogs. The current header shows breadcrumbs and descendant activity; selecting a child opens its separate transcript. The package explicitly notes a limitation: its catalog does not durably distinguish completed, failed, and cancelled outcomes, and exposes no activation identity ([subagent UI](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/client/ui-subagent/README.md)).

Workflow runs are separately reconstructed from four durable start/end events. The UI groups actually-started members by phase and shows member status and live navigation, but deliberately omits scripts, outputs, errors, logs, usage, topology, and controls ([workflow-run UI](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/client/ui-workflow-run/README.md)). Background jobs appear in a flat status/duration list; failed detail is retained, but rows are read-only and process restart empties the registry even though launch cards remain in the transcript ([background jobs UI](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/client/ui-jobs/README.md)).

Daemar should not copy `Session == Agent` as its top-level product model. Its operator hierarchy is better expressed as:

```text
Factory Run
├── Phase / gate
│   └── Agent Attempt
│       ├── Model Step
│       └── Tool / command call
└── Verification / promotion events
```

A delegated child may be another Agent Attempt beneath its initiator, but a run's security lifecycle, deterministic gates, changeset, and promotion decision remain first-class siblings rather than being forced into a chat session.

### Progress, artifacts, and diffs

Progress is intentionally modest: `todo/write` replaces a list of `{content, status}` items, and the composer dock collapses it to counts of completed, in-progress, and pending work. Workflow panels add run/phase/member status. This is useful presentation, but the todo list is agent-authored and should not be confused with trusted factory gates ([todo data type](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/core/session/src/types.ts#L179-L194)).

Successful mutation-tool results carry file locations and diff/card intent. The deliverables plugin derives a per-turn “Produced” row from those tool facts, not from whatever files the final prose happens to mention; failed calls, reads, and deletes are excluded. It links up to six file chips and can open the workspace when local native capabilities allow it. Files created indirectly through terminal commands are a documented blind spot ([deliverables UI](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/client/ui-deliverables/README.md)).

Daemar already owns a stronger primitive: the trusted host-side exact `ChangeSet`. The first UI should render that canonical result after sandbox teardown, with added/modified/deleted/unsupported classification and an on-demand textual diff. Agent tool metadata may explain intent, but must not decide what changed.

### Metrics and context

DeepSeek implements:

- cumulative uncached-input, cache-read, cache-write, and output token buckets;
- cache-hit ratio;
- turn and step counts;
- model wall time and tool wall time;
- average time to first token and decode throughput;
- per-turn elapsed time, TTFT, and throughput when observations exist;
- approximate context occupancy and a heuristic breakdown across system prompt, tools, and messages.

Whole-session stats are host-folded from boundaries, chunks, call/result pairs, and assembled messages, so history paging and compaction do not change them. Missing timing or usage samples are excluded rather than treated as zero ([session-stats projection](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/session/session-stats/README.md), [chat metrics and context meter](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/client/ui-conversation/README.md#L43-L43)). Context occupancy is explicitly approximate: provider samples and route capacity are independent last-wins facts, and heuristic composition rows need not sum to the displayed header ([token-meter types](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/llm/token-meter/src/projection.ts)).

There is no implemented per-session currency total in the web client at this pin. “Token usage” and “context pressure” must not be relabeled “cost.”

### Errors, retries, and controls

Retries are durable events. The chat view replaces a failed attempt's partial streaming tail with one stable retry row, shows scheduled delay and finite or unbounded retry policy, and retains the row after a later success. A terminal error becomes a persistent turn-boundary record with safe message and optional code. Authentication errors replace provider text that could contain credentials. Cancellation preserves a visible interrupted prefix when one exists ([retry/error projection](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/client/runtime/README.md#model-retry-projection), [live interaction E2E](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/apps/web/tests/live-interactions.e2e.ts#L55-L279)).

The assembled application includes send, queue, steer, stop generation, fork-from-completed-turn, model selection, permission presets, one-shot approval allow/reject, plan review, goal actions, and some subagent continuation/interrupt behavior. The limitations matter: background jobs are read-only, one-shot children are read-only records, approval has no durable grant option, and the raw details panel is implemented but has no assembled entry point ([conversation limitations](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/client/ui-conversation/README.md#known-limitations-and-deferred-work)).

## Outbound telemetry is a separate concern

DeepSeek also has an optional session-telemetry seam. It mirrors ledger events plus operational `agent-error` and `shutdown` signals into logical records with severity and minimal identity. Only the first assistant chunk per step is exported; other session events are exported whole. Delivery is best-effort and receivers must deduplicate on `(session.id, event.seq)`. A redaction waterfall is available, but ships with **no redaction rules** ([telemetry seam](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/docs/subsystems/session-telemetry.md)).

The only shipped backend uses the OpenTelemetry JS Logs SDK and OTLP/HTTP. It defaults disabled, supports full or feedback-only export, and warns that uploading can include prompts, responses, tool arguments/results, file content, full system prompts/tool schemas, todos, hook summaries, and cwd unless the deployment adds redaction ([OTel backend](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/session/session-telemetry-otel/README.md)).

Daemar should separate **operator observability** (the local evidence needed to understand and control a run) from **outbound telemetry** (data exported for fleet operations). The first slice needs the former and should have no outbound exporter.

## Recommended Daemar first slice

### Durable event vocabulary

Start with a small closed vocabulary whose payloads are versioned JSON, even if the Rust implementation uses typed enums internally:

| Event family | Minimum durable facts |
|---|---|
| Run lifecycle | `run.created`, `run.started`, `run.cancel_requested`, `run.finished`; terminal outcome and structured error |
| Sandbox lifecycle | preparation, boot, workload start/exit, export, teardown; substrate id/version and isolated run id |
| Agent attempt | attempt started/finished; role, model/provider, parent attempt, phase; terminal outcome |
| Model step | request started, first output, completed/failed; token buckets and finish reason when reported |
| Transcript | user/context input, assistant delta or periodic snapshot, finalized assistant message |
| Tool/command | call started/finished; stable call id, name, safe arguments summary, exit/error, stdout/stderr artifact references |
| Factory progress | phase/gate started/finished; trusted result distinct from agent-authored todo text |
| Changes | changeset ready; exact A/M/D/unsupported entries and diff artifact reference |
| Retry | retry scheduled/started/exhausted; producer id, attempt number, delay, safe failure code/message |

Every event needs `schema_version`, `run_id`, monotonic `seq`, source timestamp, producer, and correlation ids. Write terminal outcomes once; never infer “success” from a disconnected live stream. Keep raw secrets and unrestricted environment snapshots out of the event schema.

For streaming output, DeepSeek keeps every chunk. Daemar can begin more cheaply with append deltas or bounded periodic snapshots plus a finalized message, provided replay is lossless for the operator-visible transcript and sequence/gap rules are explicit.

### Persistence and API

- One append-only local run log, plus content-addressed or run-owned files for large stdout/stderr, patches, images, and exported artifacts.
- A small projection table/cache for run summary, current phase, current attempt, cumulative metrics, and latest artifact list. It is disposable and rebuildable.
- `GET /runs`, `GET /runs/{id}`, and `GET /runs/{id}/events?after_seq=N`.
- One WebSocket subscription carrying `{run_id, seq, event}`. On a gap, stop applying deltas and resync from the HTTP endpoint.
- Commands are separate HTTP requests with idempotency keys: initially only `POST /runs/{id}/cancel`.
- Bind to loopback only. Remote access and authentication are later product work; DeepSeek itself says its Host fence is reachability policy rather than authentication ([connection trust limitation](https://github.com/deepseek-ai/deepseek-harness/blob/b150a551b8d465e31e418e1b2eaf5e79bbb7d28e/packages/client/connection/README.md#L7-L13)).

### One operator screen

Use a two-pane layout first:

```text
Runs                          Selected run
--------------------          --------------------------------------
● PER-123  running            PER-123 · running · 03:41      [Stop]
✓ PER-122  passed             phase: implementation  2/4 gates
× PER-121  failed
                              Transcript / activity
                              user prompt
                              assistant message
                              ▸ bash  cargo test       failed (12s)
                              ↻ retry 2/3 in 4s
                              assistant message

                              Changes (7)   Metrics
                              M src/...     3 steps · 18k tokens
                              A tests/...   LLM 2:31 · tools 0:42
```

The activity flow should default to human-scale cards, not raw JSON. Each card can disclose exact arguments, output, timestamps, and correlation ids. Reasoning is collapsed by default. Terminal errors remain in chronological position. A run-level Changes section uses Daemar's trusted `ChangeSet`, and selecting a file opens its diff in the same pane or a modal. The summary metrics strip stays visible near the run header or composer-equivalent bottom edge.

Show a simple nested attempt/phase outline only when more than one agent exists. Do not build recursive breadcrumb catalogs before delegation exists in Daemar's domain.

### First-slice acceptance behaviors

1. Reloading a completed run reconstructs the identical ordered transcript, error outcomes, metrics, and changeset.
2. A browser disconnect during a run reconnects without duplicate or missing durable events.
3. A missing sequence forces resync and never silently advances the UI.
4. Cancellation visibly moves `requested -> terminal`; closing the browser does not cancel the run.
5. A failed tool/command retains exit/error and bounded output even if a later retry succeeds.
6. Agent-authored progress and trusted gate results are visually and structurally distinct.
7. Token and timing fields display “not reported” when absent; they do not become zero.
8. No event, WebSocket frame, or error card contains brokered credentials.
9. The changeset UI is derived from trusted post-run export, not the agent transcript.
10. A run whose process/VM dies without a normal finish is repaired or marked interrupted, never left permanently “running.”

## Borrow now, earn later

| Borrow now | Why |
|---|---|
| Append-only typed event ledger | One replayable authority for UI, audit, and debugging |
| Stable ids, per-run sequence, timestamps, correlation | Makes pairing, reconnect, retries, and diagnosis deterministic |
| Host-side projections | Keeps aggregate UI cheap without making aggregates canonical |
| Baseline plus live deltas with gap recovery | Makes the console trustworthy across reconnects |
| Inline tool calls, retries, terminal errors | Explains what the agent did and why a run is waiting or failed |
| Separate transcript facts from trusted gates/changes | Preserves the software-factory trust boundary |
| Optional disclosure cards | Keeps ordinary reading simple while retaining evidence |
| Explicit missing metrics | Avoids invented precision |

| Defer | Reason |
|---|---|
| Cordis/pluginized UI and typed slot ecosystem | Daemar has no third-party UI composition pressure yet |
| Three-column responsive inspector | A disclosure/modal is enough for the first run viewer |
| Trajectory timeline, zoom, interval selection, virtualization | Preserve timestamps now; build this after real long-run navigation pain |
| Full raw chunk persistence | Decide after measuring log volume and debugging value |
| Context-pressure estimator and breakdown | Useful for interactive context management, not initial factory truth |
| Currency cost UI | DeepSeek does not implement it here; Daemar first needs price provenance |
| OpenTelemetry/OTLP export | Local observability comes first; export introduces redaction and retention risk |
| Recursive subagent catalog and live-only navigation | Wait for Daemar's run/attempt/delegation model |
| Queue steering, forking, goals, approvals, plan review | Interactive agent-product controls exceed the first operator-console scope |
| Themes, localization, HMR, settings UI | Product polish after the observability contract proves useful |

## Decision

Use DeepSeek Harness as evidence for the **event-ledger plus projection architecture** and for the interaction design of a readable live transcript. Do not use it as a template for Daemar's full domain hierarchy or frontend framework.

Daemar's first implementation seam should be substrate-neutral and UI-neutral:

```text
sandbox / agent / gate producers
              |
              v
      append typed RunEvent
              |
      +-------+---------+
      |                 |
      v                 v
 durable run log   replayable projections
      |                 |
      +-------+---------+
              v
       HTTP + live stream
              v
       local operator UI
```

If this seam is correct, replacing Apple Container with Firecracker, Hyper-V, or another qualified substrate changes producer metadata and substrate-specific lifecycle events, not the operator's meaning of a run, attempt, command, error, artifact, or cancellation.
