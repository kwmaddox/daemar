# Daemar

Daemar is a software factory for running coding agents in secure, controlled
development environments. This language distinguishes security boundaries from
workflow and source-control conveniences.

## Language

**Agentic coding environment**:
The complete environment in which a coding agent reads code, edits it, and runs
development tools. It may contain a sandbox, a workspace, credentials, and
other explicitly granted capabilities.

**Sandbox**:
A security boundary that contains hostile agent execution with separate-kernel
isolation and externally enforced network authority.
_Avoid_: isolated worktree, workspace sandbox, permission sandbox

**Workspace isolation**:
Separation of project files and Git state between tasks, commonly through
distinct clones or worktrees. It prevents code-change collisions but provides
no process, kernel, host-filesystem, credential, or network security boundary.
_Avoid_: sandbox

**Kernel isolation**:
A boundary in which agent workloads use a guest kernel distinct from the host
kernel, normally supplied by a VM or microVM.

**Network isolation**:
Externally enforced control over every network path available to the sandbox,
including an explicit no-egress posture and narrowly granted connectivity that
guest root cannot bypass.
_Avoid_: no inbound ports, proxy environment variables
