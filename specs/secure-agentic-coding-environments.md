# Secure agentic coding environments — product intent

This document records the product outcome Daemar is pursuing. It is broader
than any current implementation slice and deliberately does not select a
substrate, lifecycle, or result-transport mechanism.

## Intent

Daemar provides coding agents with secure environments in which they can
autonomously inspect code, edit it, build it, run tests, and execute project
tooling without giving untrusted agent actions or project code uncontrolled
authority over the developer's machine or network.

The environment is allowed to feel like a capable development machine on the
inside. Security comes from the boundary around that machine, not from trusting
the agent to obey instructions or from restricting it to a small command set.

## Security objectives

### Deployment and cost

- Prefer sandbox compute, isolation enforcement, the control plane, and retained
  sandbox state to run on hardware controlled by the developer or their
  organization.
- Avoid making a recurring hosted sandbox bill a prerequisite for using Daemar.
  Cost is a first-class selection criterion, including idle charges, storage,
  egress, minimum subscriptions, and the operational cost of maintaining a
  local alternative.
- A hosted sandbox is acceptable when it is inexpensive in realistic use and
  materially reduces security risk, implementation complexity, or maintenance
  burden enough to justify the dependency and cost.
- Model inference may run locally or remotely. Model location is not a product
  constraint; any remote access still passes through the environment's explicit
  network policy.

### Kernel isolation

- Agent processes and the code they execute run behind a hardware-backed VM or
  equivalent boundary with a kernel separate from the host kernel.
- The agent may be treated as root inside that boundary. Guest privilege must
  not imply authority over host processes, the host kernel, or the host runtime.
- A container that merely shares the host kernel is not, by itself, sufficient.

### Network isolation

- The environment has no network path except capabilities that Daemar has
  explicitly granted for that environment.
- Network policy is enforced outside the guest. Root inside the guest cannot
  disable, reconfigure, or bypass it.
- Policy fails closed and is inspectable: Daemar can explain which destinations
  and protocols are reachable for a particular environment.
- A no-egress posture must be available. When egress is granted, raw IPs, IPv4
  and IPv6, DNS rebinding, redirects, alternate protocols, and access to host or
  private-network services must not silently bypass the policy.
- The exact useful default remains a product decision. "Network isolation" does
  not yet mean that every coding environment must be permanently offline; it
  means that connectivity is explicit, bounded, and adversarially verified.

### Host and credential isolation

- The agent cannot read or modify host files, credentials, processes, or tools
  merely because they are available to the user running Daemar.
- Every host resource or credential exposed to an environment is an explicit,
  least-authority capability. Prefer per-environment, scoped, revocable
  credentials over forwarding the developer's long-lived identity.
- There is no silent fallback to host execution.

### Controlled code movement

- Code enters the environment through a deliberate input boundary and results
  return through a deliberate output boundary.
- Agent activity must not mutate the developer's working state implicitly.
- Daemar makes the authority represented by an imported result clear before it
  can affect the developer's checkout or upstream repository.

## Threat model

Assume that the coding agent, its prompts, repository contents, build scripts,
dependencies, and every command they cause to run may be malicious or
compromised. They may deliberately probe for escape paths, credentials, network
access, persistence, and ways to confuse result handling.

The trusted computing base initially includes Daemar's host-side controller and
the selected VM and network-enforcement substrate. Substrate claims are not
accepted solely from documentation: security-critical boundaries require
adversarial behavioral qualification on every supported platform and pinned
version.

## Decisions intentionally left open

The product intent does not currently require any one of the following:

- a substrate built entirely by Daemar versus an integrated runtime that runs
  locally;
- an ephemeral environment per command versus a durable environment per task;
- an exact copy of a dirty worktree versus a clean Git commit as input;
- an exact filesystem change set versus Git commits or another result protocol;
- automatic teardown after every command versus retained, explicitly managed
  task environments;
- completely offline agents versus narrowly mediated model and development
  service access.

Those choices should be evaluated by how simply and convincingly they satisfy
the security objectives and the software-factory workflow, rather than treated
as product requirements inherited from the first implementation.

## Success

A developer can delegate coding work without trusting the agent or the code it
runs, while retaining confidence that compromise is contained to the assigned
environment and to the explicit filesystem, credential, and network authority
granted to it.
