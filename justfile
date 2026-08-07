# daemar recipes. `just` lists them.

# Watches ONLY source — never ledgers/, or every flight's writes would
# restart the server mid-flight. The page survives restarts fine:
# EventSource reconnects itself and the 5s tick backfills.
# The board with auto-restart on source changes.
dev:
    watchexec --restart --watch crates --watch apps --watch Cargo.toml --exts rs,toml -- cargo run -p board

# The board, plain.
board:
    cargo run -p board

# Fly a request: just fly "add a /health endpoint"
fly request:
    cargo run -q -p daemar -- "{{request}}"

# The suite.
test:
    cargo test

# The eval rig: PAID live-model flights against pinned territories. Choose
# the airframe exactly like production: DAEMAR_SCOUT_MODEL=... just eval
eval *args:
    cargo run -q -p daemar-eval -- run {{args}}

# Objective deltas between two dossiers; never a subjective winner.
eval-compare left right:
    cargo run -q -p daemar-eval -- compare {{left}} {{right}}

# The wall proofs. Docker's one remaining role: it BUILDS the executor
# image and hands it to microsandbox, which is the wall that runs it.
# Requires hardware virtualization (KVM on Linux). CI runs this
# explicitly; ordinary `just test` stays runtime-free.
wall:
    docker build -f Dockerfile.cage -t daemar-cage:latest .
    docker save daemar-cage:latest -o target/daemar-cage.tar
    msb load -i target/daemar-cage.tar -t daemar-cage:latest
    msb pull ubuntu
    cargo test -p daemar --test microvm -- --ignored

# What CI will eventually enforce: tests, lints, formatting.
check:
    cargo test -q
    cargo clippy --all-targets -- -D warnings
    cargo fmt --check

# Watch the ledgers directory raw — every event as it lands.
tail:
    tail -F ledgers/*.jsonl
