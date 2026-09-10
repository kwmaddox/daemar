# PER-84 Run2 Task4 handoff

Task4 CLI integration is implemented in `crates/daemar-card/src/main.rs`.
The `serve` command is discoverable, parses a private clap `u16` port (default
7331), resolves configuration, opens `Reader::open_existing`, binds IPv4
loopback once, publishes one startup JSON line, and serves with the same
listener. Existing commands now use the library `ConfigSource` Display.
`Control::Served` prevents the generic JSON success printer from emitting a
second line.

## Evidence

- Binary tests: 6 passed, exit 0: `run2-task4-binary.txt`.
- Strict Clippy: exit 0: `run2-task4-clippy.txt`; `git diff --check` exit 0:
  `run2-task4-diff-check.txt`.
- Library tests: sandbox execution was blocked by loopback permission;
  permission-escalated rerun passed all 60 tests:
  `run2-task4-lib-escalated.txt` (the blocked attempt is preserved in
  `run2-task4-lib.txt`).
- Browser TypeScript: exit 0: `run2-task4-typescript.txt`.
- Full escalated `just check`: repository gates passed; behavior reached 119
  scenarios, 104 passed and 15 failed, exit 101:
  `run2-task4-just-check.txt`.
- Full escalated `just browser`: 11 tests, 9 passed and 2 failed, exit 1:
  `run2-task4-browser.txt`.

The remaining failures are predecessor-owned console rendering behavior, not
CLI startup behavior: stream rows omit expected sequence/summary/link content,
and queue-read failure pages still render the queue container. The CLI startup,
configuration precedence, bind failure, missing/uninitialized/behind schema,
and startup output scenarios pass. Repairing those failures requires an
explicit ownership transfer for predecessor production files; this executor
did not self-grant that scope.

## C1-C16 checklist

C1 native errors checked/no new finding; C2 hand-written errors and existing
dependency policy checked/no new finding; C3 typed failure payloads preserved;
C4 no production panic/unwrap added; C5 reasoned suppression retained; C6
clap parse-boundary dispatch suppression reasoned; C7 `Port` used at the CLI
boundary; C8 existing contextual errors preserved; C9 no public API added; C10
Reader and console ownership reused; C11 no semantic clone introduced; C12
Arc Reader boundary unchanged; C13 startup publication uses approved buffered
formatter; C14 default port and sole ConfigSource Display preserved; C15 no
semantic boolean API; C16 CLI orchestration remains separate from Reader,
console routing, rendering, and serving.

No completion Card stage-event was appended because the required all-green
barrier is not met. Source digest:
`crates/daemar-card/src/main.rs`
`8e2472a7533ecd809064a19cf8eaee0a7ad5a9d96ecab20f8e7b3ab65f4128e1`.
