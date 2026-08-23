# How sandbox-backed harnesses relate the workload's filesystem to the host's

**Researched 2026-08-22.** Prompted by the daemar filesystem question: the first
slice is one-shot arbitrary-command execution inside a VM-grade sandbox with no
network, run against a worktree. Before choosing between a live host-directory
mount, copy-in/copy-out, overlay-with-diff-extraction, or a git-mediated
exchange, this is how the field actually does it. Primary sources (vendor docs,
source repos, upstream kernel/VMM documentation) are cited inline; every claim I
could not pin to a primary is called out explicitly rather than dropped.

**One-line takeaway:** *live read-write sharing of a host tree is offered only
where a human is watching in real time.* Every unattended system in every genre
exchanges at boundaries — copy or clone or fork in, patch or branch or declared
outputs back — and the systems that do offer a live mount attach an explicit
warning to it and call the sandbox "defense-in-depth", never a boundary. The
sharpest single precedent: **Kata Containers disables filesystem sharing
entirely under a TEE threat model and falls back to copy-in-at-start with no
synchronization** — a system that shares live by default abandons it precisely
when it has to be trusted.

The best one-line framing of the tradeoff belongs to Docker, whose sandbox ships
all three of daemar's candidate models behind one CLI: *"Treat sandbox-modified
workspace files the same way you would treat a pull request from an untrusted
contributor: review before you trust them on your host"*
(https://docs.docker.com/ai/sandboxes/security/isolation/).

Three corrections to intuitions we started with, all sourced below: "declared
outputs" as practiced is a *harvest filter*, not a write barrier;
overlay-with-diff-extraction is not exotic (bubblewrap ships it without root,
it is how every Docker layer is built, and it is Blaxel's default root
filesystem); and a **local, network-free, git-mediated exchange** is not
hypothetical — two products ship it.

---

## How the field was enumerated

Discovery-first, from landscape indexes rather than recall, so the list is not
biased by which vendors happen to be memorable:

- `dloss/awesome-agent-sandboxes`, which groups systems by *isolation layer* —
  VMs/microVMs, containers, process sandboxes, **filesystem sandboxes**, WASM
  runtimes, embedded interpreters
  (https://github.com/dloss/awesome-agent-sandboxes). The "filesystem sandboxes"
  category is what surfaced AgentFS and LocalSandbox, neither of which appears
  in the vendor-comparison articles.
- `tizkovatereza/awesome-ai-sandboxes`, which indexes cloud sandbox providers
  and states its information is sourced from official docs and landing pages
  (https://github.com/tizkovatereza/awesome-ai-sandboxes).
- `webcoyote/awesome-AI-sandbox` (open-source-leaning)
  (https://github.com/webcoyote/awesome-AI-sandbox), `restyler/awesome-sandbox`
  (https://github.com/restyler/awesome-sandbox), and
  `bradagi/awesome-cli-coding-agents` for the harness genre
  (https://github.com/bradagi/awesome-cli-coding-agents).
- Vendor and analyst comparison pages were used only to *widen the name list*,
  never as evidence; each system's own docs were then read for the mechanism.

Three genres were surveyed separately: agent-sandbox products/SDKs,
coding-agent harnesses, and CI/build sandboxes. A fourth cut — the local
VMM/substrate layer that daemar itself sits on — is reported below because it
constrains which patterns are even available to a local one-shot harness.

---

## Genre 0: the substrate layer — what the VMM will even let you share

This is not a genre of products so much as the floor under the other three, and
it is the constraint most likely to decide daemar's answer.

### Firecracker — no host filesystem sharing at all, by design

Firecracker's device model is deliberately minimal: the guest gets virtio-net,
virtio-block, virtio-vsock, a serial console, and a minimal keyboard
controller, and nothing else
(https://github.com/firecracker-microvm/firecracker/blob/main/docs/design.md).
There is **no virtio-fs and no host directory sharing**. The long-running
request for it (https://github.com/firecracker-microvm/firecracker/issues/1180)
is still open; the maintainers record that they "formerly rejected the p9-based
implementation for security concerns" and that adding filesystem sharing
requires them to "research our options and revisit the threat model impact."

The consequence is structural: on Firecracker, getting a working tree into the
guest means **a block device the host prepared** (the docs note backing files
must be pre-formatted with a filesystem the guest kernel supports —
https://github.com/firecracker-microvm/firecracker/blob/main/docs/design.md),
and getting results out means reading that block device back on the host. Every
Firecracker-based agent-sandbox product therefore *cannot* offer a live host
bind mount even if it wanted to — which explains a lot of the copy-in/copy-out
API shape seen in Genre 1.

### virtio-fs — the sharing mechanism that does exist, and its stated caveats

virtio-fs is the paravirtualized device for guest↔host filesystem sharing,
explicitly designed to provide filesystem access "without networking"
(https://www.kernel.org/doc/html/v5.4/filesystems/virtiofs.html). It is what
Kata Containers, cloud-hypervisor, and libkrun-family VMMs use.

Its own documentation is candid that a shared directory is a security surface
that has to be actively managed. The QEMU virtiofsd docs describe extended
attribute remapping and warn that "it is important to ensure that the remapping
does not allow a guest user to evade the guest access control rules" — an
unprivileged guest user can write `user.*` xattrs, so a naive remap of
`trusted.*` onto `user.virtiofs.trusted.*` lets the guest evade the restriction
(https://virtio-fs.gitlab.io/qemu/tools/virtiofsd.html). The same docs note the
daemon must do extra work to strip `security.capability` on many operations,
work the host kernel would normally do itself. SELinux labelling requires an
explicit `-o security_label`.

The honest reading: virtio-fs is a real, supported way to share a host
directory into a microVM, but the shared directory is a *host-writable surface
for the duration of the run*, and correctness of the containment depends on the
daemon's remapping configuration rather than on the VM boundary alone.

### Apple's Virtualization.framework — virtio-fs directory sharing, with a read-only flag

Because daemar runs on darwin, the relevant macOS-native path is
`VZVirtioFileSystemDeviceConfiguration`, "an object that represents the
configuration of a Virtio file system device... which allows the host to expose
directories to a guest using a [tag] label"
(https://developer.apple.com/documentation/virtualization/vzvirtiofilesystemdeviceconfiguration).
The shape:

- A device is created with `init(tag:)`; the tag "identifies this device in the
  guest VM" and is what the guest passes to `mount`. Apple's own docs state
  plainly that "Mounting a shared directory requires the user to execute a
  command in a terminal window" for Linux guests — i.e. the share is offered,
  not automatically present (macOS 13+ *guests* can automount via
  `macOSGuestAutomountTag`, which does not apply to a Linux guest).
- The share is either `VZSingleDirectoryShare` — "exposes a single directory
  from the host file system to the guest"
  (https://developer.apple.com/documentation/virtualization/vzsingledirectoryshare)
  — or `VZMultipleDirectoryShare`.
- The unit of sharing is `VZSharedDirectory`, "a directory on the host that you
  can expose to a guest," with `url` ("a file URL to a directory on the host
  system to expose to the guest") and, critically, **`isReadOnly` — "a Boolean
  value that indicates whether the directory is read-only to the guest"** — set
  at construction via `init(url:readOnly:)`
  (https://developer.apple.com/documentation/virtualization/vzshareddirectory).

So on macOS the live-mount option genuinely exists at the VM boundary, it is a
single named host directory, and read-only is a first-class flag rather than
something layered on top. That is the same rw/ro choice daemar already proved
empirically one layer up.

### daemar's own prior measurements

The repo already has empirical results on this exact question. `microsandbox-proof.md`
records that mounted-directory containment held against parent traversal
(`/mnt/work/../`) and absolute paths — a sibling directory outside the mounted
subtree was unreachable by every vector tried — and that read-only (`:ro`)
mounts were enforced, with the guest write refused and the host file unchanged
(`docs/research/microsandbox-proof.md`). `apple-container-proof.md` records the
same two results for Apple's container runtime
(`docs/research/apple-container-proof.md`). So for daemar's candidate
substrates, both "mount a subtree, nothing above it is reachable" and "mount it
read-only and the host file is safe" are already earned facts, not assumptions.

### A worktree is not, by itself, an isolation boundary

Since daemar's first slice is worktree-centric, it is worth stating what a
worktree does and does not separate — from git's own documentation
(https://git-scm.com/docs/git-worktree):

- **Per-worktree:** the working tree itself, plus pseudo-refs directly under
  `$GIT_DIR` — "The new worktree is linked to the current repository, sharing
  everything except per-worktree files such as `HEAD`, `index`, etc."
  Refs under `refs/bisect`, `refs/worktree`, and `refs/rewritten` are also
  per-worktree.
- **Shared:** "all refs starting with `refs/` are shared", and "By default, the
  repository `config` file is shared across all worktrees." Sharing config can
  be narrowed with `extensions.worktreeConfig`, which moves specific settings to
  `$(git rev-parse --git-path config.worktree)` — but that is opt-in and
  selective, not a general split.
- Hooks are **not discussed** in the worktree documentation at all; git's
  worktree page does not carve `.git/hooks` out as per-worktree, and I could not
  find a primary git statement that it is.

So if a sandboxed process can write to the shared `$GIT_DIR`, "it only has a
worktree" buys much less than it sounds like: shared `refs/`, shared `config`,
and an undocumented-as-separate hooks directory are all reachable. A secondary
write-up walks through exactly this — hook installation that later fires on the
host, `git config user.email` rewriting the parent's identity, a single
`refs/stash` namespace letting one worktree pop another's stash, and ref
mutation via `git gc` / force-push / `reset --hard` — and recommends
`git clone --shared` over a worktree when the driver is a program rather than a
person (https://fletch.sh/blog/git-worktrees-vs-clones-for-ai-agents/). **That
source is a blog, not a primary**; I am citing it as an argument to test, not as
established fact. The underlying sharing facts it relies on, however, are
confirmed by git's own docs above.

The practical consequence for daemar is narrow and concrete: if a worktree
directory is mounted into the guest, the mount must not transitively expose the
main repository's `.git` — and in a linked worktree it can, because the
worktree's `.git` is a *file* pointing back into the parent repository's
`.git/worktrees/<name>`, which lives outside the worktree subtree.

Measured in this repo on 2026-08-22, in an existing daemar worktree:

```
$ cat worktrees/019fe48c-.../scout/.git
gitdir: /Users/kendall/code/github/daemar/.git/worktrees/scout5
$ git rev-parse --git-dir --git-common-dir --path-format=absolute
/Users/kendall/code/github/daemar/.git/worktrees/scout5
/Users/kendall/code/github/daemar/.git
```

So the *common* git dir — the one holding shared `refs/`, shared `config`, and
`hooks/` — is an absolute host path two levels above the mounted subtree. If
only the worktree subtree is mounted into the guest, that absolute path simply
does not exist inside the guest and every git command that needs the common dir
fails; if it is mounted, the guest has write access to the shared repository
state described above. This is a real fork in the design and daemar's prior
containment proofs (parent traversal blocked, siblings unreachable) say the
first branch is what a subtree mount actually gives you.

Two surveyed systems address this directly, and they resolve it in **opposite
directions** — see Genre 2 for both:

- **Claude Code narrows the grant.** "When the working directory is a linked git
  worktree, the sandbox also allows writes to the main repository's shared `.git`
  directory so commands such as `git commit` can update refs and the index.
  Writes to `hooks/` and `config` inside that directory remain denied"
  (https://code.claude.com/docs/en/sandboxing). `git commit` works; the two
  paths that turn `.git` write access into host code execution and identity
  forgery stay closed.
- **Codex CLI denies it outright.** `PROTECTED_METADATA_PATH_NAMES = [".git",
  ".agents", ".codex"]`, and ".git directories (**including gitdir pointers**)
  are always read-only"
  (https://github.com/openai/codex/blob/main/codex-rs/protocol/src/permissions.rs).
  The agent can rewrite tracked files but cannot rewrite history, so `git diff`
  remains a trustworthy review substrate — at the cost of no committing inside
  the sandbox.

That "including gitdir pointers" clause is Codex explicitly handling the
`.git`-as-a-file case measured above. Every other system that runs an agent
against a repo clones or forks rather than mounting a worktree, which sidesteps
the question entirely.

---

## Genre 0b: filesystem sandboxes — the pattern we had not considered

`dloss/awesome-agent-sandboxes` has a category the vendor comparison articles do
not: **filesystem sandboxes**, where the isolation boundary *is* the filesystem
rather than a VM or container
(https://github.com/dloss/awesome-agent-sandboxes). Two entries matter here.

### AgentFS (Turso) — copy-on-write overlay with the host as read-only base

AgentFS describes itself as providing "filesystem-level copy-on-write isolation"
that is "system-wide and cannot be bypassed" (https://www.agentfs.ai/,
https://github.com/tursodatabase/agentfs). The design, from Turso's own
engineering post (https://turso.tech/blog/agentfs-overlay):

- **Two layers.** The **read-only base layer is the host filesystem**; the
  **writable delta layer is a SQLite-backed AgentFS database**. Reads check the
  delta first and "if not, it transparently read[] from the base layer."
- **Writes are copy-up.** A write "copies the entire file from the base layer to
  the delta layer (if it hasn't already been copied), then writes to the delta
  copy." Deletes never touch the host: "instead of actually deleting base layer
  files (which would modify the host), the overlay records them in a whiteout
  table."
- **Mount mechanism is platform-specific.** On Linux, FUSE plus namespaces:
  "create new namespaces with `unshare()`, bind-mount allowed writable
  paths...then remount everything else as read-only." On macOS, where there is
  no supported kernel-extension route, it "start[s] a localhost NFS server that
  exposes the same overlay filesystem, then mount[s] it using the built-in
  `/sbin/mount_nfs`."
- **`agentfs run` combines two things**: the copy-on-write overlay that
  "captures all file modifications without touching the host filesystem", and a
  sandbox that "enforces read-only access to all paths except explicitly allowed
  ones."
- **Extraction is a diff, and it is manual.** The CLI reference documents
  `agentfs diff` — "Show filesystem changes in overlay mode" — and
  `agentfs timeline` for the tool-call audit log
  (https://github.com/tursodatabase/agentfs/blob/main/MANUAL.md). It documents
  **no** commit/merge-back, branch, or fork command against the host filesystem;
  `agentfs sync` (`pull`/`push`/`stats`/`checkpoint`) syncs the SQLite database
  to a remote Turso database, not to the host tree. The overlay blog post
  likewise does not describe an automatic apply-back: the workflow shown is
  joining a session to inspect modifications and handling them with ordinary
  tools. So the write-back step is "human or script reads the diff and applies
  it" — an explicit review point by construction.
- Because the delta is a SQLite database, "you can query the filesystem", which
  the vendor frames as essential "for auditability and debugging agent
  behavior", and the write-ahead log "enables snapshotting and time-travel
  forking by capturing every filesystem change"
  (https://turso.tech/blog/agentfs-overlay).

This is the single most interesting design found: **the host tree is the input,
never the output.** Zero copy-in cost (the base layer is the real host tree, read
through), no guest-writable host surface at all, and the run's entire effect is a
first-class inspectable object rather than a set of mutations you have to
reconstruct by diffing afterwards. The cost is that the isolation is enforced by
a FUSE/NFS daemon plus a process sandbox, not by a VM boundary.

### Anthropic's sandbox-runtime — deny-by-default writes, allow-by-default reads

A process-level sandbox (https://github.com/anthropic-experimental/sandbox-runtime)
whose filesystem policy is deliberately asymmetric:

- **Reads:** "By default, read access is allowed everywhere. You can deny broad
  regions (e.g., `/Users`) and then re-allow specific paths within them (e.g.,
  `.`)." `allowRead` takes precedence over `denyRead`.
- **Writes:** "By default, write access is denied everywhere. You must explicitly
  allow paths (e.g., `.`, `/tmp`). An empty allow list means no write access."
  Here the precedence inverts — `denyWrite` beats `allowWrite`, so specific files
  stay protected inside an otherwise-writable directory.
- **Stated rationale:** "Both filesystem and network isolation are required for
  effective sandboxing. Without file isolation, a compromised process could
  exfiltrate SSH keys or other sensitive files."

Note the shape: the *writable* set is an explicit allowlist and defaults to
empty, exactly mirroring daemar's default-deny egress invariant, while reads are
permissive. That asymmetry recurs across the CI genre below.

---

## Genre 1: agent-sandbox products and SDKs

This genre splits architecturally, not stylistically. **Every hosted product is
exchange-at-boundaries — necessarily, because there is no host adjacent to the
workload to mount.** Live host bind mounts appear in exactly three surveyed
systems, all of which are local or self-hosted: Docker Sandboxes, microsandbox,
and OpenSandbox. That sub-genre is where daemar lives, so it gets the most space.

### A terminology warning before anything else

"Mount" and "volume" are badly overloaded here, and a daemar design doc that
compares itself to these products will collide vocabularies. Runloop's file,
code, and object "mounts", Daytona's and Blaxel's "volumes", and Vercel's
"drives" all mean **network-backed or bake-time content injection** — never
`-v /host:/guest`. When this document says *bind mount* it means a host
directory visible in the guest; when a vendor says "volume" it almost always
does not.

### Docker Sandboxes (`sbx`) — the closest analogue to daemar's decision

Docker ships **three filesystem modes behind one CLI** and documents the
tradeoffs with unusual frankness. This is the single most useful system in the
survey for daemar's purposes, because it is the only one that made all three
choices and wrote down why.

**Mode 1 — direct mount (the default): live virtiofs passthrough.** "Your
workspace is mounted directly into the sandbox through a filesystem passthrough.
The sandbox sees your actual host files, so changes in either direction are
instant with no sync process involved"
(https://docs.docker.com/ai/sandboxes/architecture/), at the same absolute path
as on the host. Docker's own security page states the consequence without
hedging — verified directly on 2026-08-22
(https://docs.docker.com/ai/sandboxes/security/isolation/):

> "By default, your workspace is shared into the VM as a read-write mount."
> "**There is no isolation between the agent and your workspace in this mode.**"

and then enumerates what the agent can therefore rewrite on your host: git hooks
(`.git/hooks/`), CI configuration (`.github/workflows/`, `.gitlab-ci.yml`),
build files (`Makefile`, `package.json`, `Cargo.toml`), editor and AI config
(`.vscode/tasks.json`, `.claude/settings.json`, `.codex/config.toml`), and
`.sbxenv.yaml` — whose commands run **on the host, as you, before the sandbox
exists**. Their stated rule is the best one-line framing of the whole problem I
found anywhere in this survey:

> "Treat sandbox-modified workspace files the same way you would treat a pull
> request from an untrusted contributor: review before you trust them on your
> host."

Note also the point Docker makes about *where* the boundary is: "The hypervisor
boundary is the isolation control, not in-VM privilege separation." A live
read-write workspace does not weaken the VM; it just means the workspace was
never on the protected side of it.

**Mode 2 — `--clone`: a local, network-free, git-mediated exchange.** The git
root is mounted **read-only** at `/run/sandbox/source`; `sbx` creates a separate
clone inside the sandbox where the agent works; a git daemon running as part of
the sandbox exposes that in-VM clone on an ephemeral localhost port, and the CLI
wires it into your host repo as a `sandbox-<name>` remote you can fetch from
(https://docs.docker.com/ai/sandboxes/workflows/git/, verified directly). "The
sandbox clone is not a Git worktree linked to your host checkout." The residual
Docker states is the one daemar must plan around: **clone mode protects the host
repository from modification, not from inspection** — `.env` and gitignored
files under the mount remain readable. (I verified the read-only mount, the
in-VM clone, the git daemon, and the `sandbox-<name>` remote against the page
directly; the exact modification-vs-inspection sentence did not appear on the
page I fetched, so treat that phrasing as reported-not-verified.)

**Mode 3 — host worktree**: a live mount of a `git worktree` directory where the
sandbox "can't resolve the `.git` pointer file and has no Git access", so the
agent can edit but not commit. That is precisely the failure mode I measured
from the other side in Genre 0 — Docker shipped it as a named mode rather than
treating it as a bug.

Docker's own comparison of when host changes appear: direct — "Immediately";
clone — "After fetch or agent push"; host worktree — "Immediately."

**Mount policy is path-allowlisted and create-time-fixed**: `read` and `write`
rules with `~/**`-style globs, checked at sandbox creation only, so "to apply a
filesystem policy change to a running workflow, remove the sandbox and create a
new one" (https://docs.docker.com/ai/sandboxes/governance/access-controls/filesystem/).
Symlinks pointing outside the workspace are not followed. Other stated failure
modes: do not use network, SMB, NFS, or cloud-synced folders as a workspace,
because "every file read and write goes over the network"; and the agent skills
store is a second host-shared read-write surface unless you opt out.

### microsandbox — bind mounts as first-class, with per-mount hardening

The security boundary is stated plainly: "A sandbox is a microVM with its own
Linux kernel, filesystem, and network stack... **The security boundary is
hardware virtualization, not Linux namespaces**"
(https://docs.microsandbox.dev/sandboxes/overview). Four mount types on both
backends — bind, named volumes, disk-image, tmpfs — and bind mounts are live and
bidirectional: "Mount a directory from the host directly into the sandbox.
Changes inside the sandbox are reflected on the host, and vice versa"
(https://docs.microsandbox.dev/sandboxes/volumes). This supersedes the 404 I hit
on the CLI reference in my first pass.

The detail that matters most for daemar: **per-mount hardening flags**, e.g.
`--mount-dir ./src:/app/src:ro,noexec` and
`--mount-file ./config.toml:/etc/app/config.toml:ro,nodev`, with an SDK
equivalent (`.bind("./src").readonly().noexec()`). Nested destinations
canonicalize and apply enclosing mounts before children, so the result is
order-independent across SDKs. daemar's own proof already covers `:ro`
enforcement and subtree containment; `noexec` and `nodev` are free additional
narrowing on the same mechanism.

Two further facts worth carrying: on microsandbox *cloud*, bind mounts do **not**
reach the caller's machine — "the host path resolves against your organization's
host volume, not the computer running the SDK or CLI" — with an explicit
security rationale for refusing default-volume lookup locally: "forgetting an API
key cannot expose or modify files from your machine." And **snapshots require a
non-running sandbox** ("microsandbox rejects running, draining, and paused
sandboxes"), capturing writable-filesystem changes, pinned image identity, and an
integrity hash, but not memory or processes
(https://docs.microsandbox.dev/sandboxes/snapshots). Directory-backed named
volumes mount through virtio-fs; disk-backed ones are raw ext4 through
virtio-blk. There is also a filesystem API that "uses the same channel as command
execution, so it doesn't touch the sandbox's network", with the vendor steering
callers to mounts for bulk transfer
(https://docs.microsandbox.dev/sandboxes/filesystem).

### OpenSandbox — a fail-closed host-path allowlist

"Host volume mounts enable bidirectional file sharing between the host machine
and sandbox environments" (https://open-sandbox.ai/examples/host-volume-mount),
via `Volume(name=..., host=Host(path="/data/shared"), mountPath="/mnt/data",
readOnly=False, subPath="subdir")`. The governing control is an allowlist in
`~/.sandbox.toml` — `allowed_host_paths = [...]` — with the rationale stated: "In
production, always set explicit `allowed_host_paths` to prevent sandboxes from
accessing sensitive host directories. An empty list allows all paths, which is
convenient for local development but not safe for shared environments."

Two design ideas worth stealing. The bind is **orthogonal to the isolation
layer** — runc, gVisor, Kata-QEMU, or Kata-Firecracker is chosen server-side and
transparently, and "SDK users and API callers require no code changes". And
startup is **fail-closed**: "the server validates the configured runtime at
startup and will refuse to start if the runtime is unavailable"
(https://open-sandbox.ai/guides/secure-container) — the same posture as Factory
Droid's `whole-process` mode refusing to start, and the opposite of Nix's silent
`sandbox-fallback`.

### The hosted cohort — exchange-at-boundaries, with instructive variety

The mechanisms are uniform (API transfer in and out, bake at creation, persist
out-of-band); what differs is the *durability model*, and the failure modes are
where the value is.

**E2B.** `files.write()`, `write_files()`, `make_dir()` in; `files.read()` as
text, bytes, or stream out; build-time `.copy()` in SDK-defined templates
(https://docs.e2b.dev/sandbox-template, https://docs.e2b.dev/filesystem). **No
host bind mount documented** — no mount parameter or host-path argument anywhere
in the SDK reference; whether the self-hosted `e2b-dev/infra` deployment exposes
one **could not be sourced to a primary**. The one live-ish affordance is
*observation, not sharing*: `files.watch_dir()` streams filesystem events so the
host learns of guest writes in real time, but the bytes still require a read.
Resume restores filesystem *and* memory; `keepMemory: false` gives a
filesystem-only cold boot (https://docs.e2b.dev/sandbox/persistence). The
correlation is clean: because the filesystem is a private microVM disk with no
host sharing, **the unit of durability is the whole sandbox, not a directory.**

**Modal.** gVisor-based (https://modal.com/docs/guide/security). Beyond the
copy APIs already covered, two details matter. `Image.add_local_dir` is an
**upload, not a live mount** — files are added as the container starts, and
`copy=True` bakes into the image instead (https://modal.com/docs/guide/images) —
and Volumes are explicitly *not* direct host directory mounts. Volumes are
**deferred** exchange: changes stay container-local until `.commit()` or a
background commit, and other containers must `.reload()`, with stated caveats
against "concurrent modifications of the same files" (last write wins)
(https://modal.com/docs/guide/volumes). Most relevant to daemar's option set:
**filesystem snapshots are stored as diffs against the base image** — only
modified files — and are promoted to the `image` of a new Sandbox
(https://modal.com/docs/guide/sandbox-snapshots). That is upperdir-as-diff by
another name, shipped as a product primitive.

**Blaxel** is architecturally the most interesting hosted case for daemar's
option 3: a **read-only EROFS base plus a writable in-memory tmpfs joined by
OverlayFS**, reserving about half the allocated memory — "reads to the base and
writes to RAM" (https://docs.blaxel.ai/Sandboxes/Overview). An overlay is not
exotic infrastructure; a commercial sandbox uses it as its *default root
filesystem*. Blaxel and E2B are the only two vendors offering `watch()`-style
change notification. Volumes carry hard constraints (one per sandbox, one
sandbox per volume, and the mount path "will override the existing content"),
while Agent Drive is a FUSE-backed distributed filesystem mountable read-only or
by subdirectory (https://docs.blaxel.ai/Agent-drive/Overview).

**Daytona** adds, beyond the FUSE/S3 volumes covered earlier, a `git` module
with clone and **push** for write-back
(https://www.daytona.io/docs/en/file-system-operations/), and states multi-tenancy
is "enforced at the FUSE mount boundary" with each sandbox seeing "only files
under its assigned subpath." Volumes are "generally slower for both read and
write operations compared to the local sandbox file system" and are "not
transactional — when two sandboxes write to the same path concurrently, the last
write wins."

**Runloop** offers four "mount" types, none of which is a host bind: file mounts
(literal content at creation), code mounts (git clone at creation), object mounts
(Runloop object storage), and blueprints (Dockerfile bake)
(https://docs.runloop.ai/docs/devboxes/mounts/code-mounts). It also carries the
bluntest durability warning in the survey: "Only disk state, not in-memory state
is preserved during suspend/resume operations", daemons "must be manually
restarted post-resume", and "**Please consistently snapshot your devboxes to
maintain disk state for your projects**"
(https://docs.runloop.ai/docs/devboxes/lifecycle). The filesystem is ephemeral by
design and durability is the caller's job.

**Cloudflare and Fly Sprites are the two poles of the same trigger.** Cloudflare:
`sleepAfter` defaults to 10 minutes and "**All disk is ephemeral. When a
Container instance goes to sleep, the next time it is started, it will have a
fresh disk as defined by its container image**"
(https://developers.cloudflare.com/containers/platform-details/) — the Durable
Object gives a stable *name*, not a stable *disk*. Cloudflare is also explicit
that a host bind is **not built in**: for local `wrangler dev` you may want to
mount a local directory, and "This can be done, but there is **no built-in
mechanism** for doing so" (https://developers.cloudflare.com/containers/local-dev/).
Sprites take the opposite route on the same idle trigger: "Every Sprite has a
persistent, standard ext4 filesystem" written to NVMe during execution, and "when
the Sprite goes idle, that data is backed up to durable object storage and
automatically restored when it wakes up" (https://docs.sprites.dev/), built on a
JuiceFS-derived stack (https://fly.io/blog/design-and-implementation/).

**Morph** is the only hosted vendor with an rsync/scp-shaped local-to-remote
transfer (`morphcloud instance copy -r ./local_dir inst_123:/remote/dir`,
`ssh.copy_to()`/`copy_from()`,
https://github.com/morph-labs/morph-python-sdk) — but it is a point-in-time copy
and the two sides diverge the instant either writes. It also documents a genuine
write-durability trap worth remembering for any snapshot-based design: "If new
files don't show up after booting from a snapshot, run Linux `sync` inside the VM
before creating the snapshot to flush pending writes." **Snapshot-of-disk does
not imply flush-of-page-cache.**

**Vercel** adds to the earlier notes that "Sandboxes are persistent by default:
when a sandbox stops, the SDK automatically snapshots its filesystem"
(https://vercel.com/docs/sandbox/concepts) — the filesystem model *is* the
lifecycle model. **Northflank** offers microVM-backed containers with
`ReadWriteMany` volumes; **Beam** is explicit copy-in/copy-out
(`sb.fs.upload_file` / `sb.fs.download_file`,
https://docs.beam.cloud/v2/sandbox/overview); **AWS Bedrock AgentCore Code
Interpreter** is pure copy-in/copy-out with inline uploads to 100 MB and S3
references for gigabyte-scale data
(https://docs.aws.amazon.com/bedrock-agentcore/latest/devguide/code-interpreter-tool.html);
**Kubernetes SIG agent-sandbox** delegates isolation to gVisor or Kata via
`RuntimeClass` and exposes a `files` API "without needing `kubectl exec`"
(https://github.com/kubernetes-sigs/agent-sandbox); **Arrakis** runs a microVM
per sandbox with whole-VM snapshot/restore so agents "can do some work, snapshot
a sandbox, and later backtrack to the exact previous state"
(https://github.com/abshkbh/arrakis); **Fly Machines** cap a volume to one
machine at a time (https://fly.io/docs/volumes/overview/).

### Genre 1 cross-cutting findings

1. **Hosted architecture forecloses the question**, and every hosted vendor
   converges on the same three-part answer: bake at creation, transfer at
   runtime, persist out-of-band.
2. **Git is the universal write-back channel** — Daytona push, Runloop `gh` PR,
   Claude Code on the web branch+PR+teleport, Codex Cloud diff+PR, Container Use
   shadow remote, envbuilder and Ona. Not a bind mount. Durable, reviewable, and
   auditable *at a boundary* rather than continuously.
3. **Live host visibility is nearly absent, and where it exists the vendor flags
   the cost** in the same breath — Docker most explicitly of all.
4. **The read/write asymmetry recurs independently.** Docker's clone mode
   protects against modification but not inspection; Claude Code reads the whole
   computer while writing only the cwd. Two unrelated teams landed on the same
   shape: **writes are cheap to confine, reads are expensive**, and both document
   the residual rather than hiding it. For daemar, whose slice-1 guest has no
   network, the read residual is far less dangerous than it is for everyone else
   here — there is no channel to carry what was read back out.
5. **Where a bind exists it is path-allowlisted and create-time-fixed.** Docker's
   `**` glob policy checked only at creation, OpenSandbox's `allowed_host_paths`,
   microsandbox's per-mount `:ro,noexec`. **None allow the mount set to change
   mid-run** — a property daemar gets for free from a one-shot lifecycle.



## Genre 2: coding-agent harnesses

This genre gave the clearest three-way split in the survey, and **no surveyed
cloud product bind-mounts a host tree**. Live host mutation is offered only
where a human is present in real time.

| Model | Repo in | Changes out | Host tree mutates mid-run? | Lifecycle |
|---|---|---|---|---|
| Live host tree | nothing moves, or a bind mount | edits in place | **yes** | long-lived interactive session |
| Clone in → branch/PR out | server-side clone at boot, often onto a cached snapshot | push branch → PR | no | one-shot or resumable async task |
| Copy/fork in → diff out | copy-in, or fork from branch state | patch file, or explicit `merge`/`apply` | no | one-shot, many parallel short envs |

### The cloud tier — clone in, pull request out

**OpenAI Codex cloud.** "Codex creates a container and checks out your repo at
the selected branch or commit SHA"; environments are pre-warmed by running the
clone plus setup script and **caching the container state** for up to 12 hours,
invalidated on setup or env changes
(https://learn.chatgpt.com/docs/environments/cloud-environment). Out is a
reviewable diff then a user-initiated PR: "Review the summary and diff… open a
pull request when the work is ready" (https://learn.chatgpt.com/docs/cloud).
Security shape worth copying: "During the agent phase, internet access is off by
default", with an allowlist and a GET/HEAD/OPTIONS restriction, and **"secrets
are removed before the agent phase starts"** — the docs include a worked
prompt-injection exfiltration example
(https://learn.chatgpt.com/docs/cloud/internet-access).

**Claude Code on the web.** "The cloud VM clones your current directory's GitHub
remote at your current branch, **not your local checkout**, so push first."
Non-GitHub repos fall back to a bundle upload — full history plus uncommitted
tracked changes, under 100 MB, untracked files excluded
(https://code.claude.com/docs/en/claude-code-on-the-web). Out is a branch push
and PR; `--teleport` pulls the branch into your local checkout and its
precondition states the boundary model outright: "Your working directory must
have no uncommitted changes." The VM is reclaimed on idle and a fresh one is
provisioned "with your conversation history restored" — **conversation durable,
filesystem disposable**. The distinctive security design: "sensitive credentials
such as git credentials or signing keys are never inside the sandbox with Claude
Code; authentication is handled through a secure proxy using scoped credentials"
(https://www.anthropic.com/engineering/claude-code-sandboxing), with an honest
residual — even with network off, "Claude Code can still communicate with the
Anthropic API, which may allow data to exit the VM."

**Cursor Cloud agents.** "Cloud agents clone your repo… and work on a separate
branch", with the base environment coming from agent-led setup, a **saved
snapshot** ("encrypted point-in-time copies of the virtual machine disk… start
or resume without recloning"), or a Dockerfile
(https://cursor.com/docs/cloud-agent, https://cursor.com/docs/cloud-agent/setup).
Out is a merge-ready PR with artifacts. Notable topological answer to the
git-is-also-egress problem: a **separate, narrower git egress proxy on three
fixed IPs**, alongside the general allowlist, with prompt injection named
explicitly — "attackers could execute prompt injection attacks, tricking the
agent to upload code to malicious websites"
(https://cursor.com/docs/cloud-agent/security-network).

**Devin.** The strongest boundary form: the repo is **pre-baked into a bootable
snapshot** — "a Linux-based virtual machine with your repositories cloned…
Every session boots from a snapshot"
(https://docs.devin.ai/onboard-devin/environment). Doubly bounded, since
"Devin's Workspace resets to a saved machine state at the start of every
session" and session changes do not persist backward into the snapshot. Unique
review affordance: **mid-run takeover** via interactive VS Code, to "check in on
Devin's edits in real time… touch up the changes"
(https://docs.devin.ai/work-with-devin/devin-session-tools). A stated
injection/egress rationale **could not be sourced to a primary** — Devin's docs
frame the snapshot as an effectiveness and reproducibility mechanism.

**Google Jules.** Clones into "a secure, short-lived virtual machine", with an
optional setup script validated by "Run and Snapshot" producing a per-repo
snapshot reused for future tasks (https://jules.google/docs/environment/). Out is
a **user-initiated** push — "You are the branch owner… Jules appears as the
commit author" (https://jules.google/docs/running-tasks/). Two review gates: plan
approval before any code changes, and a diff editor before push. Egress policy
and injection threat model: **could not be sourced to a primary.**

**GitHub Copilot coding agent.** Ephemeral GitHub Actions environment;
`.github/copilot-setup-steps.yml` runs first and is only honored from the default
branch; "If you do not check out your code, Copilot will do this for you"
(https://docs.github.com/en/copilot/concepts/agents/coding-agent/about-coding-agent,
https://docs.github.com/en/copilot/how-tos/use-copilot-agents/coding-agent/customize-the-agent-environment).
Out is a push to a Copilot-owned branch, and "Copilot can only work on one branch
at a time and can open exactly one pull request to address each task."
Hard-bounded one-shot: "maximum execution time of 59 minutes." Two review gates,
and the second is the interesting one: "GitHub Actions workflows will not run
automatically when Copilot pushes changes" — a human must click **Approve and
run workflows**, which closes the agent-writes-a-workflow-that-runs-with-repo-
credentials path. Default-deny firewall with blocked requests surfaced in the PR
body, rationale verbatim: "malicious instructions could lead to code or other
sensitive information being leaked to remote locations"
(https://docs.github.com/en/copilot/how-tos/use-copilot-agents/coding-agent/customize-the-agent-firewall).

**Codegen** captures "a filesystem snapshot that becomes the baseline for all
future agent interactions" after per-repo setup, out via PR
(https://docs.codegen.com/sandboxes/overview.md); whether the repo is baked in or
cloned at boot **could not be sourced**. **Charlie** clones into "repo-specific
prepared devbox images", denies force-push by default, and routes review through
"pull requests, comments, and issues… for human inspection before any automated
merging" (https://docs.charlielabs.ai/llms-full.txt).

### The local tier — live host tree behind an OS jail

**Claude Code (local Bash sandbox).** Nothing crosses a boundary; the sandbox is
a per-command OS jail (macOS Seatbelt, Linux/WSL2 bubblewrap plus a socat relay)
around your real cwd (https://code.claude.com/docs/en/sandboxing).

- **Default write:** "read and write access to the current working directory and
  its subdirectories, plus the session temp directory that `$TMPDIR` points to."
- **Default read:** "read access to the entire computer, except certain denied
  directories. Note that this default still allows reading credential files such
  as `~/.aws/credentials` and `~/.ssh/`."
- **Self-modification defense — the load-bearing part.** Inside the writable
  region, protected paths stay write-denied: `.claude/*`, `.mcp.json`,
  `.bashrc`/`.zshrc`, `.gitconfig`, `.git/hooks` and `.git/config`, plus
  bare-repo bait files (`HEAD`, `objects`, `refs`). The stated reason: "A command
  that could edit those files could grant itself permissions." No exemption
  exists.
- **Worktree carve-out:** "when the working directory is a linked git worktree,
  the sandbox also allows writes to the main repository's shared `.git`
  directory so commands such as `git commit` can update refs and the index.
  Writes to `hooks/` and `config` inside that directory remain denied."
- **Stated failure modes:** "not a complete isolation boundary" — no TLS
  inspection by default ("Allowing broad domains such as `github.com` can create
  paths for data exfiltration… domain fronting"), Unix-socket escalation
  (`docker.sock` "effectively grants access to the host"), and the `$PATH` /
  `.bashrc` self-widening path quoted in Genre 0b. Rationale: "especially in the
  case of prompt injection"
  (https://www.anthropic.com/engineering/claude-code-sandboxing).
- **The devcontainer reference implementation is the one hybrid**: "The host
  repository is bind-mounted into the container as the workspace"
  (https://code.claude.com/docs/en/devcontainer,
  https://github.com/anthropics/claude-code/blob/main/.devcontainer/devcontainer.json),
  with a default-DROP iptables firewall
  (https://github.com/anthropics/claude-code/blob/main/.devcontainer/init-firewall.sh)
  to make `--dangerously-skip-permissions` tolerable — and the docs name the
  exact residual tension: "Claude can still modify any file in the bind-mounted
  workspace, which appears directly on your host."

**OpenAI Codex CLI.** Kernel confinement whose writable root *is* the repo:
`workspace-write` grants write to cwd plus `writable_roots`
(https://github.com/openai/codex/blob/main/codex-rs/protocol/src/protocol.rs).
macOS uses Seatbelt via `/usr/bin/sandbox-exec`, pinned by absolute path "to
defend against an attacker trying to inject a malicious version on the PATH",
with a `(deny default)` profile "inspired by Chrome's sandbox policy"
(https://github.com/openai/codex/blob/main/codex-rs/sandboxing/src/seatbelt.rs);
Linux uses bubblewrap plus seccomp
(https://github.com/openai/codex/blob/main/codex-rs/sandboxing/src/landlock.rs).

**The detail daemar should weigh most heavily in this whole genre:**
`PROTECTED_METADATA_PATH_NAMES = [".git", ".agents", ".codex"]` — ".git
directories (**including gitdir pointers**) are always read-only"
(https://github.com/openai/codex/blob/main/codex-rs/protocol/src/permissions.rs,
https://learn.chatgpt.com/docs/agent-approvals-security). The agent may freely
mutate tracked files but cannot rewrite git history, **so `git diff` remains a
trustworthy review substrate.** Note that this is the exact opposite resolution
from Claude Code's: Codex makes `.git` read-only and gives up in-sandbox
committing to keep the audit trail sound; Claude Code grants `.git` writes minus
`hooks/` and `config` to keep `git commit` working. Two serious teams, two
different answers, both explicitly about the worktree/gitdir pointer.

**Aider.** No sandbox at all — it runs directly in the working tree and
substitutes git for isolation: "Whenever aider edits a file, it commits those
changes with a descriptive commit message", pre-editing "dirty commits"
quarantine the user's own changes from the AI's, and `/undo` and `/diff` provide
post-hoc review (https://aider.chat/docs/git.html). Review is strictly after the
fact; the model degrades outside git and for untracked files, and non-file side
effects appear in no commit. This is the third write-back discipline: **every
edit becomes a commit immediately, making git itself the audit log and the undo.**

**Zed.** Live in-place editing of project buffers with the strongest interactive
gate in the survey: per-hunk accept/reject in a Review Changes multibuffer, plus
message-level Restore Checkpoint (https://zed.dev/docs/ai/agent-panel). Edits sit
in a reviewable pending state rather than being committed. Git worktrees are
offered for **concurrency, not security**.

**Sourcegraph Amp.** Local CLI edits host files directly "with full undo
support", and **"Amp does not ask for approval before running tools"** by
default; for untrusted input the manual recommends "creating a custom policy
plugin or using an isolated development environment" — isolation explicitly
delegated to the user (https://ampcode.com/manual). Its cloud orbs let a
Changes Workflow setting commit straight to `origin/main`, meaning **a run can
have no human review point at all**. Orb clone mechanics **could not be sourced.**

**Gemini CLI.** Bind mount at the *identical absolute path*: with
`GEMINI_SANDBOX=docker|podman|…`, "your current working directory is mounted at
the exact same absolute path as it is on your host machine"; macOS offers six
Seatbelt profiles crossing write-restriction with open/proxied network, default
`permissive-open`; extra mounts via `SANDBOX_MOUNTS` default read-only
(https://raw.githubusercontent.com/google-gemini/gemini-cli/main/docs/cli/sandbox.md).
Live edits, no copy-out, and the honest caveat: "sandboxing reduces but doesn't
eliminate all risks." The identical-absolute-path trick is worth noting for
daemar — it makes tool output, stack traces, and compiler diagnostics
copy-pasteable between guest and host.

**Factory Droid.** Live host tree behind Seatbelt or bubblewrap+seccomp, writes
default-deny except cwd, network deny-all except Factory domains through a
filtering proxy, stated plainly as defense-in-depth with egress exfiltration
from readable files named as the residual hole; `whole-process` mode **refuses to
start** if the sandbox cannot initialize
(https://docs.factory.ai/autonomy-and-safety/sandbox.md). Parallelism via git
worktrees (https://docs.factory.ai/droid-exec/overview.md).

**Replit Agent** is the degenerate case: the workspace filesystem is
simultaneously sandbox, working tree, and deploy target, so no host repo exists.
Containment is checkpoints and rollback (git-backed auto-commits at milestones)
rather than review, plus a **pre-execution** Plan-mode gate
(https://docs.replit.com/replitai/agent).

### The copy-in / fork-in tier — and the precedent I said didn't exist

**SWE-agent** is textbook exchange-at-boundaries, confirmed in source
(https://github.com/SWE-agent/SWE-agent/blob/main/sweagent/environment/repo.py):
`LocalRepoConfig` **uploads a copy** of the tree into the container and refuses
to start on a dirty tree ("Local git repository … is dirty");
`GithubRepoConfig` does a depth-1 in-container clone pinned to a commit;
`PreExistingRepoConfig` bakes the repo into a per-instance image. All paths hard-
reset to a pinned commit. **Out is a `.patch` file and nothing else** — you
inspect it and `git apply`, with `--actions.apply_patch_locally` as the opt-in to
auto-apply (https://swe-agent.com/latest/usage/cl_tutorial/). The host is
untouched by default and the diff *is* the review gate. This is option 3 in the
synthesis, shipped and working, with the dirty-tree refusal as a detail daemar
should probably copy.

**OpenHands** has the sharpest configuration split in the survey. **Its default
is no host access at all** — "When no volume is explicitly mounted, the sandbox
operates in an isolated environment without access to host files"
(https://docs.openhands.dev/usage/runtimes/docker). The live mount is opt-in:
`SANDBOX_VOLUMES=$PWD:/workspace:rw` or `--mount-cwd`, with the docs' own
warning, "Anything mounted read-write into `/workspace` can be modified by the
agent." (I under-reported this in my first pass by reading the mount instructions
as the default; they are not.) Cloud OpenHands selects a repo and opens a PR;
its ingest mechanism **could not be sourced to a primary.**

**Dagger Container Use** — and this **corrects an open question I raised
earlier**, where I wrote that a local git-mediated exchange had no precedent.
It does. Container Use is an MCP server any agent attaches to: "Each agent gets a
fresh container in its own git branch" — a **fork from branch state, not a bind
mount**. The user's tree does not mutate during the run, and changes return only
through explicit user verbs `merge` / `apply` / `checkout`, with `diff` and `log`
for inspection that mutates nothing (https://github.com/dagger/container-use,
https://container-use.com/environment-workflow). Fork-per-session is precisely
what makes N parallel agents safe. The same pattern minus containers is the
entire worktree session-runner tier (vibe-kanban, cmux, Crystal, Claude Squad,
per https://github.com/bradagi/awesome-cli-coding-agents).

**Not verified against their own docs:** Tembo, Ona, Qodo, Blackbox, Niteshift,
Blocks, Replicas, 8090 — surfaced by discovery as PR-out sandbox-VM harnesses
(Tembo notable as a meta-harness running Claude Code/Codex/Cursor/Amp inside its
own VMs). Listed so the gap is visible.

### Genre 2 cross-cutting findings

1. **Live host mutation correlates with human presence; async correlates with
   git-mediated exchange.** Every unattended or cloud product exchanges at
   boundaries; every live-tree product assumes an interactive session. Long
   conversational sessions drift toward in-place editing because "persistent
   session plus copy-out-at-end" does not compose.
2. **Git is simultaneously the exchange format and the exfiltration channel**,
   and the four serious answers are architecturally distinct: Anthropic keeps git
   credentials *outside* the sandbox behind a scoped-credential proxy; Cursor
   isolates git egress *topologically* onto three fixed IPs; Codex cloud strips
   secrets *temporally* before the agent phase; Copilot restricts the write
   channel to *one branch, one PR*.
3. **Live-tree harnesses independently converge on protecting agent-control
   metadata inside the writable region** — Codex CLI's read-only
   `.git`/`.agents`/`.codex`, Claude Code's `.claude/*`, `.git/hooks`,
   `.git/config`, shell rc files — both justified as stopping the agent from
   widening its own boundary. Snapshot-reset architectures get this free.
4. **Isolation and review are substitutes, not complements, in practice.**
   SWE-agent uniquely has both. Aider and Replit bet on revertibility; Zed on
   per-hunk review; Amp on neither by default; OpenHands ships a real sandbox and
   then offers a flag that punches an `:rw` hole through it.
5. **Every vendor that states a rationale calls the sandbox defense-in-depth,
   not a boundary** (Anthropic "not a complete isolation boundary"; Factory's
   residual egress; Gemini CLI "reduces but doesn't eliminate"), and the
   strongest documented control is consistently **network egress allowlisting**,
   not filesystem confinement. For daemar, whose network answer is already
   default-deny with no egress at all in slice 1, that is a meaningful head start
   — it removes the control everyone else leans on hardest *and* the channel they
   all worry about.

---

## Genre 3: CI and build sandboxes

Five mechanism families emerged, in increasing strength of boundary:

1. **Restriction-only** (Landlock, sandbox-exec/Seatbelt) — no virtualization;
   the process sees the *real* tree minus denied operations.
2. **Reconstructed root** (bubblewrap, Bazel's symlink forest, systemd-nspawn,
   Nix, Guix, gVisor) — bind mounts, symlink farms, or a gofer build a private
   view; writability is per-mount policy.
3. **Copy-in / copy-out** (Android `sbox`, mock, sbuild, Kata under TEE,
   Firecracker rootfs images) — declared inputs copied into a scratch tree,
   declared outputs copied back, scratch destroyed.
4. **Content-addressed Merkle tree plus declared outputs** (REAPI, Buildbarn,
   Buildfarm, Buck2 RE, Dagger).
5. **Overlay upperdir as the diff** (BuildKit/containerd snapshotters, gVisor
   `--overlay2`, nspawn `--volatile=overlay`, bubblewrap `--overlay`) — the lower
   layer is never touched and the upperdir *is* the change set.

### The two findings that most change daemar's options

**Declared outputs is a harvest filter, not a write barrier — per the REAPI spec
itself.** `Command.output_paths`: "Only the listed paths will be returned to the
client as output... Other files or directories that may be created during
command execution are discarded"
(https://github.com/bazelbuild/remote-apis/blob/main/build/bazel/remote/execution/v2/remote_execution.proto#L777-L805).
The action may write anywhere in its scratch tree; the worker simply declines to
look, then destroys the tree. Android's `sbox` says the same: it "will ensure
that all outputs have been written, and will discard any output files that were
not specified"
(https://android.googlesource.com/platform/build/soong/+/refs/heads/main/android/rule_builder.go).
**A barrier — actually preventing the write — is strictly stronger than anything
these systems specify, and has to be built at the filesystem layer.** This is an
important correction to how I framed option 1 earlier: "declared outputs" alone
buys you a clean *result*, not a protected *host tree*. You need both.

**Read-only inputs is implementation convergence, not spec — with scar tissue.**
The REAPI proto is silent on input writability; the wire format cannot even
express "writable", since a `FileNode` is a digest plus one `is_executable` bit
(https://github.com/bazelbuild/remote-apis/blob/main/build/bazel/remote/execution/v2/remote_execution.proto#L1037-L1038).
Yet both major implementations enforce it independently. Buildbarn returns
EACCES at `open()` inside its virtual filesystem
(https://github.com/buildbarn/bb-remote-execution/blob/main/pkg/filesystem/virtual/blob_access_cas_file_factory.go#L209-L215).
Buildfarm chmods CAS entries `r--r--r--` **after an action wrote through a
hardlink and corrupted the shared CAS** — the fix PR opens "In order to prevent
operations from affecting cache data, we disable write permissions when the data
is created" (https://github.com/buildfarm/buildfarm/pull/396). Read-only inputs
is a discovered invariant with independent confirmations and a real incident
behind it, not an inherited convention.

### Bazel

- **Input:** a per-action `execroot` built as a **symlink forest** —
  processwrapper-sandbox "builds a sandbox directory consisting of symlinks that
  point to the original source files"
  (https://github.com/bazelbuild/bazel/blob/release-6.0.0/site/en/docs/sandboxing.md).
  `linux-sandbox` adds User/Mount/PID/Net/IPC namespaces and "makes the entire
  filesystem read-only except for the sandbox directory"; the tool's own source
  comment reads "The entire filesystem is made read-only. The working directory
  (-W) will be made read-write, though", with tmpfs overlays via `-e` and extra
  writable paths via `-w`
  (https://raw.githubusercontent.com/bazelbuild/bazel/master/src/main/tools/linux-sandbox.cc).
  Note the crucial detail: the source tree is reached *through symlinks to the
  real files*, so the read-only protection comes from the mount namespace, not
  from a copy — and on macOS (`darwin-sandbox`) there is no mount-level
  read-only guarantee at all.
- **sandboxfs** was the FUSE alternative, motivated because symlink-forest
  creation "scales linearly with the number of inputs"
  (https://blog.bazel.build/2017/08/25/introducing-sandboxfs.html); the repo is
  now archived (https://github.com/bazelbuild/sandboxfs). Worth knowing before
  reaching for FUSE: Bazel tried it and walked back.
- **Output:** the sandbox "moves the known output artifacts out of the sandbox
  into the execroot and deletes the sandbox."
- **Rationale is hermeticity, not security** — "preventing hidden dependencies"
  so incremental builds and remote caching stay correct
  (https://bazel.build/docs/sandboxing).
- **Admitted failure modes:** absolute-path reads escape — "The current sandbox
  has read permissions to its execroot and almost everything in /. If a rule
  reads a file with absolute path, bazel assumes it is a file provided by the
  operating system" (https://github.com/bazelbuild/bazel/issues/7313); host
  `/tmp` leaked until `--incompatible_sandbox_hermetic_tmp`
  (https://github.com/bazelbuild/bazel/issues/19915); linux-sandbox cannot nest
  inside Docker without `--privileged`; and sandboxing "effectively disables any
  cache the tool may have."

### Buildbarn and Buildfarm

Buildbarn materializes inputs either by **hardlink from a local cache** or via a
**lazy virtual filesystem** (FUSE or NFSv4) that "instantiates build input roots
lazily and loads input files on demand"
(https://github.com/buildbarn/bb-remote-execution/blob/main/pkg/proto/configuration/bb_worker/bb_worker.proto#L181-L204),
measured at ">95% of original speed while eliminating the downloading phase"
(https://github.com/buildbarn/bb-adrs/blob/main/0010-file-system-access-cache.md).
A failure mode daemar should note: **lazy loading makes wall-clock timeouts
attacker-influenceable**, mitigated by a *bounded* timeout-compensation budget
(https://github.com/buildbarn/bb-remote-execution/blob/main/pkg/proto/configuration/bb_worker/bb_worker.proto#L241-L255).
It even randomizes readdir order deliberately, to flush out irreproducible
actions.

Buildfarm hardlinks inputs from its CAS cache into a per-operation exec root;
"Outputs of actions are physically streamed into CAS writes when they are
observed after an action execution", then "the execution directory is destroyed"
(https://github.com/buildfarm/buildfarm/blob/main/_site/docs/architecture/workers.md).
Because hardlinks share mode bits, the executable bit costs a second inode per
digest (`<hash>` vs `<hash>_exec`)
(https://github.com/buildfarm/buildfarm/blob/main/_site/docs/architecture/CASFileCache.md);
chmod alone is advisory, so `exec_owner` runs actions as a different uid
(https://github.com/buildfarm/buildfarm/blob/main/_site/docs/execution/exec_owner.md).
The sharpest reusable idea in the whole genre: `linkInputDirectories` shares as
symlinks exactly those subtrees "containing no output paths of any kind"
(https://github.com/buildfarm/buildfarm/blob/main/_site/docs/configuration/configuration.md)
— **the output declaration is what partitions the tree into shared-immutable and
private-mutable regions.** Declaration is not only a restriction; it is what
buys sharing, caching, and auditability.

### Nix and Guix

Nix bind-mounts the **input closure only**, not the whole store — "This prevents
any access to undeclared dependencies. Directories are bind-mounted, while other
inputs are hard-linked"
(https://github.com/NixOS/nix/blob/master/src/libstore/unix/build/chroot.cc) —
and the build writes to a private `/build` tmpdir. Outputs are declared `$out`
store paths, and critically **the daemon, not the builder, canonicalizes, moves,
and registers them**
(https://github.com/NixOS/nix/blob/master/src/libstore/unix/build/derivation-builder.cc):
the sandbox never writes the real store; a privileged parent performs the
promotion. That separation — untrusted process produces candidate outputs, a
trusted parent promotes them — is directly applicable to daemar.

Caveats Nix's own code and tracker admit: the bind mounts are **not MS_RDONLY**,
so protection is ownership-based, and https://github.com/NixOS/nix/issues/3633
("Ideally the bind mount should be read-only by default") is still open. On
Darwin the Seatbelt profile must grant inputs `file-write*` because otherwise
"`access()` incorrectly returns EPERM"
(https://github.com/NixOS/nix/blob/master/src/libstore/darwin/build/darwin-derivation-builder.cc)
— a macOS-specific wrinkle daemar should expect to hit. And `sandbox-fallback`
defaults to true, a **silent downgrade** when user namespaces are unavailable
(https://nix.dev/manual/nix/2.30/command-ref/conf-file.html).

Guix builds in a full container holding "the subset of the store that the build
process depends on", uses a fixed build path so "the value of TMPDIR does not
leak inside build environments", and sets `HOME=/homeless-shelter` to surface
impurities
(https://guix.gnu.org/manual/devel/en/html_node/Build-Environment-Setup.html).

### BuildKit / Docker — overlay upperdir as the diff

The build context is a client-to-daemon incremental sync filtered by
`.dockerignore` "before it's sent to the builder"
(https://docs.docker.com/build/concepts/context/), so the build never holds a
handle on the host tree. `RUN --mount=type=bind` is "read-only by default", and
even `rw` mounts are scratch: "Written data will be discarded after the RUN
instruction completes and will not be committed to the image layer"
(https://docs.docker.com/reference/dockerfile/).

Each RUN executes in an overlayfs snapshot, and the kernel's own semantics are
what make the pattern work: "The lower filesystem does not need to be writable",
writes trigger copy-up, and deletions become whiteout files
(https://www.kernel.org/doc/html/latest/filesystems/overlayfs.html). The
upperdir — additions, copy-ups, and whiteouts — *is* the layer. Output leaves
only through exporters (`--output type=local,dest=` or `type=tar`,
https://docs.docker.com/build/exporters/local-tar/), and with no exporter
specified buildx "defaults to using the cacheonly exporter": **nothing escapes by
default.** Cache mounts are explicitly non-authoritative — "Your build should
work with any contents of the cache directory as another build may overwrite the
files or GC may clean it."

### Dagger and Earthly — the discipline stated as policy

Dagger is the one system naming security first-class: "Dagger Functions execute
in containers and thus do not have default access to your host environment", host
paths enter only as explicitly passed arguments, and explicit passing "ensures
both reproducibility and security... preventing hidden dependencies on ambient
host properties" and "prevents unauthorized data leaks"
(https://docs.dagger.io/api/arguments/). Write-back is equally explicit:
"modifications made to it in the container do not automatically transfer back to
the host"; you must "explicitly use the `export()` operation to write changes
back" (https://docs.dagger.io/getting-started/types/directory/). Admitted rough
edges: mounted directories and cache volumes silently cannot be exported
(https://github.com/dagger/dagger/issues/6434), and `withDirectory` vs
`withMountedDirectory` semantics are acknowledged-confusing
(https://github.com/dagger/dagger/issues/9349).

Earthly makes write-back transactional — "outputting images and artifacts locally
takes place only at the end of a successful build"
(https://docs.earthly.dev/docs/earthfile#save-artifact) — and its escape hatch is
**named and priced**: `LOCALLY` runs on the host, is "never cached", is warned
against, is restricted to a few commands, and is disabled under `--strict`
(https://docs.earthly.dev/docs/earthfile#locally).

### Buck2 — the cautionary case

Buck2 declares outputs up front via `declare_output` + `.as_output()`
(https://buck2.build/docs/rule_authors/writing_rules/) and defers materialization
("Buck2 will avoid downloading outputs until they are required by a local
action", https://buck2.build/docs/users/advanced/deferred_materialization/). But
**local actions are not sandboxed** — "builds are only sandboxed remotely", so
undeclared dependencies pass locally and fail on remote execution
(https://github.com/facebook/buck2/issues/358). Declaration discipline *without*
an enforcing boundary produces silent under-declaration. If daemar adopts a
declared-outputs contract, the enforcement has to be present from the first slice
or the declarations will rot.

### Android `sbox` — the purest copy-in/copy-out precedent

Declared inputs and tools are **copied** into a temp directory named by the
**hash of the manifest** — deterministic on purpose, because "some tools embed
the name of the temporary output in the output". Declared outputs are validated
("validateOutputFiles verifies that all files that have a rule to be copied out
of the sandbox were created") and copied back; undeclared outputs are discarded;
on failure there is a best-effort copy-out and the sandbox is left intact for
debugging, with idempotent cleanup at the next build
(https://android.googlesource.com/platform/build/soong/+/refs/heads/main/cmd/sbox/sbox.go).
The admitted tax is severe and directly relevant: the build system must know
*every* path reference, since raw string arguments break the layout. Tellingly,
Soong also ships a **parallel nsjail bind-mount mode** with `NsjailKeepGendir`
precisely because copy-trees cannot do incremental builds — the same project
refusing to pick one mechanism. Expect that same fork in daemar.

### Wrapper sandboxes and substrate

**bubblewrap** builds a fresh root on invisible tmpfs from `--bind` / `--ro-bind`
/ `--dev-bind`, and — the fact that most changes daemar's option set — **it has
overlay flags today, without root**: `--overlay RWSRC WORKDIR DEST` ("all writes
will go to RWSRC"), `--tmp-overlay` (writes "not persisted across multiple
runs"), and `--ro-overlay`, with source stacking order the *reverse* of the
kernel's lowerdir and no overlay source permitted to be an ancestor of another
(https://raw.githubusercontent.com/containers/bubblewrap/main/bwrap.xml). Its
posture is honest: "not a complete, ready-made sandbox... the level of protection
is entirely determined by the arguments", anything mounted in "can potentially be
used to escalate privileges", and TIOCSTI needs `--new-session`
(https://github.com/containers/bubblewrap/blob/main/README.md). One design
argument worth stealing: bubblewrap rejects path-allowlisting and always accesses
the filesystem as the invoking uid — "This entirely closes TOCTTOU attacks."

**Landlock** is "designed to restrict access, not virtualize resources", with
admitted gaps that matter: chroot is not denied, chmod/chown/stat/chdir are
invisible to filesystem rights, "a policy restricting an OverlayFS layer will not
restrict the resulted merged hierarchy, and vice versa", `/proc` fd paths are
unrestrictable, and there is a 16-layer cap
(https://docs.kernel.org/userspace-api/landlock.html). **sandbox-exec/Seatbelt**
is deprecated per its own man page, and `man 7 sandbox` states that restrictions
are "enforced upon acquisition of operating system resources only... if the
application already has a file descriptor opened for writing, it may use that
file descriptor regardless of restrictions" (Apple-shipped man pages, read
locally; Apple hosts no canonical web copy). Chromium exploits that fd hole
deliberately, opening resources before applying the profile
(https://chromium.googlesource.com/chromium/src/+/HEAD/sandbox/mac/seatbelt_sandbox_design.md).
**Both restriction-only mechanisms leak through already-open fds by documented
design** — which is why they cannot deliver a write *barrier*.

**gVisor** admits filesystem access only via the **gofer** — "the Sentry is not
able to create or open host file descriptors itself, it can only receive them...
from the Gofer" (https://gvisor.dev/docs/architecture_guide/resources/) — and
`--overlay2` diverts writes into an overlay as a per-mount live-vs-boundary knob
(https://gvisor.dev/docs/user_guide/filesystem/). **Kata** shares host volumes
via virtio-fs, but the striking datum is what happens under a stronger threat
model: **under TEE, filesystem sharing is disabled and "host files are copied
into the guest VM when the container starts, and file changes are not
synchronized"** (https://github.com/kata-containers/kata-containers/blob/main/docs/Limitations.md).
A forced live-to-boundary downgrade exactly when the threat model hardens — which
is the single most on-point precedent in the survey for a security-first harness.

**Distro tools** are worth one line each for their candor: mock says "Mock is not
safe for unknown RPMs" and "There are known ways to get root access once a user
is in the mock group" (https://rpm-software-management.github.io/mock/); sbuild
extracts a chroot from a tarball per build and copies `.deb`/`.changes` out
(https://manpages.debian.org/unstable/sbuild/sbuild.1.en.html); systemd-nspawn
offers genuinely read-only `--bind-ro`, `--volatile=overlay`, and `--ephemeral`
("no modifications of the container image are retained"), while warning that
"overlayfs behavior differs from regular file systems in a number of ways, and
hence compatibility is limited" and that "untrusted code must always be run in a
user namespace" (https://www.man7.org/linux/man-pages/man1/systemd-nspawn.1.html).
**toolbx and distrobox** are the deliberate anti-pattern: distrobox states
"Isolation and sandboxing are not the main aims of the project, on the contrary
it aims to tightly integrate the container with the host"
(https://github.com/89luca89/distrobox/blob/main/docs/README.md).

### Genre 3 cross-cutting findings

1. **Filter versus barrier.** Declared outputs everywhere means "we harvest only
   these and destroy the rest", never "you cannot write elsewhere." A barrier is
   achievable only structurally: ro-bind plus overlay, a copy tree, a virtual
   filesystem returning EACCES, or a VM. Restriction-only mechanisms and chmod
   bits do not get there.
2. **Every system has an escape hatch; the safe ones are named and priced.**
   Earthly's `LOCALLY` (uncacheable, banned in strict mode) versus Nix's
   `sandbox-fallback=true` (silent downgrade by default) versus Buck2's
   unsandboxed local execution (surfaced only as a bug report).
3. **Copy-in/copy-out versus incrementality is a real fork**, and Android ships
   both mechanisms rather than choosing.
4. **The recurring adversary for any live-sharing path is symlinks and absolute
   paths** — Bazel's absolute-path reads, REAPI's
   `SymlinkAbsolutePathStrategy.ALLOWED` ("possibly resulting in non-hermetic
   builds"), virtiofsd's pivot_root defense, gVisor's O_NOFOLLOW discipline.
   daemar's containment proofs already tested parent traversal and absolute
   paths, which is exactly the right target.

---

## Synthesis

### Dominant pattern per genre

| Genre | Dominant pattern | Live host share? |
|---|---|---|
| Hosted agent sandboxes | Copy/upload API in, download API out; provider-side volumes for durability | No — no host exists |
| Local/self-hosted sandbox runtimes | Path-allowlisted, create-time-fixed bind mount, per-mount `ro`/`noexec` | Yes, and every vendor flags the cost |
| Cloud coding agents | Git-mediated: clone in, branch/PR out | No |
| Local coding agents | Live read-write mount of the real tree, constrained by a path policy | Yes |
| Copy/fork coding agents | Upload copy or fork-a-branch in; patch or explicit `merge` out | No |
| CI / build | Read-only reconstructed inputs, declared outputs harvested out | No |
| Filesystem sandboxes | Host as read-only base, writes to a copy-on-write delta, `diff` to extract | No — reads only |

### What actually predicts the filesystem model

Three variables, in order of explanatory power:

1. **Human presence.** This is the strongest signal, and it came out of the
   harness data unambiguously: live host mutation is offered *only* where a human
   is watching in real time. Every unattended product exchanges at boundaries.
   A one-shot, unattended run is squarely on the exchange side of that line.
2. **Co-location.** If the sandbox is not on the user's machine, a live mount is
   not on the menu at all — and on Firecracker it is not on the menu even
   locally (Genre 0). This explains the hosted genre wholesale.
3. **Whether the system is accountable for the correctness of its result.**
   Build systems, which are, chose read-only inputs and declared outputs
   *independently of any threat model*, purely for hermeticity — and then
   discovered the security benefit. Local agent harnesses, which are not, chose
   the live mount for convenience and bolted on a path policy afterward.

My earlier framing — that lifecycle is not the driver — was half right and
needs correcting. Within the coding-agent genre, lifecycle tracks the filesystem
model almost perfectly, because lifecycle is a *proxy* for human presence:
interactive sessions are attended, async tasks are not. The counterexamples I
cited (Vercel's persistent copy-in/copy-out) come from the hosted genre, where
variable 2 dominates and variable 1 never applies.

**And the strongest single precedent for daemar's situation is Kata's:** when the
threat model hardens to TEE, Kata *disables filesystem sharing* and switches to
copy-in-at-start with no synchronization. A system that offers live sharing by
default abandons it precisely when it must actually be trusted.

### Patterns we had not considered

- **Overlay upperdir as the change set** — and it turned out to be
  *triply* precedented, not exotic. bubblewrap ships `--overlay` /
  `--tmp-overlay` **without root**; Blaxel's commercial sandbox uses a read-only
  EROFS base plus a tmpfs upper joined by OverlayFS as its **default root
  filesystem**; and Modal's filesystem snapshots are stored as **diffs against
  the base image**, only modified files, promoted into a new Sandbox's image.
  Kernel-guaranteed untouched inputs, a complete reviewable change set (upperdir
  plus whiteouts), discard-on-abort, and commit-as-artifact all come for free,
  without the FUSE/NFS machinery AgentFS needs.
- **A read-only source mount with an in-guest clone and a localhost git daemon**
  (Docker Sandboxes `--clone`) — a complete local, network-free, git-mediated
  exchange. Read-only in, fetch-from-a-`sandbox-<name>`-remote out.
- **Per-mount hardening beyond `ro`** — microsandbox's `:ro,noexec` and
  `:ro,nodev`. daemar has already proven the `:ro` half; the rest is free.
- **Fail-closed startup as a stated posture** — OpenSandbox refuses to start if
  the configured runtime is unavailable; Factory Droid's `whole-process` mode
  does the same. The counterexample is Nix's `sandbox-fallback=true`, a silent
  downgrade by default.
- **Declared outputs as the partition key.** Buildfarm shares subtrees as
  symlinks precisely because they contain "no output paths of any kind" — the
  output declaration is what splits the tree into shared-immutable and
  private-mutable.
- **Trusted-parent promotion.** Nix's builder never writes the real store; the
  daemon canonicalizes and registers outputs after the fact. The untrusted
  process produces candidates; a trusted process promotes them.
- **Two opposite resolutions of the `.git` question**, both explicit: Codex CLI
  makes `.git` (including gitdir pointers) permanently read-only so `git diff`
  stays trustworthy; Claude Code grants `.git` writes minus `hooks/` and `config`
  so `git commit` works.
- **Fork-a-branch-per-run with explicit `merge`/`apply`** (Dagger Container Use)
  — the local, network-free, git-mediated exchange I had wrongly recorded as
  having no precedent.
- **Content-addressed input trees**, making "what did this run see?" exactly
  answerable, and identical inputs shareable for free.
- **Deterministic scratch paths** (Android sbox hashing the manifest; Guix's
  fixed build path) so that tools which embed their working directory in their
  output stay reproducible.

### What this implies for daemar's first slice

Constraints: one-shot, no network in the guest, worktree-centric, local, macOS
host, VM-grade substrate already proven for `:ro` mounts and subtree containment.

- **Unattended plus one-shot puts daemar on the exchange-at-boundaries side of
  the field's sharpest line.** Nothing in the survey offers a live read-write
  host tree to an unattended run.
- **No network does *not* rule out git-mediated exchange**, and there are now
  *two* shipped local precedents. Dagger Container Use forks a branch per agent
  and returns work via explicit `merge`/`apply`. Docker Sandboxes `--clone`
  mounts the git root **read-only**, has the agent clone from it inside the VM,
  and publishes that clone back over a **localhost git daemon** wired up as a
  `sandbox-<name>` remote. Both are entirely local. I had this wrong earlier.
- **Docker Sandboxes is the closest analogue to daemar's exact decision** — same
  local-VM shape, same worktree problem, and it shipped all three answers
  (live rw, read-only-plus-clone, worktree-that-cannot-commit) with the tradeoffs
  written down. Its framing is the one to argue against or adopt deliberately:
  "Treat sandbox-modified workspace files the same way you would treat a pull
  request from an untrusted contributor."
- **"Declared outputs" alone is a filter, not a barrier.** If the goal is that
  the guest *cannot* write the host tree, that must come from the mount policy or
  an overlay, with declaration layered on top for the harvest.
- **macOS is the awkward platform for every mechanism here**: no mount-level
  read-only guarantee under Bazel's darwin-sandbox, Nix needing `file-write*` on
  Darwin because `access()` misreports, Seatbelt deprecated and fd-leaky, and
  AgentFS resorting to a localhost NFS server. daemar's VM boundary sidesteps all
  of it — which is an argument for doing the containment at the VM layer rather
  than with a host-side process sandbox.

**Five** realistic options, stated neutrally — **this is Kendall's decision.**
(The list grew by one from my earlier draft: Docker's clone mode is a mechanism
genuinely distinct from the other four, and it is shipped, so filing it under
one of them would misrepresent the evidence.)

1. **Read-only worktree in, writable scratch, declared outputs harvested out.**
   The CI contract, and the best-precedented option in the survey (Bazel, Nix,
   REAPI, Android sbox, BuildKit, Dagger). daemar's `:ro` proof already covers
   the barrier half; the declaration is the harvest half. Add Nix's
   trusted-parent promotion so the host-side write is done by daemar, not the
   guest. Cost: Android's admitted tax — the harness must know every path the
   command will produce — and Buck2's warning that declaration without
   enforcement rots.
2. **Copy-in / diff-out.** Copy the worktree in, run, diff against the pristine
   copy, surface a patch. SWE-agent ships exactly this, including a detail worth
   copying: it refuses to start on a dirty tree. No cooperation required from the
   command, explicit review point, host untouched by default. Cost: copy latency
   proportional to tree size, no incrementality (the fork Android refused to
   resolve), and you must define "changed" for modes and binaries.
3. **Overlay with the worktree as read-only lower.** Kernel-guaranteed untouched
   input, upperdir *is* the reviewable change set, discard-on-abort is free.
   Reachable via bubblewrap's `--overlay` without root on Linux, and via an
   overlay inside the guest over a `:ro` mount on macOS. This is the pattern I
   previously filed as a research spike; the bubblewrap and BuildKit evidence
   makes it a legitimate slice-1 candidate. Cost: overlayfs compatibility quirks
   that nspawn's docs warn about, and the ancestor-directory constraints
   bubblewrap documents.
4. **Read-write worktree mount, narrowed.** Mount the worktree `:rw` and nothing
   else; protect agent-control metadata inside the writable region the way both
   Codex CLI and Claude Code independently do. Cheapest to ship and what local
   harnesses actually run. Cost: it is the one option the field reserves for
   attended sessions, and daemar's slice-1 runs are unattended.
5. **Read-only source mount plus an in-guest clone, exchanged over a local git
   remote.** Docker Sandboxes `--clone`, shipped: mount the repo root `:ro`, let
   the guest clone from it into VM-private storage, and expose that clone back to
   the host as a fetchable remote. Gets `git` working *inside* the guest — which
   options 1 through 3 do not, and which matters if the one-shot command is
   itself a build or test that shells out to git. Review is `git fetch` plus
   `git log`/`diff`, the most familiar review point available. Costs, both
   stated: the read-only mount "protects your host repository from modification,
   not from inspection", so `.env` and gitignored files under it stay readable;
   and it needs a git daemon or an equivalent transport, which is machinery
   daemar would otherwise not have. Dagger Container Use is the same family with
   a different transport (`merge`/`apply` into a shadow remote).

Options 1, 2, 3, and 5 all produce a review point with no writable host surface;
they differ in *who declares what comes back* — the caller up front (1), a diff
computed afterward (2), the kernel (3), or git itself (5). Option 3 is the only
one that is both zero-copy and zero-exposure. Option 5 is the only one where
`git` works normally inside the guest.

Three details recur across enough independent systems to be worth adopting
whatever is chosen:

- **Protect `.git`, or at minimum `hooks/` and `config`.** Codex CLI, Claude
  Code, and Docker's escalation list all converge here from different directions.
- **Make every escape hatch loud, opt-in, and costly** — Earthly's `LOCALLY` and
  OpenSandbox's refuse-to-start, not Nix's silent `sandbox-fallback`.
- **Fix the mount set at creation and never mid-run.** Docker, OpenSandbox, and
  microsandbox all do this; a one-shot lifecycle gives daemar the property free.

---

## Open questions and unsourced claims

Corrected from my earlier draft:

- **"Local git-mediated exchange has no precedent" was wrong, twice over.**
  Dagger Container Use forks a branch per agent and returns changes via explicit
  `merge`/`apply` (https://github.com/dagger/container-use); Docker Sandboxes
  `--clone` mounts the git root read-only, clones in-VM, and publishes back over
  a localhost git daemon (https://docs.docker.com/ai/sandboxes/workflows/git/,
  verified directly 2026-08-22).
- **"OpenHands defaults to a read-write mount" was wrong.** Its default is no
  host access at all; the mount is opt-in
  (https://docs.openhands.dev/usage/runtimes/docker).
- **"Overlay-with-diff is unproven and a research spike" was overstated.**
  bubblewrap ships `--overlay` and `--tmp-overlay` without root, BuildKit's whole
  layer model is upperdir-as-diff, Blaxel uses EROFS+tmpfs+OverlayFS as its
  default root filesystem, and Modal stores filesystem snapshots as diffs against
  the base image.
- **"microsandbox's mount flag syntax could not be sourced" is now resolved** —
  https://docs.microsandbox.dev/sandboxes/volumes documents bind, named, disk-
  image and tmpfs mounts with per-mount `:ro,noexec` / `:ro,nodev` flags. Only
  the CLI *reference* page 404'd.
- **"No sandbox product documents the linked-worktree `.git` pointer problem" was
  wrong.** Codex CLI protects gitdir pointers explicitly, Claude Code carves out
  `hooks/` and `config`, and Docker ships a host-worktree mode whose documented
  behavior is that the guest "can't resolve the `.git` pointer file."

Still open:

- **Firecracker excluding virtio-fs specifically for security**: the design docs
  justify the minimal device model by startup time and footprint; issue #1180
  records the p9 rejection "for security concerns" but no clean statement covers
  virtio-fs. Partially sourced at best.
- **Whether Apple's Virtualization.framework virtio-fs shares QEMU virtiofsd's
  xattr-remapping caveats.** Apple's docs do not discuss it. **Could not source.**
- **REAPI input-root writability**: the spec is silent; the wire format cannot
  express it. Read-only inputs is implementation convergence only.
- **A Docker doc explicitly stating the build cannot write the host source tree**
  — inferred from context-copy plus rw-mount-discard semantics, not stated.
- **Jules' egress policy and injection threat model; Devin's security rationale;
  OpenHands Cloud's ingest mechanism; Amp orbs' clone mechanics; Anthropic
  Managed Agents' git workflow; Factory Droid Computers' current ingestion;
  Codegen's clone-vs-prebake; Antigravity CLI's mount/egress policy** — all
  **could not be sourced to a primary.**
- **Harnesses not verified against their own docs:** Tembo, Ona, Qodo, Blackbox,
  Niteshift, Blocks, Replicas, 8090. (Devin, Cursor, Amp, Aider, SWE-agent, Zed,
  Gemini CLI, Factory, Replit and Container Use *were* read and are cited in
  Genre 2; they are no longer on this list.)
- **Docker Sandboxes' "protects from modification, not from inspection"
  sentence** did not appear on the clone-mode page when I fetched it directly;
  the read-only mount, in-VM clone, git daemon, and `sandbox-<name>` remote all
  did. Treat that one phrasing as reported-not-verified.
- **Hosted-product sourcing gaps** (all **could not be sourced to a primary**):
  E2B self-hosted host-directory mounts; Daytona self-hosted runner host-path
  mounting (runners page 404'd); Runloop's hypervisor type, object-mount
  writability, and "scenarios" filesystem semantics; Fly Machines' hypervisor
  name and the circulated ~300ms Sprites checkpoint figure; CodeSandbox's
  filesystem method names, local runner, or bind mount (docs returned 403);
  Northflank's file and snapshot APIs; Vercel's `source: {type:'git'}`; whether
  Morph EFS or Blaxel Agent Drive can be mounted on a developer's local machine;
  the Kubernetes SIG agent-sandbox volume model (doc page is frontmatter only);
  Replit Agent's filesystem mechanism.
- **Build systems not read:** GitHub Actions runner internals beyond
  `actions/checkout`, GitLab CI executors, Guix beyond the environment doc,
  containerd snapshotter Prepare/Commit lifecycle, BuildKit's fsutil wire
  protocol, Buck2's input-staging mechanics (architecture page 404'd).
- **Vendor comparison pages were used only to widen the name list**, never as
  evidence. Their quantitative claims (checkpoint latencies, snapshot TTLs) were
  not verified and are not repeated here.
