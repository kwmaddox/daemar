# Sandbox substrate and cost comparables

**Research date:** 2026-08-26

## Decision summary

The common implementation pattern depends on how seriously the product treats
the workload as hostile:

- Open-source coding harnesses such as OpenHands and SWE-agent generally start
  with ordinary Docker containers. That is cheap and operationally familiar,
  but it shares the host kernel and therefore does not meet Daemar's stated
  kernel-isolation requirement
  ([OpenHands runtime](https://github.com/OpenHands/docs/blob/main/openhands/usage/architecture/runtime.mdx),
  [SWE-agent deployment](https://github.com/SWE-agent/SWE-agent/blob/main/docs/usage/cl_tutorial.md)).
- Products explicitly built to execute untrusted agent code increasingly use a
  VM per sandbox: Docker Sandboxes, E2B, Vercel Sandbox, Fly Sprites, and
  Cloudflare Sandbox all document a VM or Firecracker boundary. They commonly
  pair that with an API for create/exec/files/stop and an egress firewall or
  proxy outside the guest.
- Namespace sandboxes (bubblewrap, nsjail) and gVisor are real security tools,
  and are used in agent products, but they deliberately do not provide a
  separate guest kernel. They are comparables, not matches for the hard
  requirement.

For Daemar's current cost preference, the best candidates are:

1. **Docker Sandboxes is the first local product to trial.** The CLI is free,
   including commercial use, and runs a microVM per sandbox on Apple silicon,
   Windows, and supported Linux/KVM hosts. It has host-enforced, deny-by-default
   TCP policy and credential brokering. Use clone mode, opt out of shared skills,
   and replace the broad default domain set. The important catches are that the
   product is proprietary, requires Docker login, persists until explicitly
   removed, and direct workspace mounting is the unsafe default
   ([pricing/FAQ](https://docs.docker.com/ai/sandboxes/faq/),
   [security model](https://docs.docker.com/ai/sandboxes/security/),
   [license](https://github.com/docker/sbx-releases/blob/main/README.md)).
2. **Apple `container` remains the conservative open-source local primitive.**
   It is free, Apache-2.0, creates a lightweight VM per Linux container, and can
   omit networking entirely with `--network none`. It is less of a sandbox
   product: Daemar must own staging, controlled egress, command supervision,
   changeset extraction, and promotion. It also requires Apple silicon and
   macOS 26
   ([project](https://github.com/apple/container),
   [`--network none`](https://github.com/apple/container/discussions/743)).
3. **If cheap hosted execution becomes acceptable, trial Cloudflare Sandbox and
   Vercel Sandbox before E2B.** Both now document one VM per sandbox and
   low/no-base entry tiers. Cloudflare has the lowest small-instance rate and a
   $5/month platform floor, but its product/API is still pre-1.0 and it adds
   Workers and Durable Objects billing. Vercel has a polished Firecracker API,
   deny-all/domain/CIDR policy and credential brokering, with a free Hobby tier
   or $20/month Pro; active CPU is relatively expensive, but model-wait time is
   not billed as CPU. Both need adversarial egress tests rather than trust in
   documentation.
4. **Fly Sprites is the best low-ops hosted option for durable agent computers.**
   It has Firecracker isolation, automatic idle transition, persistent ext4,
   checkpoints, agent integrations, and usage billing. Its default egress is
   permissive and its documented policy is DNS/domain based, so it is less
   compelling for a narrowly mediated one-shot runner until raw-IP and DNS
   bypass tests pass.
5. **ECS on Fargate has a strong per-task VM boundary, but is not a sandbox
   product.** AWS owns the hardware-virtualized runtime; Daemar would still own
   the deadline supervisor, orphan reconciliation, source/result protocol,
   least-privilege IAM, and every egress route. Compute for short tasks is cheap,
   but a correctly private one-AZ design adds roughly $15--$22/month of ECR/log
   interface endpoints, while NAT or Network Firewall can dominate the bill.
   It is credible for an AWS-native deployment, not a simplification of the
   local-first design.

No candidate removes Daemar's responsibility for safe source ingress and result
promotion. A runtime's `readFile`, snapshot, volume, or Git workflow is not by
itself an exact, safely applicable added/modified/deleted changeset.

### Terminology boundary

This report uses **sandbox** only for a security boundary that constrains an
untrusted process. A Git worktree, repository clone, staging directory, overlay,
branch, or snapshot may provide **workspace isolation**—separate names and file
state—but is **not a sandbox**. It does not isolate a kernel, processes,
credentials, devices, or networking. Clone/staging is a necessary complement to
the runtime boundary, never a substitute for it.

## Price normalization

Public prices are volatile and not directly comparable. Vendors variously bill
provisioned CPU, active CPU, actual CPU, provisioned memory, actual memory,
wall-clock time, or storage only while stopped. The figures below are the
published list prices at the research date, before tax, data transfer, platform
API charges, image storage, observability, or discounts.

For a rough common point, the `2 vCPU + 4 GiB for one fully active hour` column
uses the closest public unit rates. It is **not** a benchmark or a quote.

| Hosted product | Entry/base cost | Public resource price | Approx. 2 vCPU + 4 GiB fully active hour | Idle and storage |
|---|---:|---|---:|---|
| E2B | Hobby $0 + usage; one-time $100 credit; Pro $150/mo + usage | CPU $0.000014/vCPU-s; RAM $0.0000045/GiB-s | $0.1656 | 10 GiB Hobby or 20 GiB Pro included; pause exists; Hobby sessions 1h, Pro 24h |
| Daytona | $0 PAYG; page advertises $200 free compute | CPU $0.0504/vCPU-h; RAM $0.0162/GiB-h; disk $0.000108/GiB-h after first 5 GiB | about $0.166/h plus disk | stopped/paused: disk only; archived containers free; deleted free |
| exe.dev | Personal pool $20/mo; Team $25/user/mo; usage alternative | usage CPU $0.05/core-h; active RAM $0.016/GiB-h; disk $0.08/GiB-month | about $0.164/h plus disk | stopped VM releases CPU; idle memory charged at disk rate; Personal includes 50 VMs and 100 GB pooled disk |
| Fly Sprites | no recurring minimum stated; $30 trial credit | actual CPU $0.07/CPU-h; actual RAM $0.04375/GiB-h | about $0.315/h plus storage | warm/cold compute $0; hot storage $0.000683/GB-h, cold $0.000027/GB-h |
| Modal | Starter has $30/month free compute; no usage-time minimum | one physical core (2 vCPU) $0.00003942/s; RAM $0.00000667/GiB-s | about $0.238/h | pay while executing; volume/snapshot retention is separate; VM mode is beta |
| Cloudflare Sandbox | Workers Paid $5/mo, with included usage | after allowance: $0.000020/active-vCPU-s, $0.0000025/GiB-s, $0.00000007/GB-s | no exact shape; `standard-3` (2 vCPU, 8 GiB, 16 GB disk) is at most about $0.220/h | scale to sleep; sandbox also incurs Workers, Durable Objects, optional logs, and egress charges |
| Vercel Sandbox | Hobby $0; Pro $20/mo with $20 usage credit | $0.128/active-vCPU-h; $0.0212/provisioned-GB-h; egress $0.15/GB | about $0.341/h if CPU is active the full hour; about $0.085/h while CPU-idle | Hobby includes 5 active CPU h, 420 GB-h memory, 15 GB snapshots; snapshots $0.08/GB-month after allowance |
| AWS ECS/Fargate | $0 ECS/Fargate base; surrounding VPC services billed separately | N. Virginia Linux/x86: $0.0404784/vCPU-h + $0.004446/GiB-h; first 20 GiB ephemeral included | about $0.09874/h; $0.01646 for 10 minutes | per-second with 1-minute minimum, including image-pull time; disk disappears with task; extra ephemeral $0.00011088/GiB-h |
| GitHub Codespaces | personal Free includes 120 core-h and 15 GB-month; Pro 180 core-h and 20 GB-month | 2-core $0.18/h; storage $0.07/GB-month | $0.18/h (machine price includes its memory) | compute stops when inactive; storage accrues while codespace exists |

Sources: [E2B pricing](https://e2b.dev/pricing),
[Daytona pricing](https://www.daytona.io/pricing),
[Daytona billing states](https://www.daytona.io/docs/en/billing/),
[exe.dev pricing](https://exe.dev/pricing),
[Fly Sprites pricing](https://fly.io/sprites/),
[Modal sandbox pricing](https://modal.com/products/sandboxes),
[Cloudflare Containers pricing](https://developers.cloudflare.com/containers/pricing/),
[Vercel pricing](https://vercel.com/pricing),
[AWS Fargate pricing](https://aws.amazon.com/fargate/pricing/), and
[GitHub Codespaces billing](https://docs.github.com/en/billing/concepts/product-billing/github-codespaces).

Local software has no metered compute bill, but is not literally free: it
consumes the developer machine's CPU, RAM, disk, electricity, and maintenance
time. Docker Sandboxes currently has no software-use charge; Apple `container`,
Lima/Colima, Firecracker, Kata, gVisor, nsjail, and bubblewrap are open source.

## Capability matrix

`Kernel` means a separate guest kernel, not merely namespaces or a userspace
system-call implementation. `External egress` means policy is enforced somewhere
the guest root user should not control; it does not mean the implementation has
been independently verified.

| Candidate | Where | Kernel boundary | External network enforcement | Lifecycle/API | Source/license | Evidence of harness/factory use | Fit |
|---|---|---|---|---|---|---|---|
| Docker Sandboxes | local | microVM per sandbox | host proxy; deny-by-default outbound TCP; direct UDP/ICMP blocked | CLI, SSH, persistent stop/restart/remove, templates/kits | usable product proprietary; free use; login required | built-in support for Claude Code, Codex, Gemini CLI, OpenCode and others | **Best local trial** |
| Apple `container` | local macOS | VM per container | reliable no-NIC mode; no built-in narrow egress broker | CLI, OCI images, foreground exec, stop/remove | Apache-2.0 | Daemar's current v1 substrate; general primitive rather than harness integration | **Best OSS macOS primitive** |
| Lima / Colima | local macOS/Linux | one general Linux VM per profile | guest/host network must be configured by operator; internet normally available | mature VM CLI; Colima adds Docker/containerd/Incus | Apache-2.0 / MIT | commonly under local Docker-based harnesses, but not a purpose-built agent contract | compose only |
| Firecracker | self-hosted Linux/KVM | microVM | **none built in**; operator must configure TAP, nftables/iptables | REST VMM API; operator owns kernel/rootfs/agent/cleanup | Apache-2.0 | foundation for E2B, Vercel, Fly; AWS Lambda/Fargate | strong component, poor direct fit |
| AWS ECS/Fargate | hosted AWS | hardware-virtualized environment and dedicated kernel per **task**; AWS documents Firecracker | customer-owned VPC routes, SGs, NACLs, DNS Firewall, proxy or Network Firewall; no agent-aware broker | RunTask/DescribeTasks/StopTask; no native wall-clock run timeout or result protocol | proprietary managed service; Firecracker is Apache-2.0 | boldblack `harness` documents Fargate deployment; community `openhands-infra` launches per-conversation tasks; no major reviewed harness has a native provider | strong isolation; high policy/ops burden |
| Kata Containers | self-hosted Linux | VM-backed OCI sandbox | CNI/host policy; not an agent domain broker | containerd/CRI lifecycle and guest agent | Apache-2.0 | production container/Kubernetes substrate; Firecracker can be a backend | strong Linux stack, high ops |
| gVisor/runsc | local/self-hosted Linux | **no guest kernel**; userspace application kernel | container network policy; `--network=none` supported | OCI runtime for Docker/containerd/Kubernetes | Apache-2.0 | Modal default Sandbox runtime; Google Cloud Run foundation | fails hard kernel requirement |
| nsjail | local/self-hosted Linux | **shared host kernel** | network namespace; operator supplies policy | one-shot/listener/re-exec CLI | Apache-2.0 | generic judging/fuzzing primitive; no strong reviewed coding-factory integration | fails hard kernel requirement |
| bubblewrap | local Linux | **shared host kernel** | network namespace with loopback only unless shared/composed | low-level one-shot CLI | LGPL-2.0-or-later | Anthropic Sandbox Runtime / Claude Code on Linux; Flatpak | fails hard kernel requirement |
| E2B | hosted; self-host stack targets cloud/KVM | Firecracker microVM | internet toggle and IP rules; default internet on | SDK/CLI/API; files, commands, PTY, pause/resume/kill, timeouts | SDK/infra Apache-2.0; managed service | OpenHands legacy provider; Manus and other first-party case studies | mature hosted contender |
| Daytona | hosted; OSS self-operated stack | managed default is container; explicit Linux VM class has dedicated VM | runner firewall; CIDR/domain/block-all; advanced custom policy is tier-gated | broad SDKs, CLI, REST, file/process/git, snapshots/fork/pause/archive | AGPL-3.0 platform | OpenHands legacy provider | attractive API; verify selected VM class and self-host path |
| exe.dev | hosted | Cloud Hypervisor VM | public ingress proxy/private by default; no documented general deny-all/domain egress firewall found | SSH and HTTPS API; persistent VM, copy, stop, delete | proprietary service | agents are preinstalled; Shelley is native; usable as ordinary SSH host | cheap persistent VM, weak egress fit |
| Fly Sprites | hosted | Firecracker microVM | mutable DNS/domain policy; **default permissive** | CLI/REST/MCP and Go/JS/Python/Elixir SDKs; exec, services, checkpoint/restore, automatic idle | service proprietary; client SDKs/plugins mostly MIT | official Claude Code, Codex, Cursor, DeepSeek Harness, Pi, Anthropic Managed Agents integrations | best durable hosted value |
| Modal | hosted | gVisor default; full VM mode beta | block-all, CIDR, beta TLS-domain allowlist, runtime changes alpha | Python/JS/Go API; exec, files, volumes, terminate, snapshots | proprietary service/client | OpenHands provider; Ramp Inspect testimonial | only VM-beta path meets kernel requirement |
| Cloudflare Sandbox | hosted | VM per container/sandbox | no first-party general outbound allowlist surfaced in reviewed stable Sandbox docs; Worker-side outbound handlers can broker credentials | TypeScript SDK; exec/files/processes/ports/backup; Durable Object identity; sleep | SDK Apache-2.0; service proprietary | official examples for Claude Code, OpenAI Agents SDK, OpenCode | **cheap burst contender; qualify egress** |
| Vercel Sandbox | hosted | Firecracker microVM | allow-all default; deny-all, domains, CIDRs, L7 matchers, credential transforms | TS/Python SDK, CLI, REST; named persistence, exec/files, fork/snapshot/stop/delete | CLI/SDK Apache-2.0; service proprietary | OpenCode guide, Cua, AI SDK Harness provider, Vercel's own build/v0 systems | **best polished hosted trial** |
| GitHub Codespaces | hosted | dedicated VM and network per codespace | outbound public internet always allowed; GitHub says it cannot currently be restricted | GitHub/VS Code lifecycle and REST API; persistent dev environment | proprietary service | Copilot/human development environment, not a neutral one-shot harness API | reject for network requirement |

## Local options in detail

### Docker Sandboxes

Docker now documents the exact product shape Daemar has been looking for: a
separate lightweight VM and Linux kernel per sandbox, a private Docker daemon,
host-side TCP proxy, network rules, and credentials that remain on the host
([isolation layers](https://docs.docker.com/ai/sandboxes/security/isolation/)).
The CLI runs without Docker Desktop or Engine and currently supports Apple
silicon/macOS 14+, Windows 11, and Ubuntu 24.04+/KVM
([installation](https://docs.docker.com/ai/sandboxes/install/)). It is also the
only reviewed local product whose official documentation explicitly says it is
free for commercial and professional use with no seat fee
([FAQ](https://docs.docker.com/ai/sandboxes/faq/)).

The safe configuration is not the default:

- Direct mode mounts the actual worktree read-write. `--clone` instead mounts
  the source read-only and creates a private in-VM clone.
- The default domain policy includes broad wildcards. Replace it with the
  smallest explicit model/package/Git policy.
- Supported agents receive a shared, persistent, read-write skill store unless
  creation uses `--no-share-skills`. That joins otherwise separate sandboxes
  into one instruction/code trust domain.
- A stopped sandbox and its packages, Docker images, and history persist until
  `sbx rm`; Daemar must guarantee cleanup or intentionally embrace durability
  ([architecture](https://docs.docker.com/ai/sandboxes/architecture/)).
- Some kit syntax is accepted before enforcement exists (for example the kit
  reference marks multi-label wildcards and port ranges as “enforcement
  pending”); policy tests must target the pinned version
  ([kit reference](https://docs.docker.com/ai/sandboxes/customize/kit-reference/)).

The security model and cost are compelling, but the full runtime is closed
source and requires account login. That shifts auditability and future pricing
risk to Docker. A Daemar adapter should stay narrow enough to replace.

### Apple `container`

Apple's tool is Apache-2.0 and uses Virtualization.framework to run each OCI
Linux container in a lightweight VM. `--network none` leaves only loopback, a
strong primitive for fully offline execution. It does not expose Docker
Sandboxes-style domain mediation or credential brokering, so controlled model
access would require a carefully designed host-side channel. Current support is
Apple silicon on macOS 26; the project warns that pre-1.0 minor releases may
break compatibility ([README](https://github.com/apple/container)).

This remains the lower-dependency choice when offline execution is sufficient.
Daemar must retain its own source staging, process deadline, cleanup, diff, and
promotion transaction.

### Lima and Colima

Lima is a general Linux VM manager; Colima configures Lima as a convenient
Docker/containerd/Incus host. Both are free and mature, but the usual shape is a
long-lived VM containing many containers, not a fresh VM per hostile task.
Colima defaults to shared networking and can make the VM reachable or bridged
([configuration](https://github.com/abiosoft/colima/blob/main/embedded/defaults/colima.yaml)).
Neither supplies a deny-by-default agent egress broker, exact result export, or
safe promotion. Separate profiles plus host firewall rules are composable, but
Daemar would own more privileged lifecycle and networking code than with Apple
`container` or Docker Sandboxes.

### Firecracker and Kata

Firecracker is the strongest low-level Linux foundation in this comparison,
not a complete sandbox service. It supplies a minimal KVM VMM, one microVM per
process, seccomp, a jailer, rate limits, and a REST API. Its own production guide
states that Firecracker does **not** filter network traffic; the operator must
build TAP, network namespaces, firewall policy, kernel/rootfs images, a guest
command channel, supervision, and cleanup
([design](https://github.com/firecracker-microvm/firecracker/blob/main/docs/design.md),
[production host setup](https://github.com/firecracker-microvm/firecracker/blob/main/docs/prod-host-setup.md)).
That is why hosted vendors wrap it rather than exposing it directly.

Kata Containers owns more of that stack behind the OCI/containerd interface and
has a threat model explicitly treating the guest workload as malicious
([threat model](https://github.com/kata-containers/documentation/blob/master/design/threat-model/threat-model.md)).
It is a credible future Linux provider, but is operationally a containerd/CRI
stack, not a small local macOS dependency.

### gVisor, nsjail, and bubblewrap

These are useful evidence of what security-conscious tools do when a VM is too
expensive, but they fail the chosen kernel boundary:

- gVisor intercepts Linux calls in a Go userspace application kernel and offers
  `runsc` as an OCI runtime. It explicitly says it is not a VM. Rootless
  `--network=none` is supported, while connected deployments rely on container
  network policy
  ([introduction](https://gvisor.dev/docs/architecture_guide/intro/),
  [security model](https://gvisor.dev/docs/architecture_guide/security/)).
- nsjail combines Linux namespaces, cgroups, rlimits, and seccomp, including a
  network namespace. It is Linux-only and shares the host kernel
  ([project](https://github.com/google/nsjail)).
- bubblewrap constructs namespaces and mounts for an unprivileged process. Its
  maintainers explicitly call it a low-level builder whose security depends on
  caller arguments, not a ready-made policy. `--unshare-net` supplies loopback
  only. Anthropic's Sandbox Runtime uses bubblewrap on Linux and Seatbelt on
  macOS; a 2026 Claude Code advisory is a useful reminder that persistent host
  configuration paths can defeat an otherwise correct process sandbox
  ([bubblewrap](https://github.com/containers/bubblewrap),
  [Anthropic Sandbox Runtime](https://github.com/anthropic-experimental/sandbox-runtime),
  [Claude Code advisory](https://github.com/anthropics/claude-code/security/advisories/GHSA-ff64-7w26-62rf)).

## Hosted options in detail

### AWS ECS on Fargate: adversarial evaluation

#### Boundary and verdict

Fargate meets the kernel-isolation requirement if Daemar maps **one hostile
agent run to one ECS task**. AWS's current shared-responsibility documentation
says each task runs in an isolated hardware-virtualized environment and does
not share an operating system, Linux kernel, ENI, ephemeral storage, CPU, or
memory with another task. AWS also documents that Fargate tasks execute in
Firecracker microVMs
([current ECS isolation statement](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/security-shared-model.html),
[Firecracker/Fargate architecture](https://aws.amazon.com/blogs/opensource/firecracker-open-source-secure-fast-microvm-serverless/)).
This is a VM security boundary, not a claim that each task receives a dedicated
physical host; AWS can safely co-locate microVMs on its bare-metal fleet.

The boundary is the **task**, not each container. Containers within one task
share its network namespace and can share PID and ephemeral-volume state. A
“trusted” result or credential sidecar beside hostile code therefore is not a
separate security boundary. Put only one mutually distrustful workload in a
task, and put credential mediation in a different task/VM/managed service
([task networking](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/fargate-task-networking.html),
[same-task resource sharing](https://docs.aws.amazon.com/pdfs/AmazonECS/latest/bestpracticesguide/bestpracticesguide.pdf)).

Fargate materially simplifies one hard layer: Daemar does not build, patch, or
garbage-collect a microVM monitor or host kernel. It does **not** supply the
sandbox product contract. ECS has no repository staging/promotion protocol,
agent-aware credential broker, egress allowlist, wall-clock run deadline, or
postmortem filesystem export. Daemar must construct and operate those around
`RunTask`.

There is real, but modest, coding-harness precedent. The MIT-licensed
`boldblackai/harness` packages Pi, OpenCode, and Hermes and documents ECS
Fargate as a deployment for a persistent Hermes agent; the community
`openhands-infra` project launches one Fargate task per OpenHands conversation.
These demonstrate feasibility, not an independently reviewed sandbox profile:
neither establishes the deny-by-default network and exact-result contract here
([harness](https://github.com/boldblackai/harness),
[openhands-infra](https://github.com/zxkane/openhands-infra)).

#### Root and capability constraints

Fargate does not support privileged containers, devices, custom Docker security
options, `tmpfs`, or host networking. The only Linux capability a task may add
is `CAP_SYS_PTRACE`; capabilities including `NET_ADMIN` and `SYS_ADMIN` cannot
be added. Daemar can set a non-root `user`, drop `ALL` capabilities, make the
root filesystem read-only, and mount only a writable scratch workspace
([Fargate task-definition differences](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/fargate-tasks-services.html),
[task parameters](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task_definition_parameters.html)).

The default is still root, with Docker's default capability set, unless the
image or task definition says otherwise. Root inside the task cannot become
privileged Fargate host root through a supported setting, but it can read or
rewrite everything available to its container and normally every credential
deliberately exposed to the task. For an agent that needs to install arbitrary
packages, root may be an acceptable usability decision because the VM is the
hard boundary; it should not weaken the external network, IAM, or result trust
model. ECS Exec should stay disabled: its commands run as root, it requires an
SSM task role/network path, and it is incompatible with a read-only root
filesystem
([ECS container hardening](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/security-tasks-containers.html),
[ECS Exec considerations](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/ecs-exec.html)).

Dropping `CAP_NET_RAW` prevents raw sockets and packet forgery inside the task;
it does **not** prevent an ordinary TCP client from connecting to an IP literal.
Only the ENI's external route/SG/firewall path can enforce that requirement
against hostile guest code.

#### Lifecycle, source ingress, and result egress

The useful control-plane operations are `RunTask`, `DescribeTasks`, task-state
events, and `StopTask`. `StopTask` sends the image's stop signal (`SIGTERM` by
default), waits 30 seconds by default, then sends `SIGKILL`; Fargate's task
`stopTimeout` can be at most 120 seconds. `startTimeout` only bounds container
dependency startup and is **not** a run deadline
([StopTask](https://docs.aws.amazon.com/AmazonECS/latest/APIReference/API_StopTask.html),
[container timeouts](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task_definition_parameters.html)).

The command/entrypoint and environment overrides are normally fixed at
`RunTask`; the autonomous agent loop must live inside the container. ECS does
not provide a sandbox-style repeated exec/files API. ECS Exec is an
administrator/debug channel with the root/SSM drawbacks above, not a safe
untrusted-command protocol for Daemar.

`RunTask` has no native maximum wall time. A reasonable controller is a Step
Functions Standard workflow using `ecs:runTask.sync`, an explicit
`TimeoutSeconds`, and cleanup/reconciliation. Step Functions attempts to cancel
an integrated job when an execution aborts, but AWS describes cancellation as
best effort. Daemar must still periodically find overdue `RUNNING` tasks and
stop them, and treat a task as complete only after it has a terminal ECS state
**and** a validated result object
([ECS optimized integration](https://docs.aws.amazon.com/step-functions/latest/dg/connect-ecs.html),
[best-effort cancellation](https://docs.aws.amazon.com/step-functions/latest/dg/connect-to-resource.html)).

Each Linux task receives 20 GiB of encrypted ephemeral storage by default and
can request up to 200 GiB; image layers consume part of it. Storage disappears
with the task, so there is no supported “inspect the stopped disk later” result
path. Practical ingress/egress patterns are:

- Build immutable source into a per-ticket OCI image. This gives strong content
  addressing but increases build latency, registry churn, and per-run pull size.
- Download an immutable source archive from a tightly scoped S3 object into
  scratch, then upload a result manifest/archive to a different unique object.
  The controller must validate both object identity and the exact
  added/modified/deleted changeset. A clone or archive is workspace isolation,
  **NO SANDBOX**.
- Use pre-signed single-object URLs or an extremely narrow task role. The
  hostile process can always corrupt its own claimed result, so promotion stays
  outside the task and treats every byte as untrusted.

EFS/EBS can preserve a workspace, but persistence expands the trust and cleanup
surface and is unnecessary for a one-shot runner. The default ephemeral disk is
the safer Daemar v1 fit
([Fargate ephemeral storage](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/fargate-task-storage.html)).

#### IAM credentials and secrets

Keep the **task execution role** separate from the **task role**. ECS/Fargate
uses the execution role to pull images, fetch launch-time secrets, and send
`awslogs`; AWS states those credentials are not directly accessible to task
containers. Restrict that role to the exact ECR repositories and VPC endpoints
([execution role](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task_execution_IAM_role.html)).

In contrast, every container in a task can retrieve the task role's rotating
credentials from `169.254.170.2` via
`AWS_CONTAINER_CREDENTIALS_RELATIVE_URI`. There is no intra-task “only the
sidecar gets this role” isolation. Prefer no task role for a computation-only
run. If S3 result upload requires one, allow only a unique output key and the
minimum multipart actions, deny listing/reading other objects, scope the S3
gateway endpoint and bucket policy too, and make the object short-lived
([task-role credential endpoint](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/security-iam-roles.html),
[task-role isolation warning](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task-iam-roles.html)).

Do not inject a long-lived model key through ECS environment secrets. AWS notes
that the application, logs, and debugging tools can see environment variables,
and secret rotation is not reflected until a new task starts. A compromised
agent is the application; Secrets Manager protects delivery, not use. The
narrow-egress design below keeps model credentials at an external proxy and
injects them only into approved requests
([ECS secret caveats](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/secrets-envvar-secrets-manager.html)).

#### Network enforcement: what fails closed and what does not

Fargate platform 1.4+ sends image pulls, launch-time secret retrieval, logs, and
application traffic through the task ENI, which is useful for policy and flow
logs. It also means bootstrap and hostile application traffic share an ENI;
security groups cannot express “ECS agent may reach ECR, payload may not.”

The controls have sharply different jobs:

- **Routes decide reachability.** A “private subnet” is not a policy if it has a
  `0.0.0.0/0` route to a NAT gateway, `::/0` to an egress-only internet gateway,
  NAT64/DNS64, Transit Gateway, peering, VPN, or permissive VPC endpoints. For
  no-egress, remove every unneeded path rather than relying only on filters.
- **Security groups are stateful allowlists.** A new SG commonly has allow-all
  outbound; remove it. Audit `0.0.0.0/0` and `::/0` independently, and remember
  that multiple SGs aggregate their allows. SGs cannot create explicit denies.
- **NACLs are stateless subnet guardrails** with allow and deny rules. They can
  limit blast radius if an SG is accidentally broadened, but return ports and
  separate IPv4/IPv6 rules make them error-prone. Neither SGs nor NACLs can
  filter the AmazonProvidedDNS/Route 53 Resolver addresses
  ([security-group rules](https://docs.aws.amazon.com/vpc/latest/userguide/security-group-rules.html),
  [Amazon DNS limitation](https://docs.aws.amazon.com/en_en/vpc/latest/userguide/AmazonDNS-concepts.html)).
- **NAT Gateway is translation, not a firewall.** A task behind NAT can initiate
  raw-IP TCP/UDP/ICMP connections to any routed destination its SG/NACL permits.
  A public IP is similarly not an egress policy. Both are easy internet access,
  not narrow access.
- **Resolver DNS Firewall controls resolver queries only.** It can use a strict
  domain allowlist, inspect CNAME/DNAME chains, and default to fail closed. AWS
  explicitly says it does not resolve names into blocked IPs and does not filter
  HTTPS, TLS, SSH, or other protocols. It therefore cannot stop cached-IP,
  literal-IP, or non-DNS traffic, and wildcard/domain query names can themselves
  become an exfiltration channel. Use it as DNS defense and telemetry, never as
  the data-plane boundary
  ([DNS Firewall behavior](https://docs.aws.amazon.com/Route53/latest/DeveloperGuide/resolver-dns-firewall-overview.html)).
- **AWS Network Firewall can enforce outside guest control**, but simple domain
  rules inspect HTTP `Host` or TLS SNI, not destination IP. AWS explicitly
  recommends separate IP rules for manipulated Host/SNI cases. Full outbound
  TLS inspection can inspect decrypted paths and headers but requires a trusted
  CA in the guest, adds latency and rules, and does not inspect QUIC; block UDP
  443. Routing must remain symmetric and default-drop
  ([domain-list semantics](https://docs.aws.amazon.com/network-firewall/latest/developerguide/stateful-rule-groups-domain-names.html),
  [TLS inspection limitations](https://docs.aws.amazon.com/network-firewall/latest/developerguide/tls-inspection-considerations.html)).

Also deny unintended **private** destinations. The VPC's local route exists even
without internet; other subnets, endpoint ENIs, private hosted names, and AWS
service VPC endpoints can be reachable. Give the agent SG no inbound rules and
only explicit egress destinations, keep unrelated services in other SGs/VPCs,
apply endpoint policies, and avoid adding endpoint private DNS casually. Task
metadata remains intentionally available inside the task, and task-role
credentials are available there if a task role exists.

IPv6 needs an explicit threat-model line item. A dual-stack task receives IPv6;
an egress-only internet gateway allows outbound IPv6, and NAT64/DNS64 can bridge
IPv6 workloads to IPv4. The least surprising v1 is an IPv4-only agent subnet.
If dual-stack is required, duplicate SG/NACL/firewall policy for IPv6 and prove
there is no `::/0`, egress-only gateway, or `64:ff9b::/96` NAT64 path
([egress-only gateway](https://docs.aws.amazon.com/vpc/latest/userguide/egress-only-internet-gateway.html),
[NAT64/DNS64](https://docs.aws.amazon.com/vpc/latest/userguide/nat-gateway-nat64-dns64.html)).

#### Architecture A: simple no-internet-egress runner

Use this for offline build/test execution. It cannot run an ordinary agent that
must call a remote model API; that requires the narrow-egress architecture:

1. Launch one agent per task in a dedicated IPv4-only private subnet with no
   IGW, NAT, Transit Gateway, peering, VPN, or default route. Use a dedicated SG
   with no inbound and no generic outbound; add a restrictive NACL as a backstop.
2. Pull a pinned image through `ecr.api` and `ecr.dkr` interface endpoints plus
   the free S3 gateway endpoint required for ECR layers. Fargate 1.4+ requires
   both ECR endpoints and S3. Restrict endpoint policies, ECR repositories, S3
   buckets, and the execution role. If using `awslogs`, add the Logs interface
   endpoint; otherwise omit it
   ([private ECR pull requirements](https://docs.aws.amazon.com/AmazonECR/latest/userguide/vpc-endpoints.html)).
3. Supply source and receive the result through unique S3 objects with narrow
   bucket/endpoint/IAM policy. An S3 gateway endpoint has no hourly fee. Do not
   give access to general artifact buckets
   ([S3 gateway endpoint](https://docs.aws.amazon.com/vpc/latest/privatelink/vpc-endpoints-s3.html)).
4. Associate a default-deny Resolver DNS Firewall rule group even though no
   internet route exists, log rejected DNS, and verify IPv4, IPv6, raw-IP, DNS,
   private-CIDR, metadata, and each endpoint in black-box tests.

This is strong and relatively understandable, but “no internet” still exposes
the explicitly allowed AWS endpoint surfaces. The task can talk to any allowed
endpoint as itself; endpoint and IAM policies must make those calls harmless.
Changing the ENI's SG after image pull would introduce a race and a second
cleanup state machine, so it should not be the baseline.

#### Architecture B: narrow model/API egress runner

Keep Architecture A's agent subnet and endpoints, then add a **separate trusted
egress proxy** with its own ENI and security group. The agent SG permits only
TCP to the proxy SG/port (plus required AWS endpoints); it still has no NAT,
IGW, or public IP path. The proxy lives in a different subnet/trust boundary and
has the internet route.

The proxy must terminate and independently validate each request, allow exact
host + port + method + path combinations, revalidate every redirect, resolve
destinations itself, reject literal/private/link-local/reserved IPs, cap request
and response sizes, and inject short-lived model credentials after validation.
An untrusted process can ignore `HTTP_PROXY` variables, so enforcement comes
from the agent SG/routes accepting **only** the proxy—not from client
configuration. Direct raw-IP, alternate DNS, DoH, QUIC, or CONNECT attempts then
have no data-plane path. Resolver DNS Firewall remains useful for stopping and
logging DNS tunneling attempts.

AWS Network Firewall can replace or augment the proxy when broad centralized
inspection is warranted, but it does not provide model credential brokering and
is dramatically more expensive. SNI/Host domain allowlisting without default
drop, IP rules, redirect handling, IPv6 rules, and (where needed) TLS inspection
is not equivalent to the proxy design.

#### Observability and cold start

CloudWatch `awslogs` captures container stdout/stderr through the execution
role; logs are attacker-controlled and can contain injected secrets, so bound
volume, retention, and line/event sizes. ECS task-state events and CloudTrail
cover control-plane actions. Platform 1.4+ exposes all task ENI traffic to VPC
Flow Logs, but flow logs are delayed metadata, not packet contents or Layer 7
hostnames. They also omit traffic to Amazon DNS and EC2 instance metadata, so
they cannot prove that those paths were unused. Add Resolver query logs and
proxy/Network Firewall decision logs for network-policy evidence
([ECS logs](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/using_awslogs.html),
[VPC Flow Logs](https://docs.aws.amazon.com/vpc/latest/userguide/flow-logs.html),
[flow-log exclusions](https://docs.aws.amazon.com/en_en/vpc/latest/userguide/flow-logs-limitations.html),
[Layer 7 limitation](https://docs.aws.amazon.com/prescriptive-guidance/latest/secure-outbound-network-traffic/vpc-security-requirements.html)).

Do not substitute logging for prevention. Daemar needs a per-run correlation ID
across task ARN, source/result object, ENI, log stream, timeout decision, stop
reason, image digest, task-definition revision, capacity provider, and billed
duration.

Fargate has a real per-run cold path. AWS says every task uses a single-use
instance with no cached image layers, so it pulls the full image for every run;
ENI provisioning adds several seconds. AWS publishes no general task-launch
latency SLA. Keep images small and same-region in ECR. SOCI can lazy-load images
larger than roughly 250 MiB, but it adds index production/trust and does not
remove ENI/control-plane latency
([Fargate pull behavior](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/fargate-pull-behavior.html),
[launch-time recommendations](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task-recommendations.html)).

#### Realistic short-task cost

Current N. Virginia Linux/x86 rates are $0.000011244/vCPU-second and
$0.000001235/GiB-second, billed from image-download start until termination,
per second with a one-minute minimum. Thus a 2-vCPU/4-GiB task is about
**$0.09874/hour**, **$0.01646 per 10 minutes**, or **$16.46 for 1,000 ten-minute
tasks**, before every surrounding service. A 0.25-vCPU/0.5-GiB minimum-size task
is about $0.01234/hour, but may be too slow for compilation. The first 20 GiB
ephemeral disk is included; each configured GiB above it is about
$0.00011088/hour. Billing includes cold image-pull time
([Fargate pricing and examples](https://aws.amazon.com/fargate/pricing/)).

The fixed and adjacent costs matter more at low utilization:

| Cost item, N. Virginia where regional | Current list-price implication |
|---|---|
| ECR private storage | $0.10/GB-month; same-region pulls to Fargate have no ECR data-transfer charge. A 5-GB image set is about $0.50/month. |
| ECR private no-egress path | Two interface endpoints (`ecr.api`, `ecr.dkr`) at $0.01/endpoint-AZ-hour each: about $14.60/month in one AZ, plus $0.01/GB. Required S3 gateway endpoint is free. |
| CloudWatch Logs private path | A Logs interface endpoint adds about $7.30/month per AZ plus $0.01/GB, bringing ECR + Logs endpoints to about $21.90/month in one AZ. Standard log ingestion is roughly $0.50/GB after allowance; retention/query and flow logs add charges. |
| NAT Gateway | $0.045/hour plus $0.045/GB processed, and its public IPv4 is $0.005/hour. One continuously provisioned AZ is about $36.50/month before bytes and internet transfer; resilient multi-AZ multiplies it. NAT is not filtering. |
| Per-task public IPv4 | $0.005/address-hour, per second with a 60-second minimum: about $0.00083 for a ten-minute task. Cheap, but it requires an internet route and does not solve narrow egress. |
| Internet data transfer | First 100 GB/month aggregated across eligible AWS services is free, then first 10 TB is $0.09/GB; NAT/PrivateLink/firewall processing is additional. |
| AWS Network Firewall | One primary endpoint is $0.395/hour (about $288.35/month per AZ) plus $0.065/GB; TLS Advanced Inspection can add an hourly charge. Current pricing can waive corresponding NAT hourly/data processing, but the firewall still overwhelms sparse-task compute cost. |
| Resolver DNS Firewall | Foundational rules are $0.60/million queries plus $0.0005/domain-month for customer lists. Advanced protections add $0.16/hour per rule-group/VPC association; basic allowlisting is cheap, advanced is about $116.80/month. |
| Step Functions Standard | $0.000025 per state transition; a small supervisor is pennies per thousand runs, excluding any Lambda work. |

At sparse one-AZ usage, Architecture A therefore has roughly **$14.60/month**
of fixed private endpoints without CloudWatch Logs, or **$21.90/month** with
the Logs endpoint, plus ECR storage, task compute, logs, and S3 storage/requests.
Architecture B can avoid NAT by running a minimum-size always-on proxy task in
a public subnet with its own public IPv4: about **$9.01/month** for
0.25-vCPU/0.5-GiB compute plus **$3.65/month** for the address, before proxy
logs/data and redundancy. That puts a minimal one-AZ ECR + Logs + proxy floor
near **$34.56/month**. A private proxy behind one NAT instead pushes the fixed
floor above **$67/month** after the proxy task; Network Firewall pushes it above
**$320/month**.

These are illustrative list-price floors, not production quotes—two-AZ
availability, a larger proxy, log volume, and data transfer multiply them.

Sources: [ECR pricing](https://aws.amazon.com/ecr/pricing/),
[PrivateLink pricing](https://aws.amazon.com/privatelink/pricing/),
[VPC/NAT/public IPv4 pricing](https://aws.amazon.com/vpc/pricing/),
[CloudWatch pricing](https://aws.amazon.com/cloudwatch/pricing/),
[internet transfer](https://aws.amazon.com/ec2/pricing/on-demand-backup/),
[Network Firewall pricing](https://aws.amazon.com/network-firewall/pricing/),
[Route 53 pricing](https://aws.amazon.com/route53/pricing/), and
[Step Functions pricing](https://aws.amazon.com/step-functions/pricing/).

Fargate Spot applies to Linux x86 and ARM ECS tasks and advertises up to 70%
off on-demand, with a variable current price. It is appropriate only if Daemar
can retry or checkpoint: tasks can be interrupted after a two-minute warning.
An interrupted one-shot task cannot produce a trusted complete result, so its
attempt must be marked aborted and rerun
([Spot pricing](https://aws.amazon.com/fargate/pricing/),
[Fargate Spot interruption](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/capacity-cluster-best-practice.html)).

The operational inventory for even the simple design includes ECS cluster/task
definitions, ECR lifecycle and image signing/scanning, VPC/subnets/routes,
SGs/NACLs, three endpoint and bucket policies, execution/task/controller IAM,
S3 object lifecycle, KMS choices, logs/retention, Step Functions, EventBridge
events, orphan reconciliation, quotas, budgets, and IaC drift tests. Narrow
egress adds a highly sensitive proxy deployment or the much costlier firewall
route stack.

**Daemar conclusion:** ECS/Fargate is a defensible hosted provider, especially
for AWS-native scale or multi-tenant workloads. Its 2-vCPU/4-GiB compute is
cheaper than the reviewed managed sandbox APIs, and the per-task VM boundary is
strong. It does not materially simplify Daemar's current local-first objective:
Apple `container` and Docker Sandboxes avoid persistent cloud network charges,
while Docker/Vercel supply more of the agent-specific egress and credential
contract. Keep Fargate as a future provider behind a narrow runtime interface;
do not make it the cost-conscious v1 default.

### E2B

E2B is a well-established agent-sandbox API backed by Firecracker. It offers
create/connect, commands, PTY, files, Git operations, kill, timeout, and
pause/resume. Internet is allowed by default; `allow_internet_access=false` maps
to an outbound deny for `0.0.0.0/0`, and the current SDK also accepts network
options ([SDK](https://e2b.dev/docs/sdk-reference/python-sdk/v2.15.1/sandbox_async)).

Its infrastructure repository is Apache-2.0 and documents self-hosting on GCP
and beta AWS, but not a supported single-Mac/local deployment. Local development
requires Linux KVM and Firecracker; production self-hosting is an infrastructure
project rather than a cheap desktop runtime
([infra repository](https://github.com/e2b-dev/infra),
[local development](https://github.com/e2b-dev/infra/blob/main/DEV-LOCAL.md)).
The managed Hobby tier has no base charge but a one-hour session limit; Pro's
$150/month floor makes it unattractive for a cost-conscious low-volume user once
the initial credit is exhausted.

### Daytona

Daytona has the broadest multi-language SDK surface here and its current AGPL
platform repository includes a Docker Compose local stack, customer-managed
runners, and hosted operation
([repository](https://github.com/daytonaio/daytona)). It now distinguishes
default Linux containers from an explicit Linux VM sandbox class. Only the VM
class should count toward Daemar's kernel requirement
([sandbox classes](https://www.daytona.io/docs/en/sandboxes/)).

The runner applies iptables policy for block-all, CIDRs, or domain allowlists;
custom per-sandbox policy is restricted to higher billing tiers on the hosted
service. Default lower tiers retain “essential services,” so default policy is
not equivalent to a strict Daemar allowlist
([network limits](https://www.daytona.io/docs/en/network-limits/),
[SDK enforcement note](https://www.daytona.io/docs/en/typescript-sdk/sandbox/)).
The public price page does not distinguish container and VM unit rates, so a VM
trial should verify that the listed rates actually apply to the selected class.

Self-hosting could be valuable for a future Linux box, but running Daytona's
control plane plus a VM-capable runner locally is much heavier than embedding a
runtime. Treat “dedicated kernel” as a selected class to assert and test, not a
property of every Daytona sandbox.

### exe.dev

exe.dev offers persistent root Linux VMs backed by Cloud Hypervisor, provisioned
over SSH/HTTPS in roughly seconds. Its $20/month Personal pool includes up to 50
VMs, 100 GB pooled disk, and a shared 2-vCPU/8-GB baseline; an alternate
per-second plan is competitive with E2B/Daytona
([pricing](https://exe.dev/pricing),
[architecture](https://exe.dev/docs/all)). VMs stop to disk and release compute,
making it an unusually cheap durable remote developer fleet.

It is not a strict sandbox fit. The docs describe private authenticated ingress,
VM-to-VM separation, and credential-brokering integrations, but the reviewed
first-party material did not expose a general deny-all/domain-allowlist egress
policy for arbitrary VM traffic. The product explicitly describes each VM as
having a real network stack. Use it for cheap trusted remote agent computers,
not hostile repositories, unless egress can be enforced independently.

### Fly Sprites

Sprites are persistent Firecracker VMs with a normal ext4 filesystem, API/CLI,
exec sessions, supervised services, checkpoint/restore, public URLs, and
automatic transition to non-billed warm/cold states. Fly publishes integrations
for Claude Code, Codex, Cursor, DeepSeek Harness, Pi, Google ADK, and Anthropic
Managed Agents
([ecosystem](https://fly.io/sprites/ecosystem/)). This is unusually strong proof
that the product is meant to be an agent computer rather than a generic VM.

The cost model is good for sparse, durable work because it bills actual CPU and
RAM while running and storage while inactive. A documented four-hour Claude
Code example totals $0.44. Egress is explicitly permissive by default. The
policy API supports exact and wildcard domain rules and terminates connections
when newly denied, but describes enforcement as DNS-based
([policy API](https://docs.sprites.dev/api/dev-latest/policy/)). Before using it
for hostile code, test raw IP TCP, cached DNS, alternate resolvers, DoH, IPv6,
UDP, rebinding, and established-connection behavior.

### Modal

Modal's normal sandbox boundary is gVisor, not a separate Linux VM. An opt-in
full VM runtime now exists, but is beta. The v2 Sandbox API and VM mode are also
marked beta/experimental in current docs
([VM Sandboxes](https://modal.com/docs/guide/vm-sandboxes)). That removes it from
the conservative shortlist despite a good API and $30/month Starter credit.

Network controls are stronger than many competitors: hard block-all, CIDR
allowlists, and a beta TLS-domain allowlist that blocks other raw TCP and UDP;
runtime policy replacement is alpha. Default outbound internet remains open
([networking](https://modal.com/docs/guide/sandbox-networking)). OpenHands has a
Modal provider, and Modal cites Ramp's Inspect background coding agent as a
customer. Revisit when VM mode and its network API are stable.

### Cloudflare Sandbox

Cloudflare's current security documentation says each sandbox runs in its own
VM, with separate filesystem, processes, network, and resource limits
([security model](https://developers.cloudflare.com/sandbox/concepts/security/),
[Containers architecture](https://developers.cloudflare.com/containers/platform-details/architecture/)).
The TypeScript SDK supplies command/file/process/port and backup/restore APIs and
ships official Claude Code, OpenAI Agents SDK, and OpenCode examples
([SDK repository](https://github.com/cloudflare/sandbox-sdk)).

The cost floor is only the $5/month Workers Paid plan; the included allowance
and active-CPU pricing make it the cheapest hosted burst candidate in the table.
However, the stable SDK remains pre-1.0, Sandbox pricing also includes Worker and
Durable Object usage, disks are ephemeral on sleep unless the newer backup path
is used, and the reviewed docs did not expose a general sandbox egress
allowlist/block-all control. Outbound handlers can keep credentials in the
Worker, but credential brokering is not the same as preventing arbitrary egress.
Network isolation must be clarified and empirically tested before adoption.

### Vercel Sandbox

Vercel provides a Firecracker microVM with sudo, OCI images, command/files API,
ports, snapshots, fork, and named persistent sandboxes. The CLI and SDK are
Apache-2.0; the backend is a proprietary service
([repository](https://github.com/vercel/sandbox)). Network policy can deny all,
allow domains and CIDRs, prioritize CIDR denies, broker credentials, and apply
L7 method/path/header matchers. The default is allow-all
([current SDK skill/reference](https://github.com/vercel/sandbox/blob/main/skills/sandbox/SKILL.md)).

Vercel's own OpenCode guide applies egress restrictions, and the Vercel AI SDK
contains a Harness sandbox-provider implementation
([OpenCode guide](https://vercel.com/kb/guide/running-opencode-securely-with-the-vercel-sandbox),
[AI SDK provider](https://github.com/vercel/ai/blob/main/packages/sandbox-vercel/src/vercel-sandbox.ts)).
That is stronger real-harness evidence than vendor testimonials alone. The free
Hobby allowance makes a proof inexpensive. The tradeoff is pricing: fully busy
CPU costs more than E2B/Daytona, though agent workloads waiting on model APIs pay
memory but not CPU during the wait.

### GitHub Codespaces

Codespaces provides excellent VM isolation: every codespace gets a newly built
dedicated VM and network, and two codespaces are never co-located on one VM
([security](https://docs.github.com/en/codespaces/reference/security-in-github-codespaces)).
The personal free allowance is generous enough for experiments.

It fails the network requirement. GitHub states that codespaces have public
internet access by default and that there is currently no way to restrict it
([private network documentation](https://docs.github.com/en/codespaces/developing-in-a-codespace/connecting-to-a-private-network)).
It is also designed as a persistent GitHub developer environment, not a neutral
low-latency create/exec/result API. It is a useful UX comparator but should not
be Daemar's substrate.

## Security and evidence cautions

The following claims need proof on a pinned version before adoption:

1. **Domain allowlists are especially easy to over-credit.** Test raw IPv4 and
   IPv6, custom DNS and DoH, DNS rebinding, SNI/Host disagreement, HTTP CONNECT,
   UDP/QUIC, redirects, and existing sockets after policy change. A vendor
   saying “domain policy” does not establish that guest root cannot bypass it.
2. **Defaults often negate the architecture.** Docker Sandboxes directly mounts
   the worktree and shares skills by default; E2B, Fly, Modal, and Vercel allow
   broad outbound access by default; a fresh security group permits all
   outbound traffic; Codespaces cannot restrict it. Daemar must create policy
   explicitly and fail closed if it cannot confirm the applied state.
3. **“Open source” frequently covers only the SDK.** Docker Sandboxes is
   proprietary. The E2B and Daytona control planes are available, but their
   self-host paths are infrastructure deployments. Vercel, Fly, Modal,
   Cloudflare, exe.dev, and Codespaces expose open clients at most; their hosted
   runtime/control plane remains vendor-operated.
4. **A VM does not secure host shares.** Direct read-write mounts, virtiofs,
   guest-authored archives, shared caches, agent configuration, Git hooks, and
   result directories all cross the VM boundary. Daemar must separately verify
   host-path containment and perform promotion after the guest is dead.
5. **Marketing evidence is not independent assurance.** “Secure,” “complete
   isolation,” “hardware-isolated,” and customer-logo/case-study claims are
   recorded only as vendor claims unless accompanied by an architecture,
   threat model, audit, advisory trail, and adversarial result.

## Recommended next trials

Run the same black-box qualification battery against these, in order:

1. **Docker Sandboxes 0.39.x local:** clone mode, no shared skills, explicit
   minimal policy, credential injection, stop/remove, crash recovery, and exact
   disk residue. This tests whether Daemar can buy almost the whole local
   boundary for zero software cost.
2. **Apple `container` pinned release:** keep as the open-source, no-network
   control. Compare implementation size and ergonomics, not only escape tests.
3. **Vercel Hobby and Cloudflare Workers Paid:** cap each proof at a small fixed
   spend. Verify raw-IP egress, guest-root policy bypass, timeout/delete, file
   export, billing while model-waiting, and retained snapshots/state.
4. **Fly Sprites:** test as the durable alternative if Daemar decides that an
   environment should survive tickets. Focus the proof on DNS-policy bypass and
   unexpected idle wakeups/cost.

Do not prioritize a Fargate proof for the local-first v1. If an AWS-native
provider becomes a concrete requirement, cap a one-AZ Architecture B proof and
test the route table, SG, DNS Firewall, endpoint policies, task-role endpoint,
raw IPv4/IPv6, redirect, timeout/orphan, and Spot-interruption cases before
building a general adapter.

Defer direct Firecracker/Kata ownership until Daemar has a Linux-host product
requirement and evidence that the higher-level local options cannot satisfy it.
Do not spend qualification time on Codespaces, gVisor, nsjail, or bubblewrap for
the current product contract: each already fails a hard requirement in its
documented architecture.
