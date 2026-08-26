# Current open-source sandbox-runtime survey

Research date: 2026-08-26

## Decision summary

There is no open-source drop-in that satisfies Daemar's complete sandbox contract. The missing capability is consistent across the field: general-purpose runtimes isolate and execute a workload, but they do not define an exact added/modified/deleted worktree changeset and safely promote that changeset back to the host. Daemar should expect to retain ownership of B5 and B6.

That does **not** justify owning a VM stack. The security-sensitive responsibility should be divided deliberately:

- A maintained runtime should own VM creation, the guest kernel, host/guest transport, image/root-filesystem handling, network removal, process I/O, termination, and VM cleanup.
- Daemar should own the software-factory policy: which source tree is exposed, how a run's changes are measured, what paths and file types are admissible, and the explicit promotion transaction.

The best current technical fit is **microsandbox**, subject to a proof against B1-B10 and explicit security qualification. It is the only reviewed candidate that combines current macOS and Linux support, a Rust SDK, a documented no-NIC mode, host-side command timeouts, lifecycle cleanup, and private writable filesystems. It can remove substantially more trusted custom machinery than a raw VMM. Its main caveat is governance: its own security policy calls the team small, and it has already published a moderate advisory. That is evidence of a disclosure process, but not the same assurance as a long-lived, independently audited runtime.

**Apple `container` remains the lower-migration, stronger-maintenance baseline for current macOS.** It has an active signed-release stream and Apple-maintained virtualization substrate, but its CLI supports macOS only and its current supported host requirement is macOS 26 on Apple silicon. Apple `containerization` now has a Linux backend at the library level, but adopting that library directly would replace a supported CLI with lower-level Swift integration and make Daemar responsible for more security-sensitive assembly.

**Shuru is the closest behavioral prototype, not yet the security-ownership recommendation.** Its default read-only host mount plus guest tmpfs overlay directly matches B3/B4, it is offline by default, and every run is ephemeral. However, it has no documented exact overlay export or safe promotion API, no Rust SDK, a short public history, and Linux support is explicitly experimental. Until those gaps are resolved, it relocates important complexity more than it removes it.

**Firecracker and Kata are credible Linux foundations, not current cross-platform replacements.** Firecracker has the strongest explicit release/security discipline in this set, but it is a VMM: using it directly would make Daemar own the jail, network, guest agent, storage, and cleanup. Kata owns more of that stack but assumes a Linux/containerd deployment and a monthly rolling-upgrade security model. Both are Linux-only host choices.

Docker Sandboxes is a useful contract comparison, but the full usable product is explicitly proprietary and login-gated. Open-source components around it do not make it an open-source option.

## Method and limits

This survey independently checked current upstream repositories, product documentation, release pages, security policies, threat models, and published advisories. Existing Daemar research notes were not used as evidence. “No documented capability” means it was not found in the reviewed public primary material; it is not proof that an undocumented internal mechanism is impossible. Likewise, repository activity and policies are maintenance signals, not a security audit.

## Required behavior and scoring

The required behavior comes directly from [`specs/sandbox.md`](../../specs/sandbox.md):

| ID | Requirement |
|---|---|
| B1 | VM boundary with its own kernel |
| B2 | No network egress, including raw IP attempts |
| B3 | Host worktree remains byte-identical after the run |
| B4 | Agent gets a private editable project view |
| B5 | Exact added/modified/deleted changes are reported |
| B6 | Promotion is explicit and rejects path/symlink escapes while stripping setuid/setgid |
| B7 | Host files outside the declared worktree are unreadable |
| B8 | Timed-out work is killed |
| B9 | No runtime or temporary state remains after completion |
| B10 | stdout, stderr, and exit status are faithfully returned |

The table distinguishes native capability from work Daemar must compose. `Native` means the documented product directly supplies the behavior. `Compose` means its primitives can support the behavior, but Daemar must implement and test policy or orchestration. `No` means the candidate violates the requirement or does not support the host.

| Candidate | macOS Apple silicon | Linux | B1/B2 | B3/B4/B7 | B5/B6 | B8/B9/B10 | Full usable product open source? |
|---|---|---|---|---|---|---|---|
| microsandbox | Native | Native | Native | Compose | Daemar | Native/compose | Yes: CLI, server, Rust core/SDK |
| Apple `container` | Native, macOS 26+ | No | Native | Compose | Daemar | Native/compose | Yes: full CLI; `containerization` is also OSS |
| Shuru | Native, macOS 14+ | Experimental ARM64 | Native | Native | Daemar | Native/compose | Yes: CLI and TypeScript SDK |
| containerd nerdbox | Experimental | Experimental | Compose | Compose | Daemar | Compose | Source available under Apache-2.0, but experimental and unreleased |
| Kata Containers | No | Native | Native/compose | Compose | Daemar | Native/compose | Yes: runtime stack; requires Linux/containerd integration |
| Firecracker | No | Native | Compose | Compose | Daemar | Compose | VMM component is OSS; not a complete sandbox product |
| Lima | Native | Native | Compose | Compose | Daemar | Compose | Yes: general VM manager, not an agent sandbox |
| gVisor/runsc | No | Native | **No B1** | Compose | Daemar | Native/compose | Yes: OCI runtime, but not a VM boundary |
| libkrun/krunvm | Native | Native | Compose | Compose | Daemar | Compose | OSS components/tools; libkrun alone is not a sandbox product |
| Docker Sandboxes | Native | Native | Native | Native/compose | Daemar | Compose | **No: full product is proprietary** |

## Ranked technical shortlist

This is a technical ranking, not a schedule or business-priority decision.

### 1. microsandbox: best opportunity to shrink trusted custom code

Microsandbox describes a local microVM runtime for macOS, Linux, and Windows, with OCI images, daemonless/embeddable operation, and Rust, Python, Node, and Go SDK surfaces. Its current release stream includes signed, immutable multi-platform artifacts; v0.6.8 was released on 2026-07-29. The Rust SDK exposes network disabling, lifecycle operations, idle timeouts, and filesystem access. Its command transport is independent of guest networking and returns exit status/stdout/stderr, supports streaming, and applies per-execution timeouts from the host ([project README](https://github.com/superradcompany/microsandbox/blob/main/README.md), [Rust sandbox API](https://github.com/superradcompany/microsandbox/blob/main/docs/sdk/rust/sandbox.mdx), [command API](https://github.com/superradcompany/microsandbox/blob/main/docs/sandboxes/commands.mdx), [v0.6.8 release](https://github.com/superradcompany/microsandbox/releases/tag/v0.6.8)).

The relevant security properties are unusually explicit for a young project. `--no-net`/`disable_network` omits the guest NIC rather than relying only on a proxy policy. Root filesystems are private, image layers are read-only with a sandbox-specific writable upper layer, and host mounts are opt-in. The mount broker documents Linux `openat2(RESOLVE_BENEATH)` containment and a weaker macOS `O_NOFOLLOW` fallback ([CLI sandbox options](https://github.com/superradcompany/microsandbox/blob/main/docs/cli/sandbox-commands.mdx), [filesystem security](https://github.com/superradcompany/microsandbox/blob/main/docs/security/filesystem.mdx)). That macOS difference must be in Daemar's adversarial battery; it should not be assumed equivalent.

What it can remove from Daemar:

- VM/VMM selection and launch on both target hosts.
- OCI image/rootfs assembly and the guest agent channel.
- Network-device omission, command execution/streaming, timeout termination, stop/kill, and most instance-state cleanup.
- Much of the platform-specific Rust/CLI process driving.

What Daemar still owns:

- Producing an editable project view without host mutation. The safest documented composition is staging the project into the sandbox-private filesystem rather than trusting a writable host mount.
- Enumerating exact A/M/D changes, including metadata and symlink semantics.
- The complete B6 validation and promotion transaction.
- Proving that its selected cleanup path removes every staged host artifact, not only microsandbox state.

Security/governance qualification matters. Microsandbox has a published security policy with private reporting, response targets, and explicitly scoped escape/network/secret/supply-chain classes. It also says plainly that it is a small team. A moderate 2026 advisory disclosed secrets placed in world-readable process arguments. Releases are automated, signed, checksummed, and published across platforms, but the project remains pre-1.0 ([security policy and advisories](https://github.com/superradcompany/microsandbox/security), [release process](https://github.com/superradcompany/microsandbox/blob/main/DEVELOPMENT.md)). The advisory is not a reason to reject it; it is a reason to pin a patched version, inspect the fix, and keep Daemar's security tests independent of upstream claims.

### 2. Apple `container`: strongest current macOS maintenance baseline

Apple `container` is a complete Apache-2.0 Swift CLI that runs each OCI Linux container in its own lightweight VM on Apple silicon. Foreground runs return process I/O, `--rm` removes a stopped container, stop has a configurable timeout before kill, root filesystems can be read-only, and mount sources are explicit. `--network none` is supported and documented by the maintainers as leaving loopback only ([README](https://github.com/apple/container/blob/main/README.md), [command reference](https://github.com/apple/container/blob/main/docs/command-reference.md), [`--network none` maintainer explanation](https://github.com/apple/container/discussions/743)).

Its operational limitation is material: the current supported requirement is Apple silicon on macOS 26. Earlier macOS versions are explicitly unsupported. The CLI is not a Linux solution ([requirements](https://github.com/apple/container/blob/main/README.md#requirements)).

`container` delegates to the open-source `containerization` Swift package. That package exposes per-container VMs, a vsock/gRPC guest agent, process I/O/signals/events, writable image layers, and Virtualization.framework on macOS. Its main branch now contains an experimental Linux cloud-hypervisor/KVM backend, but Linux callers must supply cloud-hypervisor, `virtiofsd`, KVM access, and networking; the full test suite remains macOS-only ([containerization README](https://github.com/apple/containerization/blob/main/README.md), [`LinuxContainer` API](https://github.com/apple/containerization/blob/main/Sources/Containerization/LinuxContainer.swift)). This makes the library an open-source component, not evidence that the supported `container` product is cross-platform.

What the current CLI removes is the VM, OCI, network, and much lifecycle machinery. Daemar still needs its private source staging/overlay, exact change enumeration, promotion validator, per-job timeout supervision, and cleanup of Daemar-created temporary storage. Moving from the CLI to `containerization` would expose more control but also transfer lower-level security ownership into Daemar and add a Swift boundary. That is contrary to the goal unless a narrowly demonstrated capability cannot be reached through the CLI.

Apple's release record is the strongest current macOS signal among the cross-platform-adjacent candidates: multiple maintainers/contributors, signed releases, and public security response. For example, a path-traversal bug in image archive extraction received CVE-2026-20613 and was fixed in a release; the advisory documents scope and proof of concept ([Apple releases](https://github.com/apple/container/releases), [GHSA-cq3j-qj2h-6rv3](https://github.com/apple/containerization/security/advisories/GHSA-cq3j-qj2h-6rv3)). That does not prove the runtime safe, but it provides an auditable patch/disclosure trail.

### 3. Shuru: closest B3/B4 behavior, higher project risk

Shuru is an Apache-2.0 local microVM sandbox built specifically for coding agents. On macOS 14+ Apple silicon it creates an ephemeral VM per run, disables networking by default, mounts host directories read-only by default, and redirects guest writes into a tmpfs overlay that is discarded when the VM exits. Direct host writes require both a `:rw` mount and a separate `--allow-host-writes` opt-in ([README](https://github.com/superhq-ai/shuru/blob/main/README.md), [v0.6.3 release](https://github.com/superhq-ai/shuru/releases/tag/v0.6.3)). Those defaults are the closest off-the-shelf match for B1-B4, B7, and B9.

Its TypeScript SDK returns stdout/stderr/exit code, supports streamed processes and killing, exposes file/stat/directory/symlink operations, and can watch filesystem events ([SDK README](https://github.com/superhq-ai/shuru/blob/main/packages/sdk/README.md)). However, filesystem watch events are not an exact changeset: watches can begin late, events can coalesce, and they do not by themselves define final content/metadata. The documented overlay is discarded at stop and has no documented complete export interface. B5/B6 therefore remain Daemar work, and implementing them may require dependence on Shuru internals or an upstream feature rather than a clean adapter.

Security ownership is the reason Shuru ranks below microsandbox despite the superior mount behavior. The public project is young, the only documented SDK is TypeScript/Bun, Linux is explicitly experimental ARM64-only and “not ready for production,” and the reviewed repository materials do not provide a threat model or security policy comparable to microsandbox, Kata, or Firecracker. This is a viable prototype target, but adopting it today would exchange Daemar's visible complexity for a smaller, less established upstream plus a non-Rust integration boundary.

### 4. containerd nerdbox: promising architecture, not release-ready

Nerdbox is an Apache-2.0, non-core containerd subproject implementing one VM per container, rootless by default, with EROFS and libkrun on macOS and Linux. Its cross-platform containerd shim design could eventually provide a standard lifecycle and snapshot boundary without a bespoke Daemar VMM ([README](https://github.com/containerd/nerdbox/blob/main/README.md)).

The current project labels itself experimental, has no releases, and its macOS instructions require assembling containerd 2.2+, an EROFS snapshotter/toolchain, libkrun, and host-specific configuration. Containerd itself has mature security guidance and patch releases, but its operator guide stresses that plugins are in containerd's trusted computing base ([containerd operator security guidance](https://github.com/containerd/containerd/blob/main/docs/security/OPERATOR_GUIDELINES.md)). Nerdbox therefore does not yet reduce operational or audit burden enough. It also inherits libkrun's host-context concerns and supplies no B5/B6 contract.

It is worth tracking because its architecture targets the correct macOS-to-Linux seam. It should not currently displace a released substrate.

### 5. Linux-specific mature choices: Kata or Firecracker

Kata Containers is the higher-level Linux option. It supplies a VM-backed OCI/containerd runtime, a guest agent, image/container lifecycle, and multiple hypervisor backends. It has an explicit threat model in which an untrusted workload must not read or alter host/infrastructure assets, and it documents shared-filesystem tradeoffs ([installation](https://github.com/kata-containers/kata-containers/blob/main/docs/installation.md), [virtualization design](https://github.com/kata-containers/kata-containers/blob/main/docs/design/virtualization.md), [threat model](https://github.com/kata-containers/documentation/blob/master/design/threat-model/threat-model.md), [virtio-fs guidance](https://github.com/kata-containers/kata-containers/blob/main/docs/how-to/how-to-use-virtio-fs-with-kata.md)). Daemar would still own worktree isolation policy, exact changes, and promotion, but not the VMM/guest-agent stack.

Kata's cost is deployment and patch discipline: it requires Linux hardware virtualization plus containerd/CRI-style integration, and only the current monthly release receives security fixes. Older releases are unsupported rather than backported ([security policy](https://github.com/kata-containers/kata-containers/security/policy), [security advisories](https://github.com/kata-containers/kata-containers/security/advisories)). That can be a sound model if automated upgrades and requalification are acceptable; it is not a low-operations library dependency.

Firecracker is the lower-level, stronger-auditability VMM choice. It is Linux/KVM-only on x86_64 and aarch64, has a documented threat model, minimal emulated-device surface, seccomp, a jailer, an OpenAPI control API, a published release-support policy, and AWS-maintained release/security ownership ([getting started](https://github.com/firecracker-microvm/firecracker/blob/main/docs/getting-started.md), [design](https://github.com/firecracker-microvm/firecracker/blob/main/docs/design.md), [production host setup](https://github.com/firecracker-microvm/firecracker/blob/main/docs/prod-host-setup.md), [release policy](https://github.com/firecracker-microvm/firecracker/blob/main/docs/RELEASE_POLICY.md)).

Direct Firecracker adoption is nevertheless the wrong way to reduce Daemar's custom security burden. Firecracker intentionally leaves TAP/network filtering, rootfs/block-device production, guest command transport, host-path sharing, instance supervision, and cleanup to the operator. Its production guide explicitly says Firecracker does not filter network traffic and recommends the jailer or a stronger containment boundary. Daemar would be building a sandbox service around a VMM. Firecracker is appropriate only underneath an established orchestration layer or if owning that layer becomes an explicit product decision.

## Required candidates not shortlisted

### Docker Sandboxes: close behavior, proprietary product

Docker Sandboxes provides a microVM with its own kernel and Docker daemon, deny-by-default outbound TCP proxying, blocked direct UDP/ICMP, and a clone mode where the host source is read-only and work occurs in a private clone. It supports macOS Apple silicon and Linux/KVM ([architecture](https://docs.docker.com/ai/sandboxes/architecture/), [security](https://docs.docker.com/ai/sandboxes/security/), [Git workflow](https://docs.docker.com/ai/sandboxes/workflows/git/), [installation](https://docs.docker.com/ai/sandboxes/install/)).

The full usable product is not open source. Docker's public release repository labels the license “Proprietary — Docker Inc.”, distributes binaries, and the local product requires Docker sign-in ([release repository](https://github.com/docker/sbx-releases/blob/main/README.md), [login requirement issue](https://github.com/docker/sbx-releases/issues/321)). Its Git handoff also does not guarantee B5 for arbitrary uncommitted/untracked outputs, promotion is Git-oriented rather than Daemar's B6 transaction, and sandboxes persist until removed. It is a useful design reference, not an OSS candidate.

### Lima: general VM manager, unsafe defaults for this contract

Lima is a mature Apache-2.0 Linux VM manager using Apple's Virtualization.framework on macOS and QEMU on Linux. Host home is mounted read-only by default, but networking, DNS/host resolution, proxy propagation, SSH, and a persistent VM are normal product features ([VZ backend](https://lima-vm.io/docs/config/vmtype/vz/), [VM types](https://lima-vm.io/docs/config/vmtype/), [usage and mounts](https://lima-vm.io/docs/usage/), [default template](https://github.com/lima-vm/lima/blob/master/templates/default.yaml)).

A hardened Daemar template could disable mounts, forwarding, and guest-agent conveniences, but Daemar would still need image provisioning, private staging, egress proof, command supervision, changesets, promotion, and per-run destruction. Lima replaces VM boot; it does not replace the sandbox policy, and its permissive/default-connected posture increases the chance of configuration regression.

### gVisor/runsc: good isolation, wrong specified boundary

gVisor is an Apache-2.0 OCI runtime for Linux x86_64/ARM64 that intercepts workload system calls in a userspace application kernel. It supports private overlay filesystems and integrates with Docker/containerd/Kubernetes, but it is explicitly not a virtual machine and network policy remains external ([project README](https://github.com/google/gvisor), [security architecture](https://gvisor.dev/docs/architecture_guide/security/), [filesystem guide](https://gvisor.dev/docs/user_guide/filesystem/)). It therefore fails literal B1 and has no macOS host path. It becomes relevant only if the specification intentionally changes from a VM boundary to a userspace-kernel boundary.

### libkrun and krunvm: components with sharp security edges

Libkrun is an Apache-2.0 Rust VMM library with KVM on Linux and Hypervisor.framework on Apple silicon macOS; krunvm is an OCI-oriented CLI around it. The library's public integration surface is C, despite its Rust implementation ([libkrun README](https://github.com/containers/libkrun), [krunvm](https://github.com/containers/krunvm)).

Its own security documentation is decisive: guest and VMM share a security context; virtio-fs does not prevent access to other host paths without host mount-namespace isolation; and if no virtual NIC is added, Transparent Socket Impersonation is automatically enabled and can proxy guest IPv4/IPv6 sockets. “No NIC” is therefore **not** B2 when using libkrun directly. A higher-level runtime such as microsandbox can configure/contain these behaviors, but direct libkrun adoption would make Daemar own them. This is a VMM component, not a safe-by-default sandbox product.

### Other screened projects

- [Gondolin](https://github.com/earendil-works/gondolin) is an Apache-2.0, programmable TypeScript/QEMU/libkrun agent microVM toolkit for macOS and Linux. It is explicitly experimental and offers policy hooks rather than Daemar's exact worktree/promotion contract. It does not improve the security-ownership trade enough over microsandbox or Shuru.
- [krunai](https://github.com/slp/krunai) is an Apache-2.0 Rust/libkrun CLI for AI-agent VMs on macOS; Linux support is described as forthcoming, projects are mounted read-write, networking is enabled by default, and instances are SSH-oriented/persistent. It conflicts with B2, B3, and B9 defaults.
- [Redan](https://github.com/getredan/redan) is a BSD-3-Clause Linux/libkrun agent sandbox with default-deny networking, but it is alpha/unaudited, Linux-only, and uses a writable project mount with Git as the safety net. It does not meet the current host or promotion contract.

## Security ownership comparison

The most important architectural question is not “which tool boots a VM?” It is “who owns each failure that could expose or corrupt the host?”

| Layer | Apple `container` | microsandbox | Shuru | Kata | Firecracker/libkrun direct |
|---|---|---|---|---|---|
| Hypervisor/VMM and guest kernel | Upstream | Upstream | Upstream | Upstream | Upstream |
| Guest agent / command transport | Upstream | Upstream | Upstream | Upstream | **Daemar** |
| OCI/rootfs lifecycle | Upstream | Upstream | Upstream | Upstream | **Daemar** |
| No-egress construction | Upstream mode; Daemar verifies | Upstream mode; Daemar verifies | Upstream default; Daemar verifies | Daemar configures upstream primitives | **Daemar constructs and verifies** |
| Host-path containment | Daemar staging/mount policy | Upstream broker plus Daemar staging policy | Upstream read-only-overlay default | Daemar configures sharing | **Daemar constructs** |
| Timeout/kill and instance cleanup | Shared | Mostly upstream | Mostly upstream | Shared | **Daemar** |
| Exact changes and safe promotion | **Daemar** | **Daemar** | **Daemar** | **Daemar** | **Daemar** |
| Patch/release security process | Strong, active | Documented but small/pre-1.0 | Limited public assurance | Mature, rolling monthly | Strong for Firecracker; component-only for libkrun |

This favors a released, higher-level runtime even when a raw VMM is technically elegant. Every new Daemar implementation of path brokering, virtual networking, guest transport, image extraction, or teardown becomes security-critical code that this project must audit indefinitely.

### Upstream-assurance evidence

| Candidate | Maintainer/release evidence | Security evidence | Confidence implication |
|---|---|---|---|
| Apple `container` | Active Apple project, several named contributors in recent signed releases | Public advisory/CVE trail and prompt release remediation | Strongest current macOS maintenance signal; no claim of formal independent audit |
| microsandbox | Automated, signed/checksummed multi-platform releases; project policy explicitly calls the team small | Specific disclosure scope and response targets; one published moderate advisory | Good transparency and packaging discipline, but concentrated pre-1.0 ownership remains a real dependency risk |
| Shuru | Released pre-1.0 product with a visibly shorter public history | No comparable public threat model/security policy was found in reviewed primary material | Treat as an inspectable prototype, not a reason to outsource security confidence |
| nerdbox | Containerd non-core project, experimental, no releases | Inherits containerd's mature policy only partially; shim/plugins are inside the containerd TCB | Parent-project reputation does not substitute for a released, qualified shim |
| Kata | Established community runtime with monthly rolling releases | Explicit threat model, reporting policy, advisories; only newest monthly release receives fixes | Strong upstream ownership if Daemar accepts continuous upgrade/requalification |
| Firecracker | AWS-owned maintainership, releases generally every two or three months, explicit support windows | Threat model, jailer/seccomp guidance, private disclosure process | Strong VMM assurance, but it leaves too much of the complete sandbox TCB to Daemar |

None of the reviewed candidate materials established an independent audit covering Daemar's exact composition. The qualification target is therefore the **assembled system**, not the upstream name: version, VMM, kernel/rootfs, network configuration, mounts, guest transport, and Daemar's promotion code all belong in the evaluated boundary.

## Evaluation gates before changing substrate

No candidate should be accepted from documentation alone. A bounded proof should run the existing B1-B10 suite plus adversarial cases against a pinned upstream version:

1. Attempt DNS, TCP/UDP, raw IP, loopback escape, IPv6, Unix/vsock bridges, host gateways, and inherited proxy settings while the no-network mode is active.
2. Probe `..`, absolute paths, symlink chains, hard links, device/FIFO/socket files, case-folding, Unicode normalization, mount crossing, setuid/setgid, and races during changeset capture/promotion.
3. Kill the Daemar parent, the runtime CLI, the guest process, and the VM at each lifecycle phase; inventory VMs, mounts, processes, sockets, containerd state, and temporary files afterward.
4. Saturate stdout/stderr, close stdin, return signals/nonzero exits, fork descendants, and exceed both command and whole-run deadlines.
5. Verify the host worktree byte-for-byte before promotion and verify exact A/M/D output independently of runtime event streams.
6. Record the runtime binary/source digest, guest kernel/rootfs digest, configuration, and security-advisory state in every qualification result.

The decisive proof for microsandbox is whether its documented private filesystem plus Rust API lets Daemar implement B3-B6 without a writable host mount or dependence on undocumented internals. The decisive proof for Shuru is whether the tmpfs overlay can be exported completely and safely before teardown through a stable public API. If either requires patching VMM/network/filesystem internals, the apparent reduction in custom code is illusory.

## Bottom line

The rapid-development path is not to replace Daemar with a general sandbox product, and it is not to keep expanding a bespoke VM runtime. It is to constrain Daemar to the factory-specific seam:

```text
maintained sandbox runtime
    owns VM + image + no-egress + guest execution + kill/cleanup
                  |
                  v
Daemar changeset boundary
    owns private source staging + exact A/M/D + validated promotion
```

Microsandbox is presently the best candidate to test against that boundary. Apple `container` is the strongest conservative macOS baseline while that proof is conducted. Shuru is the most interesting behavioral comparator and potential upstream collaborator. For Linux, Kata is the mature integrated alternative and Firecracker the mature low-level component, but neither should be used to justify rebuilding the orchestration layer inside Daemar.
