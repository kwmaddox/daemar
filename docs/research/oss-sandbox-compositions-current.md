# Open-source sandbox compositions for Daemar

**Research date:** 2026-08-26

This composition pass was folded into the corrected synthesis:
[`oss-sandbox-options-synthesis.md`](./oss-sandbox-options-synthesis.md).

The initial draft recommended a Microsandbox-based chain. That recommendation
was withdrawn after reconciliation with Daemar's existing hands-on evidence:
Microsandbox 0.6.8 allowed arbitrary raw-IP egress when a hostname allow rule
was present, and its published host-side secret exposure advisory remained
unpatched. Wreckroom inherits Microsandbox and is therefore not a recommended
composition either.

The surviving compositional direction is:

```text
maintained VM runtime
    + disposable host staging tree mounted into the VM
    + host-side comparison after the VM is stopped
    + capability-relative explicit promotion
```

See the synthesis for evidence, candidate disposition, remaining security
ownership, and the required evaluation gates.
