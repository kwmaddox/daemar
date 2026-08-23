# What sandbox harnesses run inside the box, and how their driver gets there

**Researched 2026-08-22/23.** Prompted by two daemar decisions that had to be
made together: which guest image the first slice defaults to, and whether the
in-guest driver is a generated shell script (which requires the image to carry
`/bin/sh` and coreutils) or a static injected binary (which does not). Primary
sources — vendor docs, source repos, first-party blogs — are cited inline per
claim. Everything I could not pin to a primary is called out as "could not
find" rather than dropped.

**One-line takeaway:** *no primary source in this field argues that minimal
image contents are a security control for a sandbox that runs arbitrary code by
design.* Every published security argument across the systems surveyed is about
the isolation boundary (hypervisor, gVisor, Kata, Hyper-V), the network (egress
allowlists, credential brokering, no-internet-at-all), or the control plane
(RBAC, SSRF, label spoofing). Image contents appear in threat models either not
at all, or — in the single vendor that addresses them head on — as a
*supply-chain* concern about running the wrong image, explicitly not an
escape concern.

The sharpest single line belongs to Docker, whose sandbox ships a guest running
Ubuntu with passwordless `sudo` and a privileged `dockerd` inside it: *"The
agent runs as a non-root user with sudo privileges inside the VM. The
hypervisor boundary is the isolation control, not in-VM privilege
separation"* (https://github.com/docker/docs/blob/main/content/manuals/ai/sandboxes/security/isolation.md).

The second takeaway cuts the other way and is the one that bears on daemar's
driver question: **on the injection axis the field has largely converged on
image-independence**, and it did so for engineering reasons rather than
security ones. Apple, Kata (in its hardened mode), microsandbox, Fly, and
Vercel all arrange for the supervising process to owe the guest image nothing.
Even GitHub Actions — the most maximalist image in the industry — ships a
control process with zero userland dependency on the image it supervises.

---

## Method and its limits

Discovery-first: the roster came from landscape indexes and vendor comparison
corpora rather than recall, then each system's own docs and source were read
for the mechanism. Comparison pages (notably Northflank's
https://northflank.com/blog/sandbox-providers) were used **only to widen the
name list**, never as evidence.

Two limits to record honestly. First, the WebSearch budget was exhausted
(200/200) partway through, so several sub-passes were completed by direct URL
fetch and raw-source reads instead of search; a few "could not find" entries
below might close with a fresh search pass. Second, `sbx`'s VMM, CodeSandbox's
Firecracker posts (Cloudflare 403, archive.org unreachable), and AWS Lambda's
guest init remain genuinely unretrieved — see the open questions at the end.

---

## Q1 — Image selection across the field

### The maximal pole: everything-installed, vendor-built, convenience-justified

**OpenAI Codex.** The published reference image is literally `FROM ubuntu:24.04`
(https://raw.githubusercontent.com/openai/codex-universal/main/Dockerfile), with
every apt package version-globbed (`git=1:2.43.*`, `tzdata=2026a-*`). Language
runtimes are not baked but selected at boot from an enumerated allowlist via
`mise` and `CODEX_ENV_*` variables (Python 3.10–3.14, Node 18/20/22, Rust
1.83–1.95, Go 1.22–1.25, Swift, Ruby, PHP, JDK 11–25). Docs confirm the
production path: *"The Codex agent runs in a default container image called
`universal`, which comes pre-installed with common languages, packages, and
tools"* (https://learn.chatgpt.com/codex/environments/cloud-environment). The
repo disclaims being that image: *"a reference implementation... This is not an
identical environment"* (https://github.com/openai/codex-universal).

The stated rationale is convenience, not security — *"comes with languages
pre-installed for speed and convenience"*. OpenAI's actual security argument is
entirely about network: internet off by default during the agent phase, named
risks *"Prompt injection from untrusted web content"* and *"Code or secret
exfiltration"* (https://learn.chatgpt.com/codex/cloud/internet-access).

**GitHub Actions hosted runners** are the clearest first-party statement of the
everything-installed philosophy, and the most useful because the policy is
written down. The Preinstallation Policy criteria are: Popularity, Latest
Technology, Deprecation, Licensing, Time & Space on the Image, and Support
(https://github.com/actions/runner-images). **Attack surface is not one of the
six criteria.** Multi-version side-by-side installation is explicit policy —
Java "all LTS versions", Node "3 latest LTS versions", Python/Ruby "5 most
popular major.minor versions". Images are rolling, not pinned: *"We typically
deploy weekly updates to the software on the runner images."* The only
supply-chain statement covers roughly a dozen third-party apt repos and amounts
to an annual review: *"Third-party repositories are re-evaluated every year to
identify if they are still useful and secure."*

**Fly Sprites** is the maximal-opposite-of-distroless contrast case. Ubuntu
25.04, a `sprite` user with **passwordless sudo**, pinned Node 22.20.0 / Python
3.13.7 / Go 1.25.1 / Rust 1.90.0 / Ruby 3.4.6 / Elixir / Erlang / Java 25 / Bun
/ Deno each with its version manager, plus `build-essential`, clang, git, five
shells, tmux, curl, wget, openssh-client — **and four coding agents pre-baked**
(Claude Code, Gemini CLI, Codex, Cursor)
(https://github.com/superfly/sprites-docs/tree/main/src/content/docs). The
security posture is egress-only: a DNS allowlist at
`/.sprite/policy/network.json`, read-only from inside and mutable only from
outside; raw-IP egress blocked unless resolved from an allowed domain; private
IPs always blocked; connectors broker credentials so the guest never holds a
long-lived token. Zero argument about image contents.

**Docker Sandboxes** publishes its guest as `docker/sandbox-templates:<variant>`
— *"based on Ubuntu and run as a non-root `agent` user with sudo access. Most
variants include Git, Docker CLI, and common development tools like Node.js,
Python, Go, and Java"*
(https://github.com/docker/docs/blob/main/content/manuals/ai/sandboxes/customize/templates.md).
The default `-docker` variants run *"in privileged mode inside the microVM...
and `dockerd` starts automatically inside the sandbox."* A
`claude-code-minimal` variant exists but is scoped as toolset trimming, not
security: *"Claude Code with a minimal toolset (no Node.js, Python, Go, or
Java)."* Docker's five stated layers are hypervisor, network, Docker Engine,
workspace, and credential proxy — guest contents carry no weight.

**Anthropic's own code execution tool** inverts the logic in a way worth
recording: *"The container has no internet access, so Claude can't download or
install additional packages at runtime"*
(https://platform.claude.com/docs/en/docs/agents-and-tools/tool-use/code-execution-tool).
No egress is precisely what *forces* the image to be fat.

### The middle: vendor-built defaults with a bring-your-own escape hatch

**Vercel Sandbox** offers managed images under `vercel/sandbox` —
`universal:latest` (default, Ubuntu-based, Node LTS 24, Python 3.14, coding
agents), plus `node:22|24|26`, `python:3.14`, `arch:latest`, `ubuntu:latest`
(base `ubuntu:26.04`, Ubuntu + sudo)
(https://vercel.com/docs/sandbox/concepts/images). Pinning is first-class: the
`image` field accepts a digest (`vercel/sandbox/universal@sha256:...`). Managed
images are open source (https://github.com/vercel/sandbox/tree/main/images) with
a nightly release; rolling tags *"pick up operating system updates
automatically, including security patches."* Custom images go through the
Vercel Container Registry, gated on an optimized `linux/amd64` build being
ready. Isolation is stated as Firecracker; **no security argument about image
contents — could not find one.** The stated rationale for prebuilt images is
convenience: *"nothing to install at runtime."*

**E2B**'s default base is `e2bdev/base` — vendor-built and, notably, an
**unpinned mutable tag** (`private defaultBaseImage: string = 'e2bdev/base'`,
`packages/js-sdk/src/template/index.ts:66`). Templates are now SDK-defined but
`fromDockerfile()` still exists and any OCI base works.

**Daytona** enforces pinning on user images where E2B does not: *"the
`latest`/`lts`/`stable` tags are not supported"*, plus `--platform=linux/amd64`
(https://www.daytona.io/docs/en/snapshots/). Default snapshots are resource
shapes (`daytona-small` etc.) that *"include pre-installed Python and Node.js
packages"*; **the underlying OCI image reference for those defaults could not be
found** anywhere public or in the OSS tree. Caveat: the OSS repo is dead —
*"This repository is no longer maintained. As of June 2026, Daytona's core
development has moved to a private codebase"* — so the SaaS may have drifted
from `v0.190.0`.

**Modal** has the highest userland dependence of the set and states it as a
hard requirement: *"the image needs to have `python` and `pip` installed and
available on the `$PATH`"*, plus linux/amd64 and a compatible `ENTRYPOINT`
(https://modal.com/docs/guide/existing-images). Default is *"a Debian Linux
container with a basic Python installation of the same minor version v3.x as
your local Python interpreter"* (https://modal.com/docs/guide/images). The
escape hatch, `from_registry("ubuntu:22.04", add_python="3.11")`, overlays a
standalone Python. Isolation: *"Compute jobs at Modal are containerized and
virtualized using gVisor"* (https://modal.com/docs/guide/security).

**Azure Container Apps dynamic sessions** is the most opaque. Code interpreter
pools use *"platform built-in containers"* — vendor-built, *"Image requirement:
None"*, no image URI accepted, and Microsoft never publishes the distro or a
version. Contents are described only as *"popular Python packages such as
NumPy, pandas, scikit-learn"*; the docs' actual answer to "what's in it?" is to
run `pkg_resources.working_set` inside the sandbox yourself. No manifest, no
pinning, introspection-only. The security argument is purely boundary and is
stated hard: *"Each code interpreter session is fully isolated by a Hyper-V
boundary and is designed to run untrusted code"*
(https://learn.microsoft.com/en-us/azure/container-apps/sessions).

### The minimal pole, and who actually occupies it

Almost nobody, for the workload image. The minimal-guest practice that does
exist is applied to the *VM's own* filesystem, not to the image the user's code
runs in — a distinction that turns out to be the crux of Q2.

**Apple** states a security rationale, and it is worth reading precisely: each
container has *"the isolation properties of a full VM, using a minimal set of
core utilities and dynamic libraries to reduce resource utilization and attack
surface"*
(https://github.com/apple/container/blob/main/docs/technical-overview.md). That
sentence describes Apple's own init image, not the OCI image you `container
run`. `container` consumes and produces standard OCI images with no constraint
on their contents.

**Kata** makes the same move and is the one system to justify a minimal guest
distro on explicit security grounds. In `guest-assets.md`'s image summary
table, the initrd row (default distro Alpine, init daemon = the Kata agent) has
the reason column **"Security hardened and tiny C library"**, while the
rootfs-image row's reasons are only "Fully tested in our CI" and "systemd
offers flexibility"
(https://raw.githubusercontent.com/kata-containers/kata-containers/main/docs/design/architecture/guest-assets.md).
Again: this is the *guest* image hosting the workload's VM, and Kata says so
outright — *"The guest image is unrelated to the image used in a container
workload."*

**microsandbox** runs plain upstream OCI images — *"Existing images run as-is"*
— but **auto-pins** them: *"`python` is resolved to a specific set of immutable
layers at first pull… reproducible even if the upstream tag moves"*
(`docs/images/overview.mdx`).

**Kubernetes SIG-Apps `agent-sandbox`** has no base image by design — a
`Sandbox` CRD whose image comes from the user's `spec.podTemplate`
(https://github.com/kubernetes-sigs/agent-sandbox). Its formal threat model is
the strongest negative evidence in this whole survey; see the verdict section.

---

## Q2 — How the driver gets into the guest

Four models, ordered by how much they owe the image's userland.

### (a) Platform-injected, image-independent — the dominant pattern

**Apple `vminitd`** is the cleanest instance and the one daemar sits closest
to. The mechanics, verified in source:

- It is Swift, built **statically against musl** via the Swift Static Linux SDK
  — `LIBC ?= musl`, `--swift-sdk $(MUSL_ARCH)-swift-linux-musl`
  (https://github.com/apple/containerization/blob/main/vminitd/Makefile).
- It arrives as a **separate OCI image**, unpacked to its own read-only ext4
  block device that becomes the VM's root filesystem. `InitImage.initBlock`
  runs `EXT4Unpacker(capacityInBytes: 512.mib())` and sets `fs.options = ["ro"]`
  (https://github.com/apple/containerization/blob/main/Sources/Containerization/Image/InitImage.swift).
  The doc comment states the intent: *"the image to use as the root filesystem
  for a virtual machine. Typically this image would contain the guest agent
  used to facilitate container workloads."*
- The **user's** OCI image is a different block device entirely, mounted at
  `/run/container/<id>/rootfs`, which `vmexec` then `pivot_root`s into
  (`guestRootfsPath`, `LinuxContainer.swift`; `CZ_pivot_root` in
  `vminitd/Sources/vmexec/RunCommand.swift`).
- The workload is started by **direct `execve`** — there is no `/bin/sh`
  anywhere in the exec path. `vminitd` *"is spawned as the initial process
  inside of the virtual machine and provides a GRPC API over vsock"* and
  *"provides I/O, signals, and events to the calling process when a process is
  run"* (https://github.com/apple/containerization).

Worth flagging for daemar's filesystem model specifically: `LinuxContainer`
already supports overlay natively. `writableLayer` is documented as — *"the
rootfs is mounted as the lower layer of an overlayfs, with this as the upper
layer. All writes will go to this layer instead of the rootfs"* — and must be a
block device.

**Kata `AGENT_INIT=yes`** is the same shape: the Rust `kata-agent` becomes
`/sbin/init` and needs nothing from the image's userland. *"When the
`AGENT_INIT` environment variable is set to `yes`, use Kata agent as
`/sbin/init`"*, and it is **mandatory** for shell-lean distros — *"`AGENT_INIT=yes`
must be used for the Alpine distribution since it does not use `systemd` as its
init daemon"*
(https://raw.githubusercontent.com/kata-containers/kata-containers/main/tools/osbuilder/rootfs-builder/README.md).
Default libc is musl (`LIBC ?= musl` in `src/utils.mk`), overridden to gnu only
on ppc64le/riscv64/s390x and under `USE_DEVMAPPER`. Transport is ttRPC over
vsock, carrying stdio in-band. In the *non*-AGENT_INIT default (rootfs-image
mode, Ubuntu on x86_64), systemd is PID 1 and launches the agent as a service —
i.e. Kata ships both models and reserves the image-independent one for the
hardened path.

**microsandbox `agentd`** goes furthest: the agent is embedded into the *host*
binary at compile time and materialized into the guest rootfs —
`pub const AGENTD_BYTES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/agentd"))`
(https://github.com/microsandbox/microsandbox/blob/main/crates/filesystem/lib/agentd.rs).
Static musl Rust, built in an Alpine builder to `scratch`
(`Dockerfile.agentd`). It runs as **PID 1 and does its own init** —
`rlimit::apply_baseline`, `linux::mount_filesystems()`, `mount_runtime()`,
`mount_block_root()`, `apply_user_mounts()`, `network::apply_hostname()`
(`crates/agentd/lib/init.rs`) — and execs the workload with direct
`libc::execvp` (`crates/agentd/lib/process.rs:531`; the `/bin/sh` references at
710/747 are test-only). Control is framed messages on virtio-console, not
in-guest HTTP. **Zero shell dependence.** Note the repo has moved:
`microsandbox/microsandbox` now redirects to `superradcompany/microsandbox`.

**Fly Machines**: `init` is Rust, injected by Fly and **not from the image**.
It performs the Linux mounts, overrides `resolv.conf`, runs a small SSH server
over WireGuard, and supervises the entrypoint — *"We inject a configuration
file into each VM that carries the user, network, and entrypoint information"*
(https://fly.io/blog/docker-without-docker/). The user's OCI image is unpacked
onto an LVM2 thin pool and handed to Firecracker as a CoW block device.
**Static linking and the initrd-vs-mount delivery mechanism are not stated —
could not confirm.**

**E2B** is the instructive hybrid, and the closest analogue to daemar's
question. `envd` is a static Go binary (`CGO_ENABLED=0 ... -a -o bin/envd`,
`packages/envd/Makefile:87`) that is **not baked into the user's image**: the
orchestrator synthesizes *additional OCI layers* stacked on whatever image the
user brought, writing the host's `envd` to `/usr/bin/envd` at mode 0777
(`additionalOCILayers` in
`packages/orchestrator/pkg/template/build/core/rootfs/rootfs.go`;
`GuestEnvdPath` in `packages/shared/pkg/storage/paths.go`). The same injected
layer carries a **statically musl-linked busybox** built specifically *"for E2B
sandbox systeminit... with only the applets needed for VM initialization
(mount, init, switch_root, etc.)"* — `CONFIG_STATIC=y`, `musl-gcc`, and the
build script notes *"The init script uses `#!/usr/bin/busybox ash` so ash must
be present"* (https://github.com/e2b-dev/infra/blob/main/firecracker/fc-busybox/build.sh).
It also carries a systemd unit, an OpenRC script, an inittab and a systemd
preset — four redundant init paths, precisely because E2B cannot assume what
init the user's image ships.

And yet: **E2B still hard-depends on `/bin/sh` for every exec.** `envd` wraps
every process it starts —

```go
oomWrapperScript := fmt.Sprintf(`echo %d > /proc/$$/oom_score_adj && exec %s"${@}"`, defaultOomScore, ioniceNicePrefix(...))
cmd := exec.CommandContext(ctx, "/bin/sh", wrapperArgs...)
```

(`packages/envd/internal/services/process/handler/handler.go:201-203`). The
wrapper exists to set `oom_score_adj` and ionice before exec, so it is a real
requirement rather than a removable convenience. E2B brings its own coreutils
but not its own shell — the exact split daemar is choosing between.

**Daytona** bind-mounts a static Go daemon into any image, read-only, and takes
over the entrypoint: `binds = append(binds, fmt.Sprintf("%s:%s:ro", d.daemonPath, common.DAEMON_PATH))`
with `entrypoint = []string{common.DAEMON_PATH}`, demoting the image's own
entrypoint to `cmd` (`apps/runner/pkg/docker/container_configs.go:189,154`;
`DAEMON_PATH = "/usr/local/bin/daytona"`). Injection is image-independent, but
the daemon then probes the image's userland: `exec.Command("sh", "-c", "grep '^[^#]' /etc/shells")`
(https://github.com/daytonaio/daytona/blob/v0.190.0/apps/daemon/pkg/common/get_shell.go),
degrading to `sh`, and shells out to `git`.

**Vercel and GitHub Actions treat the image as a pure filesystem**, which is
the same principle expressed at the API. Vercel states it outright: *"Vercel
Sandbox does not run Docker `ENTRYPOINT` or `CMD` for custom images. Start
processes with `sandbox.runCommand()` after the sandbox is created"* — *"Vercel
resolves the reference against VCR and boots the sandbox from that image's
filesystem"* (https://vercel.com/docs/sandbox/concepts/images). The control
plane owns PID 1. The GitHub Actions runner is the strongest engineering case:
.NET with `<SelfContained>true</SelfContained>`
(https://raw.githubusercontent.com/actions/runner/main/src/Runner.Listener.csproj)
**and** it bundles its own Node (NODE20_VERSION=20.20.2, NODE24_VERSION=24.19.0
into `_layout/externals`,
https://raw.githubusercontent.com/actions/runner/main/src/Misc/externals.sh). The
most maximalist image in the industry still refuses to depend on it. **How the
runner is provisioned onto hosted VMs could not be found** — no install step
appears anywhere in `runner-images`.

### (b) Vendor binary inside the image, via FROM-inheritance or COPY

**Cloudflare Sandbox SDK** requires users to build `FROM
docker.io/cloudflare/sandbox:<version>` because the control server is baked in:
*"The `/sandbox` binary starts the HTTP API server that enables SDK
communication"*
(https://developers.cloudflare.com/sandbox/configuration/dockerfile/). Every
shipped example confirms it — `examples/minimal/Dockerfile` is two lines,
`FROM docker.io/cloudflare/sandbox:0.12.7` and `EXPOSE 8080`. The base is
`ubuntu:22.04` with `ENTRYPOINT ["/container-server/sandbox"]`, the server a
standalone Bun binary (plus `dist/sandbox-musl` for the Alpine variant), and
the Dockerfile comments the libc choice deliberately: *"Using glibc-based
images (not Alpine) so the standalone binary works on standard Linux
distributions"*
(https://github.com/cloudflare/sandbox-sdk/blob/main/packages/sandbox/Dockerfile).
Version lockstep is strict and documented — *"Always match the Docker image
version to your npm package version"*, since *"Mismatched versions can cause
features to break."*

The nuance worth carrying: the binary is self-contained but the **feature set
is not**. The image also ships Node 24, Bun, `node_executor.js` and (in the
python variant) `ipython_executor.py` for pooled code-interpreter execution;
FUSE tooling (`s3fs`, `fuse3`, `squashfuse`, `fuse-overlayfs`) with
`user_allow_other` enabled in `/etc/fuse.conf`; and `cloudflared` preinstalled
in every sandbox by default — a working egress-tunneling daemon, discussed
nowhere as surface. Internal pinning hygiene is good (cloudflared 2026.3.0 with
per-arch SHA256 constants, CPython 3.11.14 verified with `sha256sum -c`).
**No argument about image contents or attack surface — could not find one.**

**Blaxel** is the cleanest distroless-compatible expression of this model,
since it requires no FROM: `COPY --from=ghcr.io/blaxel-ai/sandbox:latest /sandbox-api /usr/local/bin/sandbox-api`,
with docs stating *"Always include the sandbox-api binary… required for process
management and file operations"* (https://docs.blaxel.ai/Sandboxes/Templates).

### (c) Shell script that downloads the agent at boot

**Coder** is the one system flatly incompatible with a shell-less image: the
container command is literally `["sh","-c",coder_agent.main.init_script]`, so
the base image must carry a shell plus curl/wget/busybox
(https://coder.com/docs/admin/templates/managing-templates/image-management).

### (d) Systemd-dependent, agent baked into a vendor VM rootfs

**firecracker-containerd** is the opposite extreme from Apple. The Go agent
(static only opt-in — `CGO_ENABLED=0` is passed just when `STATIC_AGENT` is
set, `agent/Makefile`) is installed at image-build time into a Debian rootfs
built by `debootstrap --variant=minbase --include=udev,systemd,systemd-sysv,procps,libseccomp2,haveged bullseye`,
and launched **by that image's systemd**, not as PID 1 — the documented cmdline
carries `systemd.unit=firecracker.target init=/sbin/overlay-init`
(https://raw.githubusercontent.com/firecracker-microvm/firecracker-containerd/main/tools/image-builder/README.md).
The VM rootfs is a read-only shared squashfs made writable via overlay; container
images arrive separately as per-container block devices through the snapshotter.
Its security argument is about immutability and sharing rather than contents:
*"Because the backing root filesystem is shared among multiple microVMs, it
must not be mutable by any microVM"*
(https://raw.githubusercontent.com/firecracker-microvm/firecracker-containerd/main/docs/host-file-isolation.md).

**Upstream Firecracker takes no position at all.** Its rootfs guidance is
deliberately minimal and mechanical: *"A rootfs image is just a file system
image, that hosts at least an init system"* and *"The minimal init system would
be just an ELF binary, placed at `/sbin/init`"*. The only stated "why" for
filesystem choice is that *"support for it will have to be compiled into the
kernel"*
(https://raw.githubusercontent.com/firecracker-microvm/firecracker/main/docs/rootfs-and-kernel-setup.md).
**No security argument about guest image contents in that document.**

**Cloud Hypervisor** has no guest agent upstream at all — a genuine absence
across 47 docs. Its threat model puts the guest wholly out of scope: *"Cloud
Hypervisor considers the guest VM to be untrusted... a guest VM is only allowed
to perform I/O using the interfaces Cloud Hypervisor has been told to provide
to it"*
(https://raw.githubusercontent.com/cloud-hypervisor/cloud-hypervisor/main/docs/threat-model.md).

**Kubernetes `agent-sandbox`** has no in-guest agent contract whatsoever; the
Go and Python SDKs talk to the API server.

---

## The distroless verdict

**No primary source argues that minimal or distroless image contents are a
security control for a sandbox that runs arbitrary code by design.** Stated as
a search result rather than an inference: I looked for one specifically and
could not find it.

**What the distroless project itself claims.** Its README states the rationale
in full: *"Restricting what's in your runtime container to precisely what's
necessary for your app is a best practice... It improves the signal to noise of
scanners (e.g. CVE) and reduces the burden of establishing provenance to just
what you need"*, plus size (*"around 2 MiB"*)
(https://github.com/GoogleContainerTools/distroless). Three observations. The
phrase "attack surface" does not appear in the README. There is no threat-model
section and no post-exploitation claim. And **both stated benefits are
build-time properties** — scanner noise and provenance burden are facts about
your CI dashboard, not about what a running process can do. The `:debug` tag
then hands the shell back as a one-tag opt-out: *"Distroless images are minimal
and lack shell access. The `:debug` image set for each language provides a
busybox shell to enter."* A control undone by editing a tag was never a
boundary — and arbitrary code does not even need the tag, since a shell is a
convenience wrapper over `execve`.

**NIST SP 800-190** is the closest thing to a contrary source, and it is
pre-exploitation framing throughout, aimed at the host OS and the production
app image rather than a sandbox guest
(https://nvlpubs.nist.gov/nistpubs/SpecialPublications/NIST.SP.800-190.pdf).
§3.5.1: *"any network-accessible service provides a potential **entry point for
attackers**... The larger the attack surface is, the better the odds are that
an attacker can find and access a vulnerability."* §4.1.2 recommends
*"selection of base layers from minimalistic technologies like Alpine Linux and
Windows Nano Server to reduce attack surface areas."* The load-bearing premise
is an adversary *outside* who must exploit something to get in — void when
execution is granted at t=0 by design. Its shell guidance is likewise about
inbound remote access (*"SSH and other remote administration tools... exposes
them to greater risk of network-based attack"*), not about `/bin/sh` existing
on disk. Usefully, §3.5.2 also ranks the boundaries daemar's way: *"the level
of isolation provided by container runtimes is not as high as that provided by
hypervisors."*

**gVisor** places the boundary at the syscall interface and never mentions
image contents: *"gVisor's primary design goal is to minimize the System API
attack vector through multiple layers of defense... the application's direct
interactions with the host System API are intercepted by the Sentry"*
(https://github.com/google/gvisor/blob/master/g3doc/architecture_guide/security.md).
Note their "minimization" means minimizing the host syscall set the *Sentry*
may issue, not image contents. Their attack-vector taxonomy — System API,
System ABI, Side Channels, Other — has no slot for image composition, and the
project describes itself as *"an open-source workload isolation solution to
safely run untrusted code."* Untrusted code is the design target; the answer is
a syscall boundary, not a curated image. Also worth keeping: *"A sandbox is not
a substitute for a secure architecture."*

**The strongest negative evidence** is the Kubernetes SIG-Apps `agent-sandbox`
threat model
(https://raw.githubusercontent.com/kubernetes-sigs/agent-sandbox/main/docs/security/threat_model.md).
It is a purpose-built, upstream, security-reviewed agent sandbox with a formal
threat model that is 100% orchestration and 0% image contents. It names
container escape, cross-tenant network, Kubernetes API abuse
(`automountServiceAccountToken` defaulted false), resource exhaustion, Router
SSRF (blocking 169.254.169.254, and candidly admitting the IP-class check is
*"currently the only SSRF defense"*), and Service-selector label-spoofing
hijack. It states plainly that *"Agent Sandbox itself does not implement
isolation"*, delegating to gVisor or Kata via RuntimeClass. Image contents,
package surface, and supply chain appear nowhere.

**The one vendor that addresses image contents directly says the opposite of
the distroless intuition.** microsandbox's `docs/security/filesystem.mdx`
verifies layers against the manifest SHA-256 but notes *"There is no cosign,
Notary, or attestation check"*, and then states the load-bearing line:
**"Image contents run as untrusted code inside the VM regardless. The
supply-chain risk here is running the *wrong* image, not that image escaping
the sandbox."** Its `docs/security/overview.mdx` goes further and puts its own
agent on the untrusted side: the guest is *"Your workload and its processes,
the guest Linux kernel, and `agentd` (the in-guest agent) — Fully untrusted.
Assume it is adversarial."* That is the architectural inverse of E2B and
Daytona, where the in-guest daemon is a privileged control point.

**Two genuine exceptions, both about the VM's own filesystem, not the
workload's.** Kata's initrd row justifies Alpine on *"Security hardened and
tiny C library"*, and Apple describes its init image as using *"a minimal set
of core utilities and dynamic libraries to reduce... attack surface"*. Both
describe the filesystem the *supervisor* lives in. Neither constrains the OCI
image the workload runs in. If daemar wants a security-motivated minimal-image
argument for the *workload* image, it would be asserting something no primary
vendor source states — worth flagging explicitly rather than implying industry
backing.

**Cross-cutting:** across every system surveyed with published security
material, the shared threat model is prompt injection leading to exfiltration
(OpenAI, Cursor, and Anthropic all name it), and it is mitigated at the network
layer in every case.

---

## What this meant for daemar

Recorded decision for the first slice: **a generated shell-script driver, with
a pinned Ubuntu image as the default.** The static musl driver binary is
documented as a later engineering option — **and, per the evidence above, an
engineering one rather than a security one.**

What the evidence does and does not support, stated separately from the
decision:

- **Shell-script drivers are not unprecedented, but they are the minority and
  the least defended position.** Coder is the only surveyed system whose agent
  ingress is a shell script, and E2B — despite injecting its own static busybox
  precisely so it need not trust the image's coreutils — still requires
  `/bin/sh` on every exec for its `oom_score_adj` wrapper. So the exact split
  daemar is choosing (bring coreutils, borrow the shell) has a shipping
  precedent at the largest agent-sandbox vendor.
- **The cost of the shell-script driver is a compatibility constraint, not a
  weakened boundary.** It restricts daemar to shell-bearing images. No primary
  source suggests the presence of `/bin/sh` weakens a VM-grade sandbox; the
  arbitrary code being run can `execve` whatever the image contains regardless.
- **The static-driver path is where the field has converged for robustness.**
  Apple, Kata's hardened mode, microsandbox, and Fly all make the supervisor
  owe the image nothing, and GitHub Actions does so even atop a maximal image.
  The recurring justification is image-independence and predictable init, not
  attack surface.
- **Pinning the default image is well supported and cheap.** Daytona enforces
  digest-or-tag pinning on user images, Vercel accepts digests as first-class
  references, and microsandbox auto-pins to immutable layers at first pull.
  E2B's unpinned `e2bdev/base` is the counterexample, not the model.
- **If daemar later wants image-independence, Apple's substrate already
  supplies the mechanism.** `vminitd` is a separate read-only init block device
  and `vmexec` `pivot_root`s into the workload rootfs with a direct `execve`;
  `LinuxContainer.writableLayer` additionally provides overlayfs with the
  workload image as the lower layer, which is the filesystem model daemar
  already chose.

---

## Open questions and could-not-finds

- **OpenAI Codex**: how the agent binary enters the container, and whether any
  bring-your-own-image path exists. Not documented.
- **GitHub Actions**: how the runner is provisioned onto hosted VM images. No
  install step in `runner-images`.
- **Docker Sandboxes**: the VMM is never named. Install docs state only macOS
  Sonoma 14+/Apple silicon, Windows 11 + Windows Hypervisor Platform, and
  *"`sbx` requires KVM to start"* on Linux. The macOS constraint is consistent
  with Virtualization.framework, but Docker does not say so — inference, not
  citable. `github.com/docker/sandbox-templates` is 404; templates ship as
  Docker Hub images only.
- **Cursor**: no named or versioned default image. Docs say only *"Cloud agents
  run on isolated Ubuntu machines"*, with an implicit constraint elsewhere —
  *"Computer use is supported for repos with Dockerfiles based on
  Debian/Ubuntu-based Linux distributions"* (https://cursor.com/docs/cloud-agent/setup).
- **Devin**: no distro or version stated; environment is user-defined via
  blueprints frozen into snapshots (https://docs.devin.ai/onboard-devin/environment).
- **Jules**: Ubuntu version appears only incidentally in compiler output —
  treat as undocumented. Custom images and network policy not documented
  (https://jules.google/docs/environment/).
- **Daytona**: the OCI image reference behind the default snapshots.
- **Azure dynamic sessions**: distro and version of the built-in code
  interpreter container; introspection-only by design.
- **Fly Machines**: whether `init` is statically linked, and whether it is
  delivered by initrd or by mount. `superfly/init-snapshot` exists but its
  README 404s.
- **CodeSandbox** Firecracker posts: unreachable (Cloudflare 403, jina 401,
  archive.org blocked). Biggest remaining gap in the roster.
- **AWS Lambda**: guest init and rootfs — no public detail retrieved.
- Not reached: Together Code Sandbox, Railway, Runloop and Northflank primary
  docs; Nix- and Wasm-based sandboxes as a contrast pole; Arcade; Cloud Run /
  Vertex code execution.

### Note on the Claude Code devcontainer

Included for completeness since it came up as a lead, though it is a developer
convenience rather than a sandbox harness. Base is `FROM node:20`
(https://github.com/anthropics/claude-code/blob/main/.devcontainer/Dockerfile),
and Anthropic disclaims it: *"It is provided as a working example rather than a
maintained base image."* The recommended path is a devcontainer *feature* on
any base. `init-firewall.sh` sets `iptables -P OUTPUT DROP` and allows only
GitHub meta CIDRs plus a short allowlist, then self-verifies in both directions.
The docs are explicit that this is not part of Claude Code's own requirements —
*"The firewall script and these capabilities are not required for Claude Code
itself"* — and carry a contents-adjacent warning that is about credentials
rather than binaries: dev containers *"do not prevent a malicious project from
exfiltrating anything accessible inside the container, including the Claude Code
credentials stored in `~/.claude`"* (https://code.claude.com/docs/en/devcontainer).
