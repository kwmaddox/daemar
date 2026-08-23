# Substrate claim — adversarial refutation pass

**Retrieved 2026-08-22.** The claim under test: **H1** "Apple `container` is the
best available sandbox substrate on macOS" and **H2** "Firecracker is the
absolute best sandbox substrate on Linux," judged against daemar's requirements
(separate guest kernel; default-deny egress *by construction*; Rust drivability;
one-shot arbitrary command exec; macOS 26 dev host / Linux CI; evidence quality
as a first-class criterion). Method: **discovery-first enumeration from
landscape sources with no seed list**, then for each half the 3–5 strongest
challengers were argued *for* from primary sources and only then tested. A
claim half survives only if every fielded challenger demonstrably loses.

Primary sources only as evidence. Where a claim rests on a secondary write-up it
is marked SECONDARY. Where something was searched and not found, that is said
explicitly and distinguished from "does not exist." The repo's own
`sandboxing.md`, `apple-container-proof.md`, and `microsandbox-proof.md` were
read for framing and are **not cited as evidence** here; where this pass
independently reaches the same place as the hands-on proof, that is noted as
convergence, not as citation.

**Verdicts up front.** H1: **SURVIVED-WITH-CAVEATS** — and the caveats are
substantial enough that "best" should be read as *best packaged*, not *best
possible*. H2: **SURVIVED-WITH-CAVEATS**, with the word **"absolute" refuted**:
Cloud Hypervisor matches or beats Firecracker on exactly the criterion daemar
ranks first, evidence quality.

---

## How the field was enumerated

Deliberately not from recall. The starting points were landscape indices and
taxonomies, used only as pointers to primaries:

- [infracloudio/awesome-microvm](https://github.com/infracloudio/awesome-microvm) —
  surfaced Firecracker, Cloud Hypervisor, Kata, StratoVirt, Alioth, Hyperlight,
  Dragonball, smolvm, microvm.nix.
- [dloss/awesome-agent-sandboxes](https://github.com/dloss/awesome-agent-sandboxes) —
  surfaced the agent-sandbox layer specifically: Matchlock, Arrakis, Gondolin,
  ERA, Netclode, yolo-cage, plus the hosted tier (E2B, Modal, Daytona, Runloop).
- "Your Container Is Not a Sandbox: The State of MicroVM Isolation in 2026"
  (Beganović, 2026-03-27, <https://emirb.github.io/blog/microvm-2026/>) —
  SECONDARY, used only as a pointer; it is what surfaced Edera, SlicerVM, Lima's
  krunkit driver, and Docker Sandboxes as things to go verify at the source.
- crates.io / docs.rs, searched directly for host-side Rust linkage.

Three things the discovery surfaced that a memory-seeded list would have missed,
and that materially changed this pass:

1. **`objc2-virtualization`** — maintained, safe Rust bindings to
   Virtualization.framework. This makes "daemar *is* the VMM host process" a
   real H1 challenger rather than a thought experiment. It ended up being the
   strongest challenger on the macOS side.
2. **Matchlock** — an open-source project whose shape is almost exactly
   daemar's: one CLI over Virtualization.framework on macOS *and* Firecracker on
   Linux, with `--no-network`, host-side secret injection, and forced proxying.
   Evidence that the architecture daemar is converging on is a recognised
   pattern, not an invention.
3. **The bounty asymmetry** (below) — only visible by going to the two bounty
   programs as primaries rather than reasoning about CVE counts.

Eliminated at the enumeration stage, on requirements rather than on quality:

- **Hyperlight** — fails the workload requirement outright. Its README: *"No
  kernel or OS in the VM. Guests are regular ELF binaries written in `no_std`
  Rust or C,"* and it is explicitly unsuitable for *"running full-blown Linux
  guest workloads that need syscalls, networking, or filesystem access"*
  ([hyperlight](https://github.com/hyperlight-dev/hyperlight)). No macOS support.
- **The hosted tier** (E2B, Modal, Daytona, Runloop, Vercel Sandbox, Fly Sprites)
  — a remote service is not a substrate for a local macOS dev host, and daemar
  cannot inspect their invariant. Retained only as comparative benchmarks.
- **WASM runtimes** — already settled in `wasm-evaluation.md`; nothing in this
  discovery reopens it.

### The evidence-quality asymmetry (applies to both halves)

Requirement 6 says real adversarial exposure counts, and "no CVEs" from an
unscrutinised project is not evidence of safety. The cleanest primary-source
measure of *priced* scrutiny is what each vendor pays for a hypervisor escape:

- **KVM**: Google's kvmCTF pays **up to $250,000 for a full VM escape**, scoped
  explicitly to guest-to-host attacks on upstream mainline KVM, with mandatory
  public disclosure ([kvmCTF
  rules](https://google.github.io/security-research/kvmctf/rules.html)).
- **Apple's Virtualization.framework / Hypervisor.framework**: **no category
  exists.** The Apple Security Bounty category list covers network attacks, app
  sandbox escape, browser attacks and services — there is no VM-escape or
  guest-to-host category at all
  ([security.apple.com/bounty/categories](https://security.apple.com/bounty/categories/)).
  Compounding this, `apple/container`'s own SECURITY.md states: *"While we
  welcome reports for open source software projects, they are not eligible for
  Apple Security Bounties"*
  ([SECURITY.md](https://github.com/apple/container/blob/main/SECURITY.md)).

**This is the single most important finding of the pass, and it cuts against
H1.** The macOS hypervisor boundary has materially less priced adversarial
attention than KVM, and Apple does not underwrite it. It does not refute H1 —
nothing better exists *on macOS* — but it means the macOS wall should never be
described as equivalently evidenced to the Linux one.

---

## H1 (macOS): challengers to Apple `container`

Incumbent baseline, verified fresh. `apple/container` **1.2.2, 2026-08-08**,
five releases in ten weeks, 12 named maintainers, real coordinated disclosure
([releases](https://github.com/apple/container/releases),
[MAINTAINERS.txt](https://github.com/apple/containerization/blob/main/MAINTAINERS.txt)).
It is a staffed project, not abandonware — but see "what the incumbent actually
costs" below, because the audit found more against it than the claim implies.

### Challenger 1 — Build directly on Virtualization.framework from Rust (`objc2-virtualization`)

**The best case for it, argued properly.** This challenger wins on daemar's two
hardest requirements, and it wins them at the API level rather than by flag.

*Default-deny egress is the API default, not an option.*
`VZVirtualMachineConfiguration.networkDevices` is documented as *"List of
network adapters. **Empty by default**"* — as are `socketDevices`,
`directorySharingDevices`, and every other device array
([docs.rs](https://docs.rs/objc2-virtualization/latest/objc2_virtualization/struct.VZVirtualMachineConfiguration.html)).
A guest with no network is not a configuration you select; it is what you get
unless you write code to add one. That is default-deny by construction in the
strictest available sense.

*And the mediated path is stronger than a proxy.*
`VZFileHandleNetworkDeviceAttachment` is *"Network device attachment sending raw
network packets over a file handle,"* where *"the file handle must hold a
connected datagram socket"*
([docs.rs](https://docs.rs/objc2-virtualization/latest/objc2_virtualization/struct.VZFileHandleNetworkDeviceAttachment.html)).
The daemar host process holds the other end of that socket and *is* the guest's
entire link layer. There is no tap device, no NAT, no host route, and no host
firewall rule to get right — the raw-IP bypass class that sank microsandbox is
not merely blocked, it has no medium to travel over.

*Native Rust, no subprocess.* `objc2-virtualization` **0.3.2 (2026-08-04)** is
safe, 100%-documented, triple-licensed Rust binding over the whole VZ surface
including `VZVirtualMachine`, `VZLinuxBootLoader`, and `VZVirtioSocketDevice`
([docs.rs](https://docs.rs/objc2-virtualization/latest/objc2_virtualization/)).
Requirement 3 lists a native Rust API as a plus; this is the only macOS
candidate that delivers one.

**Why it does not take the crown — and the honest crux.** The isolation boundary
is *the same Virtualization.framework hypervisor* Apple `container` uses. So
this is **not an isolation refutation**; it is an architecture and ownership
trade. What you buy with it (egress construction, native Rust) you pay for by
building everything Apple `container` already ships: guest kernel and initramfs
sourcing, root filesystem assembly, virtiofs wiring, an in-guest exec/supervisor
agent, and the whole one-shot lifecycle. Two further costs are real: VZ requires
the `com.apple.security.virtualization` entitlement — *"A
VZVirtualMachineConfiguration is considered invalid if the application does not
have the entitlement"*
([docs.rs](https://docs.rs/objc2-virtualization/latest/objc2_virtualization/struct.VZVirtualMachineConfiguration.html))
— so the daemar binary must be signed and entitled; and every guest-plumbing bug
becomes daemar's own bug with no upstream.

**Verdict: loses on total cost, but it is a live architectural option, not a
hypothetical.** It should be recorded as the fallback if Apple `container`'s
management-plane risk (below) proves unacceptable, because it removes that
entire plane.

### Challenger 2 — vfkit / krunkit / libkrun used directly

**The best case.** One process equals one VM with no daemon and no shared state:
vfkit is *"a command-line interface to start virtual machines using the macOS
Virtualization framework"* and the VM *"will be terminated as soon as the vfkit
process exits"* ([vfkit](https://github.com/crc-org/vfkit),
[usage.md](https://github.com/crc-org/vfkit/blob/main/doc/usage.md)) — spawn and
kill is the entire one-shot lifecycle, which fits requirement 4 better than any
daemon-backed candidate. Networking is genuinely opt-in: `--device virtio-net`
with mutually exclusive `nat` / `unixSocketPath=` / `fd=` modes, and a VM can
start with no network device at all; the `unixSocketPath=` and `fd=` modes hand
the guest NIC to a userspace stack you own with no host NIC involved. krunkit
exposes the same shape, with only `virtio-blk` mandatory
([krunkit usage](https://github.com/containers/krunkit/blob/main/docs/usage.md)).
And uniquely among the packaged candidates, **`libkrun` is on crates.io**
(v1.19.3, first published 2026-04-28,
<https://crates.io/crates/libkrun>) — a Rust host program can link the VMM
in-process rather than shelling out. libkrun also offers TSI over virtio-vsock,
i.e. connectivity with no virtual interface whatsoever.

**Why it loses.** Two independent grounds, and the first is decisive.

*libkrun's own threat model puts the guest and the VMM in one security unit.*
Its README states that the guest and VMM are a single security unit and that
host isolation relies on OS mechanisms — namely Linux namespaces — with
virtio-fs and TSI requiring additional host-side protection
([libkrun](https://github.com/containers/libkrun)). On macOS those namespaces do
not exist. So the vfkit/krunkit process itself becomes the trust boundary and
must be separately confined, and the project tells you so. Against requirement 1
that is a materially weaker posture than VZ's, where the VMM is Apple's own
XPC-separated helper set.

*Evidence quality fails outright.* **SECURITY.md: NOT FOUND on any of vfkit,
krunkit, or libkrun. GitHub Security Advisories: searched, zero for all three.**
Per requirement 6 that is *absence of process*, not evidence of safety — and it
is a sharper problem here than elsewhere, because there is no published threat
model to test a finding against. (NVD was not swept for these three:
**COULD NOT CHECK.**) Maintenance is otherwise healthy — libkrun v1.19.4
(2026-07-03), vfkit v0.6.4 (2026-07-07) — though krunkit's last commit was
2026-07-03, the weakest of the set, and both krunkit and libkrun have **moved
GitHub orgs** (from `containers/` to a `libkrun/` org), which is churn a
substrate dependency should notice.

**Reconciliation — an overstated claim, corrected.** The macOS sub-audit
concluded that vfkit is the *only* candidate offering default-deny by
construction and that Apple `container` "gives you a NIC and asks you to filter
it." **That is wrong, and the correction matters.** Reading current `main`
(commit `d6de569`, 2026-08-20): `NetworkClient.swift:47` defines
`public static let noNetworkName = "none"`, and `Utility.swift:200-204` rejects
`none` unless it is the sole entry and then sets **`config.networks = []`**;
`NetworksService.swift:146` reserves the name so no network can shadow it. So
`--network none` yields **zero network attachments**, not a filtered interface,
and it is enforced in the API-service layer rather than in CLI argument parsing
— independently converging with this repo's hands-on proof. Apple `container`
therefore *does* have default-deny by construction, and vfkit's advantage on
requirement 2 is narrower than claimed: it is the *quality of the mediated path*
(unix-socket-attached userspace stack vs. `--publish-socket`) that still favours
vfkit, not the existence of the deny. The genuinely unrefuted parts of the
vfkit/libkrun case are **crates.io Rust linkage** and **process-lifetime VM
semantics**, and both stand.

The caveat runs the other way too, and is not small: **`--network none` is
undocumented.** It appears in neither
[networking.md](https://github.com/apple/container/blob/main/docs/networking.md)
nor
[command-reference.md](https://github.com/apple/container/blob/main/docs/command-reference.md);
it entered via [PR #739](https://github.com/apple/container/pull/739) as a
reserved network name. daemar's central egress primitive is an undocumented flag
in a project that shipped breaking API changes at 1.0.0. Pin the version and
assert at startup that egress actually fails.

### Challenger 3 — Docker Sandboxes (`sbx`)

**The best case, and it is a strong one.** This is the only candidate whose
*shipped product shape* is precisely one VM per task. Docker's first-party
architecture post describes a **custom VMM built from scratch**, running on
**Hypervisor.framework on macOS**, with **one dedicated microVM per agent
sandbox**, each with **its own kernel** and a private Docker daemon inside
([Why microVMs](https://www.docker.com/blog/why-microvms-the-architecture-behind-docker-sandboxes/),
[docs](https://docs.docker.com/ai/sandboxes/)). Its egress model is
policy-first and pre-committed — *"Outbound TCP traffic passes through a proxy
on your host, which enforces access rules on every connection,"* with
Open / Balanced / Locked Down presets and a real CLI grammar
(`sbx policy allow network api.anthropic.com`)
([policy docs](https://docs.docker.com/ai/sandboxes/security/policy/)). Docker's
own framing — policy *"defined before the agent runs, not enforced by the agent
itself"* — is exactly daemar's principle. It needs no Docker Desktop
(`brew install docker/tap/sbx`) and ships at a rapid clip: **v0.39.0,
2026-08-19** ([install](https://docs.docker.com/ai/sandboxes/install/)).

**Why it loses.** Three grounds, compounding.

*Egress is rule-based over a live path, not absent by construction.* Even
"Locked Down" is a host proxy policy applied to a guest that always has a
network path. Against requirement 2 as written that is strictly weaker than an
absent device, and it is the same architectural family — filter a live path —
that produced the microsandbox raw-IP failure. Docker's proxy may well hold; the
point is that it *must* hold, whereas an absent NIC cannot fail open.

*The VMM is unauditable.* `docker/sbx-releases` is a binary-release-only repo
with no source and a `NOASSERTION` license
(<https://github.com/docker/sbx-releases>). daemar's security invariant is meant
to be inspectable; a closed, from-scratch VMM at **0.x** with no published threat
model cannot be inspected. Zero advisories on that repo tells you essentially
nothing, since it carries no code.

*Drivability.* CLI-only, no REST or socket API, no Rust crate, and OAuth
sign-in is required. (The full flag list rendered as bare headings through
WebFetch: **COULD NOT CHECK.**)

**Verdict: loses as a substrate; retained as the best competitive benchmark.**
If daemar ever wanted to buy rather than build, this is the product to buy.

### Challenger 4 — Tart

**The best case.** Genuine per-task VMs on Virtualization.framework via
`tart run`, a documented clone-from-OCI → run → delete CI pattern that matches
one-shot lifecycle well, and strong maintenance (2.35.0, 2026-08-04).

**Why it loses — two independent grounds.** *No way to boot without a NIC.*
Reading `Run.swift` directly, the options are `--net-bridged`, `--net-softnet`,
`--net-softnet-allow` (CIDR), `--net-softnet-block`, `--net-softnet-expose`,
`--net-softnet-control-fd`, and `--net-host`; there is **no flag to omit the
network device**, and absent any flag it defaults to shared NAT
([Run.swift](https://github.com/cirruslabs/tart/blob/main/Sources/tart/Commands/Run.swift)).
Softnet is an explicitly filter-based userspace packet filter over a live NAT
path, and its allowlist is **CIDR-only, not hostname**
([softnet](https://github.com/cirruslabs/softnet)) — the wrong end of the very
split `microsandbox-proof.md` earned. Requirement 2 fails.

*Governance.* **Cirrus Labs joined OpenAI**: the org redirects
`cirruslabs/tart` → `openai/tart`, and the LICENSE now reads *"Functional Source
License, Version 1.1, ALv2 Future License — Copyright 2022-2026 OpenAI"*
(verified via the GitHub API). Tart is **not OSI-open-source today**. Basing
daemar's isolation boundary on a competing AI lab's internal agent infrastructure
under a source-available license is an avoidable strategic dependency.
(Reporting that Cirrus CI shut down and fees are going away: SECONDARY,
<https://macstadium.com/blog/cirrus-labs-is-joining-openai>.)

### Challenger 5 — Lima

**The best case.** CNCF incubating, 21.7k stars, last commit 2026-08-21, latest
release **v2.2.0 (2026-07-21)**, and the best *disclosure hygiene* of any macOS
candidate — two real advisories handled properly, including
[GHSA-2j9v-p4xj-cjw2](https://github.com/lima-vm/lima/security/advisories/GHSA-2j9v-p4xj-cjw2)
/ CVE-2026-53657 (high, 2026-06-19). Its `vz` driver is
Virtualization.framework, default on macOS since v1.0
([vmtype](https://lima-vm.io/docs/config/vmtype/)).

**Why it loses — architecturally, on requirement 1's spirit.** Lima's idiom is a
**long-lived shared VM that you then run containers inside** (nerdctl, Docker,
Podman, k8s). One-VM-per-task is possible by starting a fresh instance, but it is
not the designed path, and `vmType` is immutable after create. In the shared-VM
idiom two tasks share one guest kernel, which is precisely the boundary daemar
refuses. Requirement 2 also fails: **NOT FOUND (searched)** — neither
[network docs](https://lima-vm.io/docs/config/network/) nor the shipped
`templates/default.yaml` exposes any "no network device" switch; the keys are
`networks:` (`lima:`/`socket:`/`vzNAT:`), `portForwards`, `dns`, `hostResolver`.
The NIC is always present. **SECURITY.md: NOT FOUND** at repo root (API 404).

### Challenger 6 — Matchlock (noted, not fielded as a winner)

Worth recording because its *shape* is daemar's: one CLI over
Virtualization.framework on macOS and Firecracker on Linux, with `--no-network`
giving *"fully offline sandboxes (no guest NIC / no egress)"*, `--allow-host`
allowlisting, and host-side secret injection where the sandbox *"only ever sees
a placeholder"* ([matchlock](https://github.com/jingkaihe/matchlock)). On macOS
its interception is a gVisor userspace TCP/IP stack at L4 — a real forced-proxy
design, and independent confirmation that daemar's chosen architecture is a
recognised pattern.

**Why it loses:** MIT, **v0.2.4**, effectively single-maintainer, and **no
SECURITY.md and no threat model** (the repo's security section shows zero
items). Against requirement 6 a 0.2.x project cannot be the wall for untrusted
code — but it is the closest existing prior art to what daemar is building, and
is worth reading for design rather than adopting.

### What the incumbent actually costs — the caveats behind "SURVIVED"

The audit of Apple `container` turned up more against it than the claim admits,
and these are the caveats that qualify the verdict:

- **There is no threat model. This is a finding, not an omission.** SECURITY.md
  exists in both repos but is *purely* a disclosure-process document — no
  statement of the isolation boundary, no attacker model, no scope
  ([container](https://github.com/apple/container/blob/main/SECURITY.md),
  [containerization](https://github.com/apple/containerization/blob/main/SECURITY.md)).
  The entire security claim in the documentation set is one bullet: *"Security:
  Each container has the isolation properties of a full VM, using a minimal set
  of core utilities and dynamic libraries to reduce resource utilization and
  attack surface"*
  ([technical-overview.md](https://github.com/apple/container/blob/main/docs/technical-overview.md)).
  That is an unqualified assertion. Under requirement 6, daemar would be adopting
  a boundary its vendor has never scoped and explicitly excludes from its bounty.
- **Apple positions this for developer convenience, not untrusted workloads.**
  The technical overview frames the audience as *"Backend developers use
  containers on their personal systems to create predictable execution
  environments"* and CI/CD reproducibility. Nowhere does Apple describe running
  adversarial code. daemar is using it outside its stated purpose — defensible,
  but it must be a conscious decision.
- **`--internal` does not isolate, and the bug is open and unacknowledged.**
  [#2062](https://github.com/apple/container/issues/2062): *"`--internal` is
  documented and implemented as an isolation boundary, but it does not block
  egress... Outbound TCP is fully open."* It went unnoticed because the
  integration test asserted isolation via *hostname resolution*, which fails for
  want of DNS — so the test stayed green while raw TCP to arbitrary internet IPs
  succeeded. This is the microsandbox failure mode recurring in a different
  project, and it is a direct warning about how daemar writes its own egress
  tests. Related gaps [#1320](https://github.com/apple/container/issues/1320)
  and [#500](https://github.com/apple/container/issues/500) are also open.
  **`--network none` is the only trustworthy network posture.**
- **The VM wall looks sound; the management plane is where it fails.** Of eight
  advisories across the two repos, **five are guest/build-content → host
  boundary crossings**: build FS sync disclosing host files via symlinks
  ([CVE-2026-64777](https://github.com/apple/container/security/advisories/GHSA-2v2q-4q35-h585)),
  `container cp` restoring setuid/setgid and arbitrary ownership on the host
  ([GHSA-5h49-6pr7-9mv4](https://github.com/apple/containerization/security/advisories/GHSA-5h49-6pr7-9mv4)),
  archive-extraction escape
  ([CVE-2026-20613](https://github.com/apple/containerization/security/advisories/GHSA-cq3j-qj2h-6rv3)),
  host env inheritance into the guest
  ([CVE-2026-64786](https://github.com/apple/container/security/advisories/GHSA-xwgf-4rc5-p4m4)),
  and pf rule injection via `system dns create`
  ([GHSA-39g5-644c-qwcg](https://github.com/apple/container/security/advisories/GHSA-39g5-644c-qwcg)).
  Four landed in a single 11-day window in August 2026. Operationally: never
  `container cp` out of an untrusted run, never `--ssh` (it mounts
  `$SSH_AUTH_SOCK` into the guest,
  [host-integration.md](https://github.com/apple/container/blob/main/docs/host-integration.md)),
  never build from an untrusted context.
- **Mediated path conflicts with hardening.** `--publish-socket` is the only
  mediated primitive, and
  [#2101](https://github.com/apple/container/issues/2101) reports it *"cannot
  relay a socket on tmpfs with read-only root"* — precisely the hardened
  configuration daemar wants. The mediated path and the hardened rootfs are
  currently mutually exclusive.
- **Rust drivability is CLI-only and that is the ceiling.** `--format
  <json|table|yaml|toml>` is broadly supported and `inspect` emits JSON
  natively, so shelling out is clean
  ([command-reference.md](https://github.com/apple/container/blob/main/docs/command-reference.md)).
  But the API server is **XPC-only and private** (Mach services
  `com.apple.container.*`), there is no REST/UDS/gRPC host API, there are no
  non-Swift bindings (no `@_cdecl` exports, no C header), and 1.0.0 removed v0
  XPC compatibility while admitting the API is not yet versioned. Binding XPC
  from Rust would mean reimplementing an unversioned private protocol.
- **Capacity ceiling for a one-shot fleet.** Memory ballooning is only partially
  supported by Virtualization.framework, so freed guest memory is never returned
  to macOS; Apple's own doc says *"If you run many memory-intensive containers,
  you may need to occasionally restart them"*
  ([technical-overview.md](https://github.com/apple/container/blob/main/docs/technical-overview.md)).
  There is no pre-warmed micro-VM pool
  ([#1924](https://github.com/apple/container/issues/1924)), so cold-start cost
  is unmitigated for a VM-per-task design.
- **Youth and churn.** ~15 months old, 493 open items, commit rate declining
  month over month (Jun/Jul/Aug 2026: 60/41/29), a third of the maintainer roster
  already emeritus, and `containerization` releases lagging what `container`
  consumes.

### H1 verdict: **SURVIVED-WITH-CAVEATS**

No challenger refuted it. Each fielded challenger loses on a requirement:
libkrun on its own single-security-unit threat model plus absence of any security
process; Docker sbx on rule-based egress and an unauditable closed VMM; Tart on
having no way to omit the NIC and on FSL/OpenAI governance; Lima on being
architecturally a shared long-lived VM; Matchlock on 0.2.x maturity. The direct
VZ-in-Rust route does **not** lose on isolation — it shares the identical
hypervisor TCB and is *better* on requirements 2 and 3 — but it loses on total
build cost, and it cannot refute a claim about the best *available* substrate
when it is not a substrate but a construction kit.

The caveats that qualify the verdict, named explicitly:

1. **No threat model exists**, and Apple excludes the project from its bounty.
2. **No VM-escape bounty category exists for Apple's hypervisor at all**, versus
   $250k for KVM — the macOS wall is less-scrutinised, and this is structural.
3. **The central egress primitive (`--network none`) is undocumented.** Pin the
   version; assert egress failure at startup.
4. **`--internal` is broken and open.** Any design assuming a mediated-but-
   attached path is unsupported today.
5. **The management plane (cp, build sync, archive extraction, DNS) is the live
   risk surface**, not the VM wall.
6. **Virtualization/Hypervisor framework CVE history is unassessed, not clean**
   — see open questions.

---

## H2 (Linux): challengers to Firecracker

Incumbent baseline, verified fresh. Firecracker **v1.16.1, 2026-07-02**, roughly
monthly cadence with live backport branches
([releases](https://github.com/firecracker-microvm/firecracker/releases)). Its
threat model is genuinely published and admirably blunt: *"All vCPU threads are
considered to be running malicious code as soon as they have been started; these
malicious threads need to be contained"*
([design.md](https://github.com/firecracker-microvm/firecracker/blob/main/docs/design.md)).
The jailer *"sets up system resources that require elevated permissions (e.g.,
cgroup, chroot), drops privileges, and then exec()s into the Firecracker binary,
which then runs as an unprivileged process,"* and seccomp filters are *"loaded
in the Firecracker process, on a per-thread basis, before executing any guest
code."* The device model is deliberately tiny. On requirement 2, design.md is
explicit that *"Firecracker does not perform any network traffic filtering"* —
egress is the host's job, which is exactly the division daemar wants.

### Challenger 1 — Cloud Hypervisor (the strongest, and it lands a hit)

**The best case, argued to win.** On requirement 6 — the criterion daemar ranks
first — Cloud Hypervisor is at least Firecracker's equal and arguably its
better.

*A separately published, enumerated threat model.* Where Firecracker states its
trust boundary in prose inside a design document, Cloud Hypervisor ships a
dedicated `docs/threat-model.md` that enumerates both sides. Trusted: *"The CPU,
including (if present) its microcode and privileged firmware"*, *"The Linux
kernel"*, and the management interfaces (CLI, HTTP API, D-Bus API). Untrusted:
*"Cloud Hypervisor considers the guest VM to be untrusted"*, *"Cloud Hypervisor
assumes that disk images provided to it are untrusted"*, and PCI/vDPA devices.
It also states its own limits — *"Cloud Hypervisor is not hardened against
denial of service attacks from the guest kernel"*
([threat-model.md](https://raw.githubusercontent.com/cloud-hypervisor/cloud-hypervisor/main/docs/threat-model.md)).
Naming disk images as untrusted is not decoration: it is the exact class that
produced its own CVE-2026-27211, and the model predicted it.

*A stronger definition of "vulnerability," and a real disclosure machine.* Its
SECURITY.md defines a vulnerability as *"an entity defined in the threat model
as untrusted being able to cause Cloud Hypervisor to do something that the
threat model states it should not be able to cause"* — i.e. the threat model is
load-bearing, not ornamental — and commits that *"Any known or potential memory
corruption is assumed exploitable until and unless proven otherwise."* There is
a defined embargo of *"up to 14 days"* with six named organisations receiving
embargoed access (Microsoft, Crusoe, Cyberus Technology, Meta, Google, UbiCloud)
([SECURITY.md](https://raw.githubusercontent.com/cloud-hypervisor/cloud-hypervisor/main/SECURITY.md)).
Firecracker's SECURITY.md, by contrast, routes to AWS's reporting page and
**names no bounty program and no published external audit**
([SECURITY.md](https://github.com/firecracker-microvm/firecracker/blob/main/SECURITY.md)).

*Seccomp on by default, and it says why.* *"Cloud Hypervisor enables seccomp
filtering as the project believes that security should not be an option"*, with
per-thread differentiated filters
([seccomp.md](https://raw.githubusercontent.com/cloud-hypervisor/cloud-hypervisor/main/docs/seccomp.md)).
Filesystem confinement via Landlock is available — *"Cloud Hypervisor can
sandbox itself via Landlock and seccomp"* — and the project is honest about that
sandbox's limits: *"it does not prevent access to AF_UNIX sockets."*

*Requirement 2 is a clean win, evidenced at the schema level.* In the OpenAPI
`VmConfig` schema the **only required field is `payload`**; `net` is an optional
array
([cloud-hypervisor.yaml](https://raw.githubusercontent.com/cloud-hypervisor/cloud-hypervisor/main/vmm/src/api/openapi/cloud-hypervisor.yaml)).
A VM with zero NICs is the natural construction, not a special case — this is
the strongest *evidence* for default-deny by construction of any Linux
candidate, because it is a property of the API schema rather than of a flag.
vsock is the mediated path: `--vsock cid=3,socket=/tmp/ch.vsock`, stream-only,
bridged to a host UNIX socket
([vsock.md](https://raw.githubusercontent.com/cloud-hypervisor/cloud-hypervisor/main/docs/vsock.md)).
It also supports vhost-user device backends, which Firecracker does not offer at
all — Firecracker is **TAP-only**: *"Currently, Firecracker supports only a
TUN/TAP network backend with no multi queue support"*
([network-setup.md](https://github.com/firecracker-microvm/firecracker/blob/main/docs/network-setup.md)).

**Why it does not win — three grounds, and the first one corrects an
earlier reading of mine.**

*There is no jailer equivalent, and that is a gap rather than an elegance.* My
first pass framed Cloud Hypervisor's in-process Landlock+seccomp as an
architectural advantage over Firecracker's separate jailer binary. Closer
reading does not support that. Seccomp is on by default and Landlock is real,
but Landlock is **opt-in** (`--landlock`,
[landlock.md](https://raw.githubusercontent.com/cloud-hypervisor/cloud-hypervisor/main/docs/landlock.md)),
and **there is no chroot/namespace/privilege-drop wrapper equivalent to
Firecracker's `jailer`** (NOT FOUND, searched). Firecracker's jailer *ships* the
cgroup, chroot, uid/gid-drop layer; with Cloud Hypervisor you must build that
wrapper yourself. Firecracker's jailer has genuine problems — its docs concede
*"the jailer treats all its inputs as trusted"*
([prod-host-setup.md](https://github.com/firecracker-microvm/firecracker/blob/main/docs/prod-host-setup.md))
and it was itself the subject of
[CVE-2026-1386](https://github.com/firecracker-microvm/firecracker/security/advisories/GHSA-36j2-f825-qvgc)
— but a flawed shipped layer still beats an absent one that daemar would have to
write. **On host-side confinement Firecracker edges Cloud Hypervisor.**

*A CVSS 10.0 guest-to-host confidentiality break, in exactly the class that
matters.*
[CVE-2026-27211](https://nvd.nist.gov/vuln/detail/CVE-2026-27211) /
GHSA-jmr4-g2hv-mjj6, published 2026-02-20, "Host File Exfiltration via QCOW
Backing File Abuse", **CVSS 10.0 CRITICAL**, affecting v34.0–v50.0 and fixed in
v50.1. This is precisely the guest→host confidentiality break a sandbox exists
to prevent, and it is the more serious for arriving in a project whose own
threat model correctly names disk images as untrusted — the model predicted the
class and the code still got it wrong. Also
[GHSA-f47p-p25q-83rh](https://github.com/cloud-hypervisor/cloud-hypervisor/security/advisories/GHSA-f47p-p25q-83rh)
(High, 2026-05-14), use-after-free in virtio-block async I/O completion. NVD
returns `totalResults: 1` for the keyword "cloud-hypervisor"
(<https://services.nvd.nist.gov/rest/json/cves/2.0?keywordSearch=cloud-hypervisor>)
despite three real advisories — a naming artifact, not a clean record.

*No first-party performance numbers and no bounty.*
[performance_metrics.md](https://raw.githubusercontent.com/cloud-hypervisor/cloud-hypervisor/main/docs/performance_metrics.md)
documents only the harness and explicitly disclaims its sample: *"the metrics
data above is for illustration purpose only and does not represent the actual
performance."* Firecracker publishes real numbers; Cloud Hypervisor does not.
There is no bug bounty and no third-party audit was located (NOT FOUND,
searched). Drivability is also weaker than it first appears: REST over a UNIX
socket with **no first-party embeddable Rust crate** (`cloud-hypervisor-client`
on crates.io is third-party, SECONDARY) and no published API stability or
deprecation policy (NOT FOUND, searched).

**Assessment: Cloud Hypervisor beats Firecracker on threat-model documentation
and on schema-level egress evidence, and loses on host-side confinement, on
track record, and on drivability.** It is the closest thing to a peer, not a
successor.

### Challenger 2 — crosvm

**The best case, and it genuinely beats Firecracker on one axis.** crosvm does
something neither Firecracker nor Cloud Hypervisor does: **one process per
virtual device, each independently jailed.** From
[ARCHITECTURE.md](https://github.com/google/crosvm/blob/main/ARCHITECTURE.md):
*"A process per virtual device, made using fork on Linux. Each process is
sandboxed using minijail."* And it states the resulting trust boundary
precisely: *"The only interaction that the device is capable of having with the
main process is via the proxied trait methods of BusDevice, shared memory
mappings such as the guest memory, and file descriptors that were specifically
allowed by that device's security policy."* Per-device seccomp policies live at
`jail/seccomp/{arch}/{device}.policy`, and a block device *"has been limited to
having just the FD needed to access the backing file on the host and has no
ability to open new files"*
([sandboxing](https://crosvm.dev/book/appendix/sandboxing.html)). This is a
jailer-equivalent and then some: minijail supplies namespaces, seccomp, and
reduced FDs, applied *per device* rather than once per VMM.

Why that matters concretely: under Firecracker, a virtio device bug compromises
*the whole VMM process* — which is exactly what
[CVE-2026-5747](https://github.com/firecracker-microvm/firecracker/security/advisories/GHSA-776c-mpj7-jm3r)
(High 8.7, out-of-bounds write in the virtio-pci transport, guest root → VMM
arbitrary code execution, 2026-04-07) delivered. Under crosvm the same class
lands in a separate namespaced, seccomp'd, FD-starved process — and there is
evidence of the design working: CVE-2025-2509 (2025-05-06), an out-of-bounds
read in Virglrenderer, is described as allowing *"a malicious guest VM to
achieve arbitrary address access **within the crosvm sandboxed process**,
potentially leading to VM escape"*
([NVD](https://nvd.nist.gov/vuln/detail/CVE-2025-2509)) — contained, and in the
GPU/virgl path that a headless agent sandbox would never configure.

Requirement 3 is also better than expected: `crosvm_control` is a real Rust
library with a generated C header and, unusually, an explicit stability
commitment — *"Any breaking change to a `crosvm_control` entrypoint must be
handled the same way as a breaking change to the crosvm CLI"*
([programmatic_interaction](https://crosvm.dev/book/running_crosvm/programmatic_interaction.html)).
Requirement 2 holds by documented example: the book's minimal launch carries **no
`--net`**, with networking introduced afterwards in a separate section
([example_usage](https://crosvm.dev/book/running_crosvm/example_usage.html)).
And its adversarial exposure is the best of the KVM candidates — crosvm ships in
**ChromeOS and Android AVF**, i.e. on hundreds of millions of devices inside
Google's VRP perimeter.

**Why it loses — and the disqualifier is operational, not security.** **crosvm
does no releases.** The GitHub mirror states *"There aren't any releases here"*;
tags are sparse and non-semver (`v0.1.6-rutabaga-release`); canonical
development is Chromium Gerrit with `main`, `chromeos`, and many `factory-*.B`
branches
([chromium.googlesource.com/crosvm](https://chromium.googlesource.com/crosvm/crosvm/)).
Pinning a commit hash and vendoring it is the *only* versioning strategy, with
no upgrade story and no security-release channel to subscribe to. For a
substrate that must be patched on advisory, that is disqualifying on its own.
Secondary grounds: **SECURITY.md NOT FOUND (searched)**, no dedicated
threat-model chapter (security material is scattered across appendix pages on
sandboxing, seccomp, and minijail), no first-party boot numbers (NOT FOUND), and
`crosvm_control` is a typed client against a running instance's control socket
rather than in-process embedding.

**Its per-device confinement is nonetheless the single best architectural idea
any challenger raised, and it should inform how daemar confines whatever VMM it
ends up running.**

### Challenger 3 — QEMU `microvm`

**The best case.** `microvm` is explicitly *"a machine type inspired by
Firecracker and constructed after its machine model"* — *"a minimalist machine
type without PCI nor ACPI support"*, *"designed for short-lived guests"*, with up
to eight virtio-mmio devices and split irqchip by default
([microvm docs](https://www.qemu.org/docs/master/system/i386/microvm.html)). It
matches Firecracker's ergonomics, is universally packaged, is unambiguously
maintained (**11.1.0, released 2026-08-11**,
<https://www.qemu.org/download/>), and its threat model is arguably the most
honest of the lot: `security.html` names as untrusted the guest, user-facing
interfaces, network protocols, passthrough devices, and — notably —
***"User-supplied files (e.g. disk images, kernels, device trees)"***
(<https://www.qemu.org/docs/master/system/security.html>). Egress is a non-issue
(`-nic none` / `-net none`, plus vhost-user-net and vhost-vsock over unix
sockets), and it has a real disclosure process via confidential GitLab issues
coordinated by Red Hat Product Security
(<https://www.qemu.org/contribute/security-process/>).

**Why it loses, decisively, on requirement 6 — and note the irony that its own
honesty is what convicts it.** Two grounds compound.

*Seccomp is opt-in.* Confinement is *"available via the QEMU `--sandbox`
option"*, not on by default — the critical difference from Firecracker and Cloud
Hypervisor, both of which load filters before any guest code runs. QEMU states
that *"QEMU processes must run as unprivileged users"* and mentions namespaces,
chroot, SELinux and AppArmor as available, but ships no jailer: it is a menu of
things the operator must assemble, in practice via libvirt+svirt.

*The track record is enormous and actively weaponised.* NVD returns **1335
results** for "qemu"
(<https://services.nvd.nist.gov/rest/json/cves/2.0?keywordSearch=qemu>), and the
2025–2026 window alone carries guest→host bugs in **exactly the virtio devices a
`microvm` still instantiates**: CVE-2026-3195 (heap buffer overflow in
`virtio_snd_pcm_in_cb`), CVE-2026-3196 (integer overflow in virtio-snd PCM_INFO
→ unbounded allocation), CVE-2026-15264 (vhost-user-gpu heap overflow via a
32-bit `width*height*4` wrap undersizing the host buffer), CVE-2025-14876
(unbounded allocation in virtio-crypto). A full virtio-snd 0-day-to-hypervisor-
escape chain was published in March 2026 (<https://osec.io/blog/2026-03-17-virtio-snd-qemu-hypervisor-escape/>,
**SECONDARY**, pointing to CVE-2026-3195) — weaponisation is demonstrated, not
theoretical.

This must be stated fairly: QEMU's CVE count reflects the **most eyes** as much
as the largest attack surface, and by the raw measure of adversarial testing it
has the best-examined corpus of anything in this report. But `microvm` reduces
the *devices instantiated*, not the codebase behind them, and for a one-shot
agent sandbox a millions-of-lines-of-C device model with opt-in seccomp is the
wrong risk profile however well-studied it is. The existence of Firecracker,
Cloud Hypervisor, and crosvm is the industry's own verdict on this question.

### Challenger 4 — gVisor

**The best case for "defensibly equivalent barrier."** It is a real userspace
kernel, not a syscall filter: *"the application's direct interactions with the
host System API are intercepted by the Sentry, which implements the System API
instead,"* and the docs explicitly reject the ptrace-sandbox framing — *"all
system calls are interpreted and handled by the Sentry itself"*
([security model](https://gvisor.dev/docs/architecture_guide/security/)). Its
defence-in-depth principle is strong — *"No system call is passed through
directly to the host"* — and on requirement 2 it is excellent: `--network=none`
gives full disconnection, and the default Netstack keeps the entire TCP stack
inside the Sentry, *"isolated from the host network stack"*
([networking](https://gvisor.dev/docs/user_guide/networking/)).

**Why it loses — two independent grounds, one of them fatal.**

*It does not run on macOS. At all.* *"gVisor works strictly on Linux systems"*
and Google's own security blog is blunt that the host *"is always some flavor of
Linux (sorry, Windows/MacOS users)"*
([install](https://gvisor.dev/docs/user_guide/install/),
[security basics](https://gvisor.dev/blog/2019/11/18/gvisor-security-basics-part-1/)).
On the dev host it could only ever be a second layer *inside* a hypervisor
daemar already needed — which makes it a defence-in-depth addition, never a
substrate.

*It is not a separate guest kernel, and its record shows the difference.* The
barrier is a large userspace Go program, and it has had genuine host-effect
bugs: CVE-2018-16359 (seccomp policy permitted `renameat` — *"rename files on
the host OS"*) and CVE-2018-19333 (CVSS 9.8, memory overwrite via mismanaged
reference counting), plus CVE-2025-2713 (7.8, runsc file-permission handling
letting *"unprivileged users access restricted files"*). Its own docs disclaim
*"gVisor does not provide protection against hardware side channels"* and *"A
sandbox is not a substitute for a secure architecture."*

*A trap worth recording:* gVisor's GHSA page shows **no published advisories**,
and its CVE policy excludes issues rooted in Linux and issues requiring host
root ([security policy](https://gvisor.dev/security/)). Its low advisory count
is a **reporting-channel artifact plus a narrow assignment policy**, not a
cleaner history. Comparing raw advisory counts between gVisor and Firecracker
would be an invalid comparison — a general caution for how daemar reads any
"no CVEs" claim.

### Challenger 5 — Edera

**The best case, and it is the only genuine architectural challenge in the
field.** Every other Linux candidate — Firecracker, Cloud Hypervisor, crosvm,
QEMU — shares one thing: the Linux KVM module sits in the trust base. Edera
removes it. Its security model page states that *"Edera's hypervisor is based on
Xen, with critical control components rebuilt in Rust"*, that each zone runs
*"its own Linux kernel inside a hardware-enforced boundary"*, and that *"the only
communication channel between a zone and the host is inter-domain messaging
(IDM), a structured protocol over shared memory"*
(<https://docs.edera.dev/technical-overview/security/security-model/>). It
defends explicitly against zone→host escape *"via hypervisor enforcement, not
Linux namespaces"*. The control plane is real open source — `krata`, *"an
implementation of a Xen control-plane in Rust"*, GPL-2.0, with a SECURITY.md
(<https://github.com/edera-dev/krata>) — and the thesis is peer-reviewed rather
than marketed: "Goldilocks Isolation: High Performance VMs with Edera" (Moore &
Zenla, submitted 2025-01-08, revised 2026-04-16,
<https://arxiv.org/abs/2501.04580>) argues precisely daemar's premise, that a
shared kernel *"presents a large attack surface and has led to a proliferation
of container escape attacks."* The org is genuinely active, with commits across
`krata`, `styrolite`, `xen`, and `xen-oci` within days of retrieval
(<https://github.com/edera-dev>). Its threat model is also candid where it
matters: ***"The configuration that creates the zone—whether a Kubernetes pod
spec or a `protect` CLI command—is outside Edera's security boundary."***

**Why it loses — four grounds, and the first is decisive on its own.**

*Boot latency, by its own paper.* The arXiv paper reports CPU 0.9% slower than
Docker, syscalls ~3% faster, memory 0–7% faster — but **boot time +648ms over
Docker's 177.4ms baseline**, i.e. roughly **~825ms**. For a one-shot-per-task
lifecycle that is an order of magnitude worse than what Firecracker publishes.
Read the paper's framing honestly: the claim is *"runtime comparable to
Docker"* with hypervisor-grade isolation, and the threat model is **container
escape**. Edera is positioned against *containers*, not against Firecracker.

*Requirement 2 could not be confirmed.* The docs cover zone networking
standalone and under Kubernetes, but the CLI reference is version-gated and did
not return command-level flag detail. **I could not confirm from a primary
source that a zone can be created with no network device at all**, nor whether a
general vsock-equivalent exists — IDM is host↔zone control messaging, not
obviously a mediated data path. **COULD NOT CHECK**, and for daemar's central
requirement an unconfirmed answer is a failing one.

*Requirement 3 and 4 fail on shape.* The shipping interface is a `protect` CLI
plus Kubernetes RuntimeClass/CRD integration; krata is Rust and open but **no
crates.io publication was confirmed** (NOT FOUND, searched) and it is a component
of the commercial product rather than a standalone embeddable API. Edera is
architected for Kubernetes pod workloads, not for a Rust process spawning
one-shot VMs.

*The trust base is different, not obviously smaller.* Edera-specific CVEs,
bounty, and audits: **NOT FOUND (searched)** — which under our own rule is
absence of scrutiny, not evidence of safety. And removing KVM substitutes
roughly twenty years of Xen hypervisor and toolstack surface:
[xenbits.xen.org/xsa](https://xenbits.xen.org/xsa/) lists **508 advisories**,
with four dated 2026-07-28 alone (XSA-508 pygrub de-privileging; XSA-507 /
CVE-2026-62434 PoD reclaim of special pages; XSA-506 / CVE-2026-62433 DM_OP
hypercall buffer checks; XSA-505 / CVE-2026-62432 evtchn FIFO expand/reset
race). Trading KVM for Xen is a lateral move in TCB size, defensible on
architecture but not a free win.

### Also enumerated, not fielded

**Alioth** (<https://github.com/google/alioth>) — triaged out on its own words.
The README: *"Alioth /AL-lee-oth/ is an experimental Type-2 hypervisor, written
from scratch in Rust"* and *"Alioth is an experimental project and is **NOT an
officially supported Google product**."* It is the most idiomatic-Rust VMM in the
field, publishes `alioth-cli` to crates.io, offers virtio net/block/entropy,
vsock and virtiofsd, and — interestingly for H1 — supports **Apple
Hypervisor.framework on macOS** as well as KVM. But there is no threat model, no
SECURITY.md, no boot numbers, and no CVEs (NOT FOUND, searched — meaningless at
this scrutiny level). A self-declared experimental non-product cannot be the
isolation boundary for untrusted agent code. Worth watching; that is all.

**StratoVirt** (<https://gitee.com/openeuler/stratovirt>) — Rust on KVM with a
genuine `microvm` machine type, virtio-blk, and QMP-over-unix-socket control.
Real engineering (3,555 commits), but no SECURITY.md (NOT FOUND, searched), no
threat model, no first-party boot numbers, and a governance red flag: the
project states it *"has been migrated to AtomGit"*, and the GitHub mirror is
already stale by a minor version (v2.3.0 there vs v2.4.0 on Gitee). Its control
interface is QMP — QEMU's ergonomics without QEMU's scrutiny.

**Kata Containers** is an orchestration layer that runs *on top of* Firecracker,
Cloud Hypervisor, or QEMU — it changes the packaging, not the wall, so it cannot
refute a claim about the substrate. **Dragonball**, Kata's in-process VMM, rests
on enumeration only (**COULD NOT CHECK**).

### H2 verdict: **SURVIVED-WITH-CAVEATS — and "absolute" is REFUTED**

Firecracker survives as the defensible default, and it survives for a specific
reason: **it is the only candidate that loses on no axis.** It has a real
published threat model, hostile-guest-by-assumption design, a tiny device model,
seccomp on by default, a shipped jailer, first-party performance numbers, an
active monthly cadence, and deployment exposure nothing else matches. Every
challenger beats it somewhere and then falls over somewhere else:

- **QEMU microvm** — best-examined corpus in the field, and disqualified anyway
  by opt-in seccomp plus a 2026 record of weaponised virtio escapes.
- **gVisor** — not a separate guest kernel, and Linux-only, so it cannot be the
  macOS story at all.
- **Edera** — the only real architectural challenge, undone by ~825ms boot on
  its own paper, an unconfirmed no-network mode, no embeddable Rust API, and a
  startup's scrutiny over an inherited 508-XSA surface.
- **crosvm** — genuinely beats Firecracker on host-side confinement
  architecture, and is undone operationally: **no releases at all**, so there is
  no version to pin and no security-release channel.
- **Cloud Hypervisor** — genuinely beats Firecracker on threat-model
  documentation and on schema-level egress evidence, and is undone by having
  **no jailer equivalent**, no first-party boot numbers, and a **CVSS 10.0
  guest→host file exfiltration** (CVE-2026-27211) in precisely the class that
  matters.

So the claim as worded says **absolute best**, and that does not hold. Two
challengers each beat Firecracker on a criterion daemar cares about, and neither
is refuted on the merits of that criterion — each is defeated by a *different*
weakness. That is a strong incumbent, not an unbeatable one, and "absolute" is
the wrong word for it.

**A correction to my own first pass, recorded deliberately.** I initially framed
Cloud Hypervisor's in-process Landlock+seccomp as an *advantage* over
Firecracker's separate jailer. That reading does not survive the evidence:
Landlock is opt-in, and Cloud Hypervisor ships no chroot/namespace/privilege-drop
layer at all, so what looked like elegance is a gap daemar would have to fill
itself. Firecracker's jailer is flawed — it *"treats all its inputs as trusted"*
and produced CVE-2026-1386 — but a flawed shipped layer beats an absent one.
Firecracker edges Cloud Hypervisor on host-side confinement.

**One axis eliminates the entire field, including the incumbent.** If in-process
Rust drivability is treated as a hard requirement rather than a plus, *nothing*
here satisfies it: Firecracker is REST-over-unix-socket with no official SDK,
Cloud Hypervisor the same with no first-party crate and no stability policy,
crosvm's `crosvm_control` is a typed client against a running instance (though
it is the closest, and carries an explicit CLI-grade stability commitment),
Edera is a CLI plus Kubernetes CRDs, QEMU and StratoVirt are QMP JSON, and
Alioth is a crate but an experimental non-product. On Linux, daemar will be
supervising a subprocess whatever it picks — which is worth knowing before the
seam is designed, and is a notable asymmetry against the macOS side, where
`objc2-virtualization` does offer real in-process Rust.

The caveats that qualify Firecracker's survival:

1. **`prod-host-setup.md` is a list of security work Firecracker requires but
   does not do for you**, and adopting Firecracker means adopting all of it as
   daemar's own obligation: per-instance unique uid/gid, operator-configured
   cgroups and rlimits (*"highly dependent on the workload type and usecase"*),
   mandatory host `nft`/`iptables-nft` filtering including blocking IMDS, SMT
   and KSM disabled, swap disabled for guest-memory remanence, ECC RAM, and
   monitoring for signal-handler-deadlocked processes. This is the largest
   hidden cost of adoption and it is not optional.
2. **It does not mitigate hardware vulnerabilities and says so**: *"Firecracker
   is not able to mitigate host's hardware vulnerabilities. Adequate mitigations
   need to be put in place when configuring the host."* It warns at launch about
   MMIO stale data on pre-Cascade-Lake Intel with the C3 template
   ([cpu-templates.md](https://github.com/firecracker-microvm/firecracker/blob/main/docs/cpu_templates/cpu-templates.md)).
   gVisor disclaims side channels identically — **neither substrate can be
   scored on side-channel resistance**.
3. **Snapshot fan-out is documented-insecure**, which forecloses the obvious
   warm-pool optimisation: *"we consider resuming execution from the same state
   more than once insecure,"* because unique identifiers, RNG seeds, the guest
   entropy pool and cryptographic tokens are duplicated across restores; VMGenID
   reseeds the kernel PRNG but *"State other than the guest kernel entropy pool
   ... will still be replicated"*
   ([snapshot-support.md](https://github.com/firecracker-microvm/firecracker/blob/main/docs/snapshotting/snapshot-support.md)).
   The only safe pattern is boot → snapshot → terminate original → restore
   exactly once.
4. **No Rust library path.** Firecracker is a binary driven by REST over a unix
   socket. There is no official Rust SDK; `firecracker-rs-sdk` and
   `firecracker-http-client` are community wrappers around the same REST API and
   do not embed the VMM (crate officialness: SECONDARY, confirm before
   depending). rust-vmm crates *are* embeddable, but using them means building
   your own VMM, not driving Firecracker.
5. **"No network unless configured" could not be sourced to a direct quote.**
   It is structurally implied — interfaces exist only via `PUT
   /network-interfaces` and the device model has no implicit NIC — but the docs
   are procedural and state no default. **If this is load-bearing for daemar,
   verify it empirically rather than citing it.**

---

## What this changes for daemar's broader picture

**1. The two halves are not equally evidenced, and the docs should say so.**
KVM has a $250k priced escape bounty and mandatory disclosure; Apple's
hypervisor has no VM-escape bounty category at all, and Apple excludes
`container` from bounty eligibility outright. Both substrates are defensible,
but the macOS wall rests on materially less adversarial scrutiny than the Linux
one. Any language treating them as equivalent walls is overclaiming.

**2. The management plane is the real attack surface, on both platforms.** Five
of eight Apple advisories are guest-content → host crossings via `cp`, build
sync, archive extraction, and DNS config — not hypervisor escapes. Firecracker's
own containment layer was CVE'd (jailer symlink) and its docs say the jailer
*"treats all its inputs as trusted."* The lesson generalises: **daemar's own
handling of data coming *out* of a sandbox is as security-critical as the wall
itself.** Extracting artifacts from an untrusted run needs its own threat model —
in particular, never restore setuid bits or ownership, and never follow symlinks
out of the work directory.

**3. A test that passes for the wrong reason is worse than no test.** Apple
`container` [#2062](https://github.com/apple/container/issues/2062) is the
cautionary tale: the `--internal` isolation test asserted by attempting
*hostname resolution*, which failed for lack of DNS, so it went green while raw
TCP to arbitrary internet IPs was wide open. This is structurally the same
failure as the microsandbox raw-IP bypass, found in a different project by a
different mechanism. **daemar's egress tests must assert on raw IP connectivity,
never on name resolution** — that belongs in the invariant's test contract, not
just in a research note.

**4. "No CVEs" is now demonstrably unreadable as a safety signal — three
distinct ways.** gVisor shows zero GHSAs because it routes through Google and
excludes Linux-rooted and host-root-requiring bugs from CVE assignment. Cloud
Hypervisor shows `totalResults: 1` in NVD keyword search despite three real
advisories, purely as a naming artifact. Docker sbx shows zero advisories on a
repo that contains no source code. Requirement 6's scepticism is vindicated:
**advisory counts are only comparable within a single project's own disclosure
channel, never across projects.**

**5. Forced-proxy-vs-filtering remains the correct architectural split, and the
field is still divided along it.** Docker Sandboxes — the most polished
commercial product in the space — is still *filtering a live path* with a host
proxy, and Tart's softnet is CIDR-only filtering over live NAT. Only vfkit,
direct VZ, and Apple `container`'s `--network none` give an absent device.
daemar's insistence on default-deny by construction is not fastidiousness; it is
the minority position among shipping products, and it is the right one.

**6. There is a live architectural fork worth recording.** Because the same VZ
hypervisor backs both, daemar can move from `container`-the-CLI to
`objc2-virtualization`-in-process **without changing its isolation boundary at
all** — trading Apple's guest plumbing for a native Rust API, an empty-by-default
device list, and a link-layer that daemar's own process owns end to end. Keeping
the substrate behind a thin seam preserves that option, which is now a concrete
migration rather than a vague BYOS gesture.

**7. Two operational constraints to design around now**, both first-party: Apple
`container` never returns freed guest memory to macOS (*"you may need to
occasionally restart them"*) and has no pre-warmed VM pool, so a VM-per-task
fleet on the dev host has a capacity ceiling and an unmitigated cold-start cost;
and Firecracker's snapshot fan-out is documented-insecure, so the obvious
warm-pool trick on Linux is unavailable unless every restore is single-use.

---

## Open questions and unsourced claims

1. **Apple hypervisor CVE history is unassessed, not clean.** NVD keyword
   queries for `Virtualization.framework` and `Hypervisor.framework` return
   `totalResults: 0`, but Apple categorises fixes under component names like
   "Virtualization" or "Kernel" in its security release notes rather than by
   framework filename. **COULD NOT CHECK**: a sweep of support.apple.com release
   notes for those components — the session's search budget was exhausted. This
   is the highest-value follow-up before H1 is signed off, and it is the one
   place where the pass could not close the loop.
2. **Apple's own developer documentation could not be read.**
   `developer.apple.com/documentation/*` is JS-rendered and returns only the
   page title to fetching. The VZ API semantics cited here come from
   `docs.rs/objc2-virtualization`, which reproduces Apple's doc comments
   verbatim in generated form. That is a faithful mirror but it is one remove
   from the owner of the claim; confirm the `networkDevices` default and
   `VZFileHandleNetworkDeviceAttachment` datagram-socket requirement against the
   Xcode headers before building on them.
3. **"Firecracker boots with no NIC unless configured" is unsourced to a
   quote** — structurally implied by the API and device model, but stated
   nowhere. Verify empirically if load-bearing.
4. **Firecracker's real-world adversarial exposure is an inference.** "Battle-
   tested at AWS scale" cannot be cited: SECURITY.md names no bounty and no
   published external audit was found (searched). The exposure is almost
   certainly real; it is simply not evidence in the sense requirement 6 demands.
5. **vfkit / krunkit / libkrun were not swept in NVD** — only GHSA (zero for all
   three). **COULD NOT CHECK.** Given that libkrun publishes no security
   process, this gap should be closed before the fallback is ever taken
   seriously.
6. **Edera's egress model is unconfirmed, and it is the sharpest open gap on the
   Linux side.** I could not establish from a primary source whether a zone can
   be created with **no network device at all**, nor whether any general
   vsock-equivalent mediated path exists (IDM is host↔zone control messaging).
   The CLI reference is version-gated and did not return flag-level detail.
   **COULD NOT CHECK.** Edera loses on boot latency and drivability regardless,
   so closing this would not flip the verdict — but it is the one requirement-2
   answer in the report that is unknown rather than known.
7. **Cloud Hypervisor's current version could not be resolved.** The releases
   page fetch returned **v53.0 dated "July 12, 2024"**, which contradicts the
   advisory timeline (a v50.1 fix shipping February 2026). The year is almost
   certainly a fetch/cache artifact and the real date is July 2026, but this was
   not resolved before budget exhaustion. **COULD NOT CHECK.** What *is* solid:
   advisories dated May 2026 confirm active maintenance through mid-2026.
8. **Further Linux-side COULD-NOT-CHECKs**, none of which would change the
   ranking but all of which are load-bearing if a decision turns on them:
   crosvm's SECURITY.md and crosvm-specific Chromium VRP payout language;
   QEMU's Pwn2Own virtualization-category targeting via first-party ZDI
   advisories; `alioth-cli`'s crates.io version and whether it exposes a library
   API; and **first-party boot-latency numbers for both Cloud Hypervisor and
   crosvm (NOT FOUND, searched)** — only Firecracker and Edera publish real
   numbers, so any latency comparison across the field is currently
   apples-to-oranges.
9. **Dragonball rests on enumeration alone** and was never evidenced.
   Confidence that it is not a serious contender is moderate, not high.
7. **Docker sbx's full CLI surface could not be read** — the reference pages
   rendered as bare headings. Its per-task-microVM and proxy-policy claims come
   from Docker's architecture post and policy docs, which are first-party but
   are the vendor describing its own closed VMM. Unverifiable by construction.
8. **Cirrus Labs → OpenAI commercial terms are SECONDARY.** The org redirect and
   the FSL/OpenAI copyright in LICENSE are primary and verified; the reporting
   that fees are going away and Cirrus CI shut down is not.
