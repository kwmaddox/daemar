# ast-grep rule pack — L2 of the Rust quality gate

Layer 2 of the deterministic quality stack (PER-68; L0 is the clippy
workspace lint table, L1 is cargo-deny, L3 is Dylint, future PER-69).
This layer covers what clippy configuration structurally cannot: clippy's
`disallowed-types` takes type *paths* only, so it cannot express
"`Box<T>` where `T` is `dyn Error`" — a tree-sitter structural rule can.

Run via `just check` (`scripts/check.sh` runs `ast-grep test`, then
`ast-grep scan`, then the ignore-discipline guard pass described below).
Tooling is pinned: ast-grep 0.45.2, version-checked in `check.sh`.

## Rules

Every rule cites its convention ID from `conventions.md` in its message;
rationale lives as a header comment in each rule file.

| Rule | C-ID | Claim |
| --- | --- | --- |
| `no-allow-attributes` | C5 | Literal `#[allow]`/`#![allow]` syntax, incl. raw spellings, cfg_attr smuggling, and macro-transcriber emissions (PER-75; macro-*synthesized* suppressions are PER-78's jurisdiction). |
| `no-box-dyn-error` | C1 | `Box<dyn Error>` in any position, incl. `+ Send + Sync + 'static` bounds and scoped spellings. Other erased-error containers (`Rc<dyn Error>`) are review territory. |
| `no-string-literal-dispatch` | C6 | String literals as match-arm patterns; 2+ `==`/`!=` comparisons of one expression against string literals. A single sentinel comparison is review territory. |
| `no-stringly-typed-field` | C7 | `String` fields (structs and enum struct-variants) named `*_id`, `kind`, `status`, `state`, `*_type`. Extending the name list is a deliberate one-line diff with a fixture. |
| `ignore-discipline` | C5 (principle) | Every `ast-grep-ignore` comment names exactly one rule and carries `-- <reason>`. |

Scope is uniform: `crates/**/*.rs`, tests included. Tests are exempt from
C4 (panics), never from structural conventions; C4's exemptions are also
why tests don't need the `Box<dyn Error>` `?`-idiom.

## Suppressing a finding

The two heuristic rules (C6, C7) anticipate legitimate hits — a parse
boundary that converts input to an enum exactly once is *allowed* by C6.
The escape hatch is a reasoned ignore on the line above (mirroring C5's
expect-with-reason philosophy):

```rust
// ast-grep-ignore: no-string-literal-dispatch -- parse boundary, converts once
match s { "fast" => Mode::Fast, _ => Mode::Safe }
```

The `ignore-discipline` rule enforces exactly this shape: one named rule,
` -- `, a reason. Bare `// ast-grep-ignore` (a blanket waiver of every
rule on the next line) and rule-only ignores fail the gate.

### The trailing-ignore guard pass

A *trailing* bare ignore (`code(); // ast-grep-ignore`) suppresses every
finding on its own line — including `ignore-discipline`'s own — so
`ast-grep scan` alone can never flag that form. `check.sh` therefore runs
the same rule file a second time over a token-renamed copy
(`ast-grep-ignore` → `AST-GREP-IGNORE-TOKEN`) piped via `--stdin`: the
renamed token no longer suppresses anything, so every ignore comment is
visible. The rule's regexes accept both spellings, so one file serves
both passes and the channels cannot drift.

Three deliberate lexical strictnesses, priced in: a comment that merely
*mentions* `ast-grep-ignore` in prose must still use the disciplined
spelling (the fixture `prose_mention` pins this); a reason may not start
with `*` — in a block comment the closing `*/` is part of the node text,
and without lookahead in Rust's regex engine an empty reason would read
the delimiter as its reason (`-- */` bypass, PER-68 adversarial finding
P2) — reword such a reason; and the rule-test harness applies no
suppression processing, so the escape hatch is proven by the gate demo,
not by fixtures.

## Known limits

- **Syntactic and alias-blind.** `type BoxErr = Box<dyn Error>` is caught
  at the alias definition; *uses* of the alias are not. Same for
  re-exports and generic indirection. Dylint (L3, PER-69) is the semantic
  escalation for anything that needs type or expansion knowledge.
- **Heuristics claim shapes, not semantics.** C6's rule flags dispatch
  *shapes*; whether a site is a legitimate parse boundary is the
  reviewer's call, recorded in the reasoned ignore.
- **Macro-synthesized suppressions** are out of every surface rule's reach
  (PER-75 third adversarial pass); PER-78 owns that class, blocked on
  PER-69.

## Testing

`ast-grep test` runs every `rule-tests/*-test.yml` against its rule with
committed snapshots; each rule ships valid cases (including the near-miss
false-positive candidates the rule was designed against) and invalid
cases. New rules and rule changes ship with fixtures — the harness is the
regression battery that keeps the PER-75-style adversarial findings
closed.
