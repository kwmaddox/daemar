# daemar

Clean slate, again. Two prior iterations live in the archive:

- [`archive/daemar-v1`](https://github.com/kwmaddox/daemar/tree/archive/daemar-v1)
  — torn out after a core security invariant turned out not to hold,
  invalidating the design work built on top of it.
- [`archive/sandbox-era`](https://github.com/kwmaddox/daemar/tree/archive/sandbox-era)
  — the sandbox v1 slice (Apple Container one-shot runs, overlay
  promotion) plus its research record. Torn out not because it was wrong
  but because it was the wrong project: a fool box built before the
  factory that would use it. Sandboxing returns later as its own service
  (Nar'baha), sized by the factory's real needs.

What survives is the quality machinery: `conventions.md` (C-IDs), the
gate (`scripts/check.sh`, clippy/deny/ast-grep configs), and the
versioned hooks. Nothing else inherits without being re-earned.

Working here — agent or human — starts at [`AGENTS.md`](AGENTS.md).
