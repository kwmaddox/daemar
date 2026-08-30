# Card console UI prototype

> **THROWAWAY PROTOTYPE:** this is not production architecture or an
> implementation starting point.

Question: which information hierarchy makes durable Card truth and detailed
Execution Trace observation easiest to distinguish and use?

Three variants of the new Card console are available on one route, switchable
with `?variant=` and the floating bottom control:

- `A` — Queue first, with Stage selection and a detailed Trace-event inspector
- `B` — Chronology first
- `C` — Evidence first

Run it with:

```sh
just prototype-card-console
```

Then open <http://127.0.0.1:4187/prototype/card-console>.

The fixtures exercise a running Stage, a failed call followed by a successful
retry, multiple attempts, completed changes and checks, and an unavailable
Execution Trace. The HTMX interactions are deliberately fake: Stop changes its
visible state, and a tool call can request server-rendered detail. There is no
database and nothing is persisted.

In Queue First, select Frame, Plan, or Build to scope the activity stream. Then
select an activity event to inspect its invocation, retained output, timing and
usage, retry/correlation lineage, and raw normalized event. Card facts remain
visible above the inspector so detailed observation cannot masquerade as the
workflow outcome.
