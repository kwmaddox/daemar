# PER-84 run2 Task1 repair handoff

Status: complete. The bounded C13/C14 repair is implemented and verified;
Task2 may proceed. No public API, dependency, policy, accepted external test,
or later-task implementation was changed.

Changed files:

- `crates/daemar-card/src/console.rs`
  - C13 at lines 181–222: private `StartupWire` serialization writes fields
    directly through `serde_json::to_writer`; `collect_str` performs JSON
    string escaping without `Startup::url()`, `Value`, or owned string
    preparation. Existing field order and wire bytes remain unchanged.
  - C14 at lines 250–314: `DISPLAY_FIXTURE_PORT` names the repeated test-only
    port and explains its fixture purpose. The expected wire strings remain
    independent literals, and the `DEFAULT_PORT` assertion remains meaningful.
- `crates/daemar-card/src/error.rs`
  - C14 at lines 329–357: `BIND_ERROR_FIXTURE_PORT` names the independent
    bind-error display fixture; its expected display string remains literal.

Verification and provenance are recorded in
`docs/execution/per-84/run2-task1-repair-test.txt`. In short, the escalated
library run passed 43/43; strict Clippy and `git diff --check` passed; `just
check` retained only the approved 57-pass/62-missing-serve boundary. The
unprivileged baseline's 36-pass/7-loopback-denial result was environmental,
not a product regression.

C1–C16 impact checklist: C1 checked/no finding (native crate error); C2
checked/no finding (hand-written displays); C3 checked/no finding (typed
serialization path and preserved sources); C4 checked/no finding (no new
production panic); C5 checked/no finding (test-only expectation remains
reasoned); C6 checked/no finding (existing enums preserved); C7 checked/no
finding (newtypes unchanged); C8 checked/no finding (publication I/O source
mapping preserved); C9 checked/no finding (no public API added); C10 checked/no
finding; C11 checked/no finding; C12 checked/no finding; C13 corrected in the
private writer path; C14 corrected at both named fixture sites; C15 checked/no
finding; C16 checked/no finding (bind/startup/publication remain separate).

No Card escalation was needed. No commit, review, completion event, or Task2
work was performed by this executor.
