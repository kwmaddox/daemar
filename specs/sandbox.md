# Sandbox — desired behaviors

This file specifies the current one-shot sandbox slice. It is one explored
mechanism for the broader product outcome in
[`secure-agentic-coding-environments.md`](secure-agentic-coding-environments.md),
not the permanent definition of a Daemar coding environment.

The intended outcome of the sandbox slice, stated as plain, falsifiable
assertions. Every battery test cites the behavior ID(s) it proves — in the
test name or a comment. A behavior with no citing test is unproven. Future
slices extend this list; they do not rewrite it.

Scope of this slice: one-shot arbitrary-command execution against a worktree,
no network, overlay filesystem. Substrate: Apple `container` (pinned version),
driven as a supervised subprocess.

## Behaviors

- **B1.** A command runs inside a VM with its own kernel, never on the host.
- **B2.** A run has no network path: raw-IP connections to any address fail.
  (Tests assert on raw IP, never on name resolution.)
- **B3.** The host worktree is byte-identical after every run, regardless of
  what the command did.
- **B4.** The command sees the worktree at its working directory and can edit
  it in place — in its own view.
- **B5.** Every change the command made is reported: added, modified,
  deleted — nothing more, nothing less.
- **B6.** Changes reach the host only through explicit promotion, which
  strips setuid/setgid bits and rejects paths or symlinks that escape the
  destination.
- **B7.** Nothing outside the worktree mount is readable from the guest.
- **B8.** A run that exceeds its timeout is killed.
- **B9.** After any run — success, failure, timeout — no container or temp
  state remains on the host.
- **B10.** stdout, stderr, and exit code come back faithfully.

## Operating rules (constraints on how behaviors are achieved)

These are not behaviors of the sandbox but standing rules earned by research
(`docs/research/substrate-refutation.md`); the implementation and its reviews
hold to them:

- `--network none` is the only network posture; the CLI version is pinned and
  B2 doubles as the guard that the (undocumented) flag still behaves.
- Never `container cp`, never `--ssh`, never build images from untrusted
  context. Results cross the wall only via our mounts (B6).
- Escape hatches, if ever added, must be loud, opt-in, and costly — never a
  silent fallback.
