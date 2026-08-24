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
