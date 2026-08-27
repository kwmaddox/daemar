# Open-source sandbox options for Daemar: corrected synthesis

**Research date:** 2026-08-26

## Answer

There is no current open-source product I would recommend trusting as a
drop-in implementation of Daemar's complete B1-B10 contract.

There is a credible compositional path that substantially reduces Daemar's
security-sensitive code:

1. keep a maintained VM runtime responsible for the guest-kernel boundary,
   network absence, process execution, and VM lifecycle;
2. give the guest a disposable **staging copy**, never the real worktree and
   never a writable result/control directory;
3. stop the VM before the host examines results;
4. compare the staging tree with a frozen host baseline; and
5. promote accepted changes with capability-relative host filesystem
   operations.

On macOS today, the conservative runtime is Apple's open-source
[`container`](https://github.com/apple/container), which Daemar already uses.
For the host-side promotion boundary,
[`cap-std`](https://github.com/bytecodealliance/cap-std) is a useful maintained
primitive. Git can provide content identities and conflict checks, but it
cannot be the only changeset representation because Git does not represent
empty directories or general filesystem metadata.

This is not as satisfying as finding a mature turnkey project. It is a better
maintenance answer than either continuing the current in-guest overlay/export
protocol or adopting a young sandbox whose security claims Daemar must audit
itself.

## Correction to the initial survey

The first discovery passes ranked Microsandbox as the leading cross-platform
substrate and Wreckroom as the closest workflow match. That ranking was wrong.

The mistake was methodological: fresh primary-source review was correctly
required, but the pass failed to reconcile new claims with the repository's
existing empirical rejection evidence. Prior experiments should not be treated
as timeless truth, but they must be used as regression hypotheses. The relevant
evidence was already captured in
[`microsandbox-proof.md`](./microsandbox-proof.md).

That test ran Microsandbox 0.6.8 on the target Apple Silicon/macOS host and
found:

- `--no-net` by itself denied raw-IP egress;
- adding one documented hostname allow rule opened TCP access to arbitrary raw
  IP addresses;
- explicit denial of `0.0.0.0/0` or the exact destination IP did not close the
  path; and
- the real value supplied through the secret feature appeared in host process
  arguments and persisted host state.

The network result is a failure of the advertised mediation model, not a niche
feature gap. A hostile workload can bypass a hostname allowlist without DNS.
For a software factory that will eventually permit narrow model/API access,
that defeats the security boundary's purpose.

The maintenance signal compounds it. Microsandbox's published
[`GHSA-m8f5-rh7h-vgg3`](https://github.com/superradcompany/microsandbox/security/advisories/GHSA-m8f5-rh7h-vgg3)
still lists all versions as affected and no patched version. Current 0.6.15
source also pins a project-specific VMM fork, `msb_krun`, rather than consuming
the upstream libkrun release directly
([workspace manifest](https://github.com/superradcompany/microsandbox/blob/8402def43bb990b4c3b3cd2efb4428c5808857b5/Cargo.toml),
[`msb_krun` package](https://crates.io/crates/msb_krun/0.1.32)). That is not
automatically unsafe, but it concentrates responsibility for the VMM/device
code, integration, and security response in the same young upstream.

The conclusion is about upstream trust, not just whether Daemar could route
around two defects:

- **Microsandbox is rejected.** Fixing egress externally and avoiding its
  secret mechanism would return those security responsibilities to Daemar,
  defeating the reason to adopt it.
- **Wreckroom is rejected transitively.** It builds on Microsandbox, and its
  own project is an unaudited, extremely young alpha. Its current apply path
  also preserves setuid/setgid bits, lacks Daemar's wall timeout and
  unconditional cleanup semantics, and uses pathname validation followed by
  later pathname mutation
  ([repository](https://github.com/storozhenko98/wreckroom)).

No claim is made here that Microsandbox 0.6.15 still has the exact raw-IP bug;
that version was not rerun. The combination of the demonstrated core-control
failure, unresolved advisory, and concentrated security ownership is enough to
reject the project for this purpose unless its governance and empirical record
change materially.

## What Daemar actually needs to outsource

The useful unit of comparison is responsibility, not product count.

| Responsibility | Desired owner | Why |
|---|---|---|
| Hypervisor, guest kernel, OCI/rootfs boot | Runtime upstream | Too specialized and security-critical for Daemar to maintain. |
| No direct network path | Runtime upstream, verified by Daemar | A hostname filter over general egress is unacceptable. |
| Process I/O, exit, timeout, kill, VM removal | Runtime upstream plus a thin Daemar adapter | Mature lifecycle primitives should replace a custom shell/result protocol. |
| Private editable workspace | Disposable host staging tree | This can eliminate the guest overlay/export channel entirely. |
| Exact A/M/D comparison | Host-side code over two inert trees | The VM is already dead; results need not cross via an adversarial tar protocol. |
| Promotion containment | Capability-relative filesystem library plus small Daemar policy | Daemar still decides allowed entry types, modes, symlinks, conflicts, and atomicity. |

The objective is not zero Daemar security code. It is a boundary small enough
to understand: compare two trees, validate a finite set of filesystem object
types, and apply accepted operations beneath an already-open destination
directory.

## Recommended composition to evaluate first

### Apple `container` + disposable staging tree + `cap-std`

This is an architectural simplification of the existing implementation rather
than a substrate migration.

1. Create a private per-run staging directory on the host. Populate it from the
   worktree using a complete copy or filesystem clone. Do not expose the
   original worktree or its shared Git directory to the guest. If the workload
   needs Git, initialize independent repository metadata inside staging.
2. Mount only staging read-write into an Apple `container` VM. Keep
   `--network none`; do not add a general egress path.
3. Execute the workload through the runtime's foreground process interface.
   Capture stdout/stderr and the workload status. Enforce a host-side wall
   deadline and remove the VM on every path.
4. Stop and reap the VM before inspecting staging. This prevents guest
   descendants from racing result inspection.
5. On the host, compare staging with a frozen baseline. Use `symlink_metadata`
   or capability-relative equivalents so comparison never follows a guest
   symlink. Classify unsupported special files explicitly rather than silently
   omitting them.
6. Promote only after review. Resolve every destination relative to an open
   `cap_std::fs::Dir`; reject absolute/parent escapes and unsafe symlink
   targets; clear setuid/setgid; use temporary files plus rename for regular
   files; and report every rejection.
7. Remove staging through an unconditional guard and reconcile stale run
   directories at process startup to cover parent crashes as well as ordinary
   errors.

Why this is simpler than the current design:

- no in-guest OverlayFS setup;
- no guest-authored tar archive;
- no writable output mount containing trusted-looking control files;
- no driver exit-code namespace or status classification protocol;
- no archive extraction parser in the promotion path; and
- no dependency on a young cross-platform VMM fork.

The guest writes directly to a disposable host directory, not the user's
worktree. The VM/runtime boundary must still prove that the guest cannot escape
the declared share; Daemar already has an Apple `container` containment battery
and should keep running it against the pinned version.

The cost is copying or cloning the worktree. APFS clone/copy-on-write primitives
can make that inexpensive on macOS; the semantic fallback should remain a full
copy so correctness does not depend on that optimization.

## Components considered for the chain

### Git

Git is useful on the trusted host side for blob identity, binary diffs, and
conflict preflight
([`git diff`](https://git-scm.com/docs/git-diff),
[`git apply`](https://git-scm.com/docs/git-apply)). It also narrows regular file
modes to executable/non-executable, so privileged bits cannot be represented in
a Git tree.

It is not the whole B5/B6 solution. Git omits empty directories, special files,
ownership, ACLs, and xattrs. Project-controlled config, hooks, filters,
attributes, alternates, and external diff drivers must not be trusted. If used,
Git should run on the host with isolated configuration and repository metadata,
after the VM has stopped.

### `cap-std`

`cap-std::fs::Dir` exposes filesystem operations relative to an already-open
directory and is designed to prevent path traversal outside that capability
([project README](https://github.com/bytecodealliance/cap-std)). This is a
better primitive than checking a joined pathname and mutating it later.

Daemar still owns semantic policy: which entry types are accepted, how symlink
targets are evaluated, conflict handling, mode sanitization, and whether a
multi-file promotion is atomic enough for the contract.

### AgentFS

[AgentFS](https://github.com/tursodatabase/agentfs) supplies a SQLite-backed
filesystem overlay with FUSE on Linux, NFS on macOS, and a diff view. It is not
recommended for this slice. It is beta, does not provide safe host promotion,
and adds an NFS/FUSE server plus database parser to the trusted path while
leaving Daemar responsible for modes, symlinks, and apply. The staging-tree
composition is easier to reason about.

### Shuru

[Shuru](https://github.com/superhq-ai/shuru) is the most interesting macOS
behavioral comparator: Apple Virtualization.framework, network absent by
default, read-only host mounts, and an ephemeral guest overlay. It currently
discards that overlay rather than exposing a complete, stable change-export
contract. Linux support is explicitly experimental and ARM64-only. Its short
history and limited public security assurance mean it should be tested, not
trusted as the new boundary.

### NVIDIA OpenShell

[OpenShell](https://github.com/NVIDIA/OpenShell) has the strongest organizational
ownership and policy/control-plane story among the new projects. Its B1 path,
however, depends on an explicitly experimental VM driver; the normal container
driver is not a separate-kernel boundary. It also adds substantially more
machinery than Daemar's one-shot slice and still does not supply safe worktree
promotion. Track it as a possible future integrated platform, not a rapid local
replacement.

### BoxLite

[BoxLite](https://github.com/boxlite-ai/boxlite) has a broad Rust SDK and
cross-platform VM support, but it does not own the worktree transaction. More
importantly for this decision, versions before 0.9.0 had critical host-write
vulnerabilities in read-only volume and OCI extraction paths
([GHSA-f396-4rp4-7v2j](https://github.com/boxlite-ai/boxlite/security/advisories/GHSA-f396-4rp4-7v2j)).
The fixes and disclosure are positive maintenance evidence, but the size of its
host-side parsing/runtime surface means it cannot be adopted to reduce security
ownership without hands-on qualification of a pinned version.

### Kata Containers for Linux

[Kata Containers](https://github.com/kata-containers/kata-containers) is the
most mature higher-level open-source VM-backed runtime considered for a future
Linux host. It has a documented threat model and security process, but brings a
containerd-oriented operational stack and a rolling latest-release support
model. The staging-tree seam can remain the same if Daemar later adds Kata as a
runtime provider. This avoids choosing the Linux substrate during the current
macOS simplification.

### Firecracker and containerd nerdbox

[Firecracker](https://github.com/firecracker-microvm/firecracker) has strong
security and release discipline, but it is deliberately low-level. Direct use
would make Daemar own networking, guest transport, rootfs construction, jailer
configuration, and cleanup—the opposite of the maintenance goal.

[containerd nerdbox](https://github.com/containerd/nerdbox) is architecturally
interesting because it targets VM-per-container operation on macOS and Linux,
but it is currently experimental and unreleased. Track it rather than building
on it now.

### Docker Sandboxes

Docker Sandboxes is the closest polished comparator: microVM isolation,
deny-by-default proxying, and a private-clone Git workflow
([security model](https://docs.docker.com/ai/sandboxes/security/isolation/),
[`--clone` workflow](https://docs.docker.com/ai/sandboxes/workflows/git/)). The
usable product is proprietary, not an open-source answer
([release repository](https://github.com/docker/sbx-releases)). If the
open-source constraint is ever relaxed, it deserves the same independent raw-IP
and lifecycle trials rather than trust by documentation.

## Rejected architectural shapes

- **Hostname filtering over a general guest network path.** A forced proxy or
  broker must be the only path; L7 allowlists may refine an L3/L4 deny boundary
  but cannot replace it.
- **Generic tar/zip result export.** Archive parsing, path normalization,
  symlinks, hard links, special files, ownership, and modes create a large
  adversarial host-side surface.
- **A writable mount that mixes results with trusted control signals.** Workload
  output must not be able to forge success or driver state.
- **A permanent fork of a young sandbox project.** That transfers security
  maintenance back to Daemar while hiding it behind an upstream-shaped API.
- **Direct Firecracker/libkrun integration.** A VMM alone does not own enough of
  the sandbox to reduce Daemar's security burden.

## Required proof before changing the implementation

The next work should be a bounded, disposable comparison, not a migration:

1. Implement the staging-tree flow behind the existing `RunSpec`/`RunOutcome`
   boundary without changing public semantics.
2. Run the existing B1-B10 battery unchanged against the pinned Apple
   `container` version.
3. Add independent cases for parent-process death, surviving guest descendants,
   staging cleanup on restart, special files, hard links, symlink swaps,
   non-UTF-8 paths, very large trees, and host edits concurrent with promotion.
4. Measure the security-sensitive code deleted versus added. The spike fails if
   it retains the driver archive/status protocol or creates a new general
   result parser.
5. Record the exact runtime binary/source digest, guest image/kernel digest,
   mount configuration, and test result. Re-run qualification on runtime
   upgrades.

Separately, any future egress feature must begin with no guest network path and
add only an explicitly mediated proxy/broker transport. Raw IPv4 and IPv6 tests
belong in the acceptance contract from the first implementation.

## Decision

The open-source ecosystem can remove most generic sandbox machinery, but no
current project earns ownership of Daemar's complete security contract.

The rapid, maintainable direction is therefore:

```text
Apple container (now) / mature Linux VM runtime (later)
    owns VM + guest kernel + no-network + exec + kill/remove

Disposable staging tree
    owns the guest's editable view; original worktree is never mounted writable

Small host-side comparator + cap-std promotion
    owns exact changes + explicit policy + safe application
```

This does not eliminate security work. It removes the hardest-to-maintain parts
of the current design—the in-guest overlay/export and writable result protocol—
while leaving a compact policy boundary that can be understood, tested, and
reviewed without becoming a VMM or archive-security expert.
