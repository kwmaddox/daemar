# Sandbox substrate & credential protection — research synthesis

**Retrieved:** 2026-08-10. Synthesis of seven parallel primary-source
collection passes. Raw per-topic findings, with full inline citations, are in
the session scratchpad (`sbx-macos.md`, `sbx-linux.md`, `sbx-agents.md`,
`sbx-creds.md`, `sbx-egress.md`, `sbx-escapes.md`, `sbx-containers.md`); the
load-bearing citations are reproduced here. This document supports a decision;
it does not make one. Per the invariant in `CLAUDE.md`, no substrate is
"proved" until it survives **hands-on** escape and credential-extraction
testing — this research narrows the field, nothing more.

Measured against the named invariant: **every task runs inside an isolated
sandbox** — (1) kernel-level isolation, (2) protection of every secret,
default-deny egress.

---

## 0. The finding that matters most: what likely broke the false start

The false start bet on **microsandbox** for the credential-protection
invariant. Two primary-source findings show that bet was unsafe:

1. **A disclosed, unpatched secret-leakage advisory in microsandbox's
   secret-handling path.** GHSA-m8f5-rh7h-vgg3 ("Secret values exposed in
   world-readable process arguments"), Moderate / CVSS 6.5, published
   2026-06-23, **no patched version listed at fetch time**. The SDK passed
   secrets and env vars as literal CLI arguments when spawning sandbox
   processes; on Linux those are world-readable via `/proc/<pid>/cmdline`,
   and on both Linux and macOS visible via `ps`. Any unprivileged local
   process on the host could read the "protected" secret for the sandbox's
   whole runtime. This is a **host-side** leak, directly contradicting
   microsandbox's own README claim that secrets are *"unexploitable"* and
   *"never enter the VM"*.
   (github.com/superradcompany/microsandbox/security/advisories/GHSA-m8f5-rh7h-vgg3)
2. **microsandbox's isolation claims are unaudited marketing, and its
   substrate has the weakest scrutiny signal of the field.** microsandbox
   rides **libkrun**, whose own threat-model discussion states *"the guest
   and the VMM pertain to the same security context"* and that it does not
   protect against a guest reaching other host directories/filesystems under
   virtio-fs without extra isolation (github.com/containers/libkrun
   discussions/538). A third-party comparative study notes libkrun has zero
   published CVEs, no upstream fuzzer, and no academic study — and flags
   microsandbox as *"the riskiest posture in the residual-bug sense"*
   precisely because absence of CVEs cannot be distinguished from absence of
   scrutiny (arXiv 2606.08433).

**Takeaway:** the invariant's credential clause and the isolation clause were
both plausibly violated by the substrate the false start trusted on
documentation alone. This is exactly the "assumed research had proved what it
had not" failure `CLAUDE.md` names. microsandbox is not disqualified forever
— but it re-enters at the back of the field, and only hands-on testing of a
*current, patched* version could move it.

---

## 1. Kernel-level isolation — the substrate tiers

The invariant demands a **separate guest kernel from the host** (or a
functionally equivalent barrier), so a kernel exploit inside the sandbox
cannot reach the host. That sorts the field into three tiers.

### Tier A — true VM boundary (separate guest kernel)

| Substrate | Platform | Notes | Track record |
|---|---|---|---|
| **Firecracker** | Linux (KVM) | Rust microVM; jailer as 2nd layer; minimal device model; ~125ms start | **Strongest.** 2 escape-class CVEs ever (CVE-2026-5747 OOB write, only with opt-in `--enable-pci`; CVE-2026-1386 jailer symlink), both fixed. Runs Lambda/Fargate at trillions of reqs/mo. Publishes a threat model. |
| **Apple Virtualization.framework** | macOS 11+ | First-party; true guest-kernel-separate VM for Linux guests | No framework-specific CVE found; no published threat model. |
| **apple/container + Containerization** | macOS 26 + Apple silicon only | **Each OCI container gets its own lightweight VM** via Virtualization.framework — the native macOS answer to "container ergonomics, VM boundary" | **Pre-1.0** (container CLI v1.2.2, framework v0.33.3-prerelease). Immature. |
| **libkrun / krunkit** | macOS (Hypervisor.fw) + Linux (KVM) | microVM library; what microsandbox uses | **Weakest scrutiny** (see §0). Guest+VMM one security context. |
| **Kata Containers** | Linux | VM-per-container, hardware-enforced | **Rough 2026:** multiple Critical VM-escape advisories (virtiofs guest-root→host-root, config-path host code exec, Dragonball virtio-blk escape). |
| **Cloud Hypervisor / QEMU microvm** | Linux | Rust VMM (CH) / minimal QEMU machine type | QEMU full-emulation stacks carry a long, ongoing guest→host CVE history. |

### Tier B — userspace kernel (no separate guest kernel, but syscalls don't reach the host kernel)

- **gVisor** — the Sentry intercepts every guest syscall and reimplements
  the Linux System API in userspace; syscalls *"do not pass through to the
  host kernel"*. Publishes an explicit security model and a "track record"
  claiming 123/128 (96%) upstream Linux-kernel CVEs defended. Caveats it is
  honest about: **no protection against hardware side channels**, and *"a
  sandbox is not a substitute for a secure architecture."* At least one
  real documented escape exists (GKE gVisor sandbox escape, third-party DB).
  Used by Modal and (per secondary sources) OpenAI's higher-risk tasks.
  This is a *different* isolation model than Tier A — whether it satisfies
  "kernel-level isolation" is a judgment call for the invariant, not a given.

### Tier C — shared-kernel confinement — DOES NOT meet the bar

- Ordinary containers (Docker/containerd/**runc**), **bubblewrap**, raw
  namespaces+seccomp+cgroups all **share the host kernel** (Docker's own
  docs; man7 namespaces(7)). Hardening (seccomp, AppArmor, user-namespace
  remap, rootless) is *mitigation, not elimination* — Docker's own AppArmor
  docs call the default profile only "moderately protective," and NIST SP
  800-190 names "shared kernel" as an explicit host-OS risk. The runc
  escapes CVE-2019-5736 and CVE-2024-21626 ("Leaky Vessels") reach host
  root / host filesystem with no hypervisor boundary. This tier is the
  baseline the VM tier is measured *against*, not a candidate.
- **Docker Desktop on macOS** runs a Linux VM, so the host↔VM boundary is
  VM-grade — but containers *within* that VM share one guest kernel, same as
  native Linux. A VM you then pack multiple untrusted tasks into shared-kernel
  is not per-task kernel isolation.

---

## 2. The platform question decides the shortlist — and needs Kendall's ruling

Kernel-level isolation has fundamentally different answers by OS, and the
`CLAUDE.md` assumption (macOS-primary dev host, Linux-secondary CI) puts the
two best-evidenced options on *opposite* platforms:

- **The mature answer is Linux-only.** Firecracker — the substrate with the
  strongest track record and threat model — requires KVM. On the Mac dev
  host it can only run *inside* a Linux VM (nested), or the factory itself
  runs in a Linux VM / on a Linux host.
- **The native-macOS answer is immature.** Apple's Virtualization.framework
  gives a real VM boundary today; the container-ergonomic layer on top
  (`apple/container`) is pre-1.0 and Apple-silicon + macOS-26 only.

So the real fork is architectural, and it's yours: **does the factory run
tasks on the macOS host directly (→ Apple's stack, immature), or does it run
tasks inside a Linux environment (→ Firecracker, mature) even when driven
from a Mac?** This single decision collapses the substrate shortlist. I did
not assume it; it needs your ruling. (Related: is the *long-term* deployment
target actually Linux/headless, which would make Firecracker the obvious bet
and the Mac merely the driver?)

---

## 3. Credential protection — the pattern is clear and well-proven

This half of the invariant has a **dominant, independently-reimplemented,
documented pattern**, and it is *not* substrate-specific:

**Host-side egress proxy with credential injection.** The real secret lives
only on the host/sidecar; the sandbox is given a proxy URL and a *placeholder*.
A host-controlled proxy holds the credential and rewrites the `Authorization`
header on the way out, only for allowlisted destinations. Documented
first-class implementations: **Cloudflare Sandbox SDK** (`outboundByHost` —
*"the sandboxed agent never has access to these credentials"*), **LangSmith
Auth Proxy** (*"the agent can use an API without being able to read the API
key"*), **Docker `sbx`** (host proxy + OS-keychain store, sandbox sees
sentinel `proxy-managed`), **Modal** (Caddy sidecar recipe), and
**microsandbox** itself (TLS-layer placeholder swap). In every documented
case, the credential never exists in the sandbox's env, memory, or
filesystem.

**How this maps to your "outside the box" steer.** Our factory is Rust and
drives the sandbox via SDK, so *the factory is the host-side trusted context*.
The secret is held in factory Rust (decrypted in-process from SOPS+age, per
the ported secret-handling decision), and injected by a factory-controlled
egress proxy the sandbox is forced through. Dependency injection is the right
instinct, but note the mechanism the research actually supports: injection
happens at the **network/TLS layer by a host-side proxy**, not by handing the
sandbox a Rust object. The DI is *within the factory* (the proxy and secret
store are injected collaborators of the host side); the sandbox boundary is
still the network path. Any Tier-A substrate that routes 100% of egress
through a host-controlled proxy supports this — it is not a reason to pick a
particular sandbox.

**Defense in depth from the provider side.** OpenAI supports **project-scoped
keys** (`sk-proj-`) with **per-endpoint Restricted permissions**,
project-level rate/spend limits, and — notably — a **SPIFFE workload-identity
federation** flow to authenticate *without a static key at all*
(developers.openai.com/api/docs/guides/workload-identity-federation/spiffe).
Even if mediation failed, a scoped key caps the blast radius; the SPIFFE path
is worth a deeper look as a keyless option later.

**The gap worth noting:** nobody documents a **vsock broker** pattern (sandbox
calls a host broker over a vsock/unix socket that holds the secret and makes
the real call) as a shipped product — everyone intercepts at TLS instead.
Firecracker has the right vsock primitive and an *experimental*
`feature/secret-hiding` branch, uninspected. For a VM-isolated design a vsock
broker is arguably cleaner than TLS interception (no CA-in-guest, no MITM),
but it would be **ours to build** — flagged as a design option, not an
off-the-shelf capability.

---

## 4. Egress control — host-enforced, with two sharp edges

- **No substrate filters its own egress; enforcement is always outside the
  sandboxed process.** Firecracker → host iptables/nftables on the tap
  device; gVisor → "applied at the container level"; Apple VZ → a custom
  packet filter the host implements via `VZFileHandleNetworkDeviceAttachment`
  (no built-in domain filtering). Any second route out of the sandbox voids
  the allowlist, so the design must force **100%** of egress through the
  controlled path.
- **DNS is the documented hole.** A domain-allowlisting proxy does nothing
  about DNS tunneling — port 53 / DoH must be locked down *separately*, or an
  agent can exfiltrate inside DNS queries. Any egress design has to name a
  DNS policy explicitly.
- **Package registries need hostname allowlists, and more than one each:**
  crates.io needs `index.crates.io` + `static.crates.io`, PyPI needs
  `pypi.org` + `files.pythonhosted.org`, npm needs `registry.npmjs.org` — all
  CDN-fronted, so IP allowlists won't work. This is the concrete cost of
  "package registries as needed" in the default-deny posture.

---

## 5. Cross-cutting facts that constrain the whole decision

- **No third-party security audit was locatable for ANY of the seven
  substrates** (reported as "could not find," not "does not exist"). Trust
  rests on track record and design, never on a certificate. This is the
  strongest possible argument for the invariant's rule that "proved" means
  *our own* adversarial testing.
- **Even the strongest option publishes limits.** gVisor explicitly disclaims
  side-channel protection; Firecracker scopes microarchitectural attacks
  in-scope but is still a moving target (its 2 CVEs are from 2026). "Kernel
  isolation" is a floor, not an absolute — the invariant's framing of
  *mitigation, not elimination* is the honest one.
- **The Rust ecosystem's WASM sandboxes are a different model** (compiler/
  language-level, not kernel), and not immune: Wasmtime had a real sandbox
  escape (RUSTSEC-2026-0096 / CVE-2026-34971, CVSS 8.8, fixed 43.0.1). WASM
  is not a route to kernel-level isolation of arbitrary task code.

---

## 6. Where this leaves the decision (for Kendall, not decided here)

1. **Rule the platform fork (§2)** — Apple-native/immature vs.
   Linux/Firecracker/mature. Everything downstream depends on it.
2. **Shortlist for hands-on testing follows from that ruling.** On current
   evidence the leading candidates are **Firecracker** (if tasks run in a
   Linux environment) and **Apple Virtualization.framework** (if tasks run on
   the macOS host natively); **gVisor** is a serious Tier-B alternative whose
   admissibility depends on whether its userspace-kernel model counts as
   "kernel-level" for us; **microsandbox re-competes from the back** (§0) and
   only on a patched, hands-on-tested version.
3. **Credential protection is a solved pattern (§3)** — host-side egress-proxy
   injection, with the factory as the trusted host — and is largely
   independent of the substrate choice. The vsock-broker alternative is a
   build-it-ourselves option to weigh.
4. **Egress design must name a DNS policy and per-registry hostname
   allowlists (§4)** regardless of substrate.
5. **Nothing is "proved" until we run the escape and credential-extraction
   tests ourselves.** That is a distinct, later slice.

---

# Addendum (2026-08-10): open-ended discovery — correcting a biased field

The seven collectors above were seeded from a **named candidate list drawn
from Claude's prior knowledge** — the exact failure mode agreement 5 exists to
prevent, applied one level up: rigorous primary-source verification *inside a
box drawn by memory*. Kendall caught it. Three further collectors were
dispatched with **no seed list**, told to enumerate the field from surveys,
taxonomies, and landscape sources. They surfaced whole categories and
instances the first pass structurally could not. Raw files: `disc-landscape.md`,
`disc-macos.md`, `disc-docker.md`.

## The real taxonomy (from a discovery pass, not recall)

Ten categories, not the three I implicitly assumed:
1. **Hardware-virtualized microVMs** — Firecracker, Cloud Hypervisor, Kata,
   libkrun, Apple Virtualization.framework, **Docker Sandboxes' own VMM**,
   Tart, Lima; plus ~15 products atop them (E2B, Fly Sprites, Vercel Sandbox,
   Modal, Cleanroom, Volant…).
2. **Userspace kernel / syscall interposition** — gVisor, bVisor.
3. **Linux kernel primitives (no hypervisor)** — bubblewrap, Landlock,
   Firejail, Minijail, nsjail. (Linux-only; shared-kernel.)
4. **macOS-native Seatbelt/`sandbox-exec`** — what Claude Code's own
   sandbox-runtime uses; **Apple-deprecated with no replacement timeline** (a
   real forward-risk for anything built on it).
5. **Plain containers** — insufficient alone (the §1 Tier-C finding).
6. **WebAssembly runtimes** — Wasmtime, Wassette, Pyodide, and **Hyperlight**
   (Microsoft; a VM-grade boundary with *no guest OS*, Rust-native — a
   genuinely distinct category; macOS backend unverified).
7. **V8-isolate / serverless-isolate** — Cloudflare Dynamic Workers (shipped
   2026, for agent code; Cloudflare itself admits isolate hardening is harder
   than hypervisor hardening), Deno Sandboxes.
8. **TEEs / confidential computing** — Intel TDX/SGX, AMD SEV-SNP, ARM CCA,
   NVIDIA H100/H200 GPU TEE. An entire isolation tier the first pass omitted.
9. **Unikernels / library OSes** — Unikraft, OSv, Nanos.
10. **Credential/secret-broker layers** (orthogonal to isolation) — Infisical
    Agent Proxy, Hermes iron-proxy, agentgateway, and an active **IETF draft
    (CB4A)** standardizing agent credential brokering.

Two finds a mainstream list misses, both matching *our corrected architecture*
(whole agent inside, zero secrets inside, host-side credential broker):
- **Unikraft as used by Browser Use** — agent runs in a unikernel microVM
  with zero secrets, reaching the world only through a credential-holding
  control plane. The cleanest documented end-to-end match to the invariant.
- **Hyperlight** — VM boundary without a guest OS, Rust-native; a different
  point on the speed/isolation curve worth watching.

Also: the field has **no peer-reviewed agent-sandboxing taxonomy yet** — it
lives in awesome-lists and vendor blogs (confidential computing is the
exception). Source quality across this space is inherently lower than, say,
the Firecracker/gVisor threat-model docs.

## Docker Sandboxes (`sbx`) — the standout for the portability requirement

Docker's dedicated agent-sandbox product is, on current evidence, **the only
candidate that ships all four invariant properties as one integrated thing**,
and it is built around exactly the architecture the false start got wrong.
(docs.docker.com/ai/sandboxes; docker.com/blog/why-microvms-...)

- **Whole agent in the box.** *"Docker Sandboxes run each agent session inside
  a dedicated microVM with a private Docker daemon isolated by the VM boundary,
  and no path back to the host."* The entire agent session runs in the VM —
  not tool calls over a wire. This is the corrected architecture, off the shelf.
- **Uniform boundary across platforms — the portability-of-guarantee YES.**
  Docker wrote its own VMM running on *each OS's native hypervisor*
  (Apple Hypervisor.framework / Windows Hypervisor Platform / Linux KVM):
  *"A single codebase for three platforms and zero translation layers."* So
  the *guarantee* — a true per-session microVM boundary — is the same on your
  Mac, on Linux, and in CI. That is the crux you flagged: not just a portable
  interface, a portable boundary. (Contrast plain Docker Desktop: one shared
  Linux VM, containers inside share its kernel — NOT per-session VM isolation.)
- **Credential injection built in.** *"The host-side proxy injects
  authentication headers into outbound HTTP requests. The raw credential
  values never enter the VM."* Keys live in the host OS keychain via a
  built-in secrets manager. Matches the credential clause directly.
- **Deny-by-default egress built in.** Only allowlisted domains proxied over
  HTTP/HTTPS; *"Raw TCP, UDP, and ICMP are blocked at the network layer."* No
  host filesystem, host Docker daemon, host network, or localhost. This also
  sidesteps the DNS hole from §4 (only HTTP/HTTPS is proxied at all).
- **macOS install:** `brew install docker/tap/sbx`; Docker Desktop not
  required (secondary-sourced).
- **The uniform guarantee has a precondition on Linux (confirmed from
  docs.docker.com/ai/sandboxes/get-started/):** it requires KVM hardware
  virtualization enabled, and if the Linux host is *itself* a VM (the common
  CI case) **nested virtualization must be on**. `sbx` does NOT fall back to
  shared-kernel containers when a real microVM is available — but Docker's
  docs **do not state what happens when KVM/nested-virt is absent** (hard
  fail vs. silent degraded fallback). This is security-critical and a
  **must-verify** item: a silent degrade to shared-kernel would recreate the
  false-start failure. We require fail-*closed* behavior; confirm it by test
  before trusting `sbx` in any Linux/CI path. (Plain Docker, by contrast, is
  explicitly asymmetric: VM-grade shell on macOS by necessity, no VM at all
  on native Linux — which is what `sbx` exists to erase.)

**The honest counterweights — this is not a free win:**
- **Newest, least-scrutinized VM code of the whole field.** It's Docker's
  *bespoke* VMM, written for this product. No CVEs, no audit, no public
  fuzzing found — which is the *same "absence of scrutiny ≠ safety" trap* that
  just burned us with libkrin/microsandbox. Newer than Firecracker by years.
- **A product, not a primitive.** CLI-only (no Rust SDK — we'd drive `sbx`
  as a subprocess), **requires a signed-in Docker account** ("verified
  identity" per sandbox), and emits telemetry (opt-out via `SBX_NO_TELEMETRY`).
  For a security foundation, depending on a closed CLI that requires login and
  phones home is a real architectural and trust consideration, distinct from
  the isolation question.
- **Maturity unresolved.** Introduced ~April 2026 (blog); no GA/version marker
  found; a conflicting third-party launch claim couldn't be verified.

## Correction (2026-08-11): the sbx-vs-microsandbox gap, tested with evidence

The §0 framing ("microsandbox re-competes from the back") leaned on three
asymmetries. Kendall demanded evidence for each. Two targeted collectors
(`gap-sbx-vmm.md`, `gap-msb-libkrun.md`) largely dismantled the case:

- **The credential vuln (GHSA-m8f5-rh7h-vgg3) is NOT relevant to our threat
  model.** The advisory classifies itself as host-side info-disclosure
  (secrets in world-readable process args), *"not a guest-to-host escape."*
  Our threat is the untrusted agent *inside* the sandbox; this vuln's attacker
  is another local process *on the host*. It does not show the sandbox
  boundary leaking a secret to the guest. At most a code-care signal, in a
  code path mediation would avoid. Overstated originally.
- **The substrate-posture claim is DISPROVEN.** microsandbox applies the
  isolation libkrun recommends: per-directory opt-in virtio-fs shares with
  `openat2`/`RESOLVE_BENEATH` path containment (not a host-root share), a
  restricted 5-device set, and one unprivileged non-root process per sandbox.
  It does NOT use libkrun's naive weak default. Residual weakness: no
  seccomp/namespace "second line of defense" around the VMM.
- **On that same axis, sbx is no better — and documents less.** Docker's docs
  state for sbx that *"the hypervisor boundary is the isolation control, not
  in-VM privilege separation"* — i.e. no jailer-equivalent, the VM boundary
  is the sole control point. No documented VMM privileges, no published threat
  model, no stated implementation language. And a real caveat: local stdio
  MCP servers run *on the host, outside the VM,* and can reach the host Docker
  daemon — an agent-reachable surface outside the boundary.

**Corrected conclusion (revised again 2026-08-11 — "sbx leads on integration
+ portability" was also wrong for our context):**

- *Integration:* microsandbox LEADS for a Rust factory — native Rust SDK
  (`cargo add microsandbox`); sbx is CLI-only (subprocess-driven, non-native).
  Earlier framing conflated "features bundled in one product" with
  "integrates into our codebase"; on the latter, sbx trails.
- *Portability:* EQUAL within our scope. microsandbox/libkrun gives a true
  microVM boundary on macOS (Hypervisor.framework) and Linux (KVM) — the same
  uniform boundary credited to sbx. Only delta is Windows (out of our scope).
  microsandbox's cost is a heavier embedded dependency footprint.
- *Mediation:* microsandbox ALSO ships it — native TLS-layer credential
  injection + egress allowlisting (`allowed_hosts`/`allowed_ports`). sbx's
  "batteries-included" edge over microsandbox is small.
- *What sbx actually retains over microsandbox:* big-company maturity + formal
  disclosure process, and Windows (out of scope). *Its costs:* CLI-only,
  Docker login required, telemetry.
- *Security:* documentation wash — neither has a jailer/second layer, neither
  a third-party audit, sbx no published threat model.

Net: for a **Rust factory on macOS+Linux that values openness**, microsandbox
is arguably the better fit than sbx on nearly every axis except big-vendor
backing (traded against heavier deps + small-project maturity + the
false-start history, which was architectural, not microsandbox's fault).
Firecracker remains genuinely above BOTH on isolation *evidence* (jailer,
published threat model, real-adversary track record) but is Linux-native only
and pushes credential/egress mediation onto us to build.

**Ceiling of all documentation research:** even a perfect reading pass yields
only "documents a strong posture, no published break" — still documentation,
the category the invariant says isn't proof. The gap fully closes only with
our own adversarial spike (escape attempts, credential-extraction attempts,
fail-closed verification) against a running instance, behind a swappable
substrate seam. And even a passing spike proves only *these attacks, this
version, this host* — never absence of escape; hence weight track record,
blast-radius containment (scoped keys), and the seam over any hope of
certainty.

## What this does to the shortlist

The platform fork from §2 is *softened but not erased* by Docker Sandboxes,
because it offers a uniform boundary across macOS and Linux — which is
precisely why it's attractive if portability is weighted heavily. The
hands-on-testing shortlist is now, roughly:
- **Docker Sandboxes** — best integrated match + portability; test the
  boundary and the credential proxy adversarially; weigh the product/login/
  bespoke-VMM concerns.
- **Firecracker** — most-proven isolation, but Linux-only (needs a Linux
  layer on Mac) and credential/egress are ours to assemble.
- **Apple Virtualization.framework / libkrun (Rust-embeddable)** — native
  macOS, primitive-level control, we build the mediation.
- **Unikraft / Hyperlight** — architecturally closest to the ideal
  (agent-in-VM, zero secrets inside), less mature/proven for our use; watch.
- **microsandbox** — re-competes from the back (§0), patched version only.

None of this is decided. Everything still owes the hands-on escape and
credential-extraction test before "proved."

## Open questions / caveats

- Several help.openai.com pages (restricted-key mechanics, project keys)
  403'd on direct fetch; those specifics are search-excerpt-level, not
  first-party-confirmed. The SPIFFE federation URL is first-party but its
  mechanics weren't fetched in depth.
- The arXiv comparative study (2606.08433) informs the microsandbox/libkrun
  "riskiest posture" and E2B-patch-latency claims; its PDF wasn't
  text-extractable, so those are secondary characterizations, not verified
  primary data.
- Several Kata CVE-to-GHSA mappings and one gVisor blog citation carry
  date/URL discrepancies flagged in the raw files — the advisory *volume* is
  the reliable signal, not every individual mapping.
- No CVE or threat model specific to Apple's Virtualization/Containerization
  frameworks was found — absence of evidence, not evidence of absence, on a
  young stack.
- microsandbox GHSA-m8f5-rh7h-vgg3's patch status after v0.6.8 was not
  confirmed; would need a changelog check before any reconsideration.
