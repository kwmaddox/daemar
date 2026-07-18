# PROTOTYPE (PER-9) — CLI input and documentation contract

Throwaway artifacts answering: *what CLI invocation and documentation
artifacts make the Change Request JSON contract understandable, validatable,
and resistant to drift from its canonical Rust types?*

## Run it

```console
$ python3 daemar.py run examples/add-run-inspection.json
$ python3 daemar.py run examples/invalid-unknown-key.json
$ python3 daemar.py run examples/invalid-bounds.json
```

## Artifacts

| Artifact | Answer for |
| --- | --- |
| `daemar.py` | CLI surface: single `run` subcommand, positional path, Preflight-first, all-problems error report with typed rule codes previewing the `PreflightError` enum. |
| `change-request.md` | The human doc: contract table, complete example, editor-based checking, error reading, philosophy. Would ship at `docs/change-request.md`. |
| `change-request.schema.json` | Hand-draft of the schema the real bootstrap GENERATES from the canonical Rust types (schemars), checked in with a regeneration test. |
| `examples/` | One valid example (parse-tested in production) and two invalid ones demonstrating the error report. |

## Decisions (from review)

1. **CLI shape**: `daemar run <path>` — one subcommand, positional path. No `validate` command: Preflight runs first under `run` and invalid requests create nothing, so attempting a run is always safe; a valid request is what a run is for. Write-time feedback lives in the editor via the schema instead.
2. **Error report**: all problems in one pass, each tagged with a stable rule code mapping 1:1 to future `PreflightError` variants.
3. **`$schema`**: allowed as an optional fifth key, value ignored by Daemar — editors get live hints wherever the request file lives. Amends PER-11's strict-parse rule with this single exception.
4. **Drift resistance**: schemars-generated schema checked in with a regeneration test (types → schema); example request parsed by a test (example → types); doc prose links both and keeps normative content in the test-locked artifacts.
