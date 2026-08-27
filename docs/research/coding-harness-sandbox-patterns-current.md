# How open-source coding-agent harnesses sandbox execution

**Snapshot date:** 2026-08-26  
**Question:** What do current coding-agent harnesses actually do for local execution isolation, and what can a local, cost-conscious software factory learn from them?

## Executive conclusion

The modal open-source coding agent is **not sandboxed by default**. Aider, Cline, Roo Code, Goose, Continue, the ordinary `mini` entry point in mini-swe-agent, and Gemini CLI all normally execute against the host. They rely on approvals, tool filters, ignore files, Git checkpoints, or worktrees. Those are useful safety and recovery controls, but they do not create an execution boundary.

When harnesses do isolate execution, the common substrate is a Docker/Podman container or a native OS policy such as macOS Seatbelt or Linux bubblewrap. Both are normally **same-kernel** boundaries. They protect more than an approval prompt, but they do not meet a requirement for a separately isolated guest kernel. Among the projects inspected, Gemini CLI is the clearest local exception: it offers an explicit gVisor/runsc provider. A separate local VM remains unusual in the harness itself; hosted providers sometimes supply one, but that shifts cost and trust to the service.

Network containment is less mature than filesystem containment. Most optional container modes use ordinary container networking with unrestricted egress. Codex CLI, Claude Code, and current Gemini CLI have meaningful deny/proxy mechanisms, but each has scope or fail-open caveats. Credentials are also frequently colocated with agent execution: inherited environment variables, mounted auth directories, tokens embedded in clone URLs, or API keys explicitly passed into containers.

For Daemar, the market pattern is therefore evidence for building a distinct execution substrate rather than copying a harness default: one ephemeral environment per task, a separately isolated kernel, deny-by-default egress enforced outside the guest, explicit workspace import/export, narrow credential brokering, and deterministic teardown.

## Method and terminology

This is a source-level survey, not a marketing comparison. Each open-source repository was inspected at the pinned revision below. Product documentation is used where the implementation is not published (notably Claude Code), and that limitation is called out. Claims about Docker, Seatbelt, bubblewrap, and gVisor describe the configured mechanism, not an unverified claim that it withstands an arbitrary kernel exploit.

Classification rules:

- **NO SANDBOX** means the agent's commands execute as the invoking user on the host. Approval prompts do not change that classification.
- A **worktree**, branch, shadow Git repository, or checkpoint separates or restores code changes. It is **not a sandbox in any sense** and is reported only under workspace transport/recovery.
- **Same kernel** means the process is constrained but still relies on the host Linux/macOS kernel (ordinary Linux containers, Seatbelt, bubblewrap, Apptainer, LXC).
- **Separate kernel** means the workload crosses a VM-like or user-space-kernel boundary. gVisor is listed separately because it interposes a user-space application kernel; it is stronger than an ordinary container, although not identical to a hardware VM.
- “Network off” applies to tool/command execution only when the harness explicitly enforces it. Model API calls necessarily occur somewhere; several harnesses keep that control-plane traffic outside the command sandbox.

### Pinned source revisions

| Project | Revision inspected |
|---|---|
| OpenHands UI/server | [`59981caf7fd92971681b0ab5354c37e9f1cab406`](https://github.com/OpenHands/OpenHands/tree/59981caf7fd92971681b0ab5354c37e9f1cab406) |
| OpenHands software-agent SDK/workspaces | [`6fce02687281cb6f27015e18a02f7b92c87212f2`](https://github.com/OpenHands/software-agent-sdk/tree/6fce02687281cb6f27015e18a02f7b92c87212f2) |
| SWE-agent | [`3ea751c087f32b16e039a2233dd6eefecef325d5`](https://github.com/SWE-agent/SWE-agent/tree/3ea751c087f32b16e039a2233dd6eefecef325d5) |
| mini-swe-agent | [`25941c89cfbc91eb40b3f8756348c91d9977d57e`](https://github.com/SWE-agent/mini-swe-agent/tree/25941c89cfbc91eb40b3f8756348c91d9977d57e) |
| Aider | [`5dc9490bb35f9729ef2c95d00a19ccd30c26339c`](https://github.com/Aider-AI/aider/tree/5dc9490bb35f9729ef2c95d00a19ccd30c26339c) |
| Cline | [`b4fd4ee0cde1df0f23b425ada715720cb271819a`](https://github.com/cline/cline/tree/b4fd4ee0cde1df0f23b425ada715720cb271819a) |
| Roo Code | [`b867ec9145750d0ae1ff7f02d35406e9bf2a0b16`](https://github.com/RooCodeInc/Roo-Code/tree/b867ec9145750d0ae1ff7f02d35406e9bf2a0b16) |
| OpenAI Codex CLI | [`b68acc4d4b56fdfa1d5b6a2c36102c66876e0c46`](https://github.com/openai/codex/tree/b68acc4d4b56fdfa1d5b6a2c36102c66876e0c46) |
| Claude Code examples/plugins | [`cad6304e85e2767eac20044e752b010fff1bb4c3`](https://github.com/anthropics/claude-code/tree/cad6304e85e2767eac20044e752b010fff1bb4c3) |
| Goose | [`f812cbdcd85aadf8bbceef70eebcb2bec7acdecb`](https://github.com/block/goose/tree/f812cbdcd85aadf8bbceef70eebcb2bec7acdecb) |
| Continue | [`5522c6f44ca0ac3528b37244818fbfa39b5af470`](https://github.com/continuedev/continue/tree/5522c6f44ca0ac3528b37244818fbfa39b5af470) |
| Gemini CLI | [`3c311beac2e78336816dd4a123db39743f9fbf85`](https://github.com/google-gemini/gemini-cli/tree/3c311beac2e78336816dd4a123db39743f9fbf85) |

## At-a-glance matrix

| Harness | Ordinary/default execution | Optional isolation | Kernel boundary | Default command egress in isolated mode | Workspace/change handling |
|---|---|---|---|---|---|
| OpenHands | Current self-host quickstart explicitly offers agent server directly on host: **NO SANDBOX** | Docker, SDK Docker/Apptainer, remote/cloud | Docker/Apptainer: same kernel; cloud unspecified by inspected client | Docker default bridge: unrestricted | Host mode edits host paths; documented Docker setup bind-mounts project roots; SDK also supports remote clone/workspace APIs |
| SWE-agent | Docker deployment by default | Local (**NO SANDBOX**), Docker, Modal/AWS through SWE-ReX | Docker: same kernel; remote provider unspecified | Unrestricted unless runtime/operator changes it | Local repo copied into container; GitHub repo cloned there; patch/result returned |
| mini-swe-agent | Ordinary `mini`: **NO SANDBOX**; benchmark runners commonly choose Docker | Docker/Podman, Singularity, SWE-ReX, Modal, experimental bubblewrap | Same kernel for listed local modes | Docker unrestricted; bubblewrap does not unshare network | Local direct; Docker benchmark images/copies; bubblewrap RW-binds cwd |
| Aider | **NO SANDBOX** | User may launch Aider's published Docker image | Docker: same kernel | Unrestricted | Live repo bind-mounted RW at `/app`; direct edits/commits |
| Cline | **NO SANDBOX** in VS Code terminal and CLI | None found in inspected source | Host kernel, no isolation | Unrestricted | Direct edits; checkpoints and optional worktrees are recovery/change separation only |
| Roo Code | **NO SANDBOX** in VS Code terminal | None found | Host kernel, no isolation | Unrestricted | Direct edits; `.rooignore` is a tool filter; shadow-Git checkpoints restore changes |
| Codex CLI | OS-enforced command sandbox in default Auto posture | Read-only/workspace-write/danger-full-access policies; cloud containers | Seatbelt/bubblewrap: same kernel; cloud unspecified | Off by default for workspace-write; optional constrained proxy | Live host workspace; no copy/promote layer in CLI |
| Claude Code | Direct host execution with permission prompts: **NO SANDBOX** until `/sandbox` is enabled | Seatbelt/bubblewrap sandbox; reference devcontainer | Native sandbox and Docker: same kernel | Native sandbox uses proxy/allowlist; devcontainer firewall allowlist | Live cwd; devcontainer RW bind; settings/auth volumes persist |
| Goose | **NO SANDBOX**, autonomous mode default | Dagger Container Use extension; run Goose in Docker; extension-only `--container` | Ordinary containers: same kernel | Unrestricted unless user supplies policy | Direct edits by default; Container Use creates branch + container |
| Continue | **NO SANDBOX** in extension and CLI | Run the whole client in a user-managed container; no built-in executor sandbox found | Host kernel by default | Unrestricted | Direct edits and local shell; permissions gate tools, not processes |
| Gemini CLI | `--sandbox` defaults false: **NO SANDBOX** | Seatbelt, Docker/Podman, Windows low-integrity, gVisor/runsc, experimental LXC; newer tool-level policies | Seatbelt/Docker/LXC: same kernel; runsc: user-space kernel | Current container policy defaults network off; macOS default `permissive-open` allows it | Exact live cwd bind-mounted; optional worktree is change separation only |

## Findings by harness

### OpenHands

**Default and providers.** The current OpenHands quickstart explicitly labels its direct agent-server path “Without a Sandbox,” says it runs on the host, and contrasts it with a Docker launch. The architecture and self-hosting guides warn that this process can read/write the machine filesystem, execute commands, and reach the network. That is **NO SANDBOX**. The optional Docker launch maps OpenHands state plus a project root into the container. The newer SDK independently provides Docker, Apptainer, remote API, and OpenHands Cloud workspace backends. ([quickstart](https://github.com/OpenHands/OpenHands/blob/59981caf7fd92971681b0ab5354c37e9f1cab406/README.md), [self-hosting warning](https://github.com/OpenHands/OpenHands/blob/59981caf7fd92971681b0ab5354c37e9f1cab406/docs/SELF_HOSTING.md), [SDK workspace exports](https://github.com/OpenHands/software-agent-sdk/blob/6fce02687281cb6f27015e18a02f7b92c87212f2/openhands-workspace/openhands/workspace/__init__.py))

**Boundary and network.** SDK `DockerWorkspace` uses `docker run --rm`, publishes its HTTP API port, accepts arbitrary volumes, and applies `--network` only when the caller specifies one. The normal Docker bridge therefore has unrestricted outbound access. Docker and Apptainer use the Linux kernel beneath them; the cloud client's source does not establish what kernel boundary the service provides. ([Docker workspace](https://github.com/OpenHands/software-agent-sdk/blob/6fce02687281cb6f27015e18a02f7b92c87212f2/openhands-workspace/openhands/workspace/docker/workspace.py), [Apptainer workspace](https://github.com/OpenHands/software-agent-sdk/blob/6fce02687281cb6f27015e18a02f7b92c87212f2/openhands-workspace/openhands/workspace/apptainer/workspace.py))

**Code, credentials, lifecycle.** The self-hosted Docker setup exposes a broad host project path read-write, so changes are immediately host-visible. ACP integrations reuse host credential stores (`~/.claude`, `~/.codex`, `~/.gemini`) in local mode; container/cloud instructions store secrets for export into the agent subprocess, and shared HOME can be isolated only with an option. SDK Docker containers are removed on stop; state explicitly mounted into `.openhands` or a named `acp-data` volume survives. Cloud workspaces delete on cleanup unless `keep_alive` is set. ([ACP credentials and persistence](https://github.com/OpenHands/OpenHands/blob/59981caf7fd92971681b0ab5354c37e9f1cab406/docs/ACP_AGENTS.md), [cloud cleanup](https://github.com/OpenHands/software-agent-sdk/blob/6fce02687281cb6f27015e18a02f7b92c87212f2/openhands-workspace/openhands/workspace/cloud/workspace.py))

**Threat model.** The project candidly treats direct-host serving as unsafe and recommends Docker. Its local Docker defaults provide process/filesystem packaging, not adversarial network containment or a separate kernel; broad mounts and shared agent data remain operator responsibilities.

### SWE-agent

**Default and providers.** `SWEEnv` defaults to a `DockerDeploymentConfig` using `python:3.11`. Documentation also names local, Modal, and AWS deployments through SWE-ReX. Selecting local deployment is **NO SANDBOX**. ([environment configuration](https://github.com/SWE-agent/SWE-agent/blob/3ea751c087f32b16e039a2233dd6eefecef325d5/docs/config/environments.md), [`SWEEnv` default](https://github.com/SWE-agent/SWE-agent/blob/3ea751c087f32b16e039a2233dd6eefecef325d5/sweagent/environment/swe_env.py), [architecture](https://github.com/SWE-agent/SWE-agent/blob/3ea751c087f32b16e039a2233dd6eefecef325d5/docs/background/architecture.md))

**Boundary and network.** The default is an ordinary Linux container, hence a same-kernel boundary. SWE-agent itself supplies no default deny-egress Docker configuration in the inspected deployment path; network semantics come from SWE-ReX/runtime defaults.

**Code, credentials, lifecycle.** A `LocalRepo` must be clean and is uploaded into the environment rather than live-mounted. A `GitHubRepo` is cloned inside it. For private GitHub repositories, source builds an HTTPS remote containing the token, putting a credential in the sandbox's Git configuration/command context. Batch and single runners close the environment in `finally`/completion paths; exact resource deletion belongs to the SWE-ReX provider. ([repository transport](https://github.com/SWE-agent/SWE-agent/blob/3ea751c087f32b16e039a2233dd6eefecef325d5/sweagent/environment/repo.py), [batch cleanup](https://github.com/SWE-agent/SWE-agent/blob/3ea751c087f32b16e039a2233dd6eefecef325d5/sweagent/run/run_batch.py))

**Threat model.** The design is strong for benchmark reproducibility and preventing ordinary host pollution. It does not claim a hostile-code kernel boundary; unrestricted egress and in-guest Git credentials are important gaps for a secure software factory.

### mini-swe-agent

**Default and providers.** The normal `mini` command selects `local`, which uses `subprocess.run` in the caller's cwd: **NO SANDBOX**. SWE-bench entry points instead default to Docker. Other adapters include Podman, Singularity, SWE-ReX, Modal, and experimental bubblewrap. ([runner default](https://github.com/SWE-agent/mini-swe-agent/blob/25941c89cfbc91eb40b3f8756348c91d9977d57e/src/minisweagent/run/mini.py), [environment inventory](https://github.com/SWE-agent/mini-swe-agent/blob/25941c89cfbc91eb40b3f8756348c91d9977d57e/src/minisweagent/environments/README.md))

**Boundary and network.** Local mode inherits `os.environ` and the host kernel/filesystem/network. Docker/Podman/Singularity and bubblewrap are same-kernel. The experimental bubblewrap profile read-only binds system directories and RW-binds the cwd, but lacks `--unshare-net`; it retains host networking. Docker passes no network-deny flag. ([local environment](https://github.com/SWE-agent/mini-swe-agent/blob/25941c89cfbc91eb40b3f8756348c91d9977d57e/src/minisweagent/environments/local.py), [Docker environment](https://github.com/SWE-agent/mini-swe-agent/blob/25941c89cfbc91eb40b3f8756348c91d9977d57e/src/minisweagent/environments/docker.py), [bubblewrap profile](https://github.com/SWE-agent/mini-swe-agent/blob/25941c89cfbc91eb40b3f8756348c91d9977d57e/src/minisweagent/environments/extra/bubblewrap.py))

**Code, credentials, lifecycle.** Local mode directly edits the host. Bubblewrap exposes the live cwd. Container credentials are forwarded only when configured, while local mode inherits all host environment variables. Docker uses `--rm`, but cleanup is initiated from `__del__`, making prompt teardown dependent on object destruction; the temporary bubblewrap workdir has the same destructor-oriented cleanup pattern.

**Threat model.** This is deliberately a minimal research harness. Its adapters are useful hooks, not a hardened default security posture.

### Aider

**Default and optional Docker.** Aider is a local pair-programming process: its shell commands, lints, tests, file writes, and Git commits run against the user's repo. That is **NO SANDBOX**. Official Docker instructions are an optional way for the user to launch Aider, not an executor automatically provisioned per task. ([command execution](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/commands.py), [Docker guide](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/website/docs/install/docker.md))

**Boundary, network, and workspace.** The guide's Docker command maps `$(pwd):/app` read-write and passes no network restrictions, giving a same-kernel container unrestricted egress and immediate writes to the live host checkout. It passes the OpenAI API key as a process argument. The prose calls containers ephemeral, but the documented command omits Docker's `--rm`; stopped containers therefore remain until Docker/user cleanup.

**Credentials and persistence.** Host mode loads `.env` files and a persistent OAuth key file under `~/.aider`; any model-directed shell command runs in the surrounding process environment unless the user separately constrains it. Git history/commits are a durable recovery mechanism, not isolation. ([configuration loading](https://github.com/Aider-AI/aider/blob/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/aider/main.py))

**Threat model.** Aider optimizes for interactive collaboration and Git-backed undo. Its optional container is primarily a distribution/convenience path; it is not configured as a hostile-code sandbox.

### Cline

**Default.** Cline's own marketplace text contrasts sandboxed scripts with its human-in-the-loop model: terminal commands execute in the visible VS Code terminal after approval. The CLI executes locally as well; headless tool calls are auto-approved by default unless the caller disables that behavior. Both are **NO SANDBOX**. ([marketplace README](https://github.com/cline/cline/blob/b4fd4ee0cde1df0f23b425ada715720cb271819a/apps/vscode/README.marketplace.md), [CLI README](https://github.com/cline/cline/blob/b4fd4ee0cde1df0f23b425ada715720cb271819a/apps/cli/README.md))

**Naming hazard.** The CLI describes `--data-dir` as enabling “sandbox mode,” but implementation only relocates Cline's database, sessions, logs, and state directories. It does not isolate commands, filesystem, kernel, environment, or network. It must not be counted as an execution sandbox. ([CLI option handling](https://github.com/cline/cline/blob/b4fd4ee0cde1df0f23b425ada715720cb271819a/apps/cli/src/main.ts), [data-directory setup](https://github.com/cline/cline/blob/b4fd4ee0cde1df0f23b425ada715720cb271819a/apps/cli/src/utils/helpers.ts))

**Code, credentials, lifecycle.** Files are edited in place. Checkpoints provide restore, and CLI worktree options provide change separation; neither is a sandbox. Cline auth/session state persists in its normal or selected data directory. Host shell credentials and network are available to executed commands. Cleanup is user-managed state/worktree cleanup, not environment destruction.

**Threat model.** The control model is approvals plus checkpoints. That can prevent mistakes when a human notices them, but supplies no containment after an approved or auto-approved malicious command.

### Roo Code

**Default.** Roo executes commands through VS Code terminal shell integration in the selected cwd: **NO SANDBOX**, with host privileges, environment, filesystem, and network. Actions normally request approval unless auto-approval is enabled. ([first-task approval flow](https://github.com/RooCodeInc/Roo-Code/blob/b867ec9145750d0ae1ff7f02d35406e9bf2a0b16/apps/docs/docs/getting-started/your-first-task.md), [terminal process](https://github.com/RooCodeInc/Roo-Code/blob/b867ec9145750d0ae1ff7f02d35406e9bf2a0b16/src/integrations/terminal/TerminalProcess.ts))

**Filters and recovery.** `.rooignore` is explicitly documented as “Not a Full Sandbox”: it covers the current workspace and only recognizes a predefined set of file-reading commands, so custom scripts can bypass its view. Roo's checkpoints use a shadow Git repository for restoration. Neither mechanism changes execution isolation. ([`.rooignore` limitations](https://github.com/RooCodeInc/Roo-Code/blob/b867ec9145750d0ae1ff7f02d35406e9bf2a0b16/apps/docs/docs/features/rooignore.md), [checkpoint service](https://github.com/RooCodeInc/Roo-Code/blob/b867ec9145750d0ae1ff7f02d35406e9bf2a0b16/src/services/checkpoints/RepoPerTaskCheckpointService.ts))

**Threat model.** Human permission and best-effort information hiding are the boundary. Once a terminal command is approved, the extension supplies no kernel, filesystem, network, or credential containment.

### OpenAI Codex CLI

**Default.** Codex's default Auto posture combines `workspace-write` sandboxing with on-request approvals. Official documentation says command network access is off by default and writes are constrained to the active workspace. This is a real OS-enforced command sandbox, not merely a prompt. ([official security model](https://learn.chatgpt.com/docs/agent-approvals-security), [official sandboxing guide](https://learn.chatgpt.com/docs/sandboxing))

**Boundary.** On macOS Codex uses Seatbelt; on Linux/WSL it uses bubblewrap plus `no_new_privs`/seccomp. These are same-host-kernel policies, not guest kernels. The Rust implementation identifies the Linux sequence explicitly. Cloud tasks use separate isolated containers, but the inspected local client/source does not prove the cloud kernel substrate. ([Linux sandbox source](https://github.com/openai/codex/blob/b68acc4d4b56fdfa1d5b6a2c36102c66876e0c46/codex-rs/linux-sandbox/src/lib.rs))

**Network.** `workspace-write` command egress is disabled by default. Enabling network without the optional proxy allows direct outbound traffic. With the proxy, domain policy blocks private/local destinations by default, but the official guide calls out DNS classification/rebinding limits. It also scopes the proxy: built-in web search, MCP/connectors, browser/computer-use, model traffic, and authentication are outside command-proxy filtering.

**Workspace, credentials, persistence.** The sandbox operates on the live host workspace; changes are not staged in a disposable copy. Spawned-command environment policy begins from inheritance and removes names matching sensitive patterns such as `KEY`, `SECRET`, and `TOKEN`, reducing but not eliminating secret exposure. Codex auth/session data persists under `CODEX_HOME`; there is no per-task VM/container to destroy locally. ([shell environment policy](https://github.com/openai/codex/blob/b68acc4d4b56fdfa1d5b6a2c36102c66876e0c46/codex-rs/protocol/src/shell_environment.rs))

**Threat model.** Codex is one of the strongest default local postures surveyed for accidental/malicious command containment. Its explicit limitations matter for Daemar: same kernel, live workspace, and control-plane/network paths outside the command sandbox.

### Claude Code

**Source limitation and default.** Anthropic publishes plugins, examples, hooks, and issues in `anthropics/claude-code`, not the main CLI implementation. The behavior here is therefore based on Anthropic's official documentation plus its pinned reference devcontainer. Claude Code defaults to permission prompts for edits/commands; native sandboxing is enabled separately with `/sandbox`, so unsandboxed approved execution is **NO SANDBOX**. ([official security guide](https://code.claude.com/docs/en/security), [official sandbox guide](https://code.claude.com/docs/en/sandboxing))

**Native sandbox.** The feature uses macOS Seatbelt and Linux/WSL2 bubblewrap, both same-kernel. Default policy confines writes to the cwd/subdirectories but permits broad reads except denied paths. Network is mediated by a proxy and domain allowlist without TLS inspection. A serious operational caveat is documented: if sandbox dependencies are unavailable, Claude Code warns and proceeds without sandbox unless `sandbox.failIfUnavailable=true` is configured. The pinned managed-settings example demonstrates the fail-closed knobs `allowUnsandboxedCommands: false` and managed-only permission rules. ([managed sandbox example](https://github.com/anthropics/claude-code/blob/cad6304e85e2767eac20044e752b010fff1bb4c3/examples/settings/settings-bash-sandbox.json))

**Reference devcontainer.** Anthropic's devcontainer bind-mounts the live workspace read-write and persists shell history and `~/.claude` in volumes. Its firewall allowlists selected domains, but also permits outbound SSH and an entire host-network `/24` in both directions. It is an ordinary Docker same-kernel boundary. Anthropic warns that a malicious project can exfiltrate anything placed inside the container, especially mounted credentials. ([devcontainer config](https://github.com/anthropics/claude-code/blob/cad6304e85e2767eac20044e752b010fff1bb4c3/.devcontainer/devcontainer.json), [firewall](https://github.com/anthropics/claude-code/blob/cad6304e85e2767eac20044e752b010fff1bb4c3/.devcontainer/init-firewall.sh), [official devcontainer guide](https://code.claude.com/docs/en/devcontainer))

**Threat model.** Anthropic explicitly addresses prompt injection and malicious code. The native feature is meaningful, but optional/fail-open by default, readable-host-data exposure, and same-kernel execution keep it short of Daemar's target.

### Goose

**Default.** Goose enables its Developer extension by default, runs system commands with the user's privileges, and defaults to Autonomous permission mode. Its documentation states that the shell inherits the complete Goose environment, including sensitive API keys and tokens. This is **NO SANDBOX** with a notably broad default authority. ([Developer extension](https://github.com/block/goose/blob/f812cbdcd85aadf8bbceef70eebcb2bec7acdecb/documentation/docs/mcp/developer-mcp.md), [permission modes](https://github.com/block/goose/blob/f812cbdcd85aadf8bbceef70eebcb2bec7acdecb/documentation/docs/guides/managing-tools/goose-permissions.md))

**Optional containers.** The Dagger “Container Use” MCP extension can create a Git branch and development container. Users can also run Goose itself in Docker. The CLI `--container` option is narrower: it runs extensions in an existing container and should not be mistaken for isolation of all Goose behavior. These are ordinary same-kernel containers, and the inspected recipes do not install deny-by-default egress. ([Container Use tutorial](https://github.com/block/goose/blob/f812cbdcd85aadf8bbceef70eebcb2bec7acdecb/documentation/docs/tutorials/isolated-development-environments.md), [Goose-in-Docker tutorial](https://github.com/block/goose/blob/f812cbdcd85aadf8bbceef70eebcb2bec7acdecb/documentation/docs/tutorials/goose-in-docker.md))

**Code, credentials, lifecycle.** Default tools directly edit accessible files. The Container Use branch separates changes but is not itself a boundary; the paired container is. Provider secrets persist in a keychain or secrets file, while unrelated process environment secrets are exposed to the Developer shell. Sessions/history persist locally. Container/branch cleanup depends on the extension/user workflow.

**Threat model.** Goose documents prompt-injection risk and offers manual/smart approval, but its default deliberately favors autonomy. “Smart approval” is model interpretation, not enforcement after execution begins.

### Continue

**Default and boundary.** Both the VS Code extension and the current CLI execute commands locally. The extension sends text to a VS Code terminal; the CLI's Bash tool calls Node `spawn` against the local shell. No built-in process/container/VM sandbox was found in the inspected tree. This is **NO SANDBOX** with unrestricted host network and environment. ([VS Code command path](https://github.com/continuedev/continue/blob/5522c6f44ca0ac3528b37244818fbfa39b5af470/extensions/vscode/src/VsCodeIde.ts), [CLI Bash tool](https://github.com/continuedev/continue/blob/5522c6f44ca0ac3528b37244818fbfa39b5af470/extensions/cli/src/tools/runTerminalCommand.ts), [core local execution](https://github.com/continuedev/continue/blob/5522c6f44ca0ac3528b37244818fbfa39b5af470/core/tools/implementations/runTerminalCommand.ts))

**Permissions, code, and lifecycle.** Interactive CLI defaults ask before Bash/write tools, but headless mode defaults Bash and catch-all tools to allow. Plan mode blocks built-in write tools yet explicitly allows Bash, with a source TODO acknowledging the read-only concern; therefore it is not a filesystem security boundary. Files and sessions persist locally. Running Continue inside a user-managed Docker container is possible operationally, but it is not a per-task executor implemented by Continue. ([default policies](https://github.com/continuedev/continue/blob/5522c6f44ca0ac3528b37244818fbfa39b5af470/extensions/cli/src/permissions/defaultPolicies.ts), [CLI persistence](https://github.com/continuedev/continue/blob/5522c6f44ca0ac3528b37244818fbfa39b5af470/extensions/cli/README.md))

**Threat model.** Continue's policy layer controls whether a tool is offered/executed. Once Bash is allowed, it has host authority. Headless and “plan” defaults are especially unsuitable as security controls for unattended untrusted work.

### Gemini CLI

**Default and providers.** The CLI reference sets `--sandbox` default to `false`, so ordinary execution is **NO SANDBOX**. Optional full-process providers are macOS Seatbelt, Docker/Podman, Windows low-integrity, gVisor/runsc, and experimental LXC. Current source also contains opt-in tool-level sandbox policy and dynamic permission expansion. ([CLI reference](https://github.com/google-gemini/gemini-cli/blob/3c311beac2e78336816dd4a123db39743f9fbf85/docs/cli/cli-reference.md), [sandbox guide](https://github.com/google-gemini/gemini-cli/blob/3c311beac2e78336816dd4a123db39743f9fbf85/docs/cli/sandbox.md))

**Boundary.** Seatbelt, ordinary containers, Windows integrity levels, and LXC remain same-kernel mechanisms. `runsc` is the important outlier: Gemini invokes Docker with the gVisor runtime, which handles guest syscalls through a user-space kernel. It is the strongest explicitly supported local boundary found in this harness set, though opt-in and Linux-only.

**Network.** The current container configuration has `tools.sandboxNetworkAccess` default false and creates an internal Docker network when disabled. It can launch a proxy on a second network. In contrast, the default macOS `permissive-open` Seatbelt profile allows broad reads and network; users need a proxied/restrictive/strict profile for more. Separately, built-in `web_fetch` warns that it can reach local/private addresses, showing that tool/control-plane traffic must be scoped independently. ([configuration defaults](https://github.com/google-gemini/gemini-cli/blob/3c311beac2e78336816dd4a123db39743f9fbf85/docs/reference/configuration.md), [Docker network implementation](https://github.com/google-gemini/gemini-cli/blob/3c311beac2e78336816dd4a123db39743f9fbf85/packages/cli/src/utils/sandbox.ts), [web-fetch warning](https://github.com/google-gemini/gemini-cli/blob/3c311beac2e78336816dd4a123db39743f9fbf85/docs/reference/tools.md))

**Workspace, credentials, lifecycle.** Docker/Podman bind-mount the exact live cwd read-write and also mount settings/auth-related paths; source forwards `GEMINI_API_KEY` and `GOOGLE_API_KEY` into the sandbox. Containers use `--rm`, but host workspace/settings survive. LXC uses a pre-existing long-lived container. Windows integrity labels can persist after the session. An optional Git worktree separates changes only and remains **NO SANDBOX** unless paired with an actual sandbox. ([container mounts and credentials](https://github.com/google-gemini/gemini-cli/blob/3c311beac2e78336816dd4a123db39743f9fbf85/packages/cli/src/utils/sandbox.ts))

**Threat model.** Gemini exposes the widest local substrate menu and a credible no-egress Docker default in current source. The secure options are not the CLI default, live bind mounts/forwarded credentials remain valuable to an attacker, and dynamic sandbox expansion adds a human decision point that can be socially engineered.

## Cross-project patterns

### 1. Approval systems dominate; enforcement is secondary

Cline, Roo, Goose, Aider, and Continue primarily ask a user—or a model-based policy—to decide whether a command should run. This reduces accidental damage but has no post-approval containment. Headless modes frequently relax prompts exactly where unattended execution raises the risk: Cline auto-approves headless tool calls, Continue allows Bash/catch-all tools headlessly, and Goose defaults Autonomous.

### 2. “Sandbox” is overloaded

The word can mean a data directory (Cline CLI), an ignore file (Roo explicitly says it is not a full sandbox), a native OS policy, a container, or a hosted environment. A security design should name separately:

- execution/kernel boundary;
- filesystem visibility and write policy;
- network/IPC policy;
- code-change transport and promotion;
- credential delivery;
- lifecycle and evidence.

A worktree, branch, checkpoint, shadow Git repo, or clean-clone rule belongs only to code-change transport/recovery. It is **NO SANDBOX** when commands still run on the host.

### 3. Ordinary containers are the practical local baseline, not a kernel boundary

OpenHands, SWE-agent, mini-swe-agent, Aider, Goose, Claude's devcontainer, and Gemini all use or document Docker/Podman. This cheaply isolates process trees and most filesystem paths and gives reproducible images. On Linux it shares the host kernel; on macOS Docker Desktop interposes its Linux VM, but all agent containers share that guest Linux kernel and daemon trust boundary. Containers are therefore useful defense-in-depth, not evidence that “kernel isolation” is solved.

### 4. Live read-write workspace mounts are common

Aider, OpenHands self-host Docker, Claude's devcontainer, Gemini, bubblewrap mini-swe-agent, and optional Goose patterns expose live source. This gives excellent developer ergonomics but permits immediate destructive/encrypted changes and blurs artifact promotion. SWE-agent's copy/clone-then-extract pattern is closer to a software factory: input is imported and output is returned explicitly.

### 5. Network policy is usually absent or porous

Most Docker recipes accept default egress. Codex, Claude, and Gemini demonstrate more mature designs, but also why “allowlist” is not enough:

- enforcement may cover spawned commands but not web/MCP/browser/model traffic;
- direct-network fallback can silently broaden policy;
- hostname proxies without TLS inspection cannot reason about application payloads;
- DNS rebinding, redirects, CDNs, local/private addresses, Unix sockets, Docker sockets, and host-network routes need separate handling;
- dependency installation creates strong pressure for broad temporary expansion.

Network enforcement should live outside the guest, cover every guest interface, be fail-closed, and produce an auditable decision log.

### 6. Credential handling commonly defeats the boundary

Observed patterns include full environment inheritance (Goose and mini local), broad `.env` loading (Aider), API keys passed on Docker command lines (Aider), auth directories mounted into persistent containers (Claude/Gemini/OpenHands ACP), explicit key forwarding (Gemini), and Git tokens embedded in remotes (SWE-agent). Sensitive-name filtering in Codex is a useful guardrail but cannot prove that all secrets have recognizable names.

A secure factory should keep model credentials in the trusted controller and broker task credentials by capability: scoped, short-lived, destination-bound, non-enumerable where possible, and never included in the guest's general environment or persistent Git remote.

### 7. Cleanup is often best-effort and state is intentionally persistent

Docker `--rm` appears in SWE-style and SDK/Gemini implementations, but destructor-driven cleanup (mini-swe-agent), long-lived devcontainers/LXC, mounted auth/session volumes, host Git state, and user-managed Docker examples leave residue. “Container stopped” also does not erase image layers, named volumes, logs, caches, or host bind-mounted output.

A factory needs a deterministic state machine: create, import, execute, export evidence/delta, terminate, verify termination, and garbage-collect every associated disk/network/credential resource.

## Implications for a local Daemar sandbox

The harness survey suggests a narrow target architecture rather than another configurable approval wrapper:

1. **Trusted controller outside the guest.** Model calls, policy, secrets, logs, and promotion stay in a small local supervisor.
2. **One disposable environment per task.** Do not share HOME, process namespaces, caches containing secrets, or agent-server instances between tickets.
3. **A separately isolated kernel.** On Apple Silicon, a lightweight VM substrate is a closer match than a same-kernel container. On Linux, a microVM or a deliberately qualified gVisor configuration is the nearest low-cost analogue.
4. **No live source mount.** Import a content-addressed snapshot; export a patch or overlay only after execution. A worktree may be the controller's promotion destination, but never the security boundary.
5. **External deny-by-default network.** A guest with no route is the base state. Dependency access goes through a controller proxy/cache with explicit destinations, private/link-local/metadata denial, redirect/DNS revalidation, and logs.
6. **Credential broker, not environment injection.** Give a task only the short-lived capability it needs. Keep model credentials completely outside the execution environment.
7. **Fail closed.** If the VM, network policy, read-only base, mount policy, or cleanup verification cannot be established, do not run on the host as a fallback.
8. **Evidence as a product output.** Record image/base digest, kernel/substrate, input tree digest, writable surfaces, network decisions, granted capabilities, process result, exported delta, and verified teardown.

The closest reusable ideas are SWE-agent's explicit import/export workspace flow, Codex's default no-egress command posture, Claude's explicit fail-closed settings, and Gemini's gVisor plus internal-network implementation. None supplies the whole desired local security model as a default composition.

## Bottom line

“What are people doing?” has a simple answer: most coding harnesses run locally with approvals, while benchmark harnesses put work in ordinary containers. A smaller group adds native same-kernel filesystem policy and network proxies. Only isolated optional paths move toward a stronger kernel boundary. The common practice is cheap and convenient, but it is not the bar Daemar is trying to meet.
