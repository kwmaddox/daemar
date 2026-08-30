# Nono sandbox assessment

_Assessed 2026-08-29 against the official `nolabs-ai/nono` repository at commit [`bda1e7e`](https://github.com/nolabs-ai/nono/commit/bda1e7e69172e8a07cc2e0b4768fdd62734e67f9), plus current upstream documentation, releases, advisories, and Linux kernel documentation. “Verified” means confirmed in a primary source or the local trial described below; “inference” and “untested” are labeled._

## Bottom line

Nono is a serious, low-overhead **same-host capability layer**, not a VM-grade isolation boundary. Its useful differentiator for coding agents is not merely “can this process read the repo?” but brokered per-tool sandboxes: `git`, `curl`, `kubectl`, and chained tools can each receive different filesystem, network, environment, credential, argument, output, and resource grants. The kernel enforces each child’s resulting policy.

It should **not replace a local microVM where the threat model includes hostile generated code, kernel exploitation, policy/supervisor bugs, multi-tenancy, or high-value host credentials**. Upstream says the same: Nono shares the host kernel and user account, calls itself a capability model rather than a VM model, and recommends a container or microVM as the perimeter for hostile-code execution. It is a strong candidate for a complementary layer inside a microVM and a promising convenience/security layer for lower-risk local agent work. [Security model](https://nono.sh/docs/cli/internals/security-model#capability-model-vs-isolation-model), [composition guidance](https://nono.sh/docs/cli/internals/security-model#composing-with-an-outer-boundary)

Recommendation: run a bounded pilot on **v0.74.0 or newer**, retain the microVM for high-risk work, and make promotion contingent on adversarial tests of the exact profiles, host OS versions, and agent/tool chains. Nono is still explicitly alpha; upstream says its guarantees are unstable and production use is not recommended. [Security policy](https://github.com/nolabs-ai/nono/blob/bda1e7e69172e8a07cc2e0b4768fdd62734e67f9/SECURITY.md)

## Boundary and threat model

Verified:

- The restricted process and its descendants remain on the real host, with the real filesystem namespace and shared kernel. Nono reduces their ambient authority; it does not create a guest OS, virtual disk, user namespace, PID namespace, or hardware boundary. [Upstream security model](https://nono.sh/docs/cli/internals/security-model#sandboxing)
- The host kernel, Nono supervisor/broker, and selected profiles/command policies are trusted. The supervisor remains outside child sandboxes and runs with the invoking user’s ordinary host authority. A broker or policy bypass can therefore regain that user’s authority, though not root unless the user already had it. [Trust boundaries](https://nono.sh/docs/cli/internals/security-model#trust-boundaries)
- Intended protections are least-privilege access to files, network endpoints, credentials, processes/IPC, and delegated tools. The agent and tool inputs are untrusted to choose policy, but profiles and approval backends decide what is granted.
- Applied Landlock/Seatbelt restrictions are inherited by descendants and cannot be removed by the child. The supervisor may deliberately create a _new_ child with broader or different authority, or broker an approved action. [Capability expansion](https://nono.sh/docs/cli/internals/security-model#capability-expansion-and-irreversibility)

Out of scope or residual risk:

- A kernel/Seatbelt/Landlock/seccomp vulnerability, a Nono supervisor/proxy flaw, an unsafe profile, or host-side persistence through a writable directory on `PATH` can defeat the desired boundary. Upstream explicitly warns that Nono cannot stop another unsandboxed host process from later executing a binary planted in a writable `PATH` directory. [`--strict-broker-path` discussion](https://nono.sh/docs/cli/internals/security-model#host-side-persistence-risk-separate-from-broker-safety)
- Linux Landlock permits some metadata reconnaissance: upstream documents that `stat`/`access` are not trapped, so file existence/type/permission may remain observable even when contents are not. This matters more for actively adversarial code than for a cooperative coding agent. [Information-leak surface](https://nono.sh/docs/cli/internals/security-model#information-leak-surface)
- This is **confinement, not disposable-state isolation**. If a profile grants write access to a worktree, the agent can corrupt that worktree. Rollback may improve recovery but is not a substitute for an outer ephemeral disk/snapshot boundary.

## Enforcement by platform

| Property | Linux | macOS |
|---|---|---|
| Primary filesystem mechanism | Landlock `PathBeneath` allow rules followed by `PR_SET_NO_NEW_PRIVS` and `landlock_restrict_self()` | Generated Seatbelt profile beginning with `(deny default)`, applied through `sandbox_init()` |
| Network mechanism | Landlock TCP port rules where available; static seccomp and seccomp-notify/supervisor layers for block/proxy/AF_UNIX cases | Seatbelt `deny network*`; proxy-only permits the local proxy endpoint; explicit AF_UNIX rules |
| Child inheritance | Landlock and seccomp restrictions inherit across fork/clone/exec | Seatbelt policy inherits across fork/exec |
| Resource ceilings | cgroup v2 for memory and process count in supervised mode | Not equivalent in the assessed source |
| Kernel/version dependency | Linux 5.13+ for Landlock baseline; capabilities vary by detected ABI | Modern macOS Seatbelt; implementation calls Apple’s `sandbox_init()` API |

Sources: [Linux implementation](https://github.com/nolabs-ai/nono/blob/bda1e7e69172e8a07cc2e0b4768fdd62734e67f9/crates/nono/src/sandbox/linux.rs), [macOS implementation](https://github.com/nolabs-ai/nono/blob/bda1e7e69172e8a07cc2e0b4768fdd62734e67f9/crates/nono/src/sandbox/macos.rs), [platform support](https://nono.sh/docs/cli/getting_started/installation#platform-support), [Linux Landlock documentation](https://docs.kernel.org/userspace-api/landlock.html).

### Filesystem mediation and bypass considerations

Verified:

- Directory grants are recursive. Therefore `--allow-cwd` or an equivalent repo grant gives the child authority to damage every writable file beneath that directory. On Linux, Landlock is an allow-list and cannot express “allow parent except this denied child”; keep secrets and Nono state outside a broadly granted tree. macOS Seatbelt can layer targeted denies inside broader grants.
- The bundled macOS runtime baseline is materially broader than the README’s simple “current directory and nothing else” mental model. Its `system_read_macos` group recursively grants read access to `/private`, `/var`, `/tmp`, `/Applications`, `/Library`, `/Volumes`, `/opt`, and other runtime trees. In a separate local v0.74.0 trial, `nono wrap --allow /tmp/nono-trial-allowed --block-net` correctly denied unrelated repo/sensitive paths and writes outside the grant, but it **could read** `/private/tmp/nono-trial-forbidden/secret.txt` because `/private` was an authorized baseline read. This is policy breadth, not a kernel escape, but it means the default macOS profile does not provide confidentiality between sibling `/tmp` trees. [Bundled policy](https://github.com/nolabs-ai/nono/blob/bda1e7e69172e8a07cc2e0b4768fdd62734e67f9/crates/nono-cli/data/policy.json#L145-L184), [README claim](https://github.com/nolabs-ai/nono/blob/bda1e7e69172e8a07cc2e0b4768fdd62734e67f9/README.md#L75-L88)
- Nono canonicalizes paths and, on macOS, emits both original and resolved forms for symlinked locations such as `/tmp` and `/private/tmp`. The Linux implementation opens policy paths with `O_PATH|O_CLOEXEC` before applying the ruleset, reducing path-replacement ambiguity.
- Ordinary inherited descriptors are a classic Landlock caveat because read/write permission is checked when a file is opened, not on every later read/write; Linux documents that pre-policy descriptors retain their associated authority. Nono’s supervised child closes inherited descriptors above stderr except explicitly retained internal descriptors before applying/execing the workload. [Linux FD semantics](https://docs.kernel.org/userspace-api/landlock.html#rights-associated-with-file-descriptors), [Nono descriptor cleanup](https://github.com/nolabs-ai/nono/blob/bda1e7e69172e8a07cc2e0b4768fdd62734e67f9/crates/nono-cli/src/exec_strategy.rs#L1319-L1320)
- Standard input/output/error remain intentionally inherited capability channels. A caller can feed data through stdin and receive anything the child can print. Tool Sandbox optionally mediates and bounds stdout/stderr, but ordinary session execution does not turn terminal I/O into a confidentiality boundary. [Tool output mediation](https://nono.sh/docs/cli/features/tool-sandbox#stdio-output-mediation)
- Linux kernel support is ABI-sensitive. Current Nono source detects and models Landlock V1–V6; older ABIs lack controls added later (truncate V3, TCP V4, device ioctl V5, signal/abstract-socket scoping V6). Nono adds seccomp layers for some missing network/AF_UNIX functions, but this increases the project-specific attack surface. [Nono ABI feature detection](https://github.com/nolabs-ai/nono/blob/bda1e7e69172e8a07cc2e0b4768fdd62734e67f9/crates/nono/src/sandbox/linux.rs#L275-L309), [kernel ABI history](https://docs.kernel.org/userspace-api/landlock.html#previous-limitations)

Inference: the combination of canonicalization, close-on-exec preparation, descriptor cleanup, kernel policy inheritance, and recent AF_UNIX work is materially stronger than a userspace path-interception wrapper. It remains less hermetic than a mount namespace or VM because the host namespace and special kernel objects still exist; correctness depends on mediating every authority-bearing channel.

## Network and local IPC

Verified:

- Network is **allowed by default** in the core `CapabilitySet`, and the current CLI help also describes outbound network as allowed by default. A Nono launch is not automatically a no-egress launch; a profile must explicitly choose block or proxy filtering. [Network modes](https://github.com/nolabs-ai/nono/blob/bda1e7e69172e8a07cc2e0b4768fdd62734e67f9/crates/nono/src/capability.rs#L886-L912)
- `--block-net` is the coarse mode. Domain/endpoint policy runs through a trusted local HTTP proxy; credentials can remain in supervisor memory and be injected upstream instead of entering the agent process. [Proxy model](https://nono.sh/docs/cli/internals/security-model#network-proxy-security-model)
- Linux Landlock V4 TCP policy is port-based, not destination-address-based. v0.74.0 added a destination check after an audit found that a process could connect to any host listening on the proxy port. Raw `tcp_connect_ports` remain port-only by design. [GHSA-6hww-cch7-pfrh](https://github.com/nolabs-ai/nono/security/advisories/GHSA-6hww-cch7-pfrh), [current capability comment](https://github.com/nolabs-ai/nono/blob/bda1e7e69172e8a07cc2e0b4768fdd62734e67f9/crates/nono/src/capability.rs#L964-L973)
- macOS can restrict outbound TCP to `localhost:PORT` for the proxy, but Seatbelt cannot express equivalent raw per-port rules. If a profile enables listening, current source grants `network-bind` and `network-inbound` broadly because Seatbelt cannot filter them by port. [macOS network generation](https://github.com/nolabs-ai/nono/blob/bda1e7e69172e8a07cc2e0b4768fdd62734e67f9/crates/nono/src/sandbox/macos.rs#L800-L889)
- “Network blocked” does not automatically mean “all local IPC blocked.” Nono has explicit AF_UNIX capabilities and mediation. This is security-critical: versions before 0.55.0 allowed a complete Linux escape through the user D-Bus socket and `systemd-run --user`. [GHSA-27vp-2mmc-vmh3](https://github.com/nolabs-ai/nono/security/advisories/GHSA-27vp-2mmc-vmh3)
- On macOS, even blocked/proxy-only modes explicitly permit the `mDNSResponder` Unix socket so DNS resolution works. **Inference:** operators needing a strict “no externally observable lookup” boundary should not equate TCP denial with zero DNS side channel; test the exact profile and resolver behavior. A separate proxy DNS-tunneling bug was fixed in v0.74.0. [macOS source](https://github.com/nolabs-ai/nono/blob/bda1e7e69172e8a07cc2e0b4768fdd62734e67f9/crates/nono/src/sandbox/macos.rs#L800-L818), [GHSA-gcpc-cqvp-h5c8](https://github.com/nolabs-ai/nono/security/advisories/GHSA-gcpc-cqvp-h5c8)

## Subprocesses and per-tool sandboxes

Verified:

- Normal subprocesses are allowed and inherit the parent’s restrictions. This prevents the trivial “spawn a shell to escape” case but does not stop the shell from exercising every capability already granted to the session.
- Tool Sandbox changes that model for selected executables. PATH shims route a command to the supervisor, which launches it in a fresh policy-specific child. That child does **not** inherit the outer session’s broad CWD/filesystem/network grants unless its command policy says so. Chained commands cross the broker again. [Tool Sandbox mental model](https://nono.sh/docs/cli/features/tool-sandbox#mental-model)
- Direct absolute-path execution of a controlled tool is denied unless `allow_direct_exec_bypass` is configured. That setting is explicitly an escape hatch: the direct executable receives outer-session authority rather than its narrow tool sandbox. [Direct-exec bypass](https://nono.sh/docs/cli/features/tool-sandbox#direct-exec-bypass)
- Tool policies are operationally demanding. Compilers, package managers, shell wrappers, interpreters, helper binaries, credential sockets, and daemons form chains that must be modeled. A too-narrow policy breaks real builds; a broad compatibility exception weakens the boundary.

This per-tool broker is the strongest reason to consider Nono **inside** a local microVM: the VM supplies the guest/host perimeter, while Nono limits lateral authority among tools, files, and credentials inside the guest.

## Platform, installation, and runtime cost

Verified:

- Supported: macOS; Linux with Landlock (documented baseline kernel 5.13+); Windows through WSL2. Native Windows is unsupported. WSL2 is documented at 84% feature coverage, so it should be qualified separately rather than treated as native Linux parity. [Installation/platform matrix](https://nono.sh/docs/cli/getting_started/installation#platform-support)
- Installation options include Homebrew, Debian packages, RPM/COPR, Nix, AUR, release binaries, or source. The runtime needs no daemon, root privilege, container runtime, or VM. Source at the assessed commit requires Rust 1.95. [Installation](https://nono.sh/docs/cli/getting_started/installation), [workspace manifest](https://github.com/nolabs-ai/nono/blob/bda1e7e69172e8a07cc2e0b4768fdd62734e67f9/Cargo.toml#L11-L17)
- Linux resource ceilings use cgroup v2 in supervised mode and therefore depend on the host’s cgroup availability/delegation. Network feature strength depends on the detected Landlock ABI; older kernels use narrower functionality and seccomp fallback paths.
- Local macOS no-op trial: the source build completed successfully and reported `nono 0.74.0`; a supervised `/usr/bin/true` run measured **0.04 s real time**. This supports “low startup overhead” on this machine, but it is one warm-cache observation, not a benchmark.

## Maturity and maintenance

Verified:

- The latest immutable release checked was [v0.74.0](https://github.com/nolabs-ai/nono/releases/tag/v0.74.0), released 2026-08-19. GitHub showed 24 commits on `main` since that release; the assessed checkout’s newest commit was dated 2026-08-28. The project is actively changing.
- Upstream labels the project alpha, expects security issues, says guarantees are not stable, and does not recommend production use. Its security policy still lists a comprehensive third-party audit as future pre-1.0 work, but four August advisories say their findings came from an X41 audit sponsored by OSTIF. The policy is therefore stale on this point. No public full report was located during this assessment, so the published advisories verify that an audit occurred but do not establish its complete scope, methodology, or full finding count. [Security policy](https://github.com/nolabs-ai/nono/blob/bda1e7e69172e8a07cc2e0b4768fdd62734e67f9/SECURITY.md), [example X41/OSTIF attribution](https://github.com/nolabs-ai/nono/security/advisories/GHSA-vhq2-h2q7-8mmc)
- v0.74.0 patched four recent audit findings affecting `<=0.73.0`: a **high-severity** x86-64 legacy syscall ABI seccomp bypass ([GHSA-vhq2-h2q7-8mmc](https://github.com/nolabs-ai/nono/security/advisories/GHSA-vhq2-h2q7-8mmc)); a proxy destination bypass caused by Landlock’s port-only rule ([GHSA-6hww-cch7-pfrh](https://github.com/nolabs-ai/nono/security/advisories/GHSA-6hww-cch7-pfrh)); hostname normalization bypasses ([GHSA-7q3j-vfmx-gc9g](https://github.com/nolabs-ai/nono/security/advisories/GHSA-7q3j-vfmx-gc9g)); and DNS tunneling before allowlist rejection ([GHSA-gcpc-cqvp-h5c8](https://github.com/nolabs-ai/nono/security/advisories/GHSA-gcpc-cqvp-h5c8)). Earlier releases fixed the user-D-Bus escape and a registry-pack provenance fail-open ([GHSA-hc4m-q9jh-xw4j](https://github.com/nolabs-ai/nono/security/advisories/GHSA-hc4m-q9jh-xw4j)).

Inference: the disclosure and rapid fixes are positive maintenance signals, and the audit is valuable evidence. The density and recency of boundary failures also make “sole high-assurance sandbox” premature. Pinning a minimum version and tracking advisories are mandatory, not hygiene theater.

## Local harmless trial

Environment: macOS 26.4.1 arm64. The official repository was built from source in `/tmp` at commit `bda1e7e`. Nono was itself blocked when first launched inside Codex’s existing Seatbelt sandbox (`sandbox initialization failed: Operation not permitted`), so the actual test was run outside that outer sandbox with all fixtures/state redirected to `/tmp`.

Using a fully resolved manifest granting read/write only to `/tmp/nono-live/allowed`, granting the minimal system paths needed for `/bin/sh` and `/usr/bin/curl`, and setting network mode to `blocked`:

- direct write inside the allowed directory: **succeeded**;
- nested `/bin/sh` write to sibling `/tmp/nono-live/outside.txt`: **denied with `Operation not permitted`**;
- nested shell exit status for the denied write: **1**;
- `curl -I https://example.com`: **failed to connect**, exit status **7**;
- warm no-op supervised launch: **0.04 s real**.

This verifies basic macOS filesystem enforcement, descendant inheritance, and TCP denial for this exact build/configuration. It does **not** test Linux, proxy allowlists, credential injection, AF_UNIX/D-Bus attacks, DNS packet behavior, tool-sandbox command identity, daemon reparenting, resource limits, rollback, or malicious native-code bypass attempts.

A separate v0.74.0 `nono wrap` trial using the normal bundled macOS policy confirmed a consequential baseline-policy exception: a file in an unrelated sibling `/private/tmp` tree remained readable because `system_read_macos` grants `/private` recursively. Nono correctly enforced the configured policy; the policy itself was broader than the user-facing “cwd and nothing else” shorthand suggests. Avoid placing secrets in `/tmp`, and qualify the fully resolved baseline rather than testing only explicitly added grants.

One usability observation: redirecting `XDG_STATE_HOME` to `/tmp` while using the normal built-in macOS policy failed preflight because the broad built-in system-read grant for `/private` overlapped Nono’s protected state root under `/private/tmp`. The fully resolved minimal manifest avoided that conflict. This is a test-environment edge case, not evidence of a sandbox escape.

## Pilot acceptance criteria

Use Nono as defense in depth only until all of the following pass on every supported host profile:

1. Pin and verify v0.74.0 or newer; subscribe to Nono advisories and block known-vulnerable versions.
2. Inspect the fully resolved profile (`profile show`, `--dry-run`, `why`); explicitly restrict network; account for the bundled macOS `/private` read grant; reject raw credentials, broad home grants, writable `PATH` directories, unsafe Seatbelt rules, direct-exec bypasses, and permissive approval rules.
3. Independently test negative cases: read SSH/cloud credentials; write outside the worktree; symlink/rename/hardlink edges; inherited FDs; absolute-path controlled-tool invocation; direct IP at the proxy port; hostname normalization; DNS exfiltration; pathname and abstract AF_UNIX; D-Bus/launch services; detached daemons; process signaling/inspection.
4. Run real representative builds to measure policy breakage and the pressure to add broad escape hatches.
5. Keep the microVM for adversarial code, unknown third-party repositories/install scripts, high-value credentials, and any multi-tenant execution. Evaluate Nono inside the microVM for per-tool least privilege.

Decision: **complementary layer now; possible host-only convenience sandbox for bounded local use after qualification; not a microVM replacement.**
