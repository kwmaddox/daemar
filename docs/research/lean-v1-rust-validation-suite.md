# Lean V1 Rust validation suite

Research date: 2026-07-16

## Answer

Daemar V1 should require exactly three sequential Validation Operations, each in
its own Validation Stage:

1. formatting conformance with `rustfmt`;
2. compiler and default Clippy lint conformance, with warnings denied; and
3. the workspace's unit, integration, and documentation tests.

This is the smallest suite that independently rejects formatting drift, static
compiler/lint findings, and behavioral test failures. A separate `cargo check`
operation is redundant in V1: Clippy uses Cargo's check path, while the test
operation compiles executable test artifacts. Cargo also documents that
`cargo check` omits final code generation and therefore cannot find some errors
that a real build can find. [Clippy usage][clippy-usage]
[Cargo check][cargo-check]

The current pin should be Rust **1.97.1**. It is the stable point release
published on the research date, and it fixes a compiler miscompilation in
1.97.0. Do not use the moving `stable` channel for a publication gate.
[Rust 1.97.1 release][rust-1-97-1] Clippy's own CI guidance recommends using
Clippy from the same toolchain as the compiler and notes that new toolchain
versions can introduce new lints, which is another reason to advance this pin
only through an explicit repository change. [Clippy CI][clippy-ci]

## Pinned offline contract

Commit these inputs with the Rust bootstrap:

```toml
# rust-toolchain.toml
[toolchain]
channel = "1.97.1"
profile = "minimal"
components = ["clippy", "rustfmt"]
targets = ["aarch64-apple-darwin"]
```

Also commit `Cargo.lock` and a `rustfmt.toml` that explicitly sets both
`edition = "2024"` and `style_edition = "2024"`. Rustup toolchain files support
versioned channel pins, component and target declarations, and minimal profiles;
the explicit target and Apple Silicon CI runner keep the platform contract
visible without putting a host tuple in the toolchain file's `channel` field.
Rustup's documentation also recommends tracking `Cargo.lock` with a
release-pinned toolchain. [Rustup toolchains][rustup-toolchains]
[Rustup overrides][rustup-overrides] Rustfmt recommends an explicit style edition
so formatting does not depend on an inferred default.
[Rustfmt configuration][rustfmt]

Trusted setup must install that exact toolchain and its components before the
sandboxed Validation Stages begin. It must also make every locked dependency
needed for `aarch64-apple-darwin` locally available, for example with this
trusted, network-capable preparation command:

```text
cargo +1.97.1-aarch64-apple-darwin fetch --locked --target aarch64-apple-darwin --manifest-path Cargo.toml
```

`cargo fetch` is specifically intended to make locked dependencies available
for later offline commands. [Cargo fetch][cargo-fetch] Validation itself still
runs with network denied by the sandbox. The Cargo operations pass both
`--locked` and `--offline`: `--locked` rejects a missing or stale lock file, and
`--offline` forbids Cargo network access. [Cargo test manifest options][cargo-test]

If generated changes add a dependency that trusted setup did not fetch or make
`Cargo.lock` stale, V1 rejects the change. Automatically resolving or fetching
new dependencies after generation would reopen the network and trust design and
does not belong in this lean suite.

Daemar should spawn the following argument vectors directly from the dedicated
worktree root; these are not shell snippets for the model to run.

### 1. Formatting

```text
cargo +1.97.1-aarch64-apple-darwin fmt --all --manifest-path Cargo.toml -- --check
```

Success is process exit code `0`. Any completed `rustfmt` invocation with a
nonzero exit is a rejected validation result. `--check` is read-only, returns an
error exit when formatting would change, and prints the differences.
[Rustfmt check mode][rustfmt]

`rustfmt` has no stable structured diff output: its `json` and `checkstyle`
emitters are nightly-only. Daemar must retain stdout and stderr and normalize
the exit status; it must not make a stable V1 depend on the human diff format.
[Rustfmt emit modes][rustfmt]

### 2. Clippy

```text
cargo +1.97.1-aarch64-apple-darwin clippy --workspace --all-targets --all-features --target aarch64-apple-darwin --locked --offline --manifest-path Cargo.toml --message-format=json --color=never -- -D warnings
```

Success requires process exit code `0` and a terminal Cargo
`{"reason":"build-finished","success":true}` message. `--workspace` removes
Cargo's default-member ambiguity; `--all-targets --all-features` checks the one
combined V1 feature configuration across library, binary, test, benchmark, and
example targets without creating a feature or platform matrix. Clippy officially
recommends `-D warnings` for CI and documents that this makes both Clippy and
rustc warnings fail the command. [Clippy CI][clippy-ci]
[Clippy usage][clippy-usage]

Cargo's stable JSON-lines stream provides compiler messages, artifacts,
build-script results, and the terminal build result. Daemar should normalize
`compiler-message` records into diagnostics while retaining the original JSON
objects. Cargo warns that procedural macros and other tools can still write
non-JSON output, so the raw byte streams remain authoritative evidence and
unknown or non-JSON lines must be preserved rather than treated as parser
failures. [Cargo JSON messages][cargo-json]

### 3. Tests

```text
cargo +1.97.1-aarch64-apple-darwin test --workspace --all-features --target aarch64-apple-darwin --locked --offline --manifest-path Cargo.toml --no-fail-fast --message-format=json --color=never -- --test-threads=1 --color=never
```

Success is process exit code `0` after every selected test executable and
documentation test completes. `--no-fail-fast` makes Cargo continue after a
failed test executable so the retained evidence is useful; one test thread
reduces avoidable output and scheduling variation within each libtest harness.
It does not turn timing-, process-, or network-dependent tests into deterministic
tests, so such behavior remains a test-design defect. Cargo's default target
selection intentionally remains in force: it runs unit and integration tests,
builds examples, and runs library documentation tests. `--all-targets` is not
used here because it selects benches and examples explicitly and replaces that
default selection. [Cargo test target selection][cargo-test]

There is no V1 minimum test-count rule. Cargo success is the gate; the bootstrap
design is responsible for supplying meaningful tests.

`--message-format=json` structures the compilation portion only. Stable libtest
output remains human-oriented; JSON test output requires unstable options.
Cargo explicitly says test output may follow `build-finished` and that
test-specific JSON is experimental and nightly-only. Therefore Daemar must use
the final process exit as the authoritative test verdict, preserve the post-build
stdout/stderr, and treat parsed test names or counts as optional display data,
not gate inputs. [Cargo JSON test boundary][cargo-json]
[Rust test output formats][rust-test-output]

## Daemar normalization boundary

Every completed operation should produce the same typed envelope regardless of
the tool's native output:

- operation kind and ordinal;
- exact argv as an array, worktree root, target triple, and resolved tool
  versions;
- start time, end time, duration, exit code or terminating signal;
- verdict: `passed` or `rejected`;
- normalized Cargo/rustc diagnostics where stable JSON exists;
- references to losslessly retained stdout and stderr, plus any truncation or
  capture limit metadata.

The wrapper should accept known Cargo JSON objects by their `reason`, preserve
unknown objects for forward compatibility, and preserve all non-JSON lines.
Formatting and test-harness text should not be scraped into publication-gating
facts.

Failure to spawn the pinned toolchain, a timeout, sandbox termination, inability
to capture required evidence, or violation of a trusted-setup invariant is a
typed Validation Operation error: the operation did not complete. A tool that
does complete but rejects formatting, compilation, lints, the lock state, or
tests returns a typed negative validation output. This preserves the Workflow's
existing distinction between expected negative outcomes and inability to
execute an operation.

Publication eligibility is the conjunction of the three typed verdicts in
order: formatting passed, then Clippy passed, then tests passed. Any rejection
or operation error stops the Workflow before commit, push, or draft-PR creation.

## Valuable deferred candidates

Keep these outside the V1 gate until a concrete risk or release contract makes
one necessary:

- a separate `cargo check` stage (currently duplicate signal);
- release-profile build/link validation;
- documentation generation with warnings denied;
- feature-combination, minimum-supported-Rust-version, cross-target, or
  multi-platform matrices;
- advisory, license, and dependency-policy scanners;
- coverage thresholds, fuzzing, mutation testing, interpreters, sanitizers, and
  alternative test runners.

`cargo fix` and formatter write mode are not deferred validators: they mutate
the generated change and therefore contradict V1's validate-without-repair
boundary. Nightly rustfmt JSON and nightly libtest JSON are also not candidates
for V1 because they would replace the stable pinned-toolchain premise with
experimental output contracts.

## Sources

All sources are first-party Rust project documentation or source repositories,
accessed 2026-07-16.

- [Announcing Rust 1.97.1][rust-1-97-1]
- [Rustup: Toolchains][rustup-toolchains]
- [Rustup: Overrides and `rust-toolchain.toml`][rustup-overrides]
- [Cargo: `cargo fetch`][cargo-fetch]
- [Cargo: `cargo check`][cargo-check]
- [Cargo: `cargo test`][cargo-test]
- [Cargo: External tools and JSON messages][cargo-json]
- [Clippy: Usage][clippy-usage]
- [Clippy: Continuous Integration][clippy-ci]
- [Rustfmt repository documentation][rustfmt]
- [The rustc book: test output formats][rust-test-output]

[rust-1-97-1]: https://blog.rust-lang.org/2026/07/16/Rust-1.97.1/
[rustup-toolchains]: https://rust-lang.github.io/rustup/concepts/toolchains.html
[rustup-overrides]: https://rust-lang.github.io/rustup/overrides.html
[cargo-fetch]: https://doc.rust-lang.org/cargo/commands/cargo-fetch.html
[cargo-check]: https://doc.rust-lang.org/cargo/commands/cargo-check.html
[cargo-test]: https://doc.rust-lang.org/cargo/commands/cargo-test.html
[cargo-json]: https://doc.rust-lang.org/cargo/reference/external-tools.html
[clippy-usage]: https://doc.rust-lang.org/clippy/usage.html
[clippy-ci]: https://doc.rust-lang.org/clippy/continuous_integration/index.html
[rustfmt]: https://github.com/rust-lang/rustfmt
[rust-test-output]: https://doc.rust-lang.org/rustc/tests/index.html
