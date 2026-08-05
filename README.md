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
apps/board        the controller's instrument (axum, server-rendered)
fixtures/         hand-authored ledgers: accepted, cocked, in-flight, rejected
```

## The runner (phase 2)

```bash
daemar "<request>"          # cargo run -p daemar -- "<request>"
git diff | daemar -         # request from stdin
```

One slip, one phase, one model call, no tools, no checks — the board is the
check. Config via env (direnv decrypts `secrets/daemar.enc.env`):
`OPENAI_API_KEY`, `DAEMAR_MODEL`, `DAEMAR_BASE_URL` (OpenAI-compatible),
`DAEMAR_LEDGERS` (default `ledgers/`). Errors close the slip honestly
(rejected, reason on the ledger); a crash leaves no terminator and the board
derives interrupted.

## Status

Phase 1 (the board) — done.
Phase 2 (the first loop) — done; maiden flight flown 2026-08-04.
Everything after — earned, one real run at a time.

The previous life of this repo is preserved at `archive/wayfinder-v0` (branch
and `wayfinder-v0` tag) — quarry, don't inherit.
