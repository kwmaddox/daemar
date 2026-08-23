# microsandbox viability proof — adversarial spike results

**Run:** 2026-08-11, on the target host. `msb 0.6.8`, libkrunfw 5, hardware
virtualization present, arm64 macOS 26.4.1. Guest: ubuntu image, Linux 6.12.98.
Harness (throwaway spike): `scratchpad/proof/harness.sh` plus the follow-up
commands reproduced inline below.

**This is confidence-by-failed-attack, not proof of no-escape.** It attacks the
specific claims and the specific weaknesses the research flagged. Two attacks
**succeeded** — microsandbox 0.6.8 does **not**, as shipped and configured via
its documented CLI, satisfy the full invariant.

## Verdict

**NOT viable as-is.** Isolation and in-guest credential protection held cleanly;
but the **default-deny-egress** half of the invariant is breached in a way that
is **not configurable away** with the documented rule grammar, and the known
host-side secret-leak (GHSA-m8f5-rh7h-vgg3) **reproduces** in 0.6.8.

## What HELD (attacks blocked)

**Isolation / escape — all clean:**
- Mounted-dir containment holds against parent traversal (`/mnt/work/../`),
  absolute host paths, symlink-to-`/`, and `/proc/self/root`. A host secret in
  a sibling directory outside the mounted subtree was unreachable by every
  vector. (The exact libkrun-flagged virtio-fs weakness — contained; the
  `RESOLVE_BENEATH` claim is real.)
- Read-only (`:ro`) mounts enforced — guest write refused, host file unchanged.
- Guest-internal writes do not surface on the host.
- Distinct kernel `boot_id` per sandbox — separate kernels, not one shared.
- No host processes visible in the guest process table.

**In-guest credential protection — clean:**
- With `--secret PROOF_SECRET@example.com`, inside the guest the env var reads
  as a **placeholder** (`PROOF_SECRET=$MSB_PROOF_SECRET`). The real value never
  appears in `env`, `/proc/self/environ`, the `/.msb` config, or PID 1 cmdline.
  The secret value genuinely does not enter the guest.

## What BROKE (attacks succeeded)

### FINDING 1 — Egress: a hostname allowlist cannot contain raw-IP egress (invariant-central)

Under the documented default-deny allowlist pattern
(`--no-net --net-rule 'allow@example.com:tcp:443'`), raw-IP egress to arbitrary
addresses is **open**, and explicit deny rules do not close it:

```
--no-net only (no allow rules)              -> raw-IP 140.82.112.3:443 DENIED   (default-deny works)
--net none                                  -> raw-IP DENIED
--no-net + allow@example.com:tcp:443        -> raw-IP 1.1.1.1 / 93.184.216.34 / 140.82.112.3 / 9.9.9.9  ALL OPEN
  + deny@0.0.0.0/0                          -> raw-IP still OPEN
  + deny@1.1.1.1  (exact target)            -> raw-IP still OPEN
```

The moment **any** hostname allow rule is present, raw-IP egress to any address
opens, and neither a `deny@0.0.0.0/0` nor a `deny@<exact-ip>` rule blocks it.
So one cannot express "allow the model-API domain **and** deny raw-IP
exfiltration to everything else" — it is all-or-nothing. For an agent that must
reach a model API (a domain) this directly breaches **default-deny egress**: a
hostile or injected agent exfiltrates by dialing any IP directly.

Boundary of the finding: tested via the documented `msb` CLI rule grammar only.
Whether the Rust SDK / a Sandboxfile / an external host firewall around the VMM
could enforce IP-level egress was **not** tested — but the CLI's own IP/CIDR
`deny` rules demonstrably do not filter raw egress once the gateway is active.

### FINDING 2 — Credential: GHSA-m8f5-rh7h-vgg3 reproduces in 0.6.8 (host-side)

While a `--secret`-holding sandbox runs, the raw secret value appears in **host
process arguments** (3 host processes) and in **1 host-side `~/.microsandbox`
state file** — readable by any local process via `ps` / `/proc/<pid>/cmdline`.
The advisory (published 2026-06-23, "secrets in world-readable process args")
is **not fixed** in 0.6.8, despite the CLI help claiming the value is "stored
only as a source reference, never inlined."

Relevance, stated precisely: this is **host-side** disclosure, not a
guest-to-host escape — the guest itself only ever sees the placeholder (Finding
above). So against the *strict* invariant clause "nothing inside the sandbox can
read the secret," this does not breach it. It matters for (a) a shared or CI
host where other local processes/users exist, (b) a second sandbox that escapes
(then the host process table is in reach), and (c) it falsifies microsandbox's
"unexploitable secret keys" marketing on the host side.

## Reproduce

```bash
export PATH="$HOME/.local/bin:$PATH"
# Finding 1:
msb run --no-tty --no-net --net-rule 'allow@example.com:tcp:443' ubuntu -- \
  bash -c 'timeout 6 bash -c "echo > /dev/tcp/140.82.112.3/443" && echo OPEN || echo DENIED'
# Finding 2:
export PROOF_SECRET=sk-proof-DEADBEEF11223344
msb run -d --name leak --secret 'PROOF_SECRET@example.com' ubuntu -- bash -c 'sleep 25'
ps -eww -o args= | grep DEADBEEF | grep -v grep      # value visible on host
grep -ral DEADBEEF ~/.microsandbox                    # value in host state file
msb rm -f leak
```

## Known status in the community (researched 2026-08-11)

Raw notes: `scratchpad/known-msb.md`, `scratchpad/known-domain.md`.

**Finding 1 — the raw-IP egress bypass:**
- *As a flaw class: textbook-known, not novel.* AWS documents it about its own
  Network Firewall ("SNI/Host header bypass risk... traffic could bypass domain
  filtering"); public PoCs since 2023; Kubernetes NetworkPolicy is IP/CIDR-only
  by design for the same reason. Domain allowlists provably don't constrain
  direct-to-IP traffic — you need an L3/L4 default-deny underneath.
- *Against microsandbox specifically: NOT reported, and appears genuinely new.*
  No matching issue/PR/discussion in the tracker. Nearest prior art (#675/#694
  DNS-query egress gating; #1192 `allow@public` not honoured) concern *other*
  mechanisms. Critically, our specific bug — a `deny@<ip>`/`deny@0.0.0.0/0`
  rule failing to override a coexisting hostname `allow`, leaving raw-IP open —
  is unreported, and is NOT documented as a known limitation either. A silent
  gap in both directions.
- *The architectural split (the keeper).* Products divide into: **forced-proxy
  / no-direct-network** (Cloudflare Sandbox SDK, Docker sbx — the sandbox has
  no raw network path; immune to raw-IP bypass by construction) vs.
  **rule-based filtering of a direct path** (E2B, Modal, microsandbox — gap
  unless an L3 default-deny is also enforced). microsandbox is worse than the
  rule-based norm: it *offers* IP `deny` rules that then don't work.

**Finding 2 — GHSA-m8f5-rh7h-vgg3 (secrets in world-readable argv):**
- *Known AND still open/unpatched.* Advisory state "published", `patched_versions`
  empty, `vulnerable_version_range: "all"`. 0.6.8 (latest, 2026-07-29) changelog
  has no fix. Intended fix direction is issue #997 (fd-based config passing);
  no closing PR has landed ~7 weeks after disclosure.

**Net:** microsandbox — a product whose entire value proposition is isolation +
secret protection — currently ships with (a) a published, unpatched
secret-disclosure advisory against *all* versions, and (b) a new,
unreported egress-containment bug where its own deny rules don't fire. Both sit
squarely on the two properties it markets.

## Design principle earned (applies to whatever substrate wins)

**Egress must be forced-proxy / no-direct-network:** the factory owns the ONLY
network path, deny-by-default, everything mediated — never rule-based filtering
of a direct guest network path. This is what Cloudflare and Docker sbx get
right by construction, and what we would build on Firecracker (host nftables
drop + mediating proxy). Domain/hostname allowlisting is L7 sugar ON TOP of an
L3/L4 default-deny, never a substitute for it.

## What this means for the decision

- The proof did its job: microsandbox's docs claim deny-by-default egress and
  unexploitable secrets; hands-on testing found real holes in both on this
  version/platform. This is precisely the documentation-isn't-proof gap.
- microsandbox is **not** viable as the substrate *as shipped/configured via
  its CLI today*. Paths that could revive it, each requiring its own proof:
  (1) enforce egress ourselves at the host layer around the VMM (an external
  firewall / the factory owning the network path) rather than trusting `msb`'s
  rules; (2) handle secrets host-side ourselves (never via `--secret`) so the
  GHSA path is unused; (3) a future patched release. All three mean *we* own the
  mediation — which erodes microsandbox's "ships mediation" advantage over
  Firecracker.
- Next candidates owe the *same* battery: Firecracker (host-side egress via
  nftables + jailer; we build mediation anyway) and Docker sbx (its own proxy +
  deny-by-default egress — test whether raw-IP is actually contained there).
