# Super Simple Software Factory observability and frontend, current as of 2026-08-26

## Executive decision

The best part of Super Simple Software Factory's (SSSF's) observability is not
its visual styling. It is the deliberately small semantic spine underneath the
UI:

```text
run -> ordered phase -> append-only event
                    \-> typed output
                    \-> gate evidence
```

That structure gives an operator three useful answers without reading an agent
transcript as a novel:

1. **What is happening now?** A run card and a lane timeline identify the active
   phase, owner, elapsed time, and recent tool activity.
2. **Where did it fail?** Phase state, error text, failed tool calls, invalid
   outputs, and per-item gate evidence are co-located in phase detail.
3. **What did the system actually observe?** Exact compiled prompts, normalized
   tool arguments/result snippets, typed output payloads, token/cost breakdowns,
   and the raw agent stream remain inspectable.

Daemar should copy that **progressive disclosure and phase-centered diagnosis**,
but should not copy SSSF's schema wholesale. Daemar's first slice should be a
substrate-neutral `Run -> Step -> Event` trace with durable terminal outcome,
deadline/cleanup state, live logs/tool activity, and the exact result delta as a
first-class result. SSSF does not render an artifact browser or Git diff even
though artifact paths and changed files may occur inside output JSON. That is a
material omission for Daemar, whose result boundary is the product.

For the first slice, use SQLite in WAL mode and cursor polling, plus two screens:
a run list and a run detail with an ordered step rail and an event/log panel.
Defer agent-roster lanes, prompt rendering, gate/envelope specializations,
provider icons, context-window occupancy, and itemized model billing until
Daemar owns those concepts in its runtime rather than merely having a UI for
them.

## Scope, method, and pinned sources

Only first-party repository source, committed documentation, and committed
visual assets were used. No third-party descriptions were used.

| Artifact | Revision inspected | Commit date | Role |
|---|---|---:|---|
| [`disler/super-simple-software-factory`](https://github.com/disler/super-simple-software-factory) `main` | [`de31374882e7a4e3e5b7bb9bd09e69dc2f779356`](https://github.com/disler/super-simple-software-factory/tree/de31374882e7a4e3e5b7bb9bd09e69dc2f779356) | 2026-08-02 | Installable skill and canonical visualizer source |
| same repository, `example` branch | [`b2dcb8e436db9b10f7580d7568b3e251609eb36b`](https://github.com/disler/super-simple-software-factory/tree/b2dcb8e436db9b10f7580d7568b3e251609eb36b) | 2026-08-04 | Stamped factory and demo integration |

At these revisions, the visualizer source and observability reference on
`example` are byte-identical to `main`; the example branch integrates the same
UI rather than demonstrating a second frontend. Runtime trace databases are
gitignored, so the repository does not contain a committed real trace that can
be replayed as independent evidence. The committed SVGs are design
illustrations, not screenshots of a running UI.[^readme-example]

“Implemented” below means there is an execution path in committed source that
writes the data and a UI path that reads or displays it. “Documented only” means
the prose or type system describes it but the inspected producer/consumer path
does not complete it.

## 1. The implemented architecture

SSSF's execution control plane is Python. A run enters ordered phases through a
single context manager; phase kinds are `engineer`, `code`, or `agent`. Entering
a phase records `running`; an exception records its error and `fail`; only clean
exit records `success`. The run separately resolves its terminal acceptance
state, because a deterministic test phase can execute correctly while reporting
that the suite failed.[^runner]

The tracer writes each normalized event immediately to both an `events.jsonl`
file and SQLite. SQLite is configured with WAL, `synchronous=NORMAL`, and a
five-second busy timeout, allowing the visualizer to read while the workflow
writes.[^tracer-schema] The visualizer is a Vue 3 single-page app built with
Vite, backed by a small Bun HTTP server using `bun:sqlite`; Lucide supplies UI
icons, TypeScript supplies the shared API model, and Bun runs both the server
and package scripts.[^visualizer-package] [^server]

There is no WebSocket, server-sent event stream, or telemetry collector. The
browser polls the JSON API every 500 ms. Events are fetched in insertion order
using SQLite `rowid` as a monotonic cursor; bounded pages use `after`, `limit`,
and `has_more`. The same query supplies initial history and the live tail.[^db-events]
Although `observability.poll_ms` exists in the Python configuration model, the
Vue timers are hardcoded to 500 ms and the server does not expose the configured
value. Configurable cadence is therefore not implemented end to end.[^poll-config]

### Implemented hierarchy

The persisted and displayed hierarchy is:

```text
session / ADW run (`adw_id`)
  ordered phases (`seq`)
    phase-local events
    phase-local output-envelope attempts
    phase-local gate attempts and evidence
  unique named agent sessions (`adw_id`, agent)
```

The UI derives “lanes” rather than persisting them. It always creates an
engineer lane, creates one shared code lane when code phases exist, and creates
one lane for each distinct owner of an agent phase. Multiple phases owned by the
same named agent appear on that agent's lane. A completed `agent_sessions` row
supplies model, color, coding-agent session ID, and context occupancy; while an
agent is still running, the server synthesizes enough of that row from the
earlier `agent_start` event to label the lane immediately.[^types-agents]
[^db-agents] [^trace-lanes]

This is **not** a general task/project/run/attempt hierarchy. There is no
persisted project, ticket, task, workflow definition, artifact, log stream, or
retry entity. An ADW run may be joined later under the same `adw_id`, appending
new phases and reusing per-agent coding-session IDs, but it remains one session
row.[^session] [^tracer-seq]

## 2. Exact persisted data model

The tracer creates seven SQLite tables.[^tracer-schema]

| Table | Important fields | What it means in the UI |
|---|---|---|
| `sessions` | `adw_id`, ADW names, request, status, engineer, start/end, total tokens/cost, archived | One run card and the run summary strip |
| `phases` | ID, run ID, sequence, name, kind, owner, description, status, attempt/retry limits, error, start/end | Progress dots, lane blocks, phase header, failure location |
| `events` | ID, run/phase IDs, `parent_id`, type, name, JSON payload, tokens, start/end | Live activity, event list, tool ticks, console narrative |
| `envelopes` | run/phase, agent, output type, raw JSON, valid flag, attempt, time | Every valid or invalid final-response attempt |
| `gate_results` | run/phase, attempt, gate, verdict, violations, per-item checks, time | Expandable evidence of what validation inspected |
| `processes` | run, process kind/name, PID, recorded command, start/end | Headless process inspection and safe kill support; not exposed in the web UI |
| `agent_sessions` | run/agent, harness, model, color, session ID, context used/window, times | Stable agent lane identity and context bar |

The browser API mirrors these rows closely. Derived state is intentionally not
stored: phase duration, run progress, read/written token totals, and lane layout
are calculated from timestamps, phase rows, and `agent_end` payloads.[^shared-types]
[^db-usage]

### Exact event vocabulary

The type contract declares ten event types, all tied to a run and normally to a
phase.[^shared-events]

| Event | Producer behavior | Display behavior |
|---|---|---|
| `phase_start` | Enter a phase with kind, owner, description | Establishes running phase and timeline start |
| `phase_end` | Clean or failed phase exit with resolved status | Updates block and phase status |
| `agent_start` | Before an agent turn; model, thinking level, coding harness, purpose, tools, extensions, color, session ID | Immediately labels/configures a live lane and phase config panel |
| `agent_end` | After the phase's sends; accumulated usage/cost and latest context occupancy | Phase cost table, lane context, session read/written derivation |
| `tool_call` | Once per completed real tool call, with clipped args, success, clipped result, agent, actual start/end and duration | Tool tick in phase block; event row expands to args/result |
| `handoff` | After valid output persistence; summary and artifact path list | Appears as a violet event payload; no artifact browser |
| `gate_pass` / `gate_fail` | One per gate per correction round; attempt, checks, violations | Separate gate section with expandable item-level evidence, plus event row |
| `log` | Explicit phase logging and every narrated console line | Chronological event row and raw JSON payload |
| `error` | Phase exception, permission breach, or failed final acceptance | Red event plus phase/run failure state |

Tool calls are normalized rather than copying every event from the Pi agent.
The adapter observes Pi's announcement/start/end messages and emits only when a
tool returns, preserving call span, arguments, result snippet, success, and
duration. Full Pi JSONL is still appended to `raw_output.jsonl`, but assistant
messages and reasoning are **not** normalized into UI events.[^pi-tool-tracker]
[^pi-run] [^event-forwarder]

There is no distinct retry event. Invalid JSON attempts are persisted as
invalid envelope rows; gate retries create new gate/envelope attempts; the
human-readable retry line is a generic `log` event with warning level. The same
underlying agent session is resumed for corrections.[^agent-retries] [^console]

## 3. Persistence and live update

### Durable stores actually written

SSSF writes several local, gitignored records under its configured data
directory:

- SQLite `sssf.db`, the queryable mirror the UI uses;
- session-level `events.jsonl`, containing normalized tracer events;
- per-agent `raw_output.jsonl`, containing the full Pi JSONL stream;
- per-agent `prompts/system.md` and `prompts/user.md`;
- per-agent `envelope.json`, containing the last persisted valid typed response;
- session-level `agent_map.json`, which lets later ADWs resume named agents;
- `context_handoff/`, containing reference artifacts passed between phases.
  [^session-files] [^agent-files]

SQLite writes are autocommit and happen per event or lifecycle update. The raw
agent stream is flushed line by line before forwarding parsed records. This is
why a tool call can become visible before its agent phase ends.[^tracer-event]
[^pi-run]

The observability reference calls files the raw record, SQLite the queryable
mirror, and says a lost database can be rebuilt. **The inspected source contains
no rebuild/import command.** Moreover, `envelope.json` and prompt files are
named per agent rather than per phase and can be overwritten when an agent is
used again, while the SQLite tables retain multiple attempts. Treat rebuildable
SQLite as a design claim, not a shipped recovery feature.[^observability-reference]
[^agent-files]

### Browser update behavior

The list view refetches up to 200 unarchived sessions every 500 ms. Every live
run card separately tails that run's events at the same cadence, stopping after
the run leaves `running` and performing one final drain. The trace screen polls
run/phase/agent detail and new events every 500 ms; it refetches envelopes and
gates only when a boundary-like event arrives. API failures produce a visible
“api unreachable — retrying” banner and polling continues.[^sessions-list]
[^session-card-poll] [^trace-poll]

This is admirably simple for a local single-user tool, but card-local polling
creates one event request per live card in addition to the session-list poll.
That is acceptable for an early local product; it is not a scale architecture.

## 4. Screens and components that are implemented

### Screen 1: run review list

The root hash route renders a responsive grid of fixed-size run cards. Each
card shows:

- run ID, joined ADW names, and truncated engineer request;
- a miniature time axis with one row per agent owner and colored dots for
  events;
- a pulse on the most recent event while the run is live;
- session status, one status dot per phase, and start date;
- total cost, elapsed runtime, and billed tokens;
- one operator action: archive the run out of the review list.
  [^session-card-ui]

This is a strong first-glance design because it combines **queue/review state,
activity, progress, and spend** without requiring an operator to open each run.
It does not provide filtering, searching, sorting controls, grouping by task,
pagination/virtualization, bulk action, or an archived-runs screen.

### Screen 2: run trace waterfall

Selecting a card opens a run summary strip and a swim-lane waterfall. The strip
shows request, status, start time, total cost, runtime, billed tokens, raw tokens
read for the first time, and tokens written. The waterfall has an engineer lane,
optional deterministic-code lane, and one lane per named agent. Timestamped
phase blocks show status, name, description, elapsed time, and internal tool-call
tick marks; failed tool calls have a distinct red tick. Queued phases have a
dashed visual treatment. Agent lanes add model identity and a context-window
occupancy bar once known.[^trace-ui]

The timeline is honest about observed time but makes very short phases readable
by applying a minimum block width and shifting later blocks to avoid overlap.
It therefore communicates sequence and rough duration, not pixel-exact temporal
proportion for near-zero phases.[^trace-layout]

### Screen 3: phase drill-down

Selecting a phase updates the hash route and opens a two-column detail panel.
Its header contains phase name, status, runtime, owner, kind, attempt/retry
numbers, and any phase error. Collapsible sections on the left expose, when
applicable:

- engineer request;
- exact agent configuration (coding agent, model, thinking, tools, extensions,
  purpose, coding-session ID);
- phase description;
- exact compiled system and user prompts, rendered as Markdown or raw text;
- gates with attempt number, per-item pass/fail evidence, command-output notes,
  and violations;
- phase-level token and cost breakdown for input, output, thinking share, cache
  reads, cache writes, and total;
- typed output attempts, their validity, agent, attempt number, and full JSON.

The right column is the phase event stream. Each row shows clock time, event
type, a concise label, duration when present, and tokens when present. Expanding
a normalized tool call reveals exact recorded arguments and the clipped result;
other event types reveal their raw JSON payload.[^phase-detail]

This is SSSF's highest-value frontend idea: **phase selection is the join key
for intent, configuration, execution, validation, output, cost, and failure**.
It makes diagnosis local instead of scattering it among transcript, terminal,
metrics, and artifact views.

## 5. Coverage by product concern

| Concern | Current SSSF implementation | First-slice value for Daemar |
|---|---|---|
| Run status | `running/success/fail`, timestamps, visible everywhere | **Keep**, but use Daemar terminal outcomes such as succeeded, failed, timed out, cancelled, cleanup failed |
| Progress | Ordered phase dots and waterfall; no percent complete | **Keep a simpler ordered step rail**; do not invent percent complete |
| Live activity | 500 ms cursor polling; tool ticks and events | **Keep**; a boring cursor is enough locally |
| Logs | Console narration as `log` events; arbitrary payload expansion | **Keep**, with stdout/stderr chunks explicitly typed rather than only generic JSON |
| Tool calls | Normalized call args, result snippets, success, duration | **Keep if the harness emits them**; support generic process/log events when it does not |
| Transcript | Full raw Pi JSONL only on disk; no transcript/messages UI | **Defer transcript UI**; preserve raw diagnostic stream or opaque attachment first |
| Prompts | Exact compiled system/user files rendered in phase detail | **Defer** until Daemar owns agent invocation, not just sandbox execution |
| Retries | Invalid envelopes + gate attempt rows + warning log; same session resumed | **Defer agent correction UI**; do record infrastructure retry attempts explicitly if Daemar performs them |
| Errors | Phase error, error event, failed tool styling, gate violations, API banner | **Keep and strengthen** with failure stage and cleanup outcome |
| Validation | Gate verdict plus evidence for every inspected item | **Later**, unless the first slice already runs deterministic acceptance checks |
| Artifacts | Paths and claims inside handoff/output JSON; no browse/download/open UI | **Make first-class now** for exact result delta and retained logs |
| Diff | May exist as a generated handoff file or output field; no diff API/view | **Make first-class now** because promotion depends on it |
| Cost/tokens | Run totals, phase component table, read/write split, context occupancy | **Defer itemization**; retain optional aggregate fields in event payloads |
| Operator actions | Web: archive only. CLI/example: inspect sessions/phases/events/processes and kill a stuck run | **Keep cancel/terminate and evidence download; defer archive** until review backlog exists |

## 6. Implemented behavior versus aspirational or incomplete claims

### Implemented

- Immediate normalized tracing to JSONL and SQLite, with WAL-backed concurrent
  reads.[^tracer-event]
- Run cards, run waterfall, phase drill-down, hash-addressable run/phase routes,
  and 500 ms polling.[^router] [^sessions-list] [^trace-ui]
- Exact typed envelope attempts and per-item gate evidence in phase detail.[^phase-detail]
- Normalized completed tool calls and preserved full raw Pi JSONL.[^pi-tool-tracker]
- Model usage/cost accumulation across retry sends and last-valid-turn context
  occupancy.[^agent-end]
- A deliberate archive write from the otherwise read-oriented visualizer.
  [^archive-api]

### Documented or modeled, but not completed end to end

1. **Queued phases.** Types and UI support `queued`, and the README art shows a
   future phase. The current runner creates a phase only when its context block
   is entered and immediately writes it as `running`; there is no manifest or
   predeclaration producer in the inspected templates. Queued progress is
   therefore a modeled UI capability, not current runtime behavior.[^phase-types]
   [^runner]

2. **Nested spans through `parent_id`.** The schema and docs say `parent_id`
   nests an agent span and its tool children. The current event forwarder never
   sets `parent_id`, no explicit agent-span event ID is retained for children,
   and the Vue code does not consume `parent_id`. Tool calls are grouped by
   `phase_id`, which still makes phase drill-down work, but arbitrary span
   nesting is not implemented.[^shared-events] [^event-forwarder]

3. **Read-only UI.** README/package prose calls the visualizer read-only, but the
   server intentionally opens a writer for archive/restore. This is a sensible
   exception, not a security problem; it is simply not literally read-only.
   [^server] [^db-archive]

4. **Restoring archived runs.** The API accepts `archived=false`, but the only
   visible archive action removes a card and the list query excludes archived
   sessions. No archived list or restore control is implemented.[^archive-client]
   [^db-sessions]

5. **Replay/rebuild from raw files.** The docs claim SQLite can be rebuilt, but
   no importer/rebuild path ships, and some per-agent files are last-value
   snapshots rather than append-only histories.[^observability-reference]

6. **Artifacts and diffs as observable objects.** SSSF agents can declare
   artifact paths and changed files, and its deterministic change capture can
   write a diff into `context_handoff/`. The visualizer has no artifact/diff
   table, endpoint, file viewer, or download action. Operators see these only
   as strings or JSON when a handoff/output happens.[^envelope-types]
   [^phase-detail]

7. **Operator lifecycle in the web UI.** The `processes` table exists for hung
   work, and the example `justfile` can list and safely kill processes after
   checking their commands. The web API does not expose processes or a cancel
   endpoint. Restart, retry, approve, reject, promote, and delete are also absent.
   [^processes] [^server-routes] [^example-justfile]

8. **Frontend verification.** No visualizer unit, component, end-to-end, or
   screenshot test is committed at the pinned revision. The package gate is
   typecheck/build plus Oxlint. Treat the implementation as real and inspectable,
   but not independently regression-qualified.[^visualizer-package]

9. **A stale skill instruction.** The shipped `SKILL.md` still says the
   visualizer “ships in a later pass,” while the current repository contains and
   documents the full app. That sentence is stale; it should not be treated as a
   current product boundary.[^stale-skill]

## 7. Recommended Daemar first slice

### Product question the slice should answer

> A hostile one-shot run is in progress or has ended. Can the operator tell what
> Daemar is doing, why it stopped, what evidence survived, and exactly what would
> be promoted—without knowing whether the isolation substrate was Apple
> Containers, Firecracker, Hyper-V, or something else?

If the UI answers that, substrate replacement remains an adapter concern.

### Minimum durable model

Do not start with SSSF's agent-specific seven-table schema. Start with four
concepts and allow payload evolution:

```text
Run
  id, request/label, status, started_at, ended_at,
  deadline_at, terminal_reason,
  cleanup_status, substrate_kind, substrate_instance_id

Step
  id, run_id, seq, kind, label, status,
  started_at, ended_at, error

Event
  row_id, event_id, run_id, step_id?, parent_id?,
  kind, observed_at, started_at?, ended_at?, payload_json

Result
  run_id, outcome, delta_manifest, diff_artifact,
  stdout_artifact?, stderr_artifact?, evidence_artifacts,
  promotion_status
```

Use a closed event vocabulary for the first implementation, for example:

- `run_started`, `step_started`, `step_finished`, `run_finished`;
- `status`, `stdout`, `stderr`, `tool_call` (optional), `warning`, `error`;
- `deadline_reached`, `termination_requested`, `cleanup_started`,
  `cleanup_finished`;
- `result_captured`, `delta_validated`, `promotion_started`,
  `promotion_finished`.

The lifecycle events Daemar adds are more important than SSSF's agent billing
events. Kernel isolation is only trustworthy operationally when timeout,
termination, capture, and irreversible cleanup are visible.

### Minimum frontend

#### Run list

Each card/row should show:

- label/request and short run ID;
- state chip and current step;
- elapsed time and deadline countdown/overrun;
- cleanup warning when terminal compute state and cleanup state disagree;
- result summary: changed files, insertions/deletions, artifacts;
- last meaningful event, not a decorative wall of dots.

Add filtering only for `active`, `needs attention`, and `finished` if the list
becomes noisy. Archiving can wait.

#### Run detail

Use a compact ordered step rail rather than a multi-agent waterfall until
Daemar truly runs multiple owners concurrently. Selecting a step should join:

- intent/description;
- status, timing, and terminal reason;
- chronological stdout/stderr/status/tool events;
- error and retry evidence;
- resource snapshot if readily available.

Always keep the run's cleanup state visible outside the selected step.

#### Result panel

This is where Daemar should deliberately exceed SSSF in slice one:

- exact changed-path manifest;
- unified diff with binary/metadata changes represented honestly;
- retained output/log artifacts with size and digest;
- capture/validation outcome;
- promotion eligibility and, only when already safe, a promote action.

The result panel should consume Daemar's substrate-neutral export contract, not
scrape guest-specific filesystem paths.

### Transport and persistence

For a local first slice, copy SSSF's boring choices:

- SQLite beside Daemar's local state;
- WAL and a busy timeout;
- one small write per lifecycle event;
- monotonically increasing event cursor;
- HTTP polling at 500 ms while active, slower or stopped after terminal state;
- a bounded page size and one final drain when a run becomes terminal.

Keep the writer in the Daemar control plane and the frontend server read-only
until an operator action is explicitly introduced. Unlike SSSF, define whether
JSONL is a backup/replay log. If it is, ship and test the rebuild path; if it is
not, call it a diagnostic export rather than “the raw truth.”

### First-slice priority

| Priority | Component | Why |
|---|---|---|
| P0 | Durable run/step state machine | Makes lifecycle and failure location queryable |
| P0 | Append-only event cursor with stdout/stderr/error/lifecycle events | Supports live and historical views with one mechanism |
| P0 | Deadline, termination, and cleanup visibility | This is Daemar's isolation contract, not UI garnish |
| P0 | Exact result manifest and diff artifact | This is what the operator must trust before promotion |
| P0 | Run list + run detail + result panel | Smallest coherent operator workflow |
| P0 | Cancel/terminate action if safe lifecycle cancellation exists | Lets the operator act on a hung or unwanted run |
| P1 | Optional normalized tool-call events | Excellent diagnosis when the harness supplies them |
| P1 | Resource samples/peak summary | Useful for substrate qualification and capacity tuning |
| P1 | Evidence download/export | Makes failures portable and inspectable outside the UI |
| Defer | Multi-agent swim lanes | Premature until Daemar has real concurrent/distinct owners |
| Defer | Typed envelopes and gate-specific UI | Belongs to future workflow semantics, not substrate execution |
| Defer | Prompt/transcript rendering | Harness-specific and carries secret/redaction obligations |
| Defer | Token/cost/context dashboards | Provider-specific and not required to trust execution |
| Defer | Archive/review inbox | Valuable only after enough durable runs create triage pressure |
| Defer | WebSocket/SSE/telemetry backend | Cursor polling is simpler and adequate locally |

## 8. The reusable design lesson

SSSF's frontend works because its telemetry vocabulary matches decisions the
operator makes. It does not begin with generic traces and hope a dashboard will
make them meaningful. A phase has intent, owner, state, evidence, output, and
cost; selecting the phase brings those facts together.

Daemar should apply the same principle around its own deepest boundary:

```text
request -> isolated execution -> result capture -> validation -> promotion/retain
```

The virtualization substrate should appear as recorded provenance, not as the
navigation model. Apple Containers, Firecracker, or a future Windows adapter
should all emit the same lifecycle and result events. The frontend then remains
stable even while the kernel-isolation machinery changes underneath it.

## Primary-source references

[^readme-example]: SSSF README, skill/example distinction and committed visual assets: [`README.md`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/README.md#L7-L21).
[^runner]: Phase and run lifecycle implementation: [`runner.py`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/runner.py#L71-L141).
[^tracer-schema]: SQLite schema, WAL settings, and migrations: [`tracer.py`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/tracer.py#L17-L120).
[^visualizer-package]: Visualizer package, dependencies, and scripts: [`package.json`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/package.json).
[^server]: Bun API/static server and its one write exception: [`server/index.ts`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/server/index.ts#L1-L199).
[^db-events]: Cursor-paged event query: [`server/db.ts`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/server/db.ts#L362-L384).
[^poll-config]: Configured polling field versus hardcoded UI timer: [`data_types.py`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/data_types.py#L340-L349) and [`SessionTrace.vue`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/src/components/SessionTrace.vue#L82-L85).
[^types-agents]: Shared phase, event, and agent-session types: [`shared/types.ts`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/shared/types.ts#L61-L157).
[^db-agents]: Completed and in-flight agent-session merge: [`server/db.ts`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/server/db.ts#L228-L307).
[^trace-lanes]: Derived engineer, code, and agent lanes: [`SessionTrace.vue`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/src/components/SessionTrace.vue#L100-L213).
[^session]: Session creation/joining and process registration: [`session.py`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/session.py#L1-L50).
[^tracer-seq]: Joined-run phase sequencing: [`tracer.py`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/tracer.py#L205-L229).
[^shared-types]: Shared API/storage model and derived-state contract: [`shared/types.ts`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/shared/types.ts#L1-L157).
[^db-usage]: Derived read/written token calculation: [`server/db.ts`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/server/db.ts#L323-L360).
[^shared-events]: Event vocabulary and payload types: [`shared/types.ts`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/shared/types.ts#L18-L29) and [`shared/types.ts`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/shared/types.ts#L159-L224).
[^pi-tool-tracker]: Pi tool-call normalization: [`agent_pi.py`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/agent_pi.py#L120-L205).
[^pi-run]: Raw stream persistence and live callback path: [`agent_pi.py`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/agent_pi.py#L208-L285).
[^event-forwarder]: Tool call to tracer event adapter: [`agents.py`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/agents.py#L235-L250).
[^agent-retries]: JSON correction and gate correction loops: [`agents.py`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/agents.py#L139-L173) and [`agents.py`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/agents.py#L267-L300).
[^console]: Console-to-log-event mirroring, including retry narration: [`console.py`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/console.py#L39-L46) and [`console.py`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/console.py#L94-L123).
[^session-files]: Session directory, handoff directory, and agent map persistence: [`runner.py`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/runner.py#L42-L69).
[^agent-files]: Prompt, raw stream, envelope, and agent-map writes: [`agents.py`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/agents.py#L80-L137) and [`agents.py`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/agents.py#L290-L300).
[^tracer-event]: Immediate JSONL and SQLite event writes: [`tracer.py`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/tracer.py#L122-L136).
[^observability-reference]: First-party observability design/reference: [`references/observability.md`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/references/observability.md#L1-L46).
[^sessions-list]: Run-list polling and error state: [`SessionsList.vue`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/src/components/SessionsList.vue#L1-L69).
[^session-card-poll]: Live per-card event polling: [`SessionCard.vue`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/src/components/SessionCard.vue#L29-L77).
[^trace-poll]: Run trace polling and side-table refresh: [`SessionTrace.vue`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/src/components/SessionTrace.vue#L24-L90).
[^session-card-ui]: Run card presentation and archive action: [`SessionCard.vue`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/src/components/SessionCard.vue#L189-L250).
[^trace-ui]: Run strip and waterfall presentation: [`SessionTrace.vue`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/src/components/SessionTrace.vue#L429-L558).
[^trace-layout]: Timeline layout and short-phase widening: [`SessionTrace.vue`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/src/components/SessionTrace.vue#L243-L338).
[^phase-detail]: Phase detail configuration, prompts, gates, cost, outputs, and event panels: [`PhaseDetail.vue`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/src/components/PhaseDetail.vue#L341-L673).
[^router]: Hash-addressable run and phase routes: [`router.ts`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/src/lib/router.ts).
[^agent-end]: Accumulated send usage and final agent trace: [`agents.py`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/agents.py#L194-L213).
[^archive-api]: Archive HTTP route: [`server/index.ts`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/server/index.ts#L126-L147).
[^phase-types]: Phase status type includes queued: [`data_types.py`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/data_types.py#L16-L67).
[^db-archive]: Lazy archive writer: [`server/db.ts`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/server/db.ts#L126-L150).
[^archive-client]: Archive/restore client function and only visible archive action: [`api.ts`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/src/lib/api.ts#L43-L52) and [`SessionCard.vue`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/src/components/SessionCard.vue#L15-L27).
[^db-sessions]: Archived-session exclusion in list query: [`server/db.ts`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/server/db.ts#L152-L203).
[^envelope-types]: Artifact and changed-file output fields: [`data_types.py`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/data_types.py#L70-L95).
[^processes]: Process table and lifecycle writes: [`tracer.py`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/tracer.py#L71-L89) and [`tracer.py`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/tracer.py#L173-L203).
[^server-routes]: Complete visualizer API route surface: [`server/index.ts`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/apps/visualizer/server/index.ts#L113-L199).
[^example-justfile]: Example-branch process inspection and safe kill recipes: [`justfile`](https://github.com/disler/super-simple-software-factory/blob/b2dcb8e436db9b10f7580d7568b3e251609eb36b/justfile#L104-L140).
[^stale-skill]: Stale later-pass statement: [`SKILL.md`](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/SKILL.md#L72-L76).
