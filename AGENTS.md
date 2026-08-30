# What is daemar

Daemar is a software factory for agentic software development.  The core thesis of
daemar is inspired by two sources:  Air Traffic Control Slips and Factory Work Logs.  Each
task that is worked in the factory will have a card that travels along with it.  The card will be an
append-only log of all workflow-relevant facts, decisions, structured stage outputs, and evidence
associated with the task.  Each stage will record to the card in a
highly structured output.  The structure is intended to facilitate progressive disclosure of details.
Stages will be able to get data from the card in three ways.  The card will have a narrow frontmatter
that has key information every stage will need.  Each stage will have a contract of data that must be brought
in from the card.  Finally, there will be a mechanism to query the card for additional information.

# Working in this repository

Operational guidance for coding agents and ticket authors. daemar is a
Rust workspace building a software factory: a harness that runs coding
agents against small, spec-reviewed tickets. Current phase: re-founding —
the factory loop itself.

## Commands

- `just bootstrap` — once per clone: activates the versioned git hooks
  (`core.hooksPath` → `scripts/hooks`) and runs the full gate.
- `just check` — the full quality gate (`scripts/check.sh`). The
  pre-commit hook runs the same script: a red workspace cannot become a
  commit. `git commit --no-verify` is the loud, deliberate escape hatch.

## Rust conventions

`conventions.md` is the adjudication referent for all Rust in this repo —
read it before writing Rust. Every convention has a stable ID (C1…C16);
reviews, gate rules, and task prompts cite these IDs.

## Typed design first

For any ticket that adds or changes the public surface of a library crate
— new modules, new or changed `pub` signatures, new error variants —
present the typed skeleton first: error enum variants, `pub` signatures,
doc comments stating invariants. Get it approved in-conversation before
writing implementation bodies; the skeleton is not a separate commit — the
commit seam stays implemented-and-green. Tickets that leave signatures
untouched (docs, pure tests, behavior-preserving fixes) skip this.

## Definition of done

1. `just check` passes — the gates are not advisory.
2. Where the ticket touched public API, the typed skeleton was approved
   before implementation.
3. Every piece of accepted scope ships with its tests. Scope may be cut
   with written justification in the ticket; accepted-but-untested scope
   may not ship.
4. Where the diff touches Rust, a conformance review against
   `conventions.md` was run and its findings resolved or ticketed
   (findings cite C-IDs).
