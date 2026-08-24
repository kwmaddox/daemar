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

installed="$(cargo deny --version 2>/dev/null | awk '{print $2}')" || {
    echo "check: cargo-deny is not installed" >&2
    echo "check: install with: cargo install cargo-deny --locked --version $CARGO_DENY_VERSION" >&2
    exit 1
}
if [ "$installed" != "$CARGO_DENY_VERSION" ]; then
    echo "check: cargo-deny $installed differs from pinned $CARGO_DENY_VERSION" >&2
    echo "check: install with: cargo install cargo-deny --locked --version $CARGO_DENY_VERSION" >&2
    exit 1
fi

cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo deny check
cargo test --workspace
