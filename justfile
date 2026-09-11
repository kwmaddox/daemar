# justfile — task runner entry points. Bare `just` lists recipes.

set shell := ["bash", "-euo", "pipefail", "-c"]

[private]
default:
    @just --list

# Full quality gate. scripts/check.sh is the single source of truth; the
# pre-commit hook runs the same script, so green here means committable.
check:
    scripts/check.sh

# One-time per-clone setup: activate the versioned git hooks, then run the
# gate — bootstrap succeeded means hooks active AND workspace green.
bootstrap:
    git config core.hooksPath scripts/hooks
    scripts/check.sh

# Throwaway UI study for the Card console. Not production code.
prototype-card-console:
    cargo run --manifest-path prototypes/card-console/Cargo.toml

# Browser-observable proofs for the Card console (PER-84, S3): what the
# browser fetched, what the operator can see, where clicks land. Runs
# Playwright against the built `card` binary. Before acceptance, never in
# pre-commit; the Rust behavior suite stays the gate.
browser: node-preflight
    cargo build -p daemar-card --bin card
    cd browser && ./node_modules/.bin/playwright test

# One-time per-clone setup for `just browser`: Node packages and Chromium.
browser-install: node-preflight
    cd browser && npm ci && ./node_modules/.bin/playwright install chromium

# Shared preflight for browser recipes: node:sqlite is required by the
# browser proofs and is unflagged only in Node 22.13+ or 23.4+.
[private]
node-preflight:
    node -e 'try { const sqlite = require("node:sqlite"); if (typeof sqlite.DatabaseSync !== "function") throw new Error("DatabaseSync is unavailable"); } catch (error) { console.error("daemar browser tests require Node 22.13+ or 23.4+ with unflagged node:sqlite (DatabaseSync): " + error.message); process.exit(1); }'
