# AGENTS.md — how to work on daemar

Read [`CONTEXT.md`](CONTEXT.md) first. It is the ubiquitous language and the
hard rules; nothing here overrides it. This file is the operational layer:
code discipline, testing, and how to run things without breaking the
engineer's day.

## Map

```
crates/ledger   domain: events, the slip fold, loading. Runtime-free.
apps/board      the controller's UI (axum). Thin: renders folds, owns no state.
fixtures/       hand-authored ledgers — the schema's test corpus. Tracked.
```

## Rust discipline (extends CONTEXT.md hard rule 7)

1. **Core crates are runtime-free.** No tokio, no async in `ledger` or
   `factory`. The runtime stops in `crates/walls` — the one library allowed
   to link it, so every executable injects the same wall — and in apps, at
   the edges. This is what keeps a future TUI or a new binary an afternoon
   instead of a port.
2. **No `_ =>` arms on this repo's own enums.** The exhaustive match IS the
   change-impact analysis; a wildcard arm silently opts out of it. Wildcards
   are for foreign or `#[non_exhaustive]` types only.
3. **Panic policy.** `expect()` only for genuine invariants, with the
   invariant as the message (`expect("Kind serializes")`). Never
   unwrap/expect/index on external data — files, network, args, clocks. That
   is an error enum's job.
4. **Dependency budget.** Every dependency is a decision; prefer std. No
   proc-macro deps without a stated reason — hand-roll `Display`/`Error`
   (~15 lines). No `anyhow`: it is `Box<dyn Error>` with better marketing,
   and `Box<dyn Error>` is banned.
5. **Clone deliberately.** Cloning to feed a projection is design. Cloning to
   appease the borrow checker is a smell — restructure instead.
6. **Derive what you use.** `Debug` always; everything else earns its place.
7. **`pub` is a contract.** Minimize surface. Domain logic lives in
   `crates/`; apps stay thin.
8. **`cargo fmt` defaults; `cargo clippy --all-targets` stays clean.** It is
   clean today. Keep it that way — a warning you tolerate is a warning you
   ship.

## Testing

- **Test at the wire.** Build events from JSONL lines through
  `Event::from_line`, not struct literals — every test then exercises the
  boundary parse as well as the logic.
- **Every derived state gets an explicit test** (cocked, never-cocked-when-
  closed, unknown-degrades-whole, bad-lines-counted). Derived-never-asserted
  is only trustworthy while it is tested.
- **Test names are sentences stating the invariant**:
  `closed_slips_are_never_cocked`, not `test_clearance_2`.
- **std-only scaffolding**: `env::temp_dir()` + `process::id()`, no tempfile
  dependency.

## Operating

- `daemar scout [--repo <path>] "<question>"` — read-only reconnaissance
  over a territory (default: cwd). Tools: read / list_files / search —
  confined to the territory, every call on the ledger with a content-hash
  pointer. No shell, no writes, no network.
- `daemar build [--repo <path>] "<request>"` — the builder mutates a
  PINNED WORKTREE (never the live checkout) behind the wall — write seats
  wall unconditionally; prepare the image first (`just wall` builds it and
  proves the wall). A code-owned diff (gate:diff) cocks the slip at build->apply;
  APPLY is phase 3. Tools: read/list/search plus hash-guarded `edit`
  (read-first; `old` exact-match, single occurrence) and create-new-only
  `write`, and ReadWrite-only `delete` (read-first, hash-guarded, file-only).
  The guard refuses unread files and files changed outside the tools; each
  successful edit or write advances its record to the post-image, while delete
  removes its record. Several edits — including several to the same file — may
  ride one response in request order. The diff gate captures deletions. Turn
  caps are per-role: Builder 48, all read seats 12; they bound pathology,
  never task scope.
- `daemar plan "<request>"` — plan phase, then the slip cocks at
  plan->respond and the process EXITS (exit-and-resume: boundary waits and
  crashes share one recovery). `daemar grant|refuse <slip-id>` — the
  controller answers the clearance. `daemar continue <slip-id>` — flies the
  next phase from the printout, context rebuilt purely from the ledger, in a
  fresh process, possibly on a different airframe.
- `daemar dispose <slip-id> "<reason>"` — the controller's closure of a
  flight that could not close itself (failed, crashed, hung). Refuses slips
  that are already closed.
- `just` lists the recipes. `just dev` — the board with auto-restart via
  watchexec, watching source only (NEVER `ledgers/` — flight writes must not
  restart the server). `just fly "<request>"` — run the loop. `just check` —
  what CI will enforce.
- `cargo test` — the suite. `cargo run -p board` — the board on
  `http://127.0.0.1:4700` (`DAEMAR_LEDGERS`, `PORT`, `DAEMAR_STALE_SECS`).
- **Rebuild before you run.** `cargo test` does not refresh
  `target/debug/<bin>` — run `cargo build -p board` before invoking the
  binary directly. A stale binary has burned us once already.
- **Kill only PIDs you spawned** — capture the pid at spawn (pidfile).
  NEVER `lsof -ti :PORT | xargs kill`: that matches established client
  connections and kills the engineer's browser.
- **Verify UI by looking at it.** Headless Chrome with a scratch
  `--user-data-dir` and `--timeout`, screenshot to a scratch dir, then read
  the image. Judge pixels, not your own HTML.
- **Leave fixtures as you found them.** If a test appends to `fixtures/`,
  `git checkout` the file afterwards.

## Git

- Origin allows squash merges only.
- Commit subjects state the change; bodies state the why. The why is the
  part the next reader cannot reconstruct.
