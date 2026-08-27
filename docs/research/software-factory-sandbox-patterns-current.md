# How software factories sandbox coding agents today

**Current as of 2026-08-26. Primary sources and source-visible systems only.**
Repository claims are pinned to the exact revisions listed below. Hosted documentation
is cited where it describes behavior not apparent from one source file; those pages
were read on the retrieval date and are not themselves immutable.

## Answer

There is no settled factory-wide sandbox architecture. The current systems fall into
four materially different camps:

1. **No sandbox.** The factory starts a coding-agent or shell process as the worker OS
   user. A Git worktree, branch, temporary directory, command blocklist, or process
   supervisor may separate work, but none is a security boundary.
2. **Shared-kernel container.** The agent or its command executor runs in Docker. This
   limits ordinary host access, but the guest shares the host kernel; mounted
   workspaces and unrestricted networking are common.
3. **Sandbox-as-tool.** The model loop and durable orchestration remain trusted on the
   host while only model-generated commands run in an isolated container or microVM.
   This naturally keeps model credentials outside the execution guest, but every
   host-side file or MCP tool remains part of the trusted capability boundary.
4. **Agent-in-sandbox.** The coding-agent runtime itself runs inside a container or VM.
   This gives the most complete environment boundary, but creates the hard credential
   problem: the in-guest agent needs model and often Git/provider access.

Only two current local products examined here combine a separate guest kernel with
first-class network policy: **Docker Sandboxes** and **Raven's opt-in BoxLite command
executor**. They embody different architectures. Docker runs the full coding agent in
the microVM and brokers credentials through a host proxy. Raven keeps the model loop on
the host and sends shell/MCP execution to per-agent microVMs. Raven is open source and
free, but its workspace is a live read-write host mount and its ordinary filesystem
tools still operate on the host path. Docker has the more complete boundary in clone
mode, but `sbx` is a source-visible/free product rather than an open-source component.

## Classification used here

| Class | Meaning in this note |
|---|---|
| **Separate-kernel sandbox** | A VM or microVM gives the workload its own guest kernel. This is the target class for hostile-code execution. |
| **Shared-kernel container** | Docker/OCI container isolation. Useful containment, but not kernel isolation. |
| **No sandbox** | Host subprocess, host shell, worktree, branch, temp directory, prompt rule, allow/block list in application code, or a container merely used to package the whole application without a per-run boundary. |

A Git worktree is always classified **NO SANDBOX**. Workspace separation is recorded in
its own column and never upgraded into a security claim.

## Comparison at a glance

| System and pinned revision | Actual execution boundary | Network | Workspace / result flow | Credential posture | Lifecycle and local cost |
|---|---|---|---|---|---|
| [Factory `5b5e197`](https://github.com/owainlewis/factory/tree/5b5e197120acaac242515ce5b4768b0bad61662c) | Its published V1 design is **NO SANDBOX.** The agent is a host/VM worker child process with the worker user's authority. | Worker user's network. | Attempt worktree; agent commits/pushes an immutable publish ref and opens a PR. Worktree is separation only. | Agent can read ambient worker credentials and authenticated CLIs; attempt token is explicitly not a sandbox. | Local worker and SQLite control plane; no sandbox bill. Retained worktrees and supervised process groups. |
| [Fabro `56e759d`](https://github.com/fabro-sh/fabro/tree/56e759d470caed526032def95f5a56c50c2d4b1f) | Configurable: **NO SANDBOX** (`local`), **shared-kernel** (`docker`, default), or hosted Daytona VM. | No deny-by-default policy was found for local/Docker. Daytona advertises network controls, but the inspected Fabro docs do not establish that a restrictive policy is the default. | Docker/Daytona clone repo; every stage commits code and execution metadata, pushes run and metadata branches, and can resume from checkpoints. | Server/shell owns model keys. Git `token` mode injects the captured token; GitHub App mode mints a short-lived repository-scoped installation token. | Local Docker is local compute; Daytona is hosted usage cost. Daytona destroyed at run end unless preserved. |
| [OpenHands `59981ca`](https://github.com/OpenHands/OpenHands/tree/59981caf7fd92971681b0ab5354c37e9f1cab406), [docs `00e7693`](https://github.com/OpenHands/docs/tree/00e76938c86785a47734c01beb11896717767977) | V1 defaults to **shared-kernel Docker**; process mode is **NO SANDBOX**; remote provider varies. | Docker bridge has ordinary outbound access. Docs expose host/additional networking but no default-deny egress capability. | Typical local flow bind-mounts host repo read-write at `/workspace`; overlay/named volumes exist, but direct mount is the documented path. | Settings hold LLM/integration secrets. `SANDBOX_ENV_*` explicitly passes raw variables into the sandbox; no local credential broker is documented. | Local Docker requires Docker and model API only. Containers may stop/remove, pause, or be kept alive by configuration; cloud is separate. |
| [GPT Pilot `9b763fd`](https://github.com/Pythagora-io/gpt-pilot/tree/9b763fdaf0020c7d8abacc7b58b2b09e57494623) | Native install is **NO SANDBOX**. Docker Compose packages the long-running app, but is not a fresh per-task boundary; it remains a shared-kernel container. | Unrestricted host/container networking. | Generated app lives in `workspace`; Compose bind-mounts `~/gpt-pilot-workspace` read-write. State in SQLite/Postgres supports continuation. | LLM and DB credentials are placed in `.env`/Compose and are available to the same application that executes work. | Free local software plus model cost; persistent app/database rather than disposable task guests. Project is legacy rather than a strong current substrate candidate. |
| [MetaGPT `11cdf46`](https://github.com/FoundationAgents/MetaGPT/tree/11cdf466d042aece04fc6cfd13b28e1a70341b1f) | Default is **NO SANDBOX**: `subprocess.run`, persistent host shell, and some direct Python execution. Official Docker packaging does not create a per-agent boundary. | Host/container network is unrestricted. | Writes a generated project under `./workspace`; no isolated result promotion protocol. | Model keys live in `~/.metagpt/config2.yaml` or process/container config available to the framework. | Free local framework plus model cost. Processes and workspace persist according to caller/container, not per hostile-code attempt. |
| [AutoGen/Magentic-One `027ecf0`](https://github.com/microsoft/autogen/tree/027ecf0a379bcc1d09956d46d12d44a3ad9cee14) | Default Magentic-One code executor is **shared-kernel Docker**; optional local executor is **NO SANDBOX**. This is sandbox-as-tool: orchestration/model client remain outside. | Docker default network; no `network_disabled` or allowlist option in the executor surface. `extra_hosts` can widen reach. | Host `work_dir`/`bind_dir` is mounted into the container; generated files return through that directory. | Model credential remains with the outer model client unless caller explicitly injects it; extra volumes/initialization are caller-controlled. | Local Docker + Python packages; default `auto_remove=true`, `stop_container=true`; model cost only beyond local compute. |
| [SWE-agent `3ea751c`](https://github.com/SWE-agent/SWE-agent/tree/3ea751c087f32b16e039a2233dd6eefecef325d5), [SWE-ReX `5c995c3`](https://github.com/SWE-agent/SWE-ReX/tree/5c995c365dfb1fd5bc56fda688be5d8538f9931f) | Default local deployment is a **shared-kernel Docker** container controlled by SWE-ReX; direct deployment is **NO SANDBOX**. Model loop stays outside. | Ordinary container outbound network; no restrictive default found. | Local repo is copied or GitHub repo cloned into deployment, reset to base SHA, and final result is an extracted patch plus trajectory. This is strong result separation, not kernel isolation. | Model key stays with the outer agent process. Repo cloning may need deployment-visible Git credentials for private sources. | Local Docker is free; optional Modal/AWS/Fargate adds hosted cost. Deployment starts, resets between attempts, then stops. SWE-agent is maintenance-only and recommends mini-SWE-agent. |
| [SSSF `de31374`](https://github.com/disler/super-simple-software-factory/tree/de31374882e7a4e3e5b7bb9bd09e69dc2f779356) | Explicitly **NO SANDBOX**. Agents run on the current branch. | Host network. | Current branch, no branch per run or merge step; JSONL/SQLite traces are durable evidence. | Provider key is in repo-local `.env`; agent/command processes share host authority. | Free local orchestration plus model cost; workflow process owns phase lifecycle. |
| [SSSF “Factory in a Box” `92f1701`](https://github.com/disler/inkwell-agent-sandboxes-and-software-factory/tree/92f1701810993b8303562265ba04c727468fe070) | Wrapper adds a disposable Apple Container VM: **separate-kernel sandbox** around the whole SSSF run. | VM connectivity is available; no independently enforced domain allowlist/deny-by-default policy was found. | Host-side recipes mount a box, insert a task, run the factory, harvest selected output, and tear down. | Explicit environment/credential handoff into the box; credentials present in the guest remain readable by guest root. | Local Apple Silicon/macOS substrate and no hosted sandbox bill; reference implementation has only one repository commit. |
| [Raven `c9077c2`](https://github.com/EverMind-AI/Raven/tree/c9077c2aa1b272b2888f66abcb67619e7d424739), [BoxLite `d354470`](https://github.com/boxlite-ai/boxlite/tree/d354470ac25d5ce8e83d7520c3f5ad590182907d) | Opt-in BoxLite is a **separate-kernel microVM** for every shell command stream and stdio MCP server; each subagent gets its own VM. Default `backend=none` is **NO SANDBOX**. | Default unrestricted; `allow_net=false` or domain allowlist is supported and enforced by BoxLite. | `/workspace` is a live RW mount of host workspace. Raven file read/write tools still act directly on host paths; result changes are immediately visible, not promoted. | Model loop/provider stays outside the VM. No Raven credential-broker contract was found; explicit command/MCP environment values enter the VM. | Open-source local runtime, `boxlite==0.8.2`, Apple Silicon Hypervisor.framework or Linux KVM. Per-subagent VM torn down on completion; ~2–5 s warm-image start cited. |
| [Docker Sandboxes docs `4d295a7`](https://github.com/docker/docs/tree/4d295a786801cced0a5a20d560379fac0d630d80) | Full coding agent in its own **separate-kernel microVM**, with a private Docker Engine. | Host-side proxy, deny-by-default destination policy, internal DNS enforcement; outbound TCP only when allowed; external UDP/ICMP blocked. | Default direct RW mount is unsafe for result isolation. `--clone` mounts host repo RO, works in a private clone, and exposes a local Git remote for explicit fetch/push. | Host proxy replaces sentinels/injects HTTP credentials; real values do not enter VM for supported flows. Raw custom/OAuth passthrough remains an explicit downgrade. | `sbx` CLI is free including commercial use; governance subscription is paid. Local Docker Desktop/product dependency. VM persists across restarts and is fully deleted by `sbx rm`. |

## System notes and primary evidence

### Factory (owainlewis)

Factory is unusually explicit about its non-boundary. Its current design says the
worker owns the worktree and agent process, but “does not treat the agent process as
isolated from other files and services available to the Worker operating-system user.”
The security section says worktrees are not a sandbox and the agent receives the
worker user's filesystem, network, Git, and CLI permissions. The attempt update token
prevents cross-job mistakes; it does not constrain a malicious process.

The useful factory pattern is instead its delivery discipline: a fresh attempt
worktree, immutable publish ref, commit/push checkpoint before pausing for input,
validation that local `HEAD`, fetched ref, and PR head match, and retained failed
worktrees. That is excellent workspace/result accounting, but **NO SANDBOX**.

Sources: pinned [software-factory design and security model](https://github.com/owainlewis/factory/blob/5b5e197120acaac242515ce5b4768b0bad61662c/docs/software-factory/design.md), especially its Worker/supervisor, trust, and security sections.

### Fabro

Fabro exposes the substrate as a run choice: `local`, `docker` (default), or `daytona`.
The API agent loop normally remains in Fabro and invokes tools through the sandbox;
ACP mode instead launches the external agent process inside the selected local/Docker
sandbox. That distinction matters: Docker API mode can keep the model loop and its
provider credential outside the code container, while ACP-owned authentication can put
credentials inside it.

Its strongest transferable factory practice is stage-level Git checkpointing. Each
stage commits code and metadata, enabling trace, resume, and revert. For GitHub access,
the App strategy creates short-lived installation tokens scoped to one repository and
declared permissions. The simpler token strategy reuses a captured user token and is a
substantially broader credential boundary.

Sources: [official architecture/how-it-works](https://docs.fabro.sh/core-concepts/how-fabro-works), [agent backends](https://docs.fabro.sh/core-concepts/agents), [GitHub token and checkpoint flow](https://docs.fabro.sh/integrations/github), [Daytona SSH, preservation, and cleanup](https://docs.fabro.sh/human-tools/ssh-access), and pinned [source snapshot](https://github.com/fabro-sh/fabro/tree/56e759d470caed526032def95f5a56c50c2d4b1f).

### OpenHands

OpenHands V1 correctly labels three different providers: Docker, process, and remote.
The process sandbox is explicitly no isolation. Docker is the recommended local
provider, but remains a shared-kernel boundary. The common `openhands serve --mount-cwd`
workflow bind-mounts the repository read-write; the docs warn that anything mounted RW
can be modified by the agent.

The container network is not an egress capability system. Current configuration offers
host networking and additional Docker networks, while hardened guidance focuses on
binding the OpenHands service to loopback. `SANDBOX_ENV_*` is a general secret/variable
pass-through, so a value passed this way is available to guest processes.

Also note the mode-dependent trap: the supported process provider runs directly on the
host while Docker is the default. Product-level statements that “OpenHands is
sandboxed” are therefore too broad; the selected provider must be pinned.

Sources: [V1 sandbox overview](https://docs.openhands.dev/openhands/usage/sandboxes/overview), [Docker sandbox and RW mounts](https://docs.openhands.dev/openhands/usage/sandboxes/docker), [process mode warning](https://docs.openhands.dev/openhands/usage/sandboxes/process), [configuration/pass-through reference](https://docs.openhands.dev/openhands/usage/environment-variables), and pinned [runtime architecture](https://github.com/OpenHands/docs/blob/00e76938c86785a47734c01beb11896717767977/openhands/usage/architecture/runtime.mdx).

### GPT Pilot / Pythagora

GPT Pilot is a durable multi-role app builder, not a sandboxed factory. Native mode
runs directly on the host. Its optional Compose deployment places the framework in one
long-running container and bind-mounts `~/gpt-pilot-workspace`; it does not create a
fresh guest per project, stage, or command. Model and database credentials are supplied
to the same application container. This can be a convenient packaging boundary, but it
does not implement least-privilege network, credential mediation, or deliberate result
promotion.

Sources: pinned [README](https://github.com/Pythagora-io/gpt-pilot/blob/9b763fdaf0020c7d8abacc7b58b2b09e57494623/README.md) and [Compose configuration](https://github.com/Pythagora-io/gpt-pilot/blob/9b763fdaf0020c7d8abacc7b58b2b09e57494623/docker-compose.yml).

### MetaGPT

MetaGPT's “software company” roles do not imply execution isolation. The registered
shell helper invokes `subprocess.run` on the framework host, using `shell=True` for a
string command. Its terminal tool maintains a persistent shell process. The official
Docker path packages the framework, configuration, and workspace; it is not a
per-agent/per-attempt guest or a separate-kernel boundary.

Sources: pinned [`shell.py`](https://github.com/FoundationAgents/MetaGPT/blob/11cdf466d042aece04fc6cfd13b28e1a70341b1f/metagpt/tools/libs/shell.py), [`terminal.py`](https://github.com/FoundationAgents/MetaGPT/blob/11cdf466d042aece04fc6cfd13b28e1a70341b1f/metagpt/tools/libs/terminal.py), [Dockerfile](https://github.com/FoundationAgents/MetaGPT/blob/11cdf466d042aece04fc6cfd13b28e1a70341b1f/Dockerfile), and [official repository setup/config](https://github.com/FoundationAgents/MetaGPT/tree/11cdf466d042aece04fc6cfd13b28e1a70341b1f).

### AutoGen / Magentic-One

Current Magentic-One composes file/web/coder agents with a `CodeExecutorAgent` and uses
`DockerCommandLineCodeExecutor` by default. This is not a complete project VM: code
blocks are written to a host working directory, that directory is mounted into a Docker
container, and the blocks execute there. The outer model client and orchestration stay
outside. Docker auto-removal and stop-on-context-exit are good defaults.

The executor surface includes image, timeouts, bind directory, extra volumes, hosts,
and initialization, but no deny-by-default or domain egress rule. Its security ceiling
is therefore shared-kernel Docker plus whatever network controls the caller separately
installs.

Sources: pinned [Magentic-One team composition](https://github.com/microsoft/autogen/blob/027ecf0a379bcc1d09956d46d12d44a3ad9cee14/python/packages/autogen-ext/src/autogen_ext/teams/magentic_one.py), pinned [Docker executor](https://github.com/microsoft/autogen/blob/027ecf0a379bcc1d09956d46d12d44a3ad9cee14/python/packages/autogen-ext/src/autogen_ext/code_executors/docker/_docker_code_executor.py), and [official executor API](https://microsoft.github.io/autogen/stable/reference/python/autogen_ext.code_executors.docker.html).

### SWE-agent and SWE-ReX

SWE-agent is the best example here of separating model orchestration, command
execution, and result extraction. The outer process prompts the model; SWE-ReX starts
a shell server inside a local Docker or remote deployment; the repository is copied or
cloned and reset; the agent submits; the harness extracts a patch and trajectory; the
deployment stops. This is an evaluation-grade result flow.

It is still not separate-kernel local isolation. Default Docker uses the host kernel
and ordinary network. SWE-agent now describes itself as maintenance-only and recommends
mini-SWE-agent, so it is more valuable as a proven pattern than as Daemar's likely
long-term dependency.

Sources: [official architecture](https://swe-agent.com/latest/background/architecture/), [environment/deployment choices](https://swe-agent.com/latest/usage/hello_world/), [repo copy/reset configuration](https://swe-agent.com/latest/reference/repo/), [patch submission flow](https://swe-agent.com/latest/usage/hello_world/), and pinned [SWE-agent](https://github.com/SWE-agent/SWE-agent/tree/3ea751c087f32b16e039a2233dd6eefecef325d5) / [SWE-ReX](https://github.com/SWE-agent/SWE-ReX/tree/5c995c365dfb1fd5bc56fda688be5d8538f9931f) snapshots.

### Super Simple Software Factory and Factory in a Box

The base SSSF is admirably unambiguous: “There is no sandbox, no branch per run, no
merge step.” Deterministic Python owns phases, retries, gates, and trace recording while
agents own bounded work. It is a harness pattern, not a security boundary.

Factory in a Box composes that harness with a disposable Apple Container VM and explicit
mount/insert/run/harvest/teardown host recipes. This upgrades command and kernel
isolation substantially while retaining a small, inspectable factory. It does not yet
show the other half of the desired boundary: restrictive, host-enforced egress and
mediated credentials. Treat it as a useful composition prototype, not a qualified
secure runtime.

Sources: pinned [SSSF README](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/README.md) and pinned [Factory in a Box README](https://github.com/disler/inkwell-agent-sandboxes-and-software-factory/blob/92f1701810993b8303562265ba04c727468fe070/README.md).

### Raven + BoxLite

Raven is the strongest open-source local microVM integration found in an active agent
harness. Opting into `backend=boxlite` runs shell execution and stdio MCP servers in a
BoxLite microVM; each spawned subagent gets its own VM. BoxLite uses KVM on Linux or
Hypervisor.framework on Apple Silicon. Raven can allow all networking, block it, or
provide a domain allowlist. Restricted mode pre-pulls the OCI image in a separate
throwaway unrestricted VM before starting the working VM, a useful supply/setup split.

There are three important qualifications:

- sandboxing is off by default (`backend=none`);
- the workspace is a live RW volume mount, so guest writes immediately alter the host
  workspace;
- Raven's regular read/write filesystem tools continue to act directly on the host
  path. The microVM protects the host from executed code, but it is not the only route
  by which the model changes files.

This is a strong substrate/integration candidate if Daemar supplies its own deliberate
code ingress and result egress instead of adopting Raven's live mount.

Sources: pinned [Raven sandbox manual](https://github.com/EverMind-AI/Raven/blob/c9077c2aa1b272b2888f66abcb67619e7d424739/docs/sandbox/usage.md), pinned [BoxLite agent integration](https://github.com/boxlite-ai/boxlite/blob/d354470ac25d5ce8e83d7520c3f5ad590182907d/docs/guides/ai-agent-integration.md), and [BoxLite repository/security architecture](https://github.com/boxlite-ai/boxlite/tree/d354470ac25d5ce8e83d7520c3f5ad590182907d).

### Docker Sandboxes

Docker Sandboxes is the most complete local product comparison, even though it is not
an open-source runtime dependency. Every sandbox is a microVM with its own Linux kernel,
network, filesystem, and private Docker Engine. Host-side policy proxies all outbound
TCP, internal DNS applies destination policy, and supported credentials are injected by
the proxy without exposing their real values in the VM.

Its default direct workspace mode is a serious caveat: the agent edits the host working
tree live. Clone mode is the relevant design. It mounts the complete host repository
read-only at `/run/sandbox/source`, creates a private in-VM clone, and publishes that
clone through a local Git daemon exposed as a host remote. The host explicitly fetches
or the agent explicitly pushes; guest root cannot write the host repository. Clone mode
still exposes ignored/untracked files for reading, so secrets must not live in the repo.

The product is currently free to use, including commercial use; organization-wide
governance is the separately paid feature. The dependencies are Docker's local product,
login, and supported platform rather than per-run hosted compute.

Sources, all pinned to Docker docs revision `4d295a7`:
[security model](https://github.com/docker/docs/blob/4d295a786801cced0a5a20d560379fac0d630d80/content/manuals/ai/sandboxes/security/_index.md),
[isolation and clone-mode implementation contract](https://github.com/docker/docs/blob/4d295a786801cced0a5a20d560379fac0d630d80/content/manuals/ai/sandboxes/security/isolation.md),
[Git flow](https://github.com/docker/docs/blob/4d295a786801cced0a5a20d560379fac0d630d80/content/manuals/ai/sandboxes/workflows/git.md), and
[credential proxy](https://github.com/docker/docs/blob/4d295a786801cced0a5a20d560379fac0d630d80/content/manuals/ai/sandboxes/configuration/credentials.md).

## Repeated patterns worth taking

### What the field commonly does

- The dominant open-source baseline is still **host loop + Docker command execution**.
  It is cheap and keeps model credentials outside generated-code execution, but shares
  the host kernel and usually leaves egress open.
- Factories emphasize **workspace accountability more often than security**: worktrees,
  per-run branches, Git checkpoints, extracted patches, retained failures, and PRs.
  These are valuable but do not sandbox anything.
- When the entire coding CLI runs inside the guest, projects commonly inject ambient
  API/Git credentials. Short-lived scoped GitHub App tokens are better; a host-side
  credential proxy is the strongest observed pattern.
- Network policy is normally absent or unrestricted. Products that do it seriously
  route traffic through an external enforcement point; they do not rely on guest root,
  prompts, DNS configuration the guest controls, or command blocklists.
- Lifecycle is usually one guest per run/attempt with cleanup on normal and exceptional
  exits. Preserving a failed guest is useful for diagnosis but should be a deliberate,
  visible exception with credential expiry.

### Design candidates for Daemar

1. **Copy Docker Sandboxes' security shape:** separate-kernel local microVM, clone-mode
   input, host-side egress proxy, host-held credential substitution, explicit Git result
   fetch, and full guest deletion. It is the closest observed match to the desired
   product contract.
2. **Prototype BoxLite as an open-source substrate:** it is local, daemonless, OCI-native,
   separate-kernel, Apple-Silicon capable, and already demonstrates allowlisted egress.
   Do not copy Raven's RW workspace mount or host-direct file tools; use Daemar-owned
   copy-in/copy-out or a read-only source plus private guest clone.
3. **Retain Factory/SWE-agent result discipline:** immutable base SHA, one attempt-owned
   workspace, extracted patch or immutable published commit, postflight proof, retained
   failed evidence, and no claim that a worktree is security.
4. **Use sandbox-as-tool only deliberately.** It gives clean model-key separation, but
   host file tools can silently bypass the execution boundary. Either every mutation and
   command crosses the sandbox protocol, or the remaining host capabilities must be
   narrow, typed, and treated as trusted authority.
5. **Keep a cheap hosted escape hatch as a provider, not the definition.** Fabro shows a
   practical local-Docker/hosted-VM switch. Daemar can remain local-first while leaving
   room for a low-cost remote substrate later, provided the same ingress, egress,
   network, credential, and lifecycle contract is preserved.

## Bottom line

Most harnesses are not doing kernel-and-network isolation today. They are doing local
host processes, Git separation, or Docker with open egress. The notable exception is a
newer local microVM pattern: run a whole coding agent in a VM with a host policy and
credential proxy (Docker Sandboxes), or keep the loop trusted and place execution in
per-agent local microVMs (Raven/BoxLite). For Daemar, the best evidence-supported path is
to evaluate the latter as an open substrate while using the former as the behavioral
reference, and to combine either with explicit Git/copy result promotion rather than a
live writable host mount.
