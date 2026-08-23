# Apple `container` (Virtualization.framework) viability proof

**Run:** 2026-08-11, target host. Apple `container` 1.2.2 (VZ-backed, one Linux
VM per container), guest kernel 6.18.15, arm64 macOS 26.4.1. Harness (throwaway):
`scratchpad/proof/container_harness.sh`.

Apple `container` is a *plain OCI runtime on Virtualization.framework* — it ships
**no mediation** (no egress allowlist, no secret injection), as expected. So this
battery proves the SUBSTRATE's job (isolation) and characterizes its network
model so our own forced-proxy mediation can be built on it.

## Verdict

**Substrate VIABLE.** Isolation holds cleanly against the full battery, and the
`--network none` default-deny primitive lets us build correct forced-proxy
egress — avoiding microsandbox's raw-IP failure *by construction*. What remains
(egress proxy + host-side secret injection) is OUR mediation to build, which we
already decided. No isolation or containment defect found.

## Isolation — all attacks blocked (the substrate delivers)

- Mounted-dir containment holds vs parent traversal (`/mnt/work/../`), absolute
  host paths, symlink-to-`/`, and `/proc/self/root`. A host secret in a sibling
  dir outside the mounted subtree was unreachable by every vector.
- Read-only (`readonly`) mounts enforced.
- Guest-internal writes do not surface on the host.
- Distinct kernel `boot_id` per container — genuine separate kernels (VZ VM per
  container), not one shared kernel.
- No host processes visible in the guest process table.

## Network model (characterized for our mediation)

- **Default network** (`192.168.64.0/24` NAT): guest has **wide-open egress** —
  DNS resolves, raw-IP connects. A dev-runtime default; our mediation must
  replace it.
- **`--network none`**: clean **default-deny** — DNS denied, raw-IP denied, no
  external path. This is the building block: run the guest with no direct
  network, and make our host-side proxy the ONLY egress. Because we START from
  none and ADD only the proxy, there is no direct path for raw-IP to escape
  through — the exact microsandbox failure is structurally impossible here.

## Mediation primitives available for the next slice

- `--network none` — no direct network (default-deny by construction).
- `--publish-socket host_path:container_path` — bridge a host unix socket into
  the guest; candidate channel for the guest → host-proxy path without a routable
  network.
- `container network create` — a custom network reaching only our proxy is an
  alternative to the socket approach.
- Secret injection is entirely ours: host holds the secret (SOPS+age decrypted
  in-process), the proxy injects the auth header on egress; the guest never
  receives the value. Must avoid the GHSA mistake — never pass the secret as a
  process argument; read from env/fd host-side only.

## What this proves for the decision

Native macOS VM isolation (Virtualization.framework, via Apple `container`) is
viable as the first substrate: it isolates correctly and gives us a correct-by-
construction foundation for forced-proxy egress. The mediation (proxy + secret
injection) is the next prototype slice, then the first official Quint spec.
Substrate stays behind a thin seam (BYOS-ready) regardless.
