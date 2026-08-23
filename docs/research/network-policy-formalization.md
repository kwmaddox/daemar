# Formalizing access-control / allow-deny policy in Quint and TLA+ — primary-source notes

**Retrieved:** 2026-08-18. All claims cite primary sources: actual `.qnt`/`.tla`
spec files in the official example repositories, and the official Quint docs.
Repositories were cloned at HEAD on this date:

- Quint: `github.com/quint-co/quint` (the project spun out of Informal Systems
  in April 2026; the old `informalsystems/quint` URL redirects — see
  `docs/research/quint.md`). Examples under `examples/`, docs under
  `docs/content/docs/`.
- TLA+ Examples: `github.com/tlaplus/Examples`, specs under `specifications/`.
- Quint Connect: `github.com/informalsystems/quint-connect` (still under the
  `informalsystems` org; linked from the Quint docs and blog).

File paths below are relative to those repository roots.

---

## Honest scope note — what does NOT exist

There is **no dedicated firewall / allowlist / network-policy / access-control
example** in the Quint examples repository. A full-tree grep of `examples/` for
`firewall|allowlist|deny|access.?control|egress|permission|capabilit` returns
nothing on point. The idioms below are therefore drawn from the *shape* of
policy logic that does appear in Quint examples (ERC20 allowances, CosmWasm
admin checks) plus the language's core guard/effect mechanics, and from the one
real firewall spec that exists in the TLA+ examples repo.

In TLA+ Examples, the only genuine packet-filter/ACL spec is
`specifications/SDP_Verification/` (Software-Defined Perimeter, zero-trust). No
classic RBAC / access-control-matrix / capability spec was found in that repo.

I found **no example of a security *policy* spec (firewall/ACL/allowlist) whose
traces drive a conformance test against real code** — in either ecosystem. Quint
Connect exists and does exactly this kind of spec→code replay, but its shipped
examples are distributed-systems protocols (two-phase commit), not access
policy. The technique transfers; a worked policy example does not exist to cite.

---

## 1. Is DENY modeled as an explicit action/state-change, or as a pure predicate?

**Both appear. The idiomatic split is: the *decision* is a pure predicate; the
*effect of denying* is represented either as a returned error (no state change)
or as a state change into a "dropped/logged" set — and which one depends on
whether the deny needs to be observable.**

### (a) Deny as a pure predicate + returned error, no state transition (Quint idiom)

The ERC20 example is the canonical Quint pattern for "a check that gates an
effect." Guards are pure predicates that accumulate an error string; the effect
is applied only if the error is empty.

`examples/solidity/ERC20/erc20.qnt`, the `require`/`andRequire` helpers
(lines 70-77) and `_transfer` (lines 104-127):

```
pure def require(cond: bool, msg: str): str =
    if (cond) "" else msg
...
val err = require(fromAddr != ZERO_ADDRESS, "...from the zero address")
    .andRequire(toAddr != ZERO_ADDRESS, "...to the zero address")
    .andRequire(fromBalance >= amount, "...exceeds balance")
if (err != "") { returnError(err, state) }   // DENY: no state change
else { ... returnState(state.with("balanceOf", newBalances), true) }  // ALLOW
```

Here **deny is not a distinct action and produces no transition** — the function
returns the *unchanged* state plus a non-empty error. The state machine then
refuses to apply a denied result. `examples/solidity/ERC20/erc20.qnt`,
`fromResult` (lines 257-261):

```
action fromResult(result: Erc20Result): bool = all {
    result.error == "",              // guard: only allowed results advance
    erc20State' = result.state,
}
```

`fromResult` is a conjunction of a guard (`result.error == ""`) and an effect
(`erc20State' = ...`). A denied call has a non-empty error, so this action is
simply **not enabled** and the step does not happen. The same idiom recurs in
`examples/cosmwasm/zero-to-hero/vote.qnt` (functions return a record with an
`.error` field; tests assert `res.error == ""`, e.g. line 227).

This rests on Quint's core mechanic: an action is a guard/effect conjunction and
is only taken when enabled. Docs: `docs/content/docs/lang.md` describes the
`enabled` operator (lines 1832-1842, "equivalent to `ENABLED A` of TLA+") and
the "guards and effect expressions in pairs" normal form (line 977); `any { }`
"non-deterministically executes one of the *enabled* actions" (line 859).

### (b) Deny as an explicit state change into a "dropped" set (TLA+ firewall idiom)

The SDP firewall does the opposite: because a dropped packet must be
*observable* (logged, and reasoned about by invariants), **deny is a state
transition** — the packet is added to a `DropPackets` set.

`specifications/SDP_Verification/SDP_Attack_Spec/SPA_Attack.tla`,
`FwProcEndPointAccess` (lines 646-677). On no match:

```
ELSE \*CASE3 : the incoming packets not match any rule
  /\ FwDataChannel' = Tail(FwDataChannel)
  /\ AclRuleSet'    = AclRuleSet
  /\ sChannel'      = sChannel                              \* just drop the packets
  /\ DropPackets'   = DropPackets \cup {Head(FwDataChannel)} \* record it into exception log
```

Allow is also a state change (append the packet to the outbound `sChannel`);
deny is a state change into `DropPackets`. Both outcomes are observable, which is
the point of doing it this way.

**Reading for daemar:** use (a) when a denied attempt is a non-event (the effect
just doesn't happen); use (b) when "we blocked X" must itself be a recorded,
inspectable fact. For an egress wall where a blocked connection is a
security-relevant event you want invariants over, (b) — a transition into a
rejected/logged set — is the pattern with real precedent.

## 2. Is enforcement (the mechanism) separated from policy (the decision)?

**Yes, cleanly, in both real specs — via the pure-predicate / action split.**

- SDP firewall: the *policy* is pure match predicates —
  `AclMatch3Tuple(p, Acl)` and `AclMatch4Tuple(p, Acl)`
  (`SPA_Attack.tla` lines 618-632), each `\E r \in Acl: p.sIP = r.sIP /\ ... /\
  r.action = "Accept"`. They take state as an argument and only return a
  boolean; they change nothing. The *enforcement* is the action
  `FwProcEndPointAccess` (lines 646-677), which *calls* those predicates and is
  the only thing that touches `sChannel`/`DropPackets`. Rule *administration* is
  a third, separate concern: `FwProcAclCfg` (lines 600-610) installs rules
  arriving on a control channel, and `FwProcAclTimeOut` (lines 683-693) ages
  them out.
- ERC20: policy is the `require` chain (pure); enforcement is `fromResult` /
  `step` deciding whether to advance state.

The recurring structure is **pure `def` = policy, `action` = enforcement**. This
maps directly onto daemar's own "policy vs. enforcement" language and the
CLAUDE.md observation that the spec can *assert the precondition* (the predicate)
but cannot *prove confinement* (the enforcement mechanism lives in the OS, not
the spec).

## 3. Default-deny and rule ordering / precedence

**Default-deny is modeled structurally as the final `ELSE` / fallthrough, not as
an explicit catch-all deny rule.** In the SDP firewall, every rule in
`AclRuleSet` carries `action |-> "Accept"` (there are no deny rules); a packet is
forwarded only if it matches an Accept rule, and the terminal `ELSE`
(CASE3) drops anything unmatched (`SPA_Attack.tla` lines 665-670). Absence of a
matching allow *is* the deny. That is default-deny by construction — the same
"start from none, add only allows" posture daemar wants for egress.

**Precedence is encoded as the order of an `IF / ELSIF` chain**, not as rule
priority numbers. `FwProcEndPointAccess` tests the more specific 4-tuple rule
first, then the 3-tuple rule, then drops (lines 652-670). Whichever branch fires
first wins; later branches are unreachable for that packet.

**Short-circuit precedence among multiple checks** appears in ERC20's
`andRequire`: `if (prevErr != "") prevErr else require(cond, msg)`
(`erc20.qnt` line 77) — the *first* failing check determines the denial reason;
subsequent checks are not consulted. This is the "first matching rule wins"
idiom expressed over a chain of predicates.

I did **not** find, in either example repo, a spec that models *conflicting*
allow-rules-vs-deny-rules with an explicit conflict-resolution policy (e.g.
deny-overrides). The real specs sidestep conflict entirely: SDP has only allow
rules + a default drop; ERC20 has only deny-guards + a default allow. If daemar
needs allow-list *and* deny-list with precedence, no primary-source template for
the conflict case was found — it would be new design, not a borrowed idiom.

## 4. Connecting a policy spec to an implementation for validation

The first-party mechanism is **Quint Connect** (`github.com/informalsystems/
quint-connect`), a Rust model-based-testing library. Primary source: the launch
post `docs/content/posts/quint_connect.mdx` (in the quint repo) and
`docs/content/docs/model-based-testing.mdx`.

How it connects spec to code (`quint_connect.mdx`, "How it works"):

1. Implement a `State` trait: `from_driver(&Driver) -> SpecState` projects the
   concrete implementation state into a Rust type mirroring the spec's state
   (the two-phase-commit example projects each node into a `ProcState` and
   collects a `BTreeMap`). This is the **state projection**.
2. Implement a `Driver` trait: `fn step(&mut self, step: &Step)` uses a
   `switch!` macro to map each spec transition (the action picked inside `any`,
   plus each `nondet ... .oneOf()` value) to a concrete method call. This is the
   **action mapping / conformance interface**.
3. Annotate a test with `#[quint_run(spec = "...", max_samples = 1000)]` to
   replay random simulations, or `#[quint_test(spec = "...", test = "...")]` to
   replay a specific named `run` from the spec. `cargo test` then drives the
   implementation through spec-generated traces, checking state equivalence
   after each step.

The docs frame the value exactly as daemar's conformance harness does:
"Model-based testing helps you gain confidence that the code matches the design"
and prevents spec/code "drifting apart over time" (`quint_connect.mdx`; the
`Driver`/`State`/`switch!` trio is the direct analog of daemar's
Driver/State mapping). `model-based-testing.mdx` also names **trace validation**
(replaying real recorded execution traces against the spec) as a second
technique alongside MBT.

TLA+'s comparable path (not used here, noted for completeness): Apalache/TLC
generate traces, and trace validation replays logged system traces against the
spec — but no security-policy instance of this was found in the TLA+ Examples
repo.

**Caveat, restated:** Quint Connect's shipped example is two-phase commit, a
protocol, not a policy. No allow/deny policy spec wired to real code via Quint
Connect (or Apalache trace-gen) was found in primary sources.

## 5. The idiomatic structure a practitioner reaches for

Composited from the specs above — this is the recurring template, not an
invented one:

**State variables**
- A **rule set** as first-class state: a set/map of rule records, each a
  structured predicate-target with an explicit decision field. SDP:
  `AclRuleSet` = set of records `[sIP, sPort, dIP, dPort, protocol, action]`
  with `action ∈ {"Accept"}` (`SPA_Attack.tla` lines 176-183, 618-643).
- **Input/pending** items to be decided: a queue/channel of packets
  (`FwDataChannel`) or, in ERC20, the call arguments.
- **Outcome sinks** when outcomes must be observable: `sChannel` (forwarded) and
  `DropPackets` (denied/logged) — separate variables so invariants can quantify
  over each (`SPA_Attack.tla` lines 176-193).

**Pure defs (policy)**
- One match/decision predicate per rule shape:
  `Match(item, ruleSet): bool = \E r \in ruleSet: <fields align> /\ r.action = "Accept"`.
- Guard helpers that compose (`require`/`andRequire`) when a decision is a
  conjunction of preconditions with first-failure precedence.

**Actions (enforcement)**
- One dispatch action that reads a pending item, evaluates the policy
  predicate(s) in precedence order (`IF specific ELSIF general ELSE default`),
  and applies the corresponding effect — forward on allow, record-in-drop-set on
  deny, with default-deny as the terminal branch.
- Separate administration actions to install/remove rules (`FwProcAclCfg`,
  `FwProcAclTimeOut`) so policy content is decoupled from policy evaluation.
- The advance-only-if-allowed guard at the state-machine level
  (`fromResult`: `all { result.error == "", state' = result.state }`).

**Invariants**
- Safety over the outcome sinks: e.g. "every packet in `DropPackets` matched no
  Accept rule," or the security property the SDP spec actually checks — that an
  attacker's packets never reach the target server. (SPA_Attack.tla defines the
  attacker state machine explicitly and checks reachability against it; the
  invariant is the negation of "attacker packet forwarded.")
- For the ERC20 style: postconditions preserved across every allowed transition
  (`totalSupplyInv`, `noOverflowsInv`, `erc20.qnt` lines 283-287).

**The one-line summary:** rule-set-as-state + pure match predicates (policy) +
a single enforcement action with an IF/ELSIF precedence chain terminating in
default-deny (enforcement) + invariants over an observable drop/allow sink. Deny
is a returned-error no-op when it need not be seen, and a transition into a
logged "dropped" set when it must be.
