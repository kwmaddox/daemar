# PROTOTYPE (PER-9) — draft of `docs/change-request.md`. Do not ship as-is.

This is the documentation half of the PER-9 prototype. React to structure,
tone, and completeness — not to the prototype directory it lives in.

---

# Change Request

A **Change Request** is the human-approved input to a Workflow Run. It says
*what* to achieve and *how a reviewer will judge it* — never *how* Daemar
should operate. Operational policy (base branch, run bounds, model, tools,
validation suite) belongs to the repository and the compiled Workflow
Definition, not to the requester.

## The contract

A Change Request is a single JSON object, at most 16 KB, with exactly four
fields. Unknown and duplicate fields are rejected — with one exception: an
optional `$schema` key naming this document's JSON Schema is allowed so
editors can offer live hints; Daemar ignores its value.

| Field | Type | Bounds | Purpose |
| --- | --- | --- | --- |
| `schema` | string | must equal `"change_request.v1"` | Contract version. No negotiation. |
| `id` | string | 1–64 chars, lowercase kebab-case (`a-z`, `0-9`, single dashes) | Names this request. Recorded in the Run Record, commit trailers, and PR body. |
| `objective` | string | non-blank, ≤ 4,096 chars | What the Workflow Run should achieve. Shown to the model. |
| `acceptance_criteria` | string[] | 1–20 items, each non-blank, ≤ 1,024 chars | How you will judge the result. Shown to the model; never machine-checked by Daemar. |

Fields shown to the model are deliberately small — they occupy the model's
attention, so keep them sharp.

## A complete example

```json
{
  "$schema": "../daemar/docs/change-request.schema.json",
  "schema": "change_request.v1",
  "id": "add-run-inspection",
  "objective": "Add a `daemar runs list` command that prints recent Workflow Runs with their terminal outcomes, reading the Run Records under .daemar/runs/ without modifying them.",
  "acceptance_criteria": [
    "Running `daemar runs list` prints one line per Run Record, newest first, showing the run ID and terminal outcome.",
    "Running the command in a repository with no runs prints a clear empty message and exits zero.",
    "The command performs no writes: .daemar/runs/ is byte-identical before and after."
  ]
}
```

A Change Request file can live anywhere; you pass its path to the CLI. The
`id` is yours to choose — reuse it when you revise and retry a request so
your runs correlate. The `$schema` line is optional; point it at any copy of
the schema (a relative path into your checkout, or a pinned raw URL) so your
editor can help. Daemar never reads it.

## Check as you write

Wire the JSON Schema into your editor (via the `$schema` key or your
editor's schema-association settings) and malformed requests are flagged
inline as you type.

Attempting a run is always safe for a malformed request:

```console
$ daemar run path/to/change-request.json
```

Preflight is the first thing `daemar run` does. When the request is invalid
the CLI lists every problem and no Workflow Run is created — no repository
changes, no model calls, no cost. A valid request is exactly what a run is
for, so there is no separate dry-run command.

## When validation fails

Preflight lists every problem it finds, each tagged with the rule it broke:

```console
$ daemar run sloppy.json
error: invalid Change Request - 3 problem(s) in sloppy.json

  [unknown_field] unknown field `priority`; change_request.v1 accepts exactly: schema, id, objective, acceptance_criteria (at /priority)
  [bad_slug] `id` must be lowercase kebab-case (a-z, 0-9, single dashes) (at /id)
  [blank_field] `objective` must not be blank (at /objective)

no Workflow Run created
```

## What a Change Request is not

- Not operational policy. You cannot select a base branch, raise iteration
  bounds, or pick a model. If a run needs different policy, that is a code
  change to the Workflow Definition — deliberate and reviewable.
- Not machine-judged. Daemar never decides whether your objective is *good*;
  Preflight checks shape only. Approval is yours.

## How this document stays honest

The canonical definition of this contract is the Rust type in Daemar's source.
The JSON Schema (`change-request.schema.json`) is **generated** from those
types and a test fails if the checked-in schema drifts from them. The example
above is parsed by a test against the same types. Prose here summarizes;
mechanics are test-locked.
