#!/bin/sh
# The repo's check entrypoint: one command that runs every gate. Gates
# accrete here as the quality stack lands; agents and CI point at this
# script, never at individual tools.
# Order is cheapest-first: fmt, ast-grep (L2 structural gate, PER-68 —
# rule pack in rules/ast-grep), clippy (L0 lint gate, PER-66 — policy in
# the workspace lint table and clippy.toml), cargo-deny (L1), tests.
set -eu

# Pinned tool versions. Bumps are deliberate, tested events — same policy
# as the container CLI pin (specs/sandbox.md operating rules).
CARGO_DENY_VERSION="0.20.2"
AST_GREP_VERSION="0.45.2"

require_pinned() {
    tool="$1"; want="$2"; got="$3"; crate="$4"
    if [ -z "$got" ]; then
        echo "check: $tool is not installed" >&2
        echo "check: install with: cargo install $crate --locked --version $want" >&2
        exit 1
    fi
    if [ "$got" != "$want" ]; then
        echo "check: $tool $got differs from pinned $want" >&2
        echo "check: install with: cargo install $crate --locked --version $want" >&2
        exit 1
    fi
}

require_pinned cargo-deny "$CARGO_DENY_VERSION" \
    "$(cargo deny --version 2>/dev/null | awk '{print $2}')" cargo-deny
require_pinned ast-grep "$AST_GREP_VERSION" \
    "$(ast-grep --version 2>/dev/null | awk '{print $2}')" ast-grep

cargo fmt --all --check
ast-grep test
ast-grep scan

# Second ignore-discipline pass, over a token-renamed copy. A *trailing*
# bare `// ast-grep-ignore` suppresses every finding on its own line —
# including ignore-discipline's — so `ast-grep scan` alone cannot flag it.
# Renaming the token disarms suppression entirely, making every ignore
# comment visible to the same rule file (its regexes accept both
# spellings, so the two passes cannot drift). Fast path: no file mentions
# the token, nothing is scanned.
for f in $(grep -rl 'ast-grep-ignore' crates --include='*.rs' 2>/dev/null || true); do
    if ! out=$(sed 's/ast-grep-ignore/AST-GREP-IGNORE-TOKEN/g' "$f" \
        | ast-grep scan --stdin --rule rules/ast-grep/rules/ignore-discipline.yml 2>&1); then
        echo "check: undisciplined ast-grep-ignore in $f (locations are STDIN-relative):" >&2
        echo "$out" >&2
        exit 1
    fi
done

cargo clippy --all-targets --all-features -- -D warnings
cargo deny check
cargo test --workspace
