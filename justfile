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

# What CI will eventually enforce: tests, lints, formatting.
check:
    cargo test -q
    cargo clippy --all-targets -- -D warnings
    cargo fmt --check

# Watch the ledgers directory raw — every event as it lands.
tail:
    tail -F ledgers/*.jsonl
