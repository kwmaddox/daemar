#!/bin/sh
# The repo's check entrypoint: one command that runs every gate. Gates
# accrete here as the quality stack lands (PER-68: ast-grep pending);
# agents and CI point at this script, never at individual tools.
# Order is cheapest-first: fmt, clippy (L0 lint gate, PER-66 — policy in
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
cargo clippy --all-targets --all-features -- -D warnings
cargo deny check
cargo test --workspace
