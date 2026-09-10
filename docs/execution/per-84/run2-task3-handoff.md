# PER-84 Run2 Task3 handoff

Status: implementation and compile/gate checks complete, but Task3 is not
acceptance-complete. No commit was made. Task4 CLI integration remains deferred.

## Files owned and changed

- `crates/daemar-card/src/console.rs`: private Axum router, loopback Host and
  method guards, Askama page views, fresh Reader-backed queue/card/entry reads,
  stylesheet route, and a fallible Tokio accept loop mapping accept failures to
  `Error::Serve`.
- `crates/daemar-card/templates/queue.html` and `templates/error.html`: escaped
  whole-page templates.
- `crates/daemar-card/static/console.css`: single local stylesheet.
- `crates/daemar-card/Cargo.toml` and `Cargo.lock`: Axum 0.8.9, Askama 0.14.0,
  Hyper 1.11.1, and Hyper-util 0.1.20; Axum `query` feature enabled.

Predecessor Task1/Task2 files and protected behavior/browser tests were not
modified by this task.

## Actual command evidence

All commands ran from `/Users/kendall/code/github/daemar`.

- `cargo fmt --all`: exit 0; `run2-task3-fmt-final.txt`, SHA-256
  `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.
- `cargo test --locked -p daemar-card --lib`: exit 0, 51 passed / 0 failed;
  `run2-task3-lib-full-final.txt`, SHA-256
  `a864a0076f435f78ecf114e367db02d6f0f71a2a3c6335b9e70648735908bc33`.
- `cargo clippy --locked --all-targets --all-features -- -D warnings`: exit 0;
  `run2-task3-clippy-final.txt`, SHA-256
  `50b11965e4b2ecc95e0a13eece867f4d270e4efd830a038028a7373b354f89df`.
- `just check`: exit 101 only at the known pre-Task4 CLI boundary: 119
  scenarios, 57 passed / 62 failed; 413 steps, 351 passed / 62 failed. The
  failures report `card serve` as an unrecognized CLI subcommand. Full output:
  `run2-task3-just-check-final.txt`, SHA-256
  `aee0f20027bb3a8f5f612ada0eedb8db3b5c519dc3f2cd2b7e54b5b15dfca599`.
- `git diff --check`: exit 0.

The initial Task3 red-before-body snapshots and focused private tests required
by the dispatch were not added before implementation. Consequently this
handoff makes no test-first provenance claim for the nine named Task3 cases.
Those tests still need to be added/run, along with semantic review of the
private router and rendering behavior, before Task3 can be accepted.

## C1-C16 checklist

C1 native crate Error at public boundary; C2 manual existing Error and no
helper-error crate; C3 typed domain IDs/types in private views except opaque
producer text; C4 no production panic/unwrap paths in new implementation; C5
one reasoned ast-grep ignore for opaque producer identity; C6 enum dispatch for
EntryType/ProducerKind; C7 CardId/EntryId/closed types retained; C8 contextual
accept/bind errors; C9 only approved public `serve`; C10 QueueCard and Reader
reuse; C11 clones are for owned page/entry views; C12 Arc is used because Axum
clones read-only Reader state across request tasks; C13 response rendering is
buffered to guarantee whole-page failure mapping; C14 no new defaults/source
spellings; C15 no semantic bool API; C16 opening/querying, guarding/routing,
rendering and accept orchestration remain separate. No gate-reported C-ID
finding remains. Semantic C-ID review is still required at the designated
review boundary.

## Remaining acceptance work

Add genuine focused private tests (including the nine named red-first cases),
verify HEAD body suppression, inspect all HTTP/HTML security and error mappings,
and rerun the Task3 acceptance suite. Task4 must add the `card serve` CLI flow;
the current `just check` failures are therefore deferred successor failures,
not evidence that the library console is reachable from the binary yet.
