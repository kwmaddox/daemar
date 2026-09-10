# PER-84 CE-M residue disposition

Dedicated planner, 2026-09-09; inspected HEAD
`4c809b31264f6c147563c52604d6aea598487ccd` plus the existing dirty tree.
No source edits or execution results are claimed by this planning pass.

Authority: this folder's `planner-input.md`, advisory Card92/93 in
`findings.json`, selected Card16/18/45/47/74 in
`../review-resolution-2/referents.json`, `../contract.md`, predecessor
disposition/handoff, and current AGENTS.md, CONTEXT.md, conventions.md.

CE-M: accept the delayed-refresh residue, with the offered fail-closed
repair. The inspected `console/mod.rs:628` returns None without a
semicolon; `:630-635` returns None without the url token and equals sign;
`:659-664` silently skips None. Thus the two supplied strings never
reach the origin assertion. The actual 404 already enters the response
collection at `:2381-2398`, which is scanned at `:2451-2458`.
`console.feature:414` requires every response reference to be same-origin.
Card16 explicitly included refresh; Card18 did not exclude it, while
requiring the browser/markup division to stay narrow.

The primary [WHATWG shared declarative refresh steps](https://html.spec.whatwg.org/multipage/semantics.html#shared-declarative-refresh-steps)
were inspected on 2026-09-09 (page dated 8 September 2026). Step 10 admits
comma and semicolon separators. Step 11 starts with the remaining text as
the URL and conditionally consumes a URL prefix. Consequently the two
supplied 30-second examples have foreign targets under that algorithm.
This is a source-derived conclusion; the planner did not run a browser
timing experiment. Author-conformance syntax is narrower than processing
syntax and cannot justify ignoring these examples.

Chosen repair: distinguish non-refresh elements from recognized refresh
elements whose current extractor cannot produce a target. Only the former
may be skipped. For the latter, fail the existing parsed-document origin
oracle with a useful diagnostic. Keep the existing recognized `;url=`
target extraction and same-origin check, including the mixed-case,
whitespace and quoted local positive control. Do not add comma/bare-target
parsing in this packet. Both are deliberately rejected as unsupported
refresh syntax, even when their target would be local. This conservative
oracle outcome is the expressly offered fail-closed alternative, not a
new console-rendering policy or a claim of full WHATWG parser conformance.
Missing/empty content on recognized refresh likewise must not collapse
into non-refresh success. An ordinary non-refresh meta element still passes.

No surviving residue is rejected, and no refuted limb is promoted. CE-N
remains closed under Card93. CE-M's 421 limb remains withdrawn: current
`crates/daemar-card/src/console.rs:416-418` constructs Body::empty(). The
zero-delay limb remains withdrawn on the supplied predecessor disposition;
no new timing evidence supports promotion. No new review axes or parser
edge-case hunt are admitted.

Existing positive and negative controls, URL attribute scanning, response
collection, same-origin/error-page assertions, and CE-N checks remain
effective. Sole source ownership is
`crates/daemar-card/tests/behavior/console/mod.rs`; other edits are evidence.
No public surface changes, so AGENTS.md's typed-skeleton approval rule is
not triggered. No authority gap blocks dispatch.

Plan: `docs/plans/per-84-ce-m-residue-plan.md`. Direct fresh-Luna packet:
`docs/execution/per-84/review-resolution-3/task-01.md`.
