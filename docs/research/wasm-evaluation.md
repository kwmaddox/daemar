# WASM as the agent sandbox — evaluation

**Researched 2026-08-13.** Raw notes: `scratchpad/wasm-fit.md`,
`scratchpad/wasm-isolation.md`. Prompted by "we didn't explore WASM." Verdict:
**not a candidate for the agent sandbox**, ruled out on two independent grounds.

## 1. Fit — decisive on its own

WASM cannot host what our agents do (run arbitrary native tooling: shells,
compilers, npm/pip/cargo). This is a *deliberate WASI security design choice*,
not a maturity gap:
- WASI has no fork/exec to the native OS; the pending spawn proposal spawns only
  other WASM modules (eunomia.dev status writeup).
- The only path to unmodified `bash`/`apt`/`gcc` is `container2wasm`, which
  emulates a whole Linux kernel in WASM bytecode and self-labels "experimental."
- Every production "agent + WASM" system (Wasmtime/WASI, Hyperlight-Wasm,
  Wassette, Spin, wasmCloud) runs *WASM-compiled* payloads, never arbitrary
  native tooling.

The property that makes WASM elegant — no ambient authority, no arbitrary native
execution — is exactly what a coding agent must have. Right isolation, wrong
workload.

## 2. Isolation — weaker-TCB, and the field wraps it in a VM anyway

Even setting fit aside, WASM isolation is **in-process software fault
isolation**, not a VM/kernel boundary:
- Runs in the same OS process as the host; Wasmtime *explicitly disclaims*
  process isolation (docs.wasmtime.dev/security.html). The TCB is the
  compiler+runtime (Cranelift/Winch) — large, actively-changing code — vs. a
  VM's smaller, hardware-backed hypervisor TCB.
- Real, recent escapes: the April 2026 Bytecode Alliance batch had **two
  Critical sandbox escapes** giving guest WASM arbitrary host-memory read/write
  (CVE-2026-34971 aarch64 Cranelift; CVE-2026-34987 Winch), plus 10 more issues;
  the Cranelift bug had existed since v32.0.0 and evaded fuzzing until an
  LLM-audit sprint found it. A compiler bug bypasses the capability model
  entirely.
- The capability model (zero ambient authority via WASI) is a *genuine* strength
  for constraining a hostile dependency's *requests* — but not against a
  compiler-level memory escape.
- Tellingly, the WASM community's own high-assurance tooling — Microsoft's
  **Hyperlight-Wasm** — wraps Wasmtime *inside a hardware micro-VM*, "adding the
  security of a VM to the sandbox already provided by Wasmtime." The field treats
  a VM boundary as the correct isolation for untrusted code and WASM as a layer
  *inside* it, not a replacement.

## 3. Where WASM could earn a role later (not now)

The Hyperlight pattern ("WASM inside a VM") points at a *future, inner* use:
sandboxing individual **WASM-compiled tool plugins** as a capability-scoped layer
*inside* our VM sandbox. That's an earned-later structure, not the agent sandbox,
and only if we ever have tools worth compiling to WASM. Recorded so the option
isn't forgotten; explicitly not built now.

## Conclusion

WASM does not reopen the substrate decision. Apple `container` /
Virtualization.framework (a true VM boundary) remains the substrate. The
invariant's "kernel-level isolation" wording stands — WASM would have forced a
revision only if adopted, and it isn't.
