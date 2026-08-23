# Apple Container 1.2.2: B3 read-only mount boundary

**Retrieved 2026-08-23.** Primary sources only.

## Conclusion

**The current Apple Container 1.2.2 concern is refuted:** granting the workload
effective `CAP_SYS_ADMIN` does not, by itself, defeat B3's host-worktree
write-protection boundary. Linux permits a capable workload to change its own
bind mount's per-mount read-only flag, but Apple also configures the host-side
VZ VirtioFS share as read-only. Clearing a guest mount flag does not make that
host-provided share writable.

This conclusion is Apple/VZ-specific. A future Linux substrate must not rely
only on guest bind-mount `MS_RDONLY` when the workload has `CAP_SYS_ADMIN`.

## Evidence

Apple Container 1.2.2 pins Containerization 0.40.1 in
[`Package.resolved`](https://github.com/apple/container/blob/1.2.2/Package.resolved).

In Containerization 0.40.1,
[`VZVirtualMachineInstance.swift`](https://github.com/apple/containerization/blob/0.40.1/Sources/Containerization/VZVirtualMachineInstance.swift#L2191-L2228)
constructs each host-provided `VZSharedDirectory` with `readOnly` derived from
whether the mount options contain `ro`. Read-only enforcement therefore exists
at the VZ VirtioFS share, not only at the later guest bind mount.

[`LinuxContainer.swift`](https://github.com/apple/containerization/blob/0.40.1/Sources/Containerization/LinuxContainer.swift#L602-L647)
mounts the unified VirtioFS source in the VM, then
[transforms the selected share](https://github.com/apple/containerization/blob/0.40.1/Sources/Containerization/LinuxContainer.swift#L710-L733)
into a container bind mount carrying the supplied options. Its runtime spec
[creates a mount namespace but no user namespace](https://github.com/apple/containerization/blob/0.40.1/Sources/Containerization/LinuxContainer.swift#L403-L409).

Linux documents that `CAP_SYS_ADMIN` authorizes mount administration
([`capabilities(7)`](https://man7.org/linux/man-pages/man7/capabilities.7.html))
and that a bind remount can set or clear per-mount flags, including
`MS_RDONLY`; the remount ignores its source argument
([`mount(2)`](https://man7.org/linux/man-pages/man2/mount.2.html)). Thus hiding
the VM-side source after the root pivot is not the protective boundary. The
kernel locks read-only flags only when mounts cross from a more privileged to a
less privileged mount namespace, a protection associated with differing user
namespace privilege
([`mount_namespaces(7)`](https://man7.org/linux/man-pages/man7/mount_namespaces.7.html)).

The guest may therefore make its bind mount appear writable, but under the
assumptions below it cannot remove `VZSharedDirectory`'s host-side read-only
policy or mutate the macOS worktree through that share.

## Assumptions

- The runtime is stock Apple Container 1.2.2 using its pinned Containerization
  0.40.1 Virtualization.framework backend.
- The protected worktree mount reaches `VZSharedDirectory` with `ro` intact.
- No writable share exposes the same worktree, an ancestor, or an equivalent
  alias.
- No separate host-write channel exposes the protected tree.
- This conclusion covers writes through the worktree share; it does not claim
  that granting `CAP_SYS_ADMIN` is otherwise safe.

## Verification checklist

- Confirm the resolved versions are Apple Container 1.2.2 and Containerization
  0.40.1.
- Trace `readonly`/`ro` from the requested mount to
  `VZSharedDirectory.readOnly`.
- Confirm the protected path is provided by that VZ VirtioFS share and only
  then bind-mounted into the workload.
- Enumerate host shares and path aliases to exclude an overlapping writable
  route.
- Regression-test that the host worktree remains unchanged when a workload
  with the B3 capability set alters its namespace-local mount state and then
  attempts ordinary create, modify, rename, and delete operations.
- Require equivalent host- or server-side read-only enforcement before
  accepting another backend; guest bind `MS_RDONLY` alone is insufficient.
