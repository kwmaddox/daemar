# Open-source agent sandboxes that could replace Daemar's sandbox

**Research date:** 2026-08-26. **Question:** is there a current open-source,
local/self-hostable product that can replace Daemar's sandbox, rather than
continuing to grow a security-sensitive implementation in this repository?

## Bottom line

**No current candidate is a drop-in, end-to-end replacement for Daemar's B1-B10
contract.** The market has credible VM runtimes and several agent-sandbox
products, but nearly all stop at "run commands and move files." Daemar's unusual
requirement is the transaction around the worktree: a private editable view,
complete add/modify/delete accounting, and host-side sanitized promotion.

There is, however, a much better answer than "keep owning everything":

1. **Wreckroom is the closest functional match.** It already combines a
   Microsandbox microVM, read-only host source, a private OverlayFS upper layer,
   merged-view export, conflict detection, review, and explicit apply. It misses
   three important Daemar guarantees: no run timeout, retained state instead of
   unconditional cleanup, and no stripping of setuid/setgid bits on apply. It is
   also a four-commit, one-day-old early alpha with no independent audit. It is a
   strong design reference or upstream-collaboration target, not yet a boundary
   to trust without owning its audit and maintenance.
2. **Microsandbox is the strongest candidate to replace Daemar's substrate.**
   It is a Rust library and CLI, runs on Apple Silicon and Linux/KVM, provides a
   real guest kernel, can remove the network interface entirely, supports
   read-only host mounts, command timeouts, output/exit capture, and explicit
   lifecycle control. Daemar would still own the worktree OverlayFS assembly,
   exact change extraction, sanitized promotion, and fail-closed orchestration.
   That is still a meaningful reduction: Daemar stops owning VM boot, image
   preparation, guest command transport, cross-platform VMM selection, and most
   process supervision.
3. **OpenShell is the strongest policy/control-plane candidate.** NVIDIA's
   Apache-2.0 Rust project has substantially greater maintainer depth and public
   activity than the smaller runtimes, and combines filesystem/network policy,
   process control, and immutable roots. However, B1 depends on selecting its
   explicitly experimental microVM driver; the default container driver shares
   the host kernel. It also does not supply Daemar's exact worktree transaction
   or a first-party Rust client SDK. It reduces policy ownership, but introduces
   a larger control plane and does not yet provide a mature VM boundary.
4. **BoxLite is the strongest maintenance-oriented alternative substrate.** It
   has the broadest supported platform and API surface here, including a native
   Rust SDK, but it supplies no private editable overlay of a read-only host tree
   and no change-set/promotion protocol. Its project has active release and
   security processes, but two critical host-write vulnerabilities affected all
   versions before 0.9.0. Any evaluation must pin 0.9.0 or later and rerun the
   B1-B10 battery.
5. **Shuru and Matchlock are credible but less compelling for Daemar.** Shuru
   nearly supplies B3/B4 directly but discards the upper layer and has only
   experimental Linux ARM64 support. Matchlock spans macOS and Linux and has a
   clean CLI/SDK shape, but it is experimental, Go-based, and likewise discards
   overlay changes instead of exporting and promoting them.

The decision should therefore be framed as **how much security-sensitive code
Daemar can stop owning**, not simply which candidate has the most checkmarks.
The lowest-risk next move is a time-boxed adapter spike against **Microsandbox
0.6.15+**, with **OpenShell's microVM driver** as a policy-rich comparator, and
a parallel upstream conversation with **Wreckroom** about the B6, B8, and B9
gaps. Do not replace the current sandbox with Wreckroom merely because its
README looks almost identical to the spec.

## Method and evidence standard

I first extracted requirements from [`specs/sandbox.md`](../../specs/sandbox.md)
and the current public surface in `crates/sandbox`. Candidate claims below were
then checked fresh against primary sources: official repositories, source code,
SDK references, release pages, security policies, and advisories. Existing
Daemar research conclusions were not used as evidence.

"Not documented" is not treated as proof that a feature cannot exist. It means
the feature cannot currently be relied on as a public contract. No candidate was
installed or subjected to Daemar's behavioral battery during this research pass;
documentation and source inspection identify what is worth testing, not what has
already earned production trust.

## The contract a replacement must satisfy

The full contract is more than VM execution:

| ID | Required behavior |
|---|---|
| B1 | Every run has its own guest kernel; the command never runs on the host. |
| B2 | No network path at all, including raw-IP egress. |
| B3-B4 | The host worktree remains byte-identical while the guest sees an editable private view. |
| B5-B6 | Report every add/modify/delete exactly, then promote only explicitly; reject path/symlink escapes and strip setuid/setgid. |
| B7 | Nothing outside the intended worktree mount is guest-readable. |
| B8-B9 | Kill timed-out runs and leave no VM/container or temporary session state on every path. |
| B10 | Return faithful stdout, stderr, and workload exit code. |

The current Rust API also matters: one-shot argv execution, a wall-clock timeout,
typed failure, byte-preserving output, a typed `ChangeSet`, and host-side
`apply_to`. A generic "files API" or VM archive is not an equivalent change set.

## Decision matrix

`Yes` means the candidate documents a usable primitive; it does not mean Daemar
has independently proved it. `Partial` means adaptation or Daemar-owned code is
required.

| Candidate | macOS AS / Linux | B1-B2 | B3-B4 private edit | B5-B6 exact promote | B8-B10 run contract | Rust integration | Classification |
|---|---|---:|---:|---:|---:|---:|---|
| **Wreckroom 0.2.1** | Yes / x86_64+ARM64 KVM | Yes | Yes | Partial | Partial | Rust CLI | **Closest adaptable product** |
| **Microsandbox 0.6.15** | Yes / x86_64+ARM64 KVM | Yes | Partial | No | Yes, with caller cleanup | Native Rust SDK + CLI | **Best substrate candidate** |
| **OpenShell 0.0.x** | Yes / Yes | Partial: B1 only with experimental microVM driver | Partial | No | Yes, with caller policy/cleanup | Rust core + gRPC/CLI; no public Rust client SDK | **Best policy/control-plane candidate** |
| **BoxLite 0.9+** | Yes / x86_64+ARM64 KVM | Yes | Partial | No | Yes, with caller cleanup | Native Rust SDK + CLI + REST | **Strong alternate substrate** |
| **ArcBox public beta** | M3+ only / no Linux | Yes | Explicit copy, not private host edit | No | Yes, with caller cleanup | Rust core + gRPC; Python/TS SDKs | Mac-only near miss |
| **Shuru 0.7.0** | Yes / experimental ARM64 KVM | Yes | Yes | No; upper is discarded | Partial | CLI; TypeScript SDK | Adaptable, macOS-first |
| **Matchlock 0.2.x** | Yes / KVM | Yes with `--no-network` | Yes, ephemeral | No; overlay vanishes | Partial | Go/Python/TS + CLI/RPC | Adaptable, experimental |
| Tupper | macOS now / Linux experimental | Inherits Apple/Firecracker | No Daemar transaction | No | Generic exec/files | TypeScript + CLI/API/MCP | Thin wrapper, not a replacement |
| Bhatti v2 | Yes / x86_64+ARM64 KVM | **Fails B2** | No Daemar transaction | No | Strong lifecycle/exec | CLI + REST | Persistent VM platform |
| E2B self-hosted | No local macOS / cloud Linux | Firecracker | Upload/remote files | No | Strong remote exec | No Rust SDK | Hosted-platform architecture |
| OpenSandbox / K8s Agent Sandbox / Daytona | Docker/K8s-oriented | Runtime-dependent | Volumes/files APIs | No | Strong orchestration | No Rust SDK | Control plane, not boundary |
| Docker Sandboxes | Yes / Linux KVM | Strong | Git clone mode | Git transport, not B5-B6 | Strong | CLI | **Close comparator, proprietary** |

## Candidate findings

### 1. Wreckroom: closest behavior, weakest maturity

[Wreckroom](https://github.com/storozhenko98/wreckroom) is the only discovered
project whose product boundary is recognizably the same transaction as Daemar's.
Its official architecture mounts the host source read-only, makes it the lower
layer of an in-guest OverlayFS, stores upper/work state on a private ext4 volume,
exports the merged view, compares it with a launch manifest, checks concurrent
host conflicts, and applies on the host
([architecture](https://github.com/storozhenko98/wreckroom/blob/f5ec6027cadc406b5810b5ebb4a41cd83b4f3aaa/docs/ARCHITECTURE.md)).
It supports Apple Silicon and Linux x86_64/ARM64 KVM, disables networking by
default, handles add/modify/delete/type/mode/symlink changes, and is Apache-2.0
([README](https://github.com/storozhenko98/wreckroom/tree/v0.2.1)).

That is a direct B1-B5 fit and much of B6/B7. Source inspection found the
important mismatches:

- **B6:** the launch mount is `nosuid`, but host apply later restores the full
  recorded `0o7777` mode. `install_file` and directory apply do not clear
  `0o6000`
  ([apply source](https://github.com/storozhenko98/wreckroom/blob/f5ec6027cadc406b5810b5ebb4a41cd83b4f3aaa/src/apply.rs)).
  Symlink-parent traversal and escaping symlink targets are rejected, which is
  good, but Daemar's privileged-bit sanitization is absent.
- **B8:** the command path invokes `msb exec` and waits; no Wreckroom run timeout
  exists
  ([execution source](https://github.com/storozhenko98/wreckroom/blob/f5ec6027cadc406b5810b5ebb4a41cd83b4f3aaa/src/msb.rs#L153)).
- **B9:** review/apply intentionally retain the sandbox, private volume, result,
  and metadata until `wreckroom discard`; only discard removes them
  ([lifecycle](https://github.com/storozhenko98/wreckroom/blob/f5ec6027cadc406b5810b5ebb4a41cd83b4f3aaa/docs/ARCHITECTURE.md#state)).
- **B10/API:** interactive stdio and the child exit code pass through, but there
  is no structured byte-vector `RunOutcome` equivalent for an embedding caller.
- **B5 limits:** only valid-UTF-8 paths, files/directories/symlinks, and Unix
  modes are handled; hard-link identity, ACLs, xattrs, ownership, and timestamps
  are explicitly not preserved
  ([limitations](https://github.com/storozhenko98/wreckroom/tree/v0.2.1#safety-boundaries-and-alpha-limitations)).

The larger issue is trust. Its own security policy says it is early alpha, has
not received an independent security audit, and should not be the sole boundary
for hostile workloads
([SECURITY.md](https://github.com/storozhenko98/wreckroom/blob/v0.2.1/SECURITY.md)).
At the research snapshot the repository showed four commits, one visible author,
and releases 0.1.0 through 0.2.1 on one day. Adopting it wholesale would not make
the security problem someone else's mature responsibility; it would make Daemar
dependent on a very small upstream while still needing to review its apply path.

**Security ownership if adopted:** Wreckroom/Microsandbox would own VM boot,
mount transport, and most export machinery. Daemar would either fork and own the
entire alpha CLI or upstream B6/B8/B9 plus a structured one-shot interface. This
is potentially the smallest final codebase, but not currently the smallest risk.

### 2. Microsandbox: best way to shrink the trusted substrate

[Microsandbox](https://github.com/superradcompany/microsandbox) is an
Apache-2.0 embedded microVM runtime. The current Rust crate documents a real
guest kernel, OCI images, command/file APIs, and Apple Silicon or Linux/KVM
requirements
([crate 0.6.15](https://docs.rs/crate/microsandbox/0.6.15)). Its Rust builder
can fully disable networking so no interface is created, configure read-only
host binds, set per-exec timeouts and maximum sandbox duration, capture separated
stdout/stderr plus exit status, kill a sandbox, and remove it
([Rust SDK](https://github.com/superradcompany/microsandbox/blob/8402def43bb990b4c3b3cd2efb4428c5808857b5/docs/sdk/rust/sandbox.mdx)).

This covers B1, B2, B7, B8, and B10 directly and provides useful building blocks
for B3/B9. It does **not** provide Daemar's private editable view of a read-only
host tree or its exact/promotable change set. The fact that Wreckroom builds that
layer on Microsandbox is strong implementation evidence that the composition is
possible, but Daemar would still own and test it.

Maturity is materially better than Wreckroom but not complete. The project calls
itself beta. Releases publish many platform artifacts, checksums, and release
attestations; recent releases show multiple maintainers and outside contributors
([release history](https://github.com/superradcompany/microsandbox/releases)). It
has an explicit vulnerability-reporting policy and disclosure targets
([security policy](https://github.com/superradcompany/microsandbox/blob/8402def43bb990b4c3b3cd2efb4428c5808857b5/SECURITY.md)). However, the published
moderate advisory GHSA-m8f5-rh7h-vgg3 currently marks **all versions affected and
no patched version**: real secret values passed to its network substitution path
are exposed in host process arguments
([advisory](https://github.com/superradcompany/microsandbox/security/advisories/GHSA-m8f5-rh7h-vgg3)).

That advisory does not block the B1-B10 slice if Daemar does not pass secrets or
untrusted environment values through that feature, but it is a reason to keep the
integration narrow and pinned. It also demonstrates why a maintained dependency
does not remove Daemar's duty to understand which surfaces it enables.

**Security ownership if adopted:** Microsandbox owns VMM selection, kernel/OCI
boot, image extraction, virtiofs/vsock transport, guest process execution,
network-device removal, and kill primitives. Daemar owns the comparatively small
but still critical transaction: mount only the worktree read-only, construct the
private overlay, export it through a trusted channel, parse exact changes, strip
privileged bits/reject escapes, and guarantee remove-on-every-path. This is the
best current balance of reduced bespoke code and inspectable local policy.

### 3. NVIDIA OpenShell: strongest policy ownership, experimental VM boundary

[OpenShell](https://github.com/NVIDIA/OpenShell) is an Apache-2.0, Rust-based
sandbox control plane from NVIDIA. It combines immutable roots and private
overlays with filesystem policy, Landlock, seccomp and capability policy,
network namespaces, and proxy-mediated minimal egress. Its execution protocol
supports timeouts, stdout/stderr streams, exit results, and file
upload/download. The official repository supports Linux and Apple Silicon hosts
and documents Docker, Podman, Kubernetes, and microVM compute backends. It also
has a formal vulnerability-reporting policy
([security policy](https://github.com/NVIDIA/OpenShell/blob/main/SECURITY.md)).

The backend distinction is decisive. The ordinary container driver does not
provide B1. OpenShell's own documentation labels the microVM driver
**experimental**, so Daemar would have to force and verify that driver on every
run before treating OpenShell as a kernel boundary
([microVM driver](https://github.com/NVIDIA/OpenShell/blob/main/crates/openshell-driver-vm/README.md)).
It also lacks Daemar's complete add/modify/delete transaction and sanitized
promotion. Its implementation and protocol are Rust/gRPC, but its first-party
public client SDKs are Python, Go, and TypeScript rather than a stable Rust SDK.

The maintenance signal is stronger than for the young single-purpose projects:
at the snapshot the official repository showed more than 1,200 commits,
thousands of stars and forks, active work, and an organizational owner. Those
signals are not an audit, and the 0.0.x line plus experimental VM backend
prevent a production-trust claim. They do make OpenShell the strongest option
if Daemar wants an upstream to own policy machinery as well as process control.

**Security ownership if adopted:** OpenShell would own the policy engine,
process protocol, network enforcement, orchestration, and—only with the
experimental driver—the VM lifecycle. Daemar still owns verified backend
selection, the worktree transaction, sanitized promotion, and cleanup proof.
That is less bespoke policy code but more configuration surface to secure.

### 4. BoxLite: strongest project surface, more workspace work

[BoxLite](https://github.com/boxlite-ai/boxlite) is Apache-2.0 and supports
Apple Silicon, Linux x86_64/ARM64 KVM, an embedded Rust SDK, CLI, and optional
REST service. Its public feature contract includes a VM per box, a mode that
removes the guest network interface, read-only and read-write binds, per-command
timeouts, streamed stdout/stderr and exit codes, explicit lifecycle, and box
archive import/export
([README](https://github.com/boxlite-ai/boxlite/blob/d354470ac25d5ce8e83d7520c3f5ad590182907d/README.md),
[network reference](https://github.com/boxlite-ai/boxlite/blob/d354470ac25d5ce8e83d7520c3f5ad590182907d/docs/reference/README.md#network-configuration)).

The archive is a portable box archive, not an exact worktree delta. A read-only
bind is not writable in the guest, while a read-write bind changes the host.
Therefore B3-B6 still require the same private-overlay/change/promotion layer as
with Microsandbox. BoxLite's larger API can also be a liability if Daemar enables
features it does not need; the integration should use a deliberately tiny subset.

Its security posture is unusually transparent. The current policy supports only
the latest 0.9.x line and lists two critical vulnerabilities affecting every SDK
before 0.9.0: a read-only-volume remount bypass and an OCI-layer symlink escape
that allowed arbitrary host writes. Both are fixed in 0.9.0
([security policy](https://github.com/boxlite-ai/boxlite/blob/d354470ac25d5ce8e83d7520c3f5ad590182907d/SECURITY.md),
[host-write advisory](https://github.com/boxlite-ai/boxlite/security/advisories/GHSA-f396-4rp4-7v2j)).
The project shows broad, active pull-request traffic and a formal reporting and
90-day disclosure process, but it is still pre-1.0 and does not backpatch older
minors.

**Security ownership if adopted:** similar to Microsandbox, but Daemar also
accepts BoxLite's broader host-side image/archive/runtime attack surface. In
return it gets the most complete Rust-facing lifecycle and cross-platform API.
This is worth a spike if maintenance depth weighs more heavily than minimal
machinery.

### 5. ArcBox: clean agent API, incompatible platform roadmap

[ArcBox](https://github.com/arcboxlabs/arcbox) is a public-beta,
MIT/Apache-2.0 sandbox implemented in Rust around Firecracker and exposed over
gRPC with Python and TypeScript SDKs. Its contract starts from an empty
workspace with no implicit host mount, uses a copy-on-write root, supports
explicit copy in/out, process output/wait/signal, TTL and idle timeouts, and an
explicit `NONE` network mode
([agent guide](https://github.com/arcboxlabs/arcbox/blob/master/docs/agent-sandbox.md),
[API](https://github.com/arcboxlabs/arcbox/blob/master/docs/sandbox-api.md)).
Explicit file copy is not B3-B6: Daemar would still need to compare the complete
workspace and sanitize promotion.

The platform fit is blocking. ArcBox currently requires macOS 15 and Apple
Silicon M3 or newer because it relies on nested virtualization; Linux is future
work. It publishes releases and a
[security policy](https://github.com/arcboxlabs/arcbox/blob/master/SECURITY.md),
but remains public beta.

**Security ownership if adopted:** ArcBox owns the nested VM, guest process and
file-transfer transport, network-off switch, and lifecycle timers. Daemar owns
workspace import/export, exact diff, promotion, and cleanup orchestration—and
would need another Linux backend. The hardware floor and platform split
outweigh its clean API here.

### 6. Shuru: excellent B3/B4 primitive, poor Linux path

[Shuru](https://github.com/superhq-ai/shuru) is Apache-2.0, Rust-based, and
macOS-first. It boots a Linux VM using Virtualization.framework, has offline as
the default (the guest has no network device unless `--allow-net` is given), and
mounts host directories read-only with guest writes going to a tmpfs OverlayFS
upper layer. That is almost exactly B1-B4
([README](https://github.com/superhq-ai/shuru/blob/v0.7.0/README.md)). It returns
stdout, stderr, and exit status through its TypeScript SDK and supports process
kill
([SDK](https://github.com/superhq-ai/shuru/blob/v0.7.0/packages/sdk/README.md)).

The decisive gap is explicit: overlay changes are discarded when the VM stops.
The standalone product has watchers and per-path discard support, but no public
exact export or sanitized promotion API. Daemar would need to add that trusted
channel plus a wall-clock run timeout. Linux support is explicitly experimental,
ARM64-only, and not production-ready. That misses the likely mid-term Linux
x86_64 deployment target.

**Security ownership if adopted:** less runtime code on macOS, but Daemar still
owns all of B5/B6/B8 and would need a second Linux strategy. That platform split
works against rapid development.

### 7. Matchlock: credible cross-platform runtime, no transaction

[Matchlock](https://github.com/jingkaihe/matchlock) is an MIT-licensed,
experimental Go microVM sandbox using Virtualization.framework on Apple Silicon
and Firecracker on Linux. It exposes a CLI, JSON-RPC, and Go/Python/TypeScript
SDKs; `--no-network` is explicit, process cancellation is part of its API, and
ephemeral volume overlays vanish on cleanup
([README](https://github.com/jingkaihe/matchlock)).

It is a strong agent-runtime shape, but "overlay vanishes" is the wrong end of
Daemar's transaction: there is no exact export, typed change set, or sanitized
host promotion. The default network is NAT unless `--no-network` or interception
is selected, so Daemar must force the closed mode. There is no Rust SDK, and the
project labels itself experimental and subject to breaking changes.

**Security ownership if adopted:** Matchlock would own VM and network machinery;
Daemar still owns B5/B6, timeout policy, cleanup proof, and a Go/CLI boundary. It
does not beat Microsandbox or BoxLite for this Rust repository.

## Other options and why they do not change the decision

- **Tupper** is MIT and offers an E2B-style TypeScript SDK over Apple
  `container`, with experimental Firecracker support. It is explicitly early
  development and currently adds generic command/file APIs, not Daemar's
  worktree transaction
  ([official repository](https://github.com/lightbearco/tupper)). It would wrap
  the same incumbent rather than remove the hard code.
- **Bhatti v2** is Apache-2.0, supports Apple Silicon HVF and Linux KVM, and has
  strong exec/lifecycle APIs. Its documented default gives every VM public
  internet access and same-owner sibling reachability; private/host ranges are
  blocked, but there is no documented no-interface/no-egress posture
  ([network architecture](https://bhatti.sh/docs/under-the-hood/networking/)).
  It fails B2 and is optimized for persistent multi-tenant machines, not
  one-shot worktree transactions.
- **E2B** is a mature Apache-2.0 Firecracker sandbox service, but self-hosting is
  Terraform infrastructure on AWS/GCP rather than a local Apple Silicon runtime,
  and its SDK/files API has no B3-B6 equivalent
  ([self-hosting surface](https://github.com/e2b-dev/E2B#self-hosting)).
- **OpenSandbox** is Apache-2.0 and has rich execution, filesystem, timeout, and
  egress APIs. Its local backend is Docker; strong isolation depends on separately
  configuring gVisor, Kata, or Firecracker, and it does not supply the worktree
  transaction
  ([architecture](https://github.com/alibaba/OpenSandbox/blob/main/docs/architecture.md)).
- **Kubernetes SIG Agent Sandbox** is a control plane, not an isolation runtime.
  Its threat model explicitly says callers must select a secure runtime such as
  gVisor or Kata, while its managed network policy permits public internet
  egress
  ([threat model](https://github.com/kubernetes-sigs/agent-sandbox/blob/main/docs/security/threat_model.md)).
- **Daytona** is AGPL-3.0 with substantial sandbox orchestration, but its local
  open-source stack is Docker/Docker Compose-oriented and has no exact private
  worktree export/promotion contract
  ([repository](https://github.com/daytonaio/daytona)).
- **Clawker and similar Docker-agent wrappers** may be useful operationally but
  use shared-kernel containers and therefore fail B1 even when their egress
  controls are strong
  ([Clawker](https://github.com/schmitthub/clawker)).
- **Docker Sandboxes** is the closest polished commercial comparator: a microVM,
  deny-by-default proxy policy, and opt-in clone mode with a read-only source
  mount. Clone mode returns work through Git, not a complete filesystem delta,
  and the release repository explicitly says the product is proprietary
  ([isolation model](https://docs.docker.com/ai/sandboxes/security/isolation/),
  [license](https://github.com/docker/sbx-releases#license)). It is not an
  open-source answer and requires Docker sign-in, although it is free to use.

## Maintainability and security-ownership comparison

| Path | Security-sensitive code Daemar still owns | Upstream risk | Assessment |
|---|---|---|---|
| Keep current Apple `container` implementation | Driver shell protocol, VM CLI supervision, network flag posture, overlay export, change parsing, promotion, timeout, cleanup; plus a future Linux backend | Apple runtime is staffed, but Daemar owns all composition | Highest bespoke ownership and future duplication |
| Adopt Wreckroom wholesale/fork | Audit/fix its apply, timeout, cleanup, result protocol; track Microsandbox too | Four-commit unaudited alpha, apparent single-maintainer bus factor | Smallest feature gap, not smallest present risk |
| Microsandbox substrate + Daemar transaction | Overlay assembly/export, exact `ChangeSet`, sanitized apply, remove-on-all-paths, adapter battery | Beta; one unpatched moderate secret advisory; active multi-contributor releases | **Best current balance** |
| OpenShell microVM + Daemar transaction | Force/verify VM backend, exact workspace import/export/promotion, cleanup proof | Strong organizational activity and policy process, but 0.0.x and VM driver is explicitly experimental | Best upstream policy ownership; more machinery and immature B1 backend |
| BoxLite substrate + Daemar transaction | Same transaction and cleanup wrapper | Pre-1.0; two prior critical host-write bugs, fixed in 0.9.0; active formal process | Strong alternative if release discipline wins |
| ArcBox + Daemar transaction | Import/export/diff/promotion, cleanup wrapper, separate Linux backend | Public beta; M3+ floor; Linux not available | Reject as strategic platform fit |
| Shuru + custom export | Export protocol, promotion, timeout, Linux backend | macOS-first, experimental Linux ARM64 | Creates platform split |
| Matchlock + custom transaction | Exact export/promotion, timeout/cleanup proof, language boundary | Experimental | No clear advantage over Rust-native choices |

## Recommended next decision, not a migration commitment

Run three deliberately small, non-production evaluations:

1. **Microsandbox adapter spike:** implement only `RunSpec -> RunOutcome` plus a
   read-only worktree mount and private upper layer. Reuse Daemar's existing
   `ChangeSet::apply_to`; do not enable secrets, public networking, detached
   sandboxes, snapshots, port forwarding, or general file-copy APIs. Pin an exact
   version/digest. Run the existing B1-B10 battery unchanged, including raw-IP
   probes and leak checks. The spike succeeds only if it deletes more trusted code
   than it adds and gives a credible Linux path.
2. **OpenShell microVM comparator:** configure only the microVM backend and the
   smallest deny-all policy, then run the same battery. Measure whether its
   richer policy engine actually deletes security-sensitive Daemar code or
   merely moves it into configuration and generated gRPC integration. Failure
   to prove the microVM backend on every run is a B1 failure.
3. **Wreckroom upstream probe:** ask whether the maintainer will accept a
   one-shot machine-readable mode with a wall timeout, unconditional discard,
   privileged-bit stripping, and a stable export/result contract. If those land
   upstream with tests and release discipline, Wreckroom could become the first
   genuine end-to-end replacement. Until then, treat its source as a useful
   independent implementation of the same design, not as a security authority.

Keep **BoxLite 0.9+** as the comparator in the spike. Its Rust API and broader
maintenance activity may outweigh Microsandbox's smaller integration if both
pass the battery. Do not choose by README or star count; choose the smallest
pinned integration that passes Daemar's adversarial behaviors and leaves the
least security-sensitive code without a credible upstream owner.

## Final answer to the motivating concern

The effort is not proving that Daemar invented an unnecessary category. The
fresh market evidence shows the opposite: several projects have independently
converged on microVMs, no-interface networking, read-only source mounts, private
overlays, and explicit review. **Wreckroom has converged almost exactly on
Daemar's design.**

What is still uncommon is packaging all of those properties into a mature,
audited, embeddable transaction with exact promotion semantics. Daemar should
stop owning generic VM machinery where a maintained Rust substrate can take it,
but it should not surrender the small, legible promotion boundary to an
unaudited alpha merely to reduce line count.
