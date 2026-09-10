# PER-84: Add a local, read-only Card console

## Summary

Adds `card serve`: a loopback-only, queue-first console for reading the factory's Card record.

- Browse Cards, their ordered history, and individual entry provenance and producer payloads.
- Read an existing database through a schema-verified, read-only Reader, including live WAL updates. Serving never creates, migrates, or changes database permissions.
- Publish one startup JSON line using the bound listener's actual address and resolved configuration source; preserve existing CLI output behavior.
- Render server-side HTML with one local stylesheet and no JavaScript. Validate Host before method/routing, allow GET/HEAD only, escape producer text, and show whole-page errors without fabricated Card data.
- Share Store/Reader read queries and add typed console, schema, and failure contracts.

## Validation

Final command verification on the completed working-tree candidate:

- `just check` — passed: 64 library tests, 10 binary tests, 119 Cucumber scenarios / 627 steps.
- `just browser` — 11/11 passed.
- `cd browser && ./node_modules/.bin/tsc --noEmit` — passed.

Coverage includes startup failure/output sequencing, read-only access and schema refusal, live reads, request guards, payload absence, error-page escaping, and fail-closed checks for unsupported meta-refresh syntax. The latter is a conservative test oracle, not a complete browser refresh parser.

## Design and review record

Authoritative Card: `01a0693e-bc16-7272-9ce9-a20f3b07f875`.

- Typed skeleton approved at sequences 38/45/46; the implementation retains the settled public contract.
- Code-execution reviews and dedicated refutations, including C1–C16 conformance, are recorded on the Card. Final adjudication at sequences 97–98 closes all review findings and stands down the review lane.
- Final gate evidence: `docs/execution/per-84/review-resolution-3/completion.md` and its linked logs.
- Dependency-policy change: the operator approved BSD-3-Clause for the required dependency path at sequence 61.

## Factory-process evidence

Planning and implementation used separate agents, with Luna executors. Earlier manual orchestration interventions and missing historical test-first evidence remain recorded; successful delivery is not presented as an unaided validation of the intended factory workflow.
