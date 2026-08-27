# Docker Sandboxes against Daemar B1-B10

Research date: 2026-08-26. Evidence is limited to current first-party Docker
Sandboxes documentation, the official CLI reference, and Docker's official
release repositories. No claim below relies on prior Daemar research or model
recollection.

## Bottom line

No: Git-mediated B5/B6 is not the only shortfall.

Docker Sandboxes can supply the microVM boundary and, with explicit policy, an
observable no-egress posture. Clone mode also protects the host worktree.
However, Daemar would still need a supervising wrapper for timeout and
one-shot cleanup, strict configuration to remove extra host capabilities, and
hands-on qualification of command-result fidelity. Exact dirty-worktree input
semantics are not documented. Exact filesystem change reporting and sanitized
promotion remain fundamental mismatches.

The classifications used below are:

- **Native match**: Docker documents the behavior without Daemar adding it.
- **Configurable match**: a documented Docker option/policy reaches the behavior.
- **Wrapper work**: Daemar must orchestrate several CLI operations or enforce a
  postcondition.
- **Fundamental mismatch**: Docker's handoff model cannot express the behavior.
- **Undocumented**: the current first-party material is insufficient to claim it.

## Behavior matrix

| ID | Classification | Finding |
|---|---|---|
| **B1** | **Native match** | Every sandbox is a microVM with its own Linux kernel; the agent and its private Docker Engine run inside that VM. [Isolation layers](https://docs.docker.com/ai/sandboxes/security/isolation/) |
| **B2** | **Configurable match** for the observable test; **operating-rule mismatch** | A normal installation is not necessarily no-network: the first policy setup offers Open, Balanced, and Locked Down, and Balanced has broad development allow rules. Daemar can initialize `deny-all` and, defensively, create each sandbox with `--deny-network "**"`. Docker documents that deny rules outrank allow rules, `"**"` blocks all outbound traffic, and policy resources include hostnames, IP addresses, and CIDRs. UDP and ICMP are blocked at the network layer; TCP is mediated by a host proxy. This should make raw-IP TCP fail, but it is not Daemar's mandated Apple `container --network none` mechanism and still needs a real raw-IP battery test. [Local policy](https://docs.docker.com/ai/sandboxes/governance/access-controls/local/) · [`sbx policy deny network`](https://docs.docker.com/reference/cli/sbx/policy/deny/network/) · [Network isolation](https://docs.docker.com/ai/sandboxes/security/isolation/#network-isolation) |
| **B3** | **Native match in clone mode**, with metadata caveat | `--clone` mounts the host repository read-only and puts writes in a private in-VM clone. Docker says the host working tree and `.git` cannot be modified by the agent. The CLI itself does add a `sandbox-<name>` remote to host Git configuration until removal, so the repository's control metadata is not literally untouched during the sandbox lifetime. [Workspace isolation](https://docs.docker.com/ai/sandboxes/security/isolation/#clone-mode) · [Git remote behavior](https://docs.docker.com/ai/sandboxes/workflows/git/#sandbox-remote-behavior) |
| **B4** | **Native for a clean checked-out ref; undocumented / wrapper work for arbitrary initial state** | The private clone is writable and used as the agent's workspace, but Docker only says it follows the checked-out host ref. It does not say staged, unstaged, untracked, or ignored host state is materialized into that writable clone. Instead, the complete host tree, including untracked and ignored files, remains separately readable at `/run/sandbox/source`. Clone mode also rejects secondary Git worktrees. Daemar cannot claim that the writable starting tree exactly matches an arbitrary host worktree without staging it itself and testing the result. [Clone-mode constraints](https://docs.docker.com/ai/sandboxes/usage/#clone-mode) · [Clone-mode boundary](https://docs.docker.com/ai/sandboxes/security/isolation/#clone-mode) |
| **B5** | **Fundamental mismatch** | Docker exports Git refs, not an exhaustive filesystem transaction. The documented flow asks the agent to create a branch, then the host fetches it. Uncommitted changes are not a fetched ref, and Git does not represent all arbitrary filesystem objects or metadata. Thus Docker does not report every added, modified, and deleted filesystem entry, “nothing more, nothing less.” [Git single-task workflow](https://docs.docker.com/ai/sandboxes/workflows/git/#single-task) |
| **B6** | **Fundamental mismatch under the current contract** | Fetching a sandbox branch is explicit, but Docker documents no promotion operation that validates every destination path and symlink or applies Daemar's setuid/setgid policy. A Git-only product contract could replace B6 with review/fetch/merge semantics; keeping B6 means Daemar still owns validation and promotion. [Clone-mode handoff](https://docs.docker.com/ai/sandboxes/workflows/git/#clone-mode) |
| **B7** | **Configurable match plus wrapper work** | Host filesystem access is otherwise blocked, and complete user-level agent configuration is not imported. Strict use must pass `--no-share-skills`, because supported agents otherwise receive a persistent host-side skills store read-write; mount no extra workspaces; keep clipboard image access disabled; and avoid host MCP integrations. Also, when the launching environment has `SSH_AUTH_SOCK`, Docker forwards the host SSH agent, giving guest processes a signing capability even though private key bytes stay on the host. Docker documents no `sbx` no-forward flag, so Daemar must launch with that variable absent. Project configuration inside the worktree remains visible by design. [Default security posture](https://docs.docker.com/ai/sandboxes/security/defaults/) · [Configuration FAQ](https://docs.docker.com/ai/sandboxes/faq/#why-doesnt-the-sandbox-use-my-user-level-agent-configuration) · [SSH agent](https://docs.docker.com/ai/sandboxes/configuration/credentials/#ssh-agent) |
| **B8** | **Wrapper work** | Neither `sbx run` nor `sbx exec` documents a timeout option. Daemar must supervise the attached command and, at deadline, force removal. `sbx rm --force` is documented to stop a running sandbox and delete it even while in use, but timeout-to-kill behavior is not a single native operation. [`sbx exec`](https://docs.docker.com/reference/cli/sbx/exec/) · [`sbx rm`](https://docs.docker.com/reference/cli/sbx/rm/) |
| **B9** | **Wrapper work**, not the default | Sandboxes intentionally persist across agent exits and stops. Daemar needs a unique sandbox per run and an unconditional `sbx rm --force` cleanup path for success, failure, cancellation, and timeout, followed by a postcondition check. Docker documents that removal deletes the VM and contents and removes the clone remote; it does not expose an atomic one-shot “run and always destroy” operation. [Lifecycle](https://docs.docker.com/ai/sandboxes/architecture/#lifecycle) · [`sbx rm`](https://docs.docker.com/reference/cli/sbx/rm/) |
| **B10** | **Undocumented; requires qualification** | `sbx exec` provides an attached one-shot command surface and should be invoked without a TTY, but the reference does not promise byte-faithful, separately recoverable stdout/stderr or propagation of the command's exact exit status. The 0.39.0 notes only mention reporting the exit code when the sandbox container dies during startup. This must be tested before adoption. [`sbx exec`](https://docs.docker.com/reference/cli/sbx/exec/) · [0.39.0 release](https://github.com/docker/sbx-releases/releases/tag/v0.39.0) |

## Clone mode is narrower than “the same worktree, but private”

The important initial-state distinction is:

1. Docker creates the writable private clone from the ref checked out when the
   sandbox is created; no branch is created automatically.
2. Docker also mounts the original repository read-only at
   `/run/sandbox/source`, including dirty, untracked, and `.gitignore`-excluded
   files.
3. The docs do not say those non-ref bytes are copied into the writable clone.

Therefore a clean committed repository is the only initial state supported by
the documented clone semantics. Supporting Daemar's arbitrary-worktree B4
would require a host-side staging step. Likewise, retrieving output requires
the guest to create commits: fetching the sandbox remote is not an export of
uncommitted working-tree state. Removing the sandbox discards anything not
first fetched or pushed. [Usage](https://docs.docker.com/ai/sandboxes/usage/#clone-mode)

## Operational and adoption gaps outside B1-B10

- **Automation surface:** Docker explicitly supports CI through `sbx create`,
  `sbx exec`, and `sbx rm`, so a supervised subprocess integration is viable.
  The current docs expose no supported SDK or local daemon API; declarative
  `.sbxenv.yaml` automation is marked experimental. [CI workflow](https://docs.docker.com/ai/sandboxes/workflows/automation/) · [`sbx env`](https://docs.docker.com/reference/cli/sbx/env/)
- **Login and host networking:** a Docker account sign-in is required. Headless
  automation uses a Docker PAT, and the host must reach Docker authentication,
  API, and registry domains even when guest egress is denied. This is a material
  operational dependency for an otherwise local sandbox. [Install and sign in](https://docs.docker.com/ai/sandboxes/install/#sign-in) · [Why sign-in is required](https://docs.docker.com/ai/sandboxes/faq/#why-do-i-need-to-sign-in) · [CI login](https://docs.docker.com/ai/sandboxes/workflows/automation/)
- **License and source:** the CLI is free for commercial use, while organization
  governance is paid. The release repository contains binary artifacts and an
  all-rights-reserved proprietary license, not the implementation source. This
  is a hard mismatch if Daemar requires an open-source substrate. [Product terms](https://docs.docker.com/ai/sandboxes/) · [Official release repository](https://github.com/docker/sbx-releases) · [License](https://github.com/docker/sbx-releases/blob/main/LICENSE)
- **Platforms:** supported macOS is Sonoma 14+ on Apple silicon only. Supported
  Linux is Ubuntu 24.04+ on x86-64 or ARM64 with KVM; Docker explicitly does not
  test Ubuntu derivatives. That covers Daemar's current Apple-silicon target
  and a bounded Linux target, not Intel macOS or general Linux. [System requirements](https://docs.docker.com/ai/sandboxes/install/#prerequisites)
- **Pinning and state compatibility:** 0.39.0 is the current stable release in
  the consulted sources. Docker publishes versioned release artifacts; its
  official macOS cask pins both version and SHA-256, so Daemar could vendor or
  otherwise lock that exact artifact and reject another `sbx version`.
  However, normal package installation tracks the current release, and Docker
  warns that downgrading after a newer daemon upgraded local state can require
  destructive sandbox-state reset. The docs do not specify how to pin the
  implicit built-in template/VM assets as one reproducible bundle. [0.39.0 release](https://github.com/docker/sbx-releases/releases/tag/v0.39.0) · [Official `sbx` cask](https://github.com/docker/homebrew-tap/blob/main/Casks/sbx.rb) · [Downgrade warning](https://docs.docker.com/ai/sandboxes/troubleshooting/#daemon-fails-to-start-after-downgrading)
- **Security history:** Docker's 0.38.0 notes say it fixed a destination-escape
  flaw in `sbx cp` copy-out (CVE-2026-17106). Version qualification is therefore
  security-relevant, and Daemar's existing “never `container cp`” rule should
  extend to `sbx cp`; clone/fetch should be the only Docker handoff considered.
  [Docker release notes](https://docs.docker.com/ai/sandboxes/release-notes/#0380)
- **Telemetry:** CLI invocation, success/failure, duration, and signed-in Docker
  username are collected by default; `SBX_NO_TELEMETRY=1` opts out. A strict
  Daemar wrapper should set it. [Telemetry FAQ](https://docs.docker.com/ai/sandboxes/faq/#does-the-cli-collect-telemetry)

## Decision

Docker Sandboxes would remove substantial substrate work: Daemar would not own
the hypervisor, guest kernel, private Docker daemon, or network-policy engine.
It does **not** reduce the current design to only a B5/B6 choice. At minimum,
Daemar still owns:

1. exact input staging for dirty/untracked/ignored state;
2. timeout supervision and forced termination;
3. guaranteed cleanup and leak detection;
4. stripping default host integrations/capabilities;
5. output and exit-status qualification; and
6. either the existing B5/B6 filesystem transaction or a deliberate product
   decision to replace it with a clean-repository, commit/fetch/merge contract.

The fastest responsible next step is a pinned 0.39.0 trial against B2, B4,
B8-B10, plus cleanup-failure injection. Until that passes, Docker Sandboxes is
a promising proprietary VM substrate with a Git workflow, not a demonstrated
implementation of Daemar's sandbox contract.
