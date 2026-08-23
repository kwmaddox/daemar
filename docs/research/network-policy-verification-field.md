# How the network-verification field models "allow vs. deny" and enforcement

**Researched 2026-08-18.** Prompted by the daemar egress question: our
invariant is *default-deny egress with an explicit allowlist*, and the
microsandbox proof (`microsandbox-proof.md`) already earned the principle that
"deny" must be an L3/L4 property of *the only network path*, not an L7 rule on a
direct path. Before we invent our own vocabulary for "the policy says deny" vs.
"the packet actually can't get out," this is how the established
network-policy-verification field separates those ideas. Primary sources read
directly (papers pulled as PDF and read in full for the cited sections);
gaps where I could only reach slides or secondary descriptions are called out
explicitly at the end.

The one-line takeaway: **the field is unanimous that "block/deny" is not an
action or a state transition — it is the empty output of a function.** A policy
denotes a function from packet to a *set* of output packets; deny is that set
being empty (`∅` / `0` / the zero of the algebra). "The destination is
unreachable" is then a *derived* property — the composition of policy functions
*and* a separately-modeled topology function yields the empty set — and every
serious tool is explicit that topology/containment is an assumption or a second
model, never folded into the policy decision.

---

## The four sources and their model of "deny"

### 1. NetKAT — deny is `0`, the zero of the Kleene algebra

NetKAT (Anderson, Foster, Guha, Jeannin, Kozen, Schlesinger, Walker, POPL 2014;
read via Kozen's APLAS 2014 exposition *"NetKAT — A Formal System for the
Verification of Networks"*, cs.cornell.edu/kozen/Papers/NetKAT-APLAS.pdf) is the
cleanest answer to the question.

A NetKAT policy `e` **denotes a function** `⟦e⟧ : H → 2^H` — from a packet
history to a *set* of output packet histories (§2.4, "Every NetKAT expression e
denotes a function ⟦e⟧ : H → 2^H"). Within that semantics:

- **`drop` is literally `0`** — the paper writes "We also use pass and drop for
  1 and 0, respectively" (§2.3), and the semantics gives
  `⟦0⟧(σ) = ⟦drop⟧(σ) = ∅` and `⟦1⟧(σ) = ⟦pass⟧(σ) = {σ}` (§2.4). Deny =
  "returns the empty set of outputs." Pass = "returns the input unchanged."
- **A test that fails drops the packet the same way**: `⟦x = n⟧(π::σ)` is
  `{π::σ}` if the field matches and `∅` if it doesn't (§2.4). So filtering is
  not a separate "deny action" — a failed predicate *is* the empty-set output.
  A firewall filter is a Boolean test `b`; the traffic it blocks is exactly the
  packets where `⟦b⟧ = ∅`.
- Because `drop` is the semiring zero, the algebra's own axiom `p·0 = 0·p = 0`
  (§2.1) means: anything sequenced with a drop is a drop. Blocking is
  absorbing, and that falls straight out of the algebra rather than needing a
  special rule.

**Policy vs. topology is a first-class split in the syntax.** NetKAT models the
switch policy and the physical topology as *two separate terms* that are
multiplied (§3.1–3.2):

- switch policy `p` = "sum of all switch policies," each of the form
  `switch = A ; pA` (§3.2);
- topology `t` = "sum of all link expressions," each link being
  `switch = A ; port = n ; switch ← B ; port ← m` (§3.1) — i.e. the topology is
  its *own* NetKAT term that moves a packet across a wire;
- one hop of the whole network is the product `p·t`; the whole network is
  `(p·t)*` (§3.2, "The expression `(pt)*` describes the multistep behavior").

**Reachability is an equation, and "unreachable" is equality to `drop`.** §3.3:
to ask whether a packet can get from switch `A` to switch `B`, you check whether

```
switch = A ; t·(p·t)* ; switch = B
```

**is equivalent to `0` (drop)**. Equivalent-to-`drop` *means* unreachable — "is
equivalent to 0 (drop)" is the paper's own phrasing. So "denied destination is
unreachable" is *not* a special property type: it is the statement that the
end-to-end policy∘topology function, restricted to that source/destination,
denotes the empty set. This is exactly the shape our egress question wants:
"can a packet leave the sandbox to a non-allowlisted host" = "is the
sandbox→host policy composed with the real network path equivalent to drop."

Security properties beyond pure reachability are the same style of equation —
e.g. **waypointing** ("all untrusted traffic must traverse the firewall F",
§3.5) is written as an inequation `≤` between two NetKAT terms and holds iff
every output history was forced through `F`. This is the formal ancestor of
"all egress must go through the mediator."

### 2. Header Space Analysis — deny is the transfer function returning `{}`

HSA (Kazemian, Varghese, McKeown, NSDI 2012). I read the authors' own slide deck
in full (usenix.org protected-file `headerspace_nsdi.pdf`) and corroborated the
formal definitions of Ψ and Γ against the NetPlumber paper
(yuba.stanford.edu/~nickm/papers/net_plumber-nsdi13.pdf) and USPTO patent
restatements; the same-model caveat is noted at the end.

- A packet is a point in `{0,1}^L` header space; **a box is a transfer function**
  `T(h,p) : (h,p) → {(h₁,p₁), (h₂,p₂), …}` — header-and-port in, a *set* of
  header-and-port pairs out (a set, "to allow multicasting"). Same shape as
  NetKAT: input → set of outputs.
- **Drop = the transfer function returns the empty set.** For the topology
  function specifically, `Γ(h,p) = {(h,p*)}` if port `p` connects to `p*`, else
  `{}` (empty) if `p` is not connected — an unconnected port drops. A firewall
  that filters a packet is a `T` that maps it to `{}` for those headers.
- **Topology is a separate function `Γ` from the box function `Ψ`.** HSA keeps
  the network transfer function `Ψ` (what boxes do to packets) distinct from the
  topology transfer function `Γ` (what links do), and **reachability is the
  alternating composition** of the two along a path: apply a box `T`, cross a
  link `Γ`, apply the next box, and so on. Same policy/topology decomposition as
  NetKAT, in function-composition form rather than a Kleene star.
- **Reachability is checked by asking whether the propagated header space is
  empty.** "Can host A talk to B?" is computed by pushing A's full header space
  through the composed transfer functions toward B; the reachable set is the
  (possibly empty) header space that survives. Empty surviving set = unreachable
  = denied. Isolation/leakage checks are the same test on the intersection.

### 3. Batfish — deny is a *predicate/relation* over (packet, network), derived over a computed data plane

Batfish (Fogel, Fung, Pedrosa, Walraed-Sullivan, Govindan, Mahajan, Millstein,
NSDI 2015; ratul.org/papers/nsdi2015-batfish.pdf, read in full for §3–4) is the
one that makes "deny" a **logical relation**, and it is instructive *because* it
sits at a different layer than NetKAT/HSA.

- Batfish's move is to first *derive the data plane* from configs via a Datalog
  (LogiQL) fixed-point computation (§3.1), then represent forwarding as **logical
  facts/relations**: `Drop(node, flow)` ("a packet with certain headers should
  be dropped") and `Forward(node, flow, neighbor)` (§3.1). So at the per-device
  level, deny is a *relation that holds of (node, flow)* — a predicate over
  (packet, policy-state), not an action.
- End-to-end reachability is then two *derived predicates* over the whole data
  plane (§4): `accepted_E(H,S,D)` and `dropped_E(H,S,D)`, "which hold if there
  is some path through the network for which header `H` is eventually accepted
  [resp. dropped] at node `D` when injected at `S`." Note the explicit
  existential-over-paths — reachability is quantified over the topology's paths,
  not a local decision.
- **Crucially, Batfish decouples "the policy/config says X" from "the packet
  reaches Y" by building a data-plane model as an explicit intermediate
  representation.** The config (control plane) is analyzed to *produce* a data
  plane; properties are checked against the data plane, "irrespective of the
  correctness properties of interest" (§3.2). The `FlowTrace` relation gives a
  traceroute-like path with per-hop dispositions (`accepted`, `nullRouted`, …),
  and provenance queries tie a `Drop` back to "the particular line of an ACL
  that caused the packet to be dropped" (§3.1). This is the *policy-decision vs.
  reachability* separation made completely concrete: an ACL `deny` line is one
  fact; whether a flow is `dropped` end-to-end is a different, derived fact that
  also depends on routing and topology.
- Batfish is explicit about **what it assumes vs. verifies** (§3.3): it assumes
  "routers behave as expected based on their configurations" (cannot catch
  hardware/firmware bugs), and it analyzes only the *environments* (route
  announcements, link states) the operator supplies — so it verifies the
  config→dataplane→property chain but *assumes* the device faithfully enforces
  its config and assumes the supplied topology/environment. That is precisely
  our split: the spec can assert the precondition; it cannot prove the box obeys
  it (cf. CLAUDE.md "Enforcement is not the spec's job").
- The `dropped`/`accepted` predicates being **non-mutually-exclusive** under
  multipath (§4.1: "accepted and dropped are not mutually exclusive") drives
  their **multipath-consistency** property `∀H,S : accepted_E(H,S) ⇒
  ¬dropped_E(H,S)` — i.e. a packet must be denied on *all* paths or permitted on
  *all* paths. This is the formal statement of "the enforcement point must be on
  every path," which is exactly the topology assumption our egress design has to
  make explicit (one wall, the only network path).

### 4. Margrave — deny is a *decision predicate* selected by rule order; enforcement (routing) is a separate model

Margrave (Nelson, Barratt, Dougherty, Fisler, Krishnamurthi, LISA 2010;
cs.brown.edu/people/sk/…/nbfdk-margrave-firewall, read in full for §2, §5–6) is
the firewall-specific tool and is the sharpest on **rule precedence, shadowing,
and the policy-vs-routing split.**

- **Decisions are first-order predicates over a request.** A policy maps a
  request `<req>` = `⟨hostname, src-addr-in, src-port-in, protocol, …⟩` (a
  packet) to decisions; for ACLs the decisions are `Permit(<req>)` and
  `Deny(<req>)`, modeled as first-order-logic relations. "Deny" is a predicate
  over (packet, policy), evaluated by *scenario finding* (SAT/model-finding over
  FOL).
- **Rule precedence / default-deny is modeled with two formulas per rule R**
  (§2): `R_matches(<req>)` ("`<req>` satisfies the rule's conditions") and
  `R_applies(<req>)` ("the rule both matches and *determines the decision* as
  the **first matching rule** within the policy"). "Rules are checked in order
  from top to bottom; the first … rule whose conditions apply determines the
  decision." This is how the field formalizes ordering: `applies = matches ∧
  ¬(any earlier rule matches)`. A **catch-all `deny any`** at the bottom is just
  a rule whose `matches` is always true; **shadowing** is detected as a rule
  whose `applies` is unsatisfiable (§ Rule-level Reasoning: "the absence of the
  rule at line 14 (the catch-all deny) … indicates that that rule never applies
  to any packet from the blacklisted address" — i.e. an earlier rule shadows
  it). Default-deny, precedence, and shadowing all fall out of the
  matches/applies distinction rather than needing bespoke machinery.
- **Policy decision vs. enforcement/routing is an explicit decomposition.**
  Margrave models a device as a pipeline (§6): `InboundACL → InsideNAT →
  Internal Routing → OutsideNAT → OutboundACL`. The decisions differ by stage:
  ACLs yield `Permit`/`Deny`; **routing yields `Drop`** (null route) or assigns
  a `next-hop` and `exit-interface`; NAT *transforms* but "never drops packets."
  Then a compound relation `passes-firewall(<req>)` is defined as: requests that
  the ACLs **permit** *and* that are in `internal-result` (i.e. **not dropped in
  internal routing**). So "the ACL permits" and "the packet is actually
  forwarded" are *deliberately different predicates*, combined explicitly.
- **Topology across multiple firewalls is encoded, not assumed away.** For a
  multi-firewall network (§ "Reasoning about … networks of firewalls," Query 6),
  Margrave uses shorthands that "encode the topology" by binding one firewall's
  `exit-interface`/next-hop to the next firewall's entry — "Lines 12–16 capture
  both network topology and the effects of NAT," via `internal-result` and
  `passes-firewall`. End-to-end "does traffic get through" is a conjunction of
  per-firewall `passes-firewall` predicates chained by the topology encoding.

### 5. Classic firewall-policy analysis (default-deny, precedence, shadowing)

The rule-anomaly vocabulary Margrave operationalizes comes from Al-Shaer &
Hamed, *"Discovery of Policy Anomalies in Distributed Firewalls"* (IEEE INFOCOM
2004, pp. 2605–2616) and the related *Firewall Policy Advisor* line. Their
classification — **shadowing** (an earlier rule fully preempts a later one of the
opposite action, so the later rule never applies), **correlation**,
**generalization**, and **redundancy** — is defined by *relations between the
packet-sets two rules match* combined with *rule order*. The model there is:
a firewall is an **ordered list of (predicate → action∈{permit,deny}) rules**,
evaluated first-match, with an implicit **default-deny** final rule; "block" is
the `deny` action attached to the matched packet-set. This is the
predicate-over-(packet, ordered-policy) view, the same one Margrave encodes in
FOL. (Citation confirmed from primary bibliographic records; I did **not** read
the full INFOCOM 2004 text from a primary PDF — see gaps below. The
shadowing/first-match formalization *is* directly verified from the Margrave
paper, which is primary.)

### P4 / data-plane verification (noted, not deeply read)

P4-program verifiers (e.g. Petr4, *"Formal Foundations for P4 Data Planes,"*
arxiv 2011.05948; p4v) model a *drop* as the program setting a `drop` flag /
assigning the egress spec to a drop port — an assignment/state-update in an
operational semantics of the pipeline, checked with symbolic execution / a
verification-condition calculus. This is the one family where "drop" looks more
like an *action* (a mutation of packet metadata) than a set-emptiness — because
P4 models the *device's imperative program*, not the network's denotational
policy. Noted for completeness; I did not read the P4 sources in full, so treat
this paragraph as orientation, not a verified claim.

---

## Answers to the four specific questions, cited

**Q1 — Is "deny" an action/state-transition or a predicate over (packet,
policy)? What's dominant and why?**
Two idioms, and they agree on the essential thing:
- *Denotational (dominant in the semantics-first tools, NetKAT & HSA):* a policy
  is a **function packet → set-of-packets**, and deny is that function returning
  the **empty set** (`⟦drop⟧ = ∅`, NetKAT §2.4; `Γ(h,p) = {}`, HSA). Not an
  action — the *absence* of outputs.
- *Logical-relation (Batfish, Margrave, classic firewall analysis):* deny is a
  **predicate/relation over (packet, policy-state)** — `Drop(node, flow)`
  (Batfish §3.1), `Deny(<req>)` (Margrave §2). Evaluated by Datalog fixpoint or
  FOL model-finding.
These are the same fact at two resolutions: "the output set is empty" ≡ "the
`Drop`/`Deny` predicate holds." **Neither treats deny as a state transition of a
running system** — it is the meaning of the policy on a packet. The reason the
denotational form dominates the foundational work: making deny = `0`/`∅` gives
you an *algebra* (composition, absorption `p·0=0`, decision procedures) instead
of an operational trace to simulate.

**Q2 — "policy says deny" vs. "packet actually doesn't reach": do the tools
assume the enforcement point is on the only path, and how is that stated?**
Yes, and they state it as an **explicit second model plus an explicit
assumption**:
- The *policy decision* (`Deny`/`drop`/failed test) is always a distinct object
  from *reachability*. Reachability is the policy function **composed with a
  separately-modeled topology** (NetKAT `t` and `(p·t)*`, §3.1–3.3; HSA `Γ`
  composed with `Ψ`; Margrave's `passes-firewall` chained by the topology
  encoding, Query 6; Batfish's `accepted/dropped` quantified over paths, §4).
- The "on the only path" assumption is made explicit as a **consistency
  property**: Batfish's multipath consistency `∀H,S: accepted ⇒ ¬dropped` (§4.1)
  is precisely "the enforcement result is the same on every path" — a check that
  fails loudly when an enforcement point is *not* on all paths. Margrave's
  `passes-firewall` requires passing *the* firewall(s) the topology says the
  traffic crosses.
- And every tool states the **faithfulness assumption**: Batfish "assume[s]
  routers behave as expected based on their configurations" (§3.3) — the device
  actually enforces its config. That is the boundary between what the model
  verifies and what it assumes, stated in the paper.

**Q3 — How is "a denied destination is unreachable" expressed and checked?**
As a **reachability invariant over the composed network model**, checked as
emptiness/equality-to-drop:
- NetKAT: check `switch=A ; t·(p·t)* ; switch=B ≡ 0` — equality to `drop`,
  decided by the bisimulation/decision procedure (§3.3). Unreachable ⇔ the
  end-to-end term denotes `∅`.
- HSA: push A's header space through composed `T`/`Γ`; the destination is
  unreachable iff the surviving header space at B is **empty**.
- Batfish: `¬∃D : accepted_E(H,S,D)` for the forbidden class — no path accepts
  it; equivalently `dropped` on all paths (§4).
- Margrave: `IS POSSIBLE? Permit(<req>) ∧ <req∈forbidden-class>` returns
  **false** (Query 2 returns `false` for the blacklisted host); for end-to-end,
  `NOT passes-firewall(...)` (Query 6).
In all four, the property is "the reachable set for the forbidden traffic is
empty over the *whole* network model," not "some rule says deny."

**Q4 — Standard decomposition: policy decision vs. enforcement mechanism vs.
topology/containment — verify vs. assume?**
The established three-way split:
1. **Policy decision** — the local `Permit/Deny/drop` meaning of a config/filter
   on a packet. *Verified* by all four (it's the modeled object).
2. **Enforcement / forwarding mechanism** — that the device *acts on* the
   decision (routing forwards or null-routes; the box actually filters). Batfish
   models this as the derived data plane (`Forward`/`Drop`) but **assumes the
   hardware faithfully realizes the config** (§3.3). Margrave models routing as
   its own stage yielding `Drop`/next-hop (§6). This layer is *partly modeled,
   ultimately assumed* — the device honoring its own decision is an assumption,
   not proven.
3. **Topology / containment** — that the enforcement point sits on the path(s)
   the traffic takes. Modeled *explicitly and separately* (NetKAT `t`, HSA `Γ`,
   Margrave's topology encoding, Batfish's path quantification) and *assumed
   correct as input* (the supplied environment/link-state). Multipath-consistency
   is the check that this assumption isn't silently violated.

---

## What this means for daemar (why it matters here)

Our egress invariant is a network-reachability property, and the field says
exactly how to keep its parts from collapsing into each other:

- **"Deny" for us should be modeled denotationally — the empty output set / the
  `0` of a policy — not as an action a stage takes.** "A non-allowlisted host is
  unreachable from the sandbox" = the composed `sandbox-policy · network-path`
  function denotes `drop` for that destination. This is the NetKAT `≡ 0`
  framing, and it's the natural thing for a Quint invariant to *assert* (every
  egress is either to an allowlisted target or denotes drop).
- **Keep three objects distinct, as the field does:** (a) the *policy decision*
  ("is this host on the allowlist") — spec-assertable; (b) the *enforcement
  mechanism* (the mediating proxy / host nftables actually drops) — an
  OS-mechanism property, *assumed* by any spec and provable only by adversarial
  test, exactly as `sandboxing.md`/`microsandbox-proof.md` already argue; (c)
  the *topology/containment* assumption ("the wall is the **only** network path")
  — this is the network-verification analogue of Batfish's multipath consistency
  and NetKAT's `(p·t)*`, and it is the assumption the microsandbox proof found
  *violated* (raw-IP egress was a second path around the L7 allowlist). The
  field's lesson matches ours precisely: **an allowlist decision is worthless
  unless the enforcement point is provably on the only path** — "denied ⇒
  unreachable" holds only under the single-path topology assumption, which must
  be stated and separately enforced, never assumed by the policy layer.
- Naming: the field already separates **policy decision** (permit/deny predicate)
  / **enforcement** (forward/drop mechanism) / **topology-containment**
  (on-the-only-path). We should reuse this three-way vocabulary rather than mint
  our own, so our spec's "carries the sandbox/egress precondition" language lines
  up with a well-understood decomposition.

---

## What I could NOT verify from primary sources (honesty ledger)

- **HSA formal definitions of `Ψ` and `Γ`**: I read the authors' NSDI 2012
  *slide deck* in full (primary, but slides), plus the NetPlumber paper and
  USPTO patent restatements for the `Γ(h,p)={} if unconnected` definition. I did
  **not** get the NSDI 2012 *paper* text itself through a primary PDF (the
  usenix protected-file URL 403s to WebFetch; the copy that downloaded was the
  slides). The `T`/`Ψ`/`Γ`/empty-set-drop model is consistent across all three
  primary artifacts, so I'm confident, but the exact paper section numbers for
  HSA are not cited here.
- **Al-Shaer & Hamed INFOCOM 2004 full text**: citation and the
  shadowing/correlation/redundancy taxonomy are confirmed bibliographically and
  the first-match/shadowing *formalization* is verified from the Margrave paper
  (primary), but I did not read the INFOCOM 2004 PDF directly. Treat the
  four-way anomaly taxonomy attribution as bibliographic, not text-verified.
- **P4 (Petr4 / p4v)**: the "drop = metadata assignment in an operational
  semantics" claim is from familiarity + abstract, not a full read of the
  primary sources. Flagged as orientation only.
- **NetKAT & Batfish & Margrave**: read directly from the cited PDFs; the quoted
  equations, predicate names, and section numbers above are from the primary
  text. NetKAT was read via Kozen's APLAS 2014 exposition (a primary paper by a
  NetKAT co-author) rather than the POPL 2014 paper itself — the semantics is
  identical; §-numbers cite the APLAS exposition.
</content>
