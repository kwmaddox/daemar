# Change Request

A **Change Request** is the human-approved input to a Workflow Run. It states
what to achieve and how a reviewer will judge the result. It does not control
how Daemar operates.

Pass a Change Request JSON file to Daemar with:

```console
daemar run path/to/change-request.json
```

## Authoring contract

A Change Request is one UTF-8 JSON object no larger than 16 KiB. It has four
required fields and may include the optional `$schema` editor hint. Unknown and
duplicate fields are rejected.

| Field | Type | Bounds | Purpose |
| --- | --- | --- | --- |
| `schema` | string | exactly `"change_request.v1"` | Identifies the contract version. |
| `id` | string | 1–64 characters; lowercase kebab-case using `a-z`, `0-9`, and single dashes | Correlates independent attempts for the same request. |
| `objective` | string | non-blank; at most 4,096 characters | States what the Workflow Run should achieve. |
| `acceptance_criteria` | array of strings | 1–20 non-blank items; each at most 1,024 characters | States how a human reviewer judges the result. |
| `$schema` | any JSON value | optional and ignored by Daemar | Lets an editor associate the document with the JSON Schema; use a path or URL string for editor support. |

Only `objective` and `acceptance_criteria` enter the Context Surface. The
`schema` and `id` fields remain trusted bookkeeping.

The generated [JSON Schema](change-request.schema.json) provides live editor
feedback. A complete, parse-tested [example](examples/change-request.json) is
also available. A Change Request may live anywhere; adjust the `$schema` path
relative to its location, or configure the association in your editor.

## Preflight diagnostics

Preflight is the first operation performed by `daemar run`. It checks encoding,
document size, JSON shape, fields, field types, version, slug grammar, blank
values, and bounds. It reports every applicable problem in deterministic order.
Each diagnostic includes a stable rule code and JSON Pointer:

```console
$ daemar run sloppy.json
error: invalid Change Request - 3 problem(s) in sloppy.json

  [unknown_field] unknown field `priority`; change_request.v1 accepts: schema, id, objective, acceptance_criteria, $schema (`$schema` is optional metadata) (at /priority)
  [bad_slug] `id` must be lowercase kebab-case (a-z, 0-9, single dashes) (at /id)
  [blank_field] `objective` must not be blank (at /objective)

no Workflow Run created
```

An invalid Change Request exits with status 1. CLI usage errors exit with
status 2. A request that fails Preflight creates no Workflow Run, Run Record,
repository mutation, model call, or provider cost.

Preflight validates structure and bounds, not whether an objective or criterion
is useful. That judgment remains with the human author and reviewer.

## Operational policy is not requester-controlled

A Change Request cannot select a base branch, model, Model Tools, Validation
Operations, Sandboxed Execution capabilities, or resource bounds. Those
controls belong to the repository and compiled Workflow Definition so that
changing them requires a reviewed code change.

## Keeping the schema synchronized

The checked-in schema is generated from the canonical Rust authoring type and
the same policy constants used by Preflight. Regenerate it with the pinned
toolchain:

```console
cargo +1.97.1-aarch64-apple-darwin run --offline --locked --example generate_change_request_schema > docs/change-request.schema.json
```

The `change_request_authoring` test regenerates the schema in memory and fails
if the checked-in file differs. The same test parses the complete example
through the production Preflight interface.
