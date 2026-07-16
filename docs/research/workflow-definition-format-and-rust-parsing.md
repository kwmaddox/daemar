# Workflow Definition document format and Rust parsing stack

Date: 2026-07-15
Status: Recommendation

## Decision

Use **TOML 1.1.0** for Workflow Definition documents and the maintained Rust
`toml` 1.1 + Serde stack for parsing. A Workflow Definition is data only: it
contains a mandatory integer `schema-version` and an ordered `[[stages]]` array
of closed, internally tagged stage variants. It does not contain expressions,
templates, includes, callbacks, shell fragments, or an extension language.

The runtime stack is:

- `toml = "1.1"` with `default-features = false` and the `std`, `parse`, and
  `serde` features;
- `serde = "1"` with `derive`;
- `serde_path_to_error = "0.1"` for the failing field/index path;
- `miette = "7"` (and `thiserror = "2"` for the error type) for a named source,
  source label, error code, and help text.

The schema-tooling stack is `schemars = "1"` plus `serde_json = "1"` to emit a
JSON Schema artifact from the same Rust input types. JSON Schema is an editor,
documentation, and review aid; it is not the runtime validation authority.

The current `toml` documentation identifies the 1.1 line as implementing TOML
1.1.0 and exposing Serde deserialization, and the latest API documentation at
the time of this decision is `1.1.3+spec-1.1.0`.[^toml-crate] TOML arrays and
arrays of tables encode order directly, so a `Vec<StageDefinitionV1>` preserves
the Workflow's stage sequence without assigning meaning to table-key order.[^toml-spec]

An illustrative document shape is:

```toml
schema-version = 1
name = "bounded-change"

[[stages]]
type = "generation"
id = "generate"
max-iterations = 8
max-tokens = 120000

[[stages]]
type = "validation"
id = "validate"
tools = ["format", "lint"]
```

The stage names and fields above illustrate the representation; the compiled
stage registry owns the actual closed set and its configuration types.

## Typed representation and loading contract

Define a distinct raw Rust input type per document schema version. The V1 shape
should be equivalent to:

```rust
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct WorkflowDefinitionV1 {
    schema_version: u32,
    name: String,
    stages: Vec<StageDefinitionV1>,
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum StageDefinitionV1 {
    Generation(GenerationStageDefinitionV1),
    Validation(ValidationStageDefinitionV1),
    // The rest of the compiled V1 registry.
}

#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct GenerationStageDefinitionV1 {
    id: String,
    max_iterations: u32,
    max_tokens: u64,
}
```

Every top-level and stage-configuration struct must use
`deny_unknown_fields`. Serde documents that this turns unknown fields into
deserialization errors; without it, self-describing formats normally ignore
them.[^serde-container] Do not use `#[serde(flatten)]`: Serde explicitly does
not support it together with `deny_unknown_fields`.[^serde-flatten] Use an
internally tagged enum because the `type` field selects a closed compiled stage
variant while retaining ordinary named configuration fields; Serde supports
this representation for unit variants and newtype variants containing structs
or maps, but not tuple variants.[^serde-enums]

Load in this order:

1. Read at most `MAX_WORKFLOW_DEFINITION_BYTES + 1` bytes and reject an input
   larger than **256 KiB** before parsing. Validate UTF-8.
2. Parse a minimal
   `VersionProbe { #[serde(rename = "schema-version")] schema_version: u32 }`.
   This one probe type intentionally allows other fields; it selects a decoder
   but does not create a runnable Workflow Definition.
3. Reject `0` and every unsupported version. For `1`, parse the original source
   again directly into `WorkflowDefinitionV1` through
   `toml::Deserializer::parse` and `serde_path_to_error::deserialize`. Do not
   parse through `toml::Value`, an open-ended map, or an untagged enum.
4. Convert the raw V1 value with `TryFrom` into the executable, validated
   Workflow Definition. Require `schema-version == 1`, require **1 through 64
   stages**, require unique stage IDs, validate every numeric/string/collection
   bound, and validate cross-stage invariants before a Workflow Run can exist.
   Every newly introduced collection or resource field must receive an explicit
   semantic maximum in this step.
5. Resolve every `type` against the V1 closed registry. An unknown type is an
   error, never a plugin lookup or deferred interpretation.

The two typed parses are intentional. They preserve source-aware errors from
the full V1 deserializer and ensure an unsupported future document is never
partly interpreted as V1. The 256 KiB pre-parse bound makes the extra parse
bounded.

## Strictness and duplicate keys

Duplicate-key rejection comes from TOML syntax, not from post-parse map
behavior. TOML 1.1.0 says defining a key multiple times is invalid and also
constrains table redefinition.[^toml-spec] The `toml` parser therefore sees and
rejects duplicates before Serde could collapse them. Add conformance tests for
duplicate top-level keys, duplicate keys inside one stage, dotted-key/table
redefinition, and duplicate `type` keys. Keep duplicate stage IDs as a separate
semantic error: they are equal *values in different tables*, not duplicate TOML
keys.

Required strictness tests are:

- unknown top-level field;
- unknown field in every stage variant;
- missing `schema-version`, missing required fields, and unsupported versions;
- unknown stage `type` and a stage whose fields belong to another variant;
- every duplicate-key form above;
- zero stages, more than 64 stages, duplicate stage IDs, and each configured
  resource bound;
- an input of exactly 256 KiB, one byte over it, and nesting beyond the parser
  limit.

Do not enable the `toml` crate's `unbounded` feature. In the current parser
source the normal document parser is wrapped in a `RecursionGuard` with a depth
limit of 80; the crate's feature metadata warns that `unbounded` permits
arbitrarily deep structures without stack-overflow protection.[^toml-depth][^toml-features]
That depth is **parser-enforced but is an upstream implementation detail**, not
a Daemar-configurable guarantee. Pin resolved versions in `Cargo.lock`, test
that deeply nested input is rejected, and re-audit this behavior on parser
upgrades.

Daemar, not the TOML parser, enforces the 256 KiB source limit, 1..=64 stage
count, unique stage IDs, field/collection maxima, and Workflow resource-policy
invariants. The parser's depth guard does not replace those limits, and the
source-size cap does not prove semantic validity.

## Diagnostics

Return one structured diagnostic containing:

- the input path as a named source;
- a stable Daemar code such as `workflow-definition::syntax`,
  `workflow-definition::unknown-version`, or
  `workflow-definition::semantic`;
- the Serde field/index path when available;
- the parser's byte span when available;
- a concise remediation message.

`serde_path_to_error` wraps any Serde deserializer and records the chain of
field names to the failure.[^serde-path] `toml::de::Error` exposes the error
message and a byte range into the original document.[^toml-error] `miette`
renders a `SourceSpan` against a named source and supports labels and help
text.[^miette] Preserve both path and span when wrapping errors; neither is a
substitute for the other. Semantic validation should attach the span of the
relevant field where practical, but the initial implementation may report the
field path when a reliable span is unavailable. This is an important Serde
caveat: typed deserialization is excellent for paths, but arbitrary semantic
errors do not automatically retain every field's source span.

## Schema and compatibility contract

Generate a JSON Schema from each versioned raw input type with Schemars' **input
(Deserialize) contract**. Schemars supports Serde's `tag` and
`deny_unknown_fields`; the latter becomes `additionalProperties: false` in the
generated schema.[^schemars-derive] Pin the JSON Schema dialect explicitly
rather than accepting Schemars' default, because its documentation says the
default dialect may change. Snapshot the generated schema in tests and review
diffs on dependency upgrades; Schemars also states that the exact generated
schema structure may change without a semver-major release.[^schemars] Override
the generated V1 `schema-version` property to `const: 1`; the runtime
`TryFrom` check remains authoritative.

The versioning rules are:

- `schema-version` is mandatory and has no default.
- A decoder accepts only versions it explicitly implements. There is no
  best-effort forward compatibility.
- A shape or meaning change that would make an existing document deserialize
  or behave differently requires a new integer version and a new raw type.
- Additive fields within an existing version are allowed only when old and new
  readers have identical behavior. Because unknown fields are rejected, most
  additions should be treated as a version bump rather than silently ignored.
- Convert each accepted version explicitly into one internal validated model;
  migrations are code, are tested, and never rewrite the source implicitly.
- The TOML 1.1.0 text and versioned raw Rust types are normative. Generated
  JSON Schema, examples, and editor integration are derived aids.

This preserves strict old-reader behavior, makes upgrades reviewable, and keeps
the Workflow Definition declarative rather than growing compatibility
expressions into a DSL.

## Alternatives considered

### JSON

JSON has the strongest cross-language JSON Schema story and ordered arrays, but
it is a worse default for hand-maintained Workflow Definitions. It has no
comments, and RFC 8259 says object names only **SHOULD** be unique; it explicitly
records that implementations variously keep the last value, error, or expose
all duplicates.[^json-rfc] Meeting Daemar's duplicate-key requirement would
therefore need an additional duplicate-aware streaming pre-pass or a custom
deserializer, and every code path would have to avoid first materializing a
`serde_json::Value`. TOML makes duplicate keys invalid at the document-format
layer and remains readable for ordered stage tables.

### YAML

YAML is pleasant for nested configuration, but reject it for V1. The YAML 1.2.2
specification includes anchors, aliases, tags, schemas that resolve plain
scalars, and multi-document streams: more syntax than this closed typed format
needs.[^yaml-spec] More decisively for the requested
maintained stack, the canonical `serde_yaml` crate labels itself
`0.9.34+deprecated` and states that it is no longer maintained.[^serde-yaml]
Selecting a fork would make Daemar own a larger parser-behavior and resource
limit audit than TOML requires.

### RON

RON is maintained, integrates directly with Serde, and exposes a configurable
recursion limit. It is nevertheless Rust-specific, resembles Rust syntax, and
its own documentation describes limited support for internally tagged enums,
adjacently tagged enums, untagged enums, and flattened structs.[^ron] That is a
poor long-term public document contract for a Workflow Definition whose
discriminator and schema tooling should be understandable independently of the
Rust implementation.

## Consequences

TOML is not a complete validation system. Its parser rejects malformed syntax,
duplicate definitions, and excessive nesting; Serde rejects wrong types,
missing required fields, unknown fields, and unknown variants; Daemar must
still enforce stage count, uniqueness, numeric bounds, cross-stage ordering,
and resource policy. Keeping those layers explicit is the main safety property
of the selection.

TOML's table-key order is not part of Workflow semantics. Only the `[[stages]]`
array order is. The loader should never use `preserve_order` to infer meaning
from configuration-field order.

## Primary sources

[^toml-spec]: [TOML v1.1.0 specification](https://toml.io/en/v1.1.0), especially “Key/Value Pair,” “Array,” and “Array of Tables.”
[^toml-crate]: [`toml` 1.1.3+spec-1.1.0 API documentation](https://docs.rs/toml/1.1.3/toml/).
[^toml-depth]: [`toml` 1.1.3 document parser source: guarded parsing and `LIMIT`](https://docs.rs/toml/1.1.3/src/toml/de/parser/mod.rs.html).
[^toml-features]: [`toml` 1.1.3 feature metadata, including `unbounded`](https://docs.rs/crate/toml/1.1.3/source/Cargo.toml.orig).
[^toml-error]: [`toml::de::Error` API](https://docs.rs/toml/1.1.3/toml/de/struct.Error.html).
[^serde-container]: [Serde container attributes](https://serde.rs/container-attrs.html).
[^serde-flatten]: [Serde struct flattening](https://serde.rs/attr-flatten.html).
[^serde-enums]: [Serde enum representations](https://serde.rs/enum-representations.html).
[^serde-path]: [`serde_path_to_error` API documentation](https://docs.rs/serde_path_to_error/0.1.20/serde_path_to_error/).
[^miette]: [`miette` diagnostics and source-span documentation](https://docs.rs/miette/7.6.0/miette/).
[^schemars-derive]: [Schemars `JsonSchema` derive and supported Serde attributes](https://docs.rs/schemars/1.2.1/schemars/derive.JsonSchema.html).
[^schemars]: [Schemars schema generation and versioning notes](https://docs.rs/schemars/1.2.1/schemars/).
[^json-rfc]: [RFC 8259, JSON Data Interchange Format, section 4](https://www.rfc-editor.org/rfc/rfc8259.html#section-4).
[^yaml-spec]: [YAML 1.2.2 specification](https://yaml.org/spec/1.2.2/).
[^serde-yaml]: [`serde_yaml` 0.9.34+deprecated documentation](https://docs.rs/serde_yaml/0.9.34+deprecated/serde_yaml/).
[^ron]: [RON 0.12.2 documentation, specification and limitations](https://docs.rs/ron/0.12.2/ron/).
