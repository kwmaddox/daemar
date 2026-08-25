# Rust conventions

The adjudication referent for Rust code in this repository: reviews cite it,
gate rules cite it, task prompts excerpt it. A conformance review needs only
this file and a diff.

Rules of the file:

- Every convention has a stable ID (`C1`…), a falsifiable statement, and an
  enforcement channel (**gate**: a build-failing check enforces it — cited;
  **review**: adjudicated by a reviewer against this text). A convention with
  no enforcement channel is a wish and does not belong here.
- Scope is code only. Process discipline (slice size, workflow, budgets) is
  codified in the factory harness, not here.
- This file governs **form**. Whether code behaves correctly, and whether
  that behavior is proven, is the jurisdiction of `specs/` and its
  batteries — a conformance review against this file never adjudicates
  correctness or test coverage.
- When a worked example conflicts with a convention's rule — another
  convention's or its own — the rule text wins. Examples illustrate; they
  never license.
- Future slices extend this list; they do not rewrite it. IDs are never
  reused. Entries marked **override** deliberately depart from community
  idiom and say why.
- This is not a Rust tutorial. An entry exists only because model-generated
  code recurrently gets it wrong, or because this project overrides idiom.

## Errors

### C1 — Errors are crate-native

Every fallible `pub fn` in a library crate returns `Result<_, E>` where `E`
is defined in this crate. No foreign error type, no `Box<dyn Error>`, no
`String` error crosses a public boundary.
*Enforcement: gate, partial (ast-grep `no-box-dyn-error` — live via
`scripts/check.sh`; alias uses are blind, see `rules/ast-grep/README.md`);
review for foreign error types. The gate deliberately over-approximates
this rule: it bans `Box<dyn Error>` at any position — private items and
tests included — not just at the `pub` boundary the rule text governs. A
private erased error has no role in the house pattern (it reaches the
contract only via a C8 or C3 violation), and C4's test exemptions remove
the test-side need; a genuine exception takes a reasoned
`ast-grep-ignore`.*

```rust
// wrong
pub fn run(spec: &RunSpec) -> Result<RunOutcome, std::io::Error>
// right
pub fn run(spec: &RunSpec) -> Result<RunOutcome, Error> // crate::Error
```

Why: the public error type is the crate's failure contract; a foreign type
leaks implementation and cannot grow variants without breaking callers.

### C2 — No error-helper crates (override)

`anyhow`, `eyre`, and `thiserror` are banned everywhere, including binaries.
Error enums derive `Debug` only; `Display` and `std::error::Error`
(including `source()`) are written by hand.
*Enforcement: gate (cargo-deny bans, `deny.toml`; clippy disallowed
types/macros, `clippy.toml` — both live via `scripts/check.sh`).*

The house pattern:

```rust
#[derive(Debug)]
pub enum Error {
    Session { path: PathBuf, source: std::io::Error },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Session { path, .. } => {
                write!(f, "failed to prepare session directory {}", path.display())
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Session { source, .. } => Some(source),
        }
    }
}
```

Why (override): the community accepts these crates because they save humans
boilerplate. Agents write the boilerplate, so the saving is void; what
remains is cost — proc-macros execute code at compile time (supply-chain
surface), and derived impls are invisible and ungreppable. An explicit
failure surface is machine-checkable documentation for reviewing agents.

### C3 — Error variants carry typed data

A variant whose only payload is `String` is a defect unless it wraps text
that has no structured form at the construction site (raw parser input,
subprocess stderr). Stringifying an available structured error
(`.to_string()` on an error value) is a violation — carry the source
instead.
*Enforcement: review (Dylint candidate, PER-69).*

```rust
// wrong
Archive(String)                                   // caller must parse prose
// right
Driver { stage: DriverStage, code: Option<i32> }  // caller can match on it
```

Why: typed payloads let calling code and reviewing agents branch on failure
structure instead of string-matching error prose.

### C4 — Panics stay in tests

`unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!`, `dbg!`, and
unchecked indexing/slicing appear only in test code.
*Enforcement: gate (clippy restriction lints + test exemptions — live,
workspace lint table + `clippy.toml` via `scripts/check.sh`).*

Why: a panic in the factory is an unreported failure path; every production
failure flows through the error contract (C1).

### C5 — Suppressions carry reasons

The only lint suppression is `#[expect(lint, reason = "...")]`. A bare
`#[allow]` — including file-level — is a defect.
*Enforcement: gate, for literal suppression syntax — clippy
`allow_attributes` + `allow_attributes_without_reason` (canonical outer
forms, reason-less `#[expect]`) and ast-grep `no-allow-attributes` (inner
`#![allow]`, raw `r#allow` spellings, cfg_attr smuggling, literal
transcriber emissions), both live via `scripts/check.sh`. Suppressions a
macro synthesizes from non-literal tokens (caller-supplied or split-token
meta) are beyond surface analysis — review, until the macro-suppression
follow-up (PER-78) lands a sound channel.*

Why: an unexplained suppression hides a decision; `#[expect]` additionally
fails when the lint stops firing, so stale suppressions self-report.

## Types

### C6 — Domain states are enums

A value drawn from a closed set of meanings is an enum. Matching on string
literals is legitimate only at a parse boundary that converts input to the
enum exactly once.
*Enforcement: gate, partial (ast-grep `no-string-literal-dispatch` — match
arms over string literals and `==`/`!=`-literal chains, live via
`scripts/check.sh`; parse boundaries take a reasoned ast-grep-ignore);
review for the semantic remainder (single comparisons, stringly flow).*

```rust
// wrong
if change.kind == "deleted" { ... }
// right
if matches!(change, Change::Deleted { .. }) { ... }
```

Why: the compiler proves exhaustive handling of enums; strings make every
consumer a parser and every typo a silent bug.

### C7 — Domain values get newtypes

A `String` or bare numeric that identifies something (`run_id`,
`container_name`) or carries a unit (`timeout_secs`, `memory_mb`) does not
cross a `pub` boundary as a primitive. A std type with the right semantics
(`Duration`, `PathBuf`) counts as typed; a newtype is required only when std
has no fitting type. A bare numeric is acceptable when it is a dimensionless
count whose meaning is unambiguous from its name and which carries no
invariant (`cpus: u32`); a unit (`timeout_secs`) or an invariant (nonzero,
bounded) requires a type that encodes it.
*Enforcement: gate, partial (ast-grep `no-stringly-typed-field` — `String`
fields named `*_id`/`kind`/`status`/`state`/`*_type`, live via
`scripts/check.sh`); review for primitives beyond the name list and for
`pub fn` parameters. The gate deliberately over-approximates this rule:
it flags matching fields at any visibility, not just those crossing a
`pub` boundary. The heuristic targets the storage site — a private
stringly field loses the closed set at the point of representation and
leaks the design through accessors, `Debug`, or serialization regardless
of visibility; a genuine exception takes a reasoned `ast-grep-ignore`.*

```rust
// wrong
pub fn reap(name: &str, timeout_secs: u64)
// right
pub fn reap(name: &ContainerName, timeout: Duration)
```

Why: primitives make arguments swappable without a compile error; newtypes
turn misuse into type errors and carry validation at construction.

## Conversions

### C8 — `From` is deliberate

`From`/`Into` impls onto an error type exist only where the conversion is
total and specific to one boundary. No blanket `From<io::Error>` (or
similar) that erases which operation failed.
*Enforcement: review.*

```rust
// wrong
impl From<io::Error> for Error { ... }  // every `?` now loses its context
// right
fs::write(&path, data).map_err(|e| Error::Session { path, source: e })?
```

Why: `?`-through-`From` is invisible control flow; per-site wrapping keeps
the failure's context — which file, which stage — in the variant.

## Surface and duplication

### C9 — `pub` earns its place

An item `pub` at the crate boundary has a consumer outside the crate or a
documented external contract; internal-only use takes narrower visibility
(`pub(crate)` or private). Speculative API surface is a defect. `pub` items
inside private modules are rustc's dead-code jurisdiction, not this rule's.
*Enforcement: review (rustc's dead-code lint stops at the crate-boundary
`pub`; unused public surface accretes silently).*

```rust
// wrong
pub fn with_env(...)   // no caller anywhere; "might be useful later"
// right
fn with_env(...)       // private until something needs it — or deleted
```

Why: models measurably over-produce surface; every speculative `pub` is API
contract nobody asked for and review burden on every future change.

### C10 — No near-duplicate types

Before defining a type, extend or reuse an existing type whose meaning
overlaps. Two types with overlapping fields and purpose are a defect unless
the divergence is documented where the second type is defined. This applies
equally to a recurring unnamed shape — a tuple used in multiple field
positions is a type that never got a name; name it.
*Enforcement: review.*

```rust
// wrong
pub struct RunConfig { command: Vec<String>, timeout: Duration }
// RunSpec already models this — extend it, or document why this must diverge
```

Why: redundant code is the worst-measured category of model output relative
to human baseline; parallel types fork behavior later and double the cost of
every change.

## Ownership and allocation

### C11 — Clones carry semantics

A `.clone()` exists only where an independent copy is semantically required.
A clone whose purpose is to end a borrow-checker fight is a design defect —
restructure ownership instead.
*Enforcement: review (`clippy::redundant_clone` is nursery-tier and cannot
judge intent).*

```rust
// wrong
process(config.path.clone());   // process() only reads the path
// right
process(&config.path);          // fn process(path: &Path)
```

Why: the canonical model escape hatch around ownership — it compiles
cleanly, so only review can see that ownership was routed around rather
than designed.

### C12 — Shared mutability is justified, never default

Introducing `Rc`, `Arc`, `Mutex`, or `RwLock` requires a stated reason, at
the introduction site, why single ownership or message passing does not fit.
*Enforcement: review.*

```rust
// wrong
state: Arc<Mutex<Sessions>>,   // no concurrency exists here
// right
state: Sessions,               // single owner — or, where sharing is real:
/// Shared with the reaper thread, which marks sessions dead (B9).
state: Arc<Mutex<Sessions>>,
```

Why: sharing is the sibling escape hatch to C11; it silently taxes
performance and design, so the reach must be loud and reviewable.

### C13 — Write through, don't buffer up

Where the destination already accepts incremental writing (`write!` to a
formatter, an `io::Write`, an iterator draining to a sink), building an
intermediate `String` or `Vec` first is a defect. Building a collection
where no sink exists is fine, and a transformation that inherently requires
the whole collection (e.g. reversal) may buffer once — the defect is
buffering that a streaming write could replace.
*Enforcement: gate, partial (pedantic allocation lints under `-D warnings`
— live via `scripts/check.sh`); review for design-level cases.*

```rust
// wrong
let s = format!("run {} failed", self.id);
f.write_str(&s)
// right
write!(f, "run {} failed", self.id)
```

Why: buffering where a sink exists doubles allocation for nothing — the
design-level remainder of the measured idiomaticity gap that lints can't
fully see.

## Structure and signatures

### C14 — Load-bearing literals are named

A literal whose value carries meaning — a default, a limit, a poll
interval, a fallback mode — is a named `const` with a comment stating why
that value; the same meaningful value never appears as an unnamed literal
in more than one place.
*Enforcement: review.*

```rust
// wrong
timeout: Duration::from_secs(300)   // and `default_value_t = 300` in the CLI
// right
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);
```

Why: an unnamed value cannot be checked against intent, and the same
default written twice drifts independently.

### C15 — No load-bearing `bool` parameters

A `bool` parameter whose value changes semantics or security is a
two-variant enum; call sites must read as self-describing.
*Enforcement: review.*

```rust
// wrong
mount_arg(&worktree, GUEST_LOWER, true)             // true… what?
// right
mount_arg(&worktree, GUEST_LOWER, Mount::ReadOnly)
```

Why: a transposed bare `true` compiles; when the flag is a security
property (a read-only mount), that typo is a boundary breach.

### C16 — Functions hold one altitude

A function operates at one level of abstraction: it either orchestrates
steps or implements one. A function interleaving separable concerns
(assembly, supervision, classification) is split at those seams. Length is
a signal, not the rule.
*Enforcement: review.*

Why: agent-generated code degrades locally-plausibly — each added concern
looks right in isolation, and a multi-altitude function is exactly where a
later edit drops a load-bearing flag without a reviewer noticing.
