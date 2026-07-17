# macOS sandbox substrate for Daemar's first Workflow

Research date: 2026-07-15
Scope: macOS 26 on Apple silicon; Daemar's first self-hosted Workflow only

## Answer

Use a **minimal macOS guest managed with Apple's public Virtualization framework** as the sandbox boundary for the first Workflow. The trusted setup stage should create or restore a disposable guest, expose only the dedicated Git worktree through one writable VirtioFS share, omit all virtual network devices, attach one Virtio socket for a typed Daemar control protocol, and allocate fixed CPU and memory. The guest-private disk is disposable; the only persistent host path writable by Sandboxed Execution is the worktree.

This is the highest-bootstrap-cost credible option, but it is the only evaluated option that simultaneously provides:

- a public, Apple-supported VM boundary;
- native macOS execution semantics for a self-hosted, macOS-only Daemar codebase;
- an explicit configuration with no virtual network device, intended to yield zero guest network and verified by the prototype gate below;
- an explicit single-directory host share rather than broad host filesystem access; and
- a narrow, non-network host/guest channel for deterministic Model Tool requests and results.

Apple documents that `VZVirtualMachineConfiguration` is assembled from the devices the VM should have, that CPU and memory are explicit configuration values, and that the configuration is validated before start. Its command-line Linux sample demonstrates a VM with only the devices it adds. From that explicit-device model, this research **infers** that network access is a positive configuration choice and that omitting every network device removes the guest's network path; the required prototype must verify that inference on macOS 26 ([Virtualize Linux on a Mac](https://developer.apple.com/documentation/virtualization/virtualize-linux-on-a-mac), [Running Linux in a Virtual Machine](https://developer.apple.com/documentation/virtualization/running-linux-in-a-virtual-machine)). Apple also documents both a single-directory VirtioFS share and a Virtio socket specifically for host/guest communication ([`VZVirtioFileSystemDeviceConfiguration`](https://developer.apple.com/documentation/virtualization/vzvirtiofilesystemdeviceconfiguration), [`VZVirtioSocketDeviceConfiguration`](https://developer.apple.com/documentation/virtualization/vzvirtiosocketdeviceconfiguration)). For native validation, Apple provides a supported macOS-on-Apple-silicon guest path using restore images and a VM bundle ([Running macOS in a virtual machine on Apple silicon](https://developer.apple.com/documentation/virtualization/running-macos-in-a-virtual-machine-on-apple-silicon)).

Do **not** build on `sandbox-exec` or its Sandbox Profile Language. Apple Developer Technical Support says `sandbox-exec` is deprecated because custom sandbox profiles are undocumented and unsupported for third-party products ([Apple DTS explanation](https://developer.apple.com/forums/thread/661939)).

## Required sandbox shape

The first implementation should make the following configuration a closed Rust type and reject a run before VM start if any field is missing or exceeds repository policy:

| Boundary | First-Workflow enforcement |
| --- | --- |
| Host filesystem | One writable `VZSingleDirectoryShare` for the run's worktree. No home-directory share, SSH agent, host socket, or other host path. |
| Guest filesystem | Locally prepared, version-pinned macOS guest image. Treat the guest disk as disposable run state and destroy or revert it after the retention window. |
| Network | Configure no `VZNetworkDeviceConfiguration`; verify in the prototype that the guest has no NIC or network path. Provider access and credential mediation remain a separate decision. |
| Credentials | Inject none into the guest: no environment secrets, Keychain sharing, credential files, SSH agent, or GitHub token. |
| Host/guest IPC | One `VZVirtioSocketDeviceConfiguration`; expose only a versioned, length-bounded protocol for Model Tool requests, results, cancellation, and structured status. |
| Processes | Run the guest-side Daemar tool service as a non-administrator. The model receives only repository-navigation and structured-editing Model Tools; it never receives a process or shell tool. Guest process freedom remains contained by the VM boundary. |
| CPU and memory | Set explicit `cpuCount` and `memorySize`; do not use host-derived unbounded defaults. Apple shows both as validated VM configuration fields ([sample](https://developer.apple.com/documentation/virtualization/running-linux-in-a-virtual-machine)). |
| Time | The trusted coordinator owns an absolute deadline, requests graceful guest stop, then force-stops the VM and records a structured timeout result. |
| Persistent disk use | Cap the disposable guest image separately. A writable host worktree does not itself provide a byte quota, so the prototype must measure and bound worktree growth or identify a host volume-quota mechanism before claiming the full resource contract. |
| Observability | Record the validated device manifest, image identity, start/stop/error transitions, deadline action, guest service events, and every Model Tool exchange in the Run Record. Do not treat console text as the typed result channel. |

The launcher executable needs the `com.apple.security.virtualization` entitlement; Apple says `validate()` checks for the entitlement required to run guests ([entitlement reference](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.virtualization)). This means signing and packaging the launcher are real bootstrap work, even for a local-only first Workflow.

## Why the guest should be macOS, not merely the host

"Supported host: macOS Apple Silicon" and "execution environment: macOS" are different decisions. Both Apple's `container` and Docker Desktop run **Linux** guests on a Mac. That is adequate for platform-independent generation, but Daemar's first Workflow is self-hosting and its sandbox implementation will use macOS-specific APIs and entitlements. A Linux guest cannot establish that the generated Daemar code builds or behaves as a macOS program. Using a macOS guest preserves the selected platform contract inside Sandboxed Execution instead of making native validation a trusted host exception.

The cost is a substantially heavier local image lifecycle than a Linux container: trusted setup must obtain an Apple restore image, install and version a guest bundle, provision the guest-side tool service, and define reset/retention behavior. Apple’s sample explicitly models installation and execution as separate tools and stores guest state in a VM bundle ([macOS VM sample](https://developer.apple.com/documentation/virtualization/running-macos-in-a-virtual-machine-on-apple-silicon)). The first Workflow should create its local base image conventionally during bootstrap; it should not download or install a guest during each run.

## Alternatives evaluated

| Mechanism | What it provides | Contract gap / cost | Disposition |
| --- | --- | --- | --- |
| **Virtualization.framework + minimal macOS guest** | Public Apple API; full VM boundary; explicit devices; native macOS; single-directory VirtioFS; Virtio socket; explicit CPU/memory | Highest bootstrap cost; signed entitlement-bearing launcher; guest image lifecycle; worktree byte quota still needs a design | **Selected** |
| **Apple `container` 1.1.x** | Apple-signed tool for Apple silicon/macOS 26; one lightweight Linux VM per container; OCI images; bind mounts; read-only root; capability drops; ulimits; CPU/memory; logs and machine-readable inspect. Apple describes per-container VM isolation and mount-only-needed-data privacy ([technical overview](https://github.com/apple/container/blob/main/docs/technical-overview.md), [command reference](https://github.com/apple/container/blob/main/docs/command-reference.md), [1.1.0 release](https://github.com/apple/container/releases/tag/1.1.0)). | Linux guest, not native macOS. Default networking is attached; the strongest documented current CLI network option is `network create --internal`, described as **host-only**, not no-network. Installer writes under `/usr/local` and prompts for an administrator password ([README](https://github.com/apple/container#requirements)). | Best lower-cost prototype comparator, but does not meet native semantics or zero-network as directly as the selected design. |
| **Docker Desktop** | Linux VM is the Mac host boundary; `--network none` leaves only loopback; bind mount can expose only the worktree; read-only root, capability drops, no-new-privileges, cgroups/ulimits, logs/inspect, and host timeout are mature controls ([Mac permission model](https://docs.docker.com/desktop/setup/install/mac-permission-requirements/), [none network](https://docs.docker.com/engine/network/drivers/none/), [resource constraints](https://docs.docker.com/engine/containers/resource_constraints/)). | Linux guest; third-party runtime and licensing terms; shared daemon is a privileged control surface and must never be mounted into the sandbox. Docker warns that daemon control can mount arbitrary host paths ([Docker Engine security](https://docs.docker.com/engine/security/)). | Credible fallback if Daemar later decides Linux guest semantics are acceptable; not selected for the self-hosted macOS slice. |
| **App Sandbox** | Public, kernel-enforced deny-by-default file/network capability model for signed apps. Omitting client-network entitlement denies outgoing connections; embedded command-line helpers inherit the app sandbox ([configuration](https://developer.apple.com/documentation/xcode/configuring-the-macos-app-sandbox), [helper-tool signing](https://developer.apple.com/documentation/xcode/embedding-a-helper-tool-in-a-sandboxed-app)). | Arbitrary external paths are normally granted through user-selected-file UI/security-scoped URLs; executable access has additional restrictions. It is an app distribution boundary, not a per-run VM, and does not provide the complete CPU/memory/disk/time lifecycle needed here ([file access](https://developer.apple.com/documentation/security/accessing-files-from-the-macos-app-sandbox)). | Useful later as defense in depth for a packaged coordinator, not the run sandbox. |
| **Endpoint Security / system extension** | Can monitor and authorize many process and filesystem events through a supported mandatory-access-control API ([Endpoint Security](https://developer.apple.com/documentation/endpointsecurity/)). | Requires an entitlement, packaged system extension, installation/user approval, complete fail-closed event handling, and a separate network/resource solution. It changes the host globally rather than creating a disposable run boundary. | Reject for V1. |
| **`sandbox-exec` / custom Seatbelt profile** | Fine-grained native process policy and low startup cost | Deprecated command; private, undocumented policy language; no supported product contract | Reject. |

## Linked-worktree constraint surfaced by the research

A writable worktree mount is sufficient for repository navigation and structured editing, but **not for an isolated Git commit**. Git's own documentation says a linked worktree's `.git` file points to private administrative data under the main repository's `$GIT_DIR/worktrees`, while `$GIT_COMMON_DIR` points back to the main repository's `.git`; shared refs and objects live there ([`git-worktree`](https://git-scm.com/docs/git-worktree), [`gitrepository-layout`](https://git-scm.com/docs/gitrepository-layout)).

Consequences:

1. Sharing only the worktree directory into the guest makes ordinary Git commit operations unable to reach their administrative paths.
2. Sharing the main `.git` directory read-write would let Sandboxed Execution mutate repository-wide refs, config, hooks, and objects, violating the intended single-workspace write boundary.

The safe default is therefore: Sandboxed Execution edits and validates the worktree; a deterministic **trusted Workflow stage** outside the VM creates the commit after it verifies the diff and validation result. This remains inside the Workflow, but outside Sandboxed Execution. If the map requires commit creation inside the sandbox, it needs a separate decision between a disposable full clone (not a linked worktree) and a narrowly mediated Git operation; merely mounting more of `.git` is not acceptable.

## Failure modes that must be explicit

- **Guest boot or entitlement failure:** reject before any model request; preserve validation and start error in the Run Record.
- **VirtioFS share mismatch:** the guest service must attest the expected mount identity and write probe before accepting Model Tool calls.
- **Unexpected device:** compare the actual validated device manifest with the closed policy; any NIC or additional share is a policy failure.
- **Guest compromise:** assume the entire guest and its private disk are hostile; retain no credentials there and discard/revert it after collection of bounded artifacts.
- **Host crash:** recover the run as interrupted; do not reuse the guest state for automatic resume.
- **Deadline:** terminate the VM from trusted control, mark the run failed/interrupted, and do not publish.
- **Share escape or quota exhaustion:** fail closed and retain diagnostics. The prototype must test symlinks, hard links, mount traversal, oversized files, inode exhaustion, and writes through a linked worktree's `.git` file.

## Required proof before implementation lock-in

Treat the selection as implementation-ready only after a narrow, non-production prototype proves all of the following on the actual macOS 26 Apple-silicon dogfood host:

1. A signed launcher can create, validate, start, stop, and force-stop the version-pinned macOS guest without interactive approval during each run.
2. With no virtual network device, the guest cannot reach the internet, LAN, host loopback, or host services, while the Virtio socket protocol still works.
3. The guest can read and edit the dedicated worktree through the single VirtioFS share and cannot read or write a sibling path, coordinator checkout, home directory, Keychain, SSH agent, or environment credential.
4. The non-administrator guest service can perform every initial repository-navigation and structured-editing Model Tool operation without a shell-facing API.
5. CPU, memory, time, process, guest-disk, and worktree-growth limits fail with structured, attributable results.
6. Native Rust static analysis and linting for Daemar run inside the guest with no network and with all dependencies already present in the pinned guest/toolchain image.
7. Cleanup/revert leaves the host worktree as the only intended persistent mutation and leaves no guest credential or unbounded diagnostic data.

If any of checks 1-6 cannot be made reliable at acceptable bootstrap cost, reopen the substrate decision and run the same contract test against Apple `container` and Docker Desktop, explicitly accepting Linux guest semantics rather than quietly treating a macOS host as macOS execution.

## Newly visible map decisions

- Decide whether commit creation is a trusted deterministic Workflow stage or whether the worktree requirement must change to a self-contained clone for sandboxed commits.
- Define the macOS guest base-image provisioning, version pinning, reset, retention, and update policy.
- Define a byte/inode quota for the writable worktree share; VM CPU/memory limits do not bound host worktree growth.
- Resolve the separate credential/network mediation ticket around a no-NIC guest and its narrow Virtio socket; this research does not choose which trusted component owns the OpenAI call.
