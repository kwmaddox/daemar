# IndyDevDan's SSSF sandboxing, current as of 2026-08-26

## Executive answer

The standalone **Super Simple Software Factory (SSSF) does not sandbox its
agents**. Its README says this twice and characterizes the omission as
deliberate: it runs on the current branch, while a per-run branch, sandbox, and
merge step are left for the user to add so the core remains small enough to
read and trust. Its agents are "bounded" in the orchestration sense—named
phases, typed envelopes, retry limits, configured tools, and post-call gates—not
in the OS-isolation sense.[^sssf-no-sandbox-1] [^sssf-no-sandbox-2]

IndyDevDan demonstrates his sandboxed composition in a **different repository**,
**Factory In A Box** (`inkwell-agent-sandboxes-and-software-factory`). That
system puts an entire checkout and SSSF instance inside a disposable **exe.dev
VM**. The isolation boundary is one VM per sandbox run, not one VM per agent,
phase, command, or ADW session. A sandbox can host multiple factory runs before
the human explicitly tears it down.[^fiab-tiers] [^fiab-run-ids]

This is therefore a useful comparison for Daemar, but it is not the same
transaction. Factory In A Box is a remote, credential-bounded development
workspace with Git-based harvest. Daemar's `specs/sandbox.md` is a one-shot,
no-network execution transaction over an exact worktree snapshot, with an exact
filesystem delta, validated promotion, a deadline, and unconditional cleanup.

## Sources and pinned revisions

Only first-party sources were used.

| Artifact | Revision inspected | Commit date | Role |
|---|---|---:|---|
| [`disler/super-simple-software-factory`](https://github.com/disler/super-simple-software-factory) `main` | [`de31374882e7a4e3e5b7bb9bd09e69dc2f779356`](https://github.com/disler/super-simple-software-factory/tree/de31374882e7a4e3e5b7bb9bd09e69dc2f779356) | 2026-08-02 | Installable standalone SSSF skill |
| same repository, `example` branch | [`b2dcb8e436db9b10f7580d7568b3e251609eb36b`](https://github.com/disler/super-simple-software-factory/tree/b2dcb8e436db9b10f7580d7568b3e251609eb36b) | 2026-08-04 | Stamped factory and demo application |
| [`disler/inkwell-agent-sandboxes-and-software-factory`](https://github.com/disler/inkwell-agent-sandboxes-and-software-factory) `main` | [`92f1701810993b8303562265ba04c727468fe070`](https://github.com/disler/inkwell-agent-sandboxes-and-software-factory/tree/92f1701810993b8303562265ba04c727468fe070) | 2026-08-09 | Separate exe.dev sandbox composition, branded "Factory In A Box" |

The two branch revisions were resolved from the repositories' branch refs on
2026-08-26. The commit links above make the evidence reproducible if `main`
moves.

## 1. What standalone SSSF actually does

### Architecture and isolation boundary

SSSF is a Python control plane for agent workflows. An ADW script sequences
engineer, agent, and deterministic-code phases; typed JSON envelopes move
context between phases; gates decide whether an agent output passes; and events
stream to SQLite. Its own definition of the boundary is that code owns
sequencing, retries, and acceptance while an agent owns the work inside one
named phase.[^sssf-architecture]

That is a **logical workflow boundary**, not a security boundary. The Pi coding
agent is launched with `subprocess.Popen`, with `cwd` set to the repository root
and `env=operator_env()`.[^sssf-popen] `operator_env()` deliberately copies the
engineer's environment and restores the engineer's ordinary `PATH`; the agent
is meant to see the same PATH, toolchains, and globally installed packages as
the operator.[^sssf-env] The starter prompts say the agents inherit the
operator's shell environment, including credentials.[^sssf-prompt-env]

There is no Docker, container, VM, or worktree creation in the standalone
execution path. The README explicitly says there is no sandbox or branch per
run.[^sssf-no-sandbox-2]

### Filesystem and worktree handling

Agents operate directly in the current checkout. The builder has `bash`,
`edit`, and `write` and is unrestricted within the repo except for configured
protected paths. Planner, scout, reviewer, and documenter have narrower
declared write patterns.[^sssf-config]

SSSF's important compensating control is **after-the-fact Git comparison**:

1. before an agent call, it fingerprints paths that differ from `HEAD` using
   `git diff --numstat` and untracked files using `git ls-files`;
2. after the call, it compares fingerprints;
3. paths outside that agent's allowlist fail the phase and are rolled back where
   possible.[^sssf-permissions]

The module explicitly says `tools:` is a capability list, **not a sandbox**.
It also documents the critical limitation: if an agent reverts an already-dirty
file, the operator's uncommitted content cannot be reconstructed; SSSF reports
`REVERTED-BY-AGENT (uncommitted work lost, cannot restore)`.[^sssf-permissions]
Gitignored paths do not appear in the snapshot at all.[^sssf-permissions]

This is a useful policy/gating mechanism, but it is not prevention, an exact
snapshot, or a transactionally isolated worktree.

### Network and credentials

Standalone SSSF applies no network policy. Network access is necessary for its
model-provider calls. Provider credentials come from the process environment,
and the stock roster expects OpenRouter, Fireworks, and OpenAI keys unless it is
customized.[^sssf-keys] Because the Pi subprocess receives the operator
environment, there is no agent-specific credential boundary in SSSF itself.

### Host command execution, timeouts, and lifecycle

An agent with the `bash` tool can run arbitrary commands on the host under the
operator account. SSSF also runs deterministic quality commands directly as
host subprocesses in the repo root with the operator environment. Those quality
commands have per-command timeouts (600 seconds for the starter test block),
but the Pi agent subprocess read loop has no overall timeout; a hung coding
agent is recorded by PID and requires an external kill.[^sssf-quality]
[^sssf-popen] The README describes a silent hang as something to diagnose and
kill children-first rather than as an automatically enforced deadline.[^sssf-hang]

Sessions and their handoff/output directories intentionally persist under
`adws/adw_data`; passing the same `--adw-id` resumes each agent's existing
context window.[^sssf-resume] There is no per-call machine teardown because
there is no machine boundary to tear down.

### Standalone SSSF's stated security posture

SSSF does not state a hostile-code threat model. The clearest first-party
statement is narrower: tool filtering alone cannot enforce which repo paths an
agent changes, so SSSF verifies the Git change-set after the call and tries to
undo unauthorized changes.[^sssf-permissions] The documented inability to
restore reverted dirty work establishes that this mechanism is a conformance
guard, not containment of a malicious or compromised agent.

## 2. What Factory In A Box adds

### Architecture and isolation boundary

Factory In A Box has three nested tiers:

1. a host-side "out-sandbox" orchestrator;
2. an in-VM orchestrator agent;
3. SSSF ADW agents as phases inside that same VM.[^fiab-command-tiers]

The host creates an exe.dev VM, clones and provisions the repo in it, starts the
factory, observes it over SSH/HTTP, harvests commits, and later destroys the VM.
Both direct execution and agent-mediated execution use the same already-mounted
box. Direct execution starts an ADW detached with `nohup`; the agent-mediated
path runs resumable Claude Code in the VM with
`--dangerously-skip-permissions`.[^fiab-execute] [^fiab-run]

Therefore the isolation granularity is **sandbox run / VM**. It is not per
command, per agent, per phase, or per ADW. One VM can contain many ADW runs and
resumable agent sessions until teardown.[^fiab-run-ids]

The recipes use exe.dev VMs over SSH. Docker is installed in the VM image, but
no audited lifecycle recipe uses Docker or a container as the isolation
boundary; describing this composition as Docker-based would be incorrect.
That last sentence is an inference from the committed recipes, not an explicit
design claim.

### Filesystem and result transport

`fill` performs a public, unauthenticated Git clone inside the VM, optionally
checks out a pin, and creates `sbx/<run-id>` at that HEAD. It deliberately does
not copy the host's current checkout or dirty worktree.[^fiab-fill]

SSSF agents edit that VM-local clone in place and its code phases commit to the
run branch. `harvest` creates a Git bundle for the range from the recorded base
commit to the run branch, transfers it to the host, verifies it, and fetches it
into `refs/sandbox/<run-id>`. It does not merge, check out, or modify the host
working tree; the human compares and chooses.[^fiab-harvest]

Consequences:

- committed Git content is the result protocol;
- uncommitted, ignored, or non-Git-representable output is not an exact reported
  delta and can be lost at teardown;
- empty directories and general filesystem metadata are outside the protocol;
- the starting state is a repository commit, not the caller's exact byte
  snapshot;
- importing a ref is explicit and non-destructive, but it is not Daemar-style
  path-by-path promotion.

The bullets above are inferences from the clone/branch/bundle implementation.

### Network controls

Factory In A Box does **not** disable networking. Its setup gate explicitly
proves that sandbox egress works by calling OpenRouter from inside the VM, with
60- and 180-second HTTP timeouts.[^fiab-egress] The app is deliberately exposed
on a public port, while the observability port remains access-controlled by
exe.dev.[^fiab-ports] No allowlist, denylist, or raw-IP egress restriction is
implemented in the audited recipes.

### Credential boundary

The strongest security design in Factory In A Box is its credential boundary:

- the exe.dev account and OpenRouter provisioning key remain on the host;
- `create` mints a per-run `sbx-<run-id>` OpenRouter key with a $50 default cap;
- `fill` injects only that runtime key into the VM as `app/.env` mode 0600;
- the VM receives no GitHub PAT, deploy key, or SSH identity, so an autonomous
  agent cannot push to the source repository;
- harvest uses a Git bundle, requiring no Git credential;
- teardown revokes the runtime key and verifies it is absent from the
  authoritative key list.[^fiab-credential-boundary] [^fiab-create]
  [^fiab-fill-key] [^fiab-harvest]

The entire repository—including host orchestration recipes—exists in the VM.
The author explicitly says enforcement comes from absent credentials, not
absent files: the VM cannot create nested sandboxes because it has neither the
exe.dev account nor the provisioning key.[^fiab-credential-boundary]

There is a second inference credential path: the agent-mediated Claude Code
recipe points at exe.dev's internal key-free LLM gateway with a placeholder API
key.[^fiab-run] The repository describes this as needing no key, but the exact
provider-side authorization and spending boundary is owned by exe.dev and is
not specified here.

### Host command execution

The host-side orchestrator runs SSH and control-plane commands on the host, but
the coding agents execute inside the VM. `just sbx run cmd` is an intentional
arbitrary-command escape hatch: it sends the caller's command verbatim to a
remote shell under `cd app` and synchronously prints the result.[^fiab-run]
This is remote command execution in the sandbox, not fallback execution on the
host. The absent host credentials prevent an in-VM agent from invoking the
host-only control plane, but there is no protocol-level proof against every
possible exfiltration route.

### Lifecycle and cleanup

The lifecycle is `create -> fill -> setup -> execute -> observe -> teardown`,
with a durable host run record written before VM creation so cleanup retains a
handle after a crash.[^fiab-create]

Cleanup is intentionally **not automatic**. `mount` stops at observe; failures
leave the VM alive to preserve evidence; only the human decides when to run
teardown. Teardown performs spend capture, artifact/log collection, harvest,
key revocation, VM destruction, and record close. A separate `reap` command
finds orphaned managed keys; it is dry-run by default and needs `--yes` to
revoke them.[^fiab-hard-rules] [^fiab-lifecycle]

This is good forensic ergonomics, but it means leaked VMs and runtime keys are
an expected operational condition. The project's own gotchas note that
OpenRouter keys have no TTL and live forever if teardown never runs.[^fiab-reap]

### Stated threat model

There is no document titled or structured as a complete threat model. Direct
first-party statements nevertheless establish these protected assets and
assumptions:

- the host machine should not be the place autonomous coding agents run;
- the host exe.dev account and provisioning key must not enter the VM;
- an autonomous agent runs with permissions bypassed inside the VM, so it must
  not receive a credential capable of pushing to the upstream Git repository;
- inference spend is bounded with a per-run key and key revocation;
- evidence should survive failures until a human chooses teardown.
  [^fiab-hard-rules] [^fiab-harvest] [^fiab-credential-boundary]

The repository does **not** state or implement protection against network
exfiltration from the VM, malicious dependencies contacting the Internet,
denial of service inside a VM, guest-to-exe.dev hypervisor escape, or exact
filesystem-delta ambiguity. Treat those omissions as outside the demonstrated
threat model, not as proven-safe properties.

## 3. Comparison to Daemar B1-B10

The comparison below evaluates **Factory In A Box**, since standalone SSSF has
no OS sandbox. “Partial” means it has a related property but does not establish
the falsifiable Daemar behavior as written.

| Daemar behavior | Factory In A Box | Evidence and difference |
|---|---|---|
| **B1. Command runs in a VM with its own kernel, never on host** | **Partial / close** | Coding agents and ADWs run in an exe.dev VM, and host recipes SSH into it.[^fiab-command-tiers] The repo calls the substrate a VM but does not independently prove kernel separation or the exe.dev implementation. The generic command path is remote; host orchestration itself still runs host commands. |
| **B2. No network path** | **Contradicted** | Working egress is a setup assertion, model calls require it, and the app is deliberately published.[^fiab-egress] [^fiab-ports] |
| **B3. Host worktree byte-identical after every run** | **Partial, different input model** | The agent receives a public clone in the VM, not a mount of the host worktree.[^fiab-fill] Agent work therefore does not edit the host worktree. However, it also does not execute against the host's exact dirty/untracked state; host run records, key files, and harvested Git refs are created outside the worktree. |
| **B4. Command sees worktree and edits it in its own view** | **Partial** | Agents edit a VM-local checkout in place, but it is a clone of a commit rather than an isolated view of the caller's exact worktree.[^fiab-fill] |
| **B5. Exact added/modified/deleted change report** | **Not met** | Harvest transports committed Git history. There is no complete filesystem `ChangeSet`; uncommitted/ignored output and non-Git state are not covered.[^fiab-harvest] |
| **B6. Explicit, validated safe promotion** | **Partial analogy only** | Harvest is explicit and fetches a verified Git bundle into a namespaced ref without merging or touching the worktree.[^fiab-harvest] It does not perform Daemar's exact path validation, symlink containment checks, or setuid/setgid stripping because it imports Git objects/refs rather than promotes a filesystem delta. |
| **B7. Nothing outside host worktree readable by guest** | **Largely met for host separation, not proved as written** | No host directory is mounted; the guest receives a public clone and a disposable runtime key. Host control-plane credentials stay outside.[^fiab-fill] [^fiab-credential-boundary] The repo does not provide a negative test that no other host path is exposed by exe.dev. The guest can of course read its own VM filesystem. |
| **B8. Timeout kills the run** | **Not met** | Setup HTTP calls and SSSF quality subprocesses have local timeouts, but the detached ADW/agent run has no end-to-end deadline. It returns a PID and is later observed or manually killed.[^fiab-execute] |
| **B9. No container/temp state after success, failure, or timeout** | **Contradicted by design** | Failures deliberately preserve the VM. Mount never chains teardown, and the human must decide. Orphan cleanup is a separate, opt-in reaper.[^fiab-hard-rules] [^fiab-reap] |
| **B10. Faithful stdout, stderr, exit code** | **Not an equivalent contract** | `run cmd` synchronously exposes ordinary SSH output/exit behavior, but the default `execute` path detaches, returns a PID, redirects combined output to `run.log`, and reports through traces/observation.[^fiab-run] [^fiab-execute] It has no typed one-shot result containing separate stdout, stderr, and exit code. |

Standalone SSSF fares worse as a Daemar substrate: B1, B2, B3, B7, B8, and B9
are absent or contradicted; B4 is direct host editing rather than a private
view; its Git snapshot is neither B5 nor B6; and subprocess output handling is
workflow-specific rather than a B10 execution result.

## 4. What Daemar can borrow

The most transferable ideas are above, rather than inside, the isolation
substrate:

- a durable run record created before resources, so cleanup always has a handle;
- host-only control-plane credentials and per-run, capped, revocable data-plane
  credentials;
- no upstream Git write credential in an autonomous environment;
- a non-destructive, namespaced result-import seam for human comparison;
- health gates that test real behavior rather than trusting exit status;
- a reaper for resources that survive crashes.

The key non-transferable choice is lifecycle intent. Factory In A Box preserves
a development VM for inspection and resumed sessions; Daemar promises that a
one-shot hostile command loses all runtime state after every outcome. Likewise,
Factory In A Box needs inference egress and uses committed Git history as its
result protocol, while Daemar's v1 requires zero network and an exact
filesystem delta. These are design differences, not missing convenience flags.

## References

[^sssf-no-sandbox-1]: Standalone SSSF README, “Where it can still fail”: [current branch; add a branch, sandbox, and merge step for real work](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/README.md#L350-L369).
[^sssf-no-sandbox-2]: Standalone SSSF README, explicit omissions and reason: [“There is no sandbox ... left out so the core stays small enough to read”](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/README.md#L373-L392).
[^sssf-architecture]: Standalone SSSF README, [Python control plane and bounded phases](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/README.md#L16-L43).
[^sssf-popen]: `agent_pi.py`, [Pi launched by `subprocess.Popen` in the repo cwd with operator env; no overall timeout](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/agent_pi.py#L210-L285).
[^sssf-env]: `utils.py`, [`operator_env()` copies the engineer's environment and PATH](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/utils.py#L16-L38).
[^sssf-prompt-env]: Builder system prompt, [operator shell, toolchains, and credentials are live](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/prompt_engineering/builder/system.md#L9-L14).
[^sssf-config]: Starter config, [tools, protected paths, and per-agent write patterns](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/sssf.config.yaml#L1-L143).
[^sssf-permissions]: `permissions.py`, [post-hoc Git enforcement, ignored paths, and unrecoverable dirty-work loss](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/permissions.py#L1-L185).
[^sssf-keys]: Standalone SSSF README, [provider keys and environment-driven selection](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/README.md#L61-L101).
[^sssf-quality]: `quality.py`, [host subprocess execution with operator env and per-command timeout](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/.claude/skills/sssf/templates/adws/adw_modules/quality.py#L62-L100).
[^sssf-hang]: Standalone SSSF README, [hung agent diagnosis and manual kill](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/README.md#L350-L365).
[^sssf-resume]: Standalone SSSF README, [`--adw-id` reuses directories and resumes agent context](https://github.com/disler/super-simple-software-factory/blob/de31374882e7a4e3e5b7bb9bd09e69dc2f779356/README.md#L306-L346).
[^fiab-tiers]: Factory In A Box README, [three tiers and the sandbox mount system](https://github.com/disler/inkwell-agent-sandboxes-and-software-factory/blob/92f1701810993b8303562265ba04c727468fe070/README.md#L1-L17).
[^fiab-run-ids]: Factory In A Box README, [a sandbox run id versus multiple ADW ids inside one box](https://github.com/disler/inkwell-agent-sandboxes-and-software-factory/blob/92f1701810993b8303562265ba04c727468fe070/README.md#L180-L218).
[^fiab-command-tiers]: Factory In A Box README, [host orchestrator, in-VM orchestrator, and ADW agents](https://github.com/disler/inkwell-agent-sandboxes-and-software-factory/blob/92f1701810993b8303562265ba04c727468fe070/README.md#L85-L119).
[^fiab-execute]: `execute.just`, [detached VM execution, PID result, combined `run.log`, and explicit teardown](https://github.com/disler/inkwell-agent-sandboxes-and-software-factory/blob/92f1701810993b8303562265ba04c727468fe070/just/sandbox/lifecycle/execute.just#L1-L101).
[^fiab-run]: `run/mod.just`, [synchronous arbitrary remote command and resumable in-VM Claude Code](https://github.com/disler/inkwell-agent-sandboxes-and-software-factory/blob/92f1701810993b8303562265ba04c727468fe070/just/sandbox/run/mod.just#L1-L90).
[^fiab-fill]: `fill.just`, [public clone, optional pin, and per-run branch](https://github.com/disler/inkwell-agent-sandboxes-and-software-factory/blob/92f1701810993b8303562265ba04c727468fe070/just/sandbox/lifecycle/fill.just#L1-L129).
[^fiab-fill-key]: `fill.just`, [runtime key injected into VM `.env` with mode 0600](https://github.com/disler/inkwell-agent-sandboxes-and-software-factory/blob/92f1701810993b8303562265ba04c727468fe070/just/sandbox/lifecycle/fill.just#L131-L157).
[^fiab-harvest]: `harvest.just`, [no Git credentials, bundle range, verify, and fetch to a namespaced ref](https://github.com/disler/inkwell-agent-sandboxes-and-software-factory/blob/92f1701810993b8303562265ba04c727468fe070/just/sandbox/manage/harvest.just#L1-L148).
[^fiab-egress]: `setup.just`, [gate deliberately proves sandbox egress with OpenRouter calls](https://github.com/disler/inkwell-agent-sandboxes-and-software-factory/blob/92f1701810993b8303562265ba04c727468fe070/just/sandbox/lifecycle/setup.just#L147-L204).
[^fiab-ports]: Factory In A Box README, [public app and auth-gated agent/observability surface](https://github.com/disler/inkwell-agent-sandboxes-and-software-factory/blob/92f1701810993b8303562265ba04c727468fe070/README.md#L220-L242).
[^fiab-credential-boundary]: Sandbox orchestrator skill, [two credential layers; enforcement by credentials, not file absence](https://github.com/disler/inkwell-agent-sandboxes-and-software-factory/blob/92f1701810993b8303562265ba04c727468fe070/.claude/skills/sssf-sandbox-orchestrator/SKILL.md#L56-L67).
[^fiab-create]: `create.just`, [run record first, VM creation, capped runtime key, and host-only provisioning key](https://github.com/disler/inkwell-agent-sandboxes-and-software-factory/blob/92f1701810993b8303562265ba04c727468fe070/just/sandbox/lifecycle/create.just#L1-L169).
[^fiab-hard-rules]: Sandbox orchestrator skill, [explicit human teardown, never run ADWs on host, preserve failed VMs](https://github.com/disler/inkwell-agent-sandboxes-and-software-factory/blob/92f1701810993b8303562265ba04c727468fe070/.claude/skills/sssf-sandbox-orchestrator/SKILL.md#L132-L151).
[^fiab-lifecycle]: Sandbox orchestrator skill, [command/lifecycle table including teardown order and reaper](https://github.com/disler/inkwell-agent-sandboxes-and-software-factory/blob/92f1701810993b8303562265ba04c727468fe070/.claude/skills/sssf-sandbox-orchestrator/SKILL.md#L85-L107).
[^fiab-reap]: First-party gotchas, [keys have no TTL; teardown and opt-in reap are required](https://github.com/disler/inkwell-agent-sandboxes-and-software-factory/blob/92f1701810993b8303562265ba04c727468fe070/.claude/skills/sssf-sandbox-orchestrator/references/gotchas.md#L74-L83).
