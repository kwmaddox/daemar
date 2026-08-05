# daemar

A software factory. Deterministic Rust owns the workflow; models work inside
bounded phases; one artifact — the **slip** — carries each task through its
whole flight. Read [`CONTEXT.md`](CONTEXT.md) first: it is the ubiquitous
language, and every word in this repo defers to it.

## The board (phase 1)

```bash
cargo run -p board            # http://127.0.0.1:4700, renders ./fixtures
DAEMAR_LEDGERS=path/to/ledgers PORT=4700 cargo run -p board
```

Slip faces in bays — cocked strips first (and literally tilted), then in
flight, then closed. Click a strip for progressive disclosure: face → phases,
sections, clearances → the raw ledger. The page polls every 2 seconds; the
board holds no state — ledger files are the only truth.

## Layout

```
CONTEXT.md        the ubiquitous language — read this first
crates/ledger     event types (ledger.v1), the slip fold, loading
crates/factory    the factory itself: roster, engine, workflows, pens
apps/daemar       the CLI — a thin skin over crates/factory
apps/board        the controller's instrument (axum, server-rendered)
fixtures/         hand-authored ledgers: accepted, cocked, in-flight, rejected
```

## The runner (phase 2)

```bash
daemar "<request>"                       # cargo run -p daemar -- "<request>"
git diff | daemar -                      # request from stdin
daemar scout --repo <path> "<question>"  # read-only recon over any repo
daemar plan --repo <path> "<request>"    # grounded plan, cock at plan->respond
```

Workflow stages name **roles** (scout, planner, responder); the **roster**
(`crates/factory/src/roster.rs`) binds each role to an agent — persona,
airframe, tool access. The planner is grounded: it reads the territory with the scout's
read-only tools before planning, and every read lands on the ledger. Config
via env: `OPENAI_API_KEY`, `DAEMAR_MODEL` (per-role overrides
`DAEMAR_SCOUT_MODEL` / `DAEMAR_PLAN_MODEL` / `DAEMAR_RESPOND_MODEL`),
`DAEMAR_BASE_URL` (OpenAI-compatible), `DAEMAR_LEDGERS` (default
`ledgers/`). `DAEMAR_HOME` roots the relative defaults (ledgers, airframes,
secrets) so daemar works from anywhere; on a missing env var the tower
decrypts `$DAEMAR_HOME/secrets/daemar.enc.env` itself, in-process — the key
never rides in any exec environment. Failure is witnessed: a failed flight
stays open until the controller disposes it; a crash leaves no terminator
and the board derives interrupted.

## The tower over MCP

```bash
daemar mcp        # stdio MCP server: one process per client
```

Any MCP client becomes a factory interface. Tools: `scout`, `plan`,
`prompt`, `continue`, `slip` — flights and reads only; the controller's pens
(grant/refuse/dispose) are deliberately not exposed, so a delegated agent
may request clearances but never sign them. Slips opened over MCP are
signed `engineer: "mcp:<client>"` from the handshake's `clientInfo.name`.
Client config (`cargo install --path apps/daemar` puts `daemar` on PATH):

```json
{
  "mcpServers": {
    "daemar": {
      "command": "daemar",
      "args": ["mcp"],
      "env": { "DAEMAR_HOME": "/path/to/daemar" }
    }
  }
}
```

## Status

Phase 1 (the board) — done.
Phase 2 (the first loop) — done; maiden flight flown 2026-08-04.
Everything after — earned, one real run at a time.

The previous life of this repo is preserved at `archive/wayfinder-v0` (branch
and `wayfinder-v0` tag) — quarry, don't inherit.
