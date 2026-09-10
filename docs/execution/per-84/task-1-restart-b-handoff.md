# PER-84 Task 1 restart checkpoint B handoff

Status: **STOPPED at tests-only compile-red checkpoint.**

Added only crate-local tests and evidence files. No implementation bodies,
approved API declarations, dependencies, Card writes, commits, or external
test/browser/migration changes were made.

Tests added at the approved public seams:

- `listener_zero_reports_bound_port`
- `occupied_port_returns_bind_error_without_retry`
- `startup_uses_bound_listener`
- `loopback_address_formats_both_families`
- `config_source_has_contract_spellings`
- `publish_writes_one_line_and_flushes`
- `publish_handles_partial_writes`
- `publish_write_failure_preserves_source`
- `publish_flush_failure_preserves_source`
- `publish_startup_error_category_and_display`
- `migration_version_and_schema_reason_displays_are_stable`
- `new_error_categories_displays_and_sources_are_typed`

The publication tests use a real ephemeral loopback listener for `Startup`
and deterministic `io::Write` fakes for partial writes, write failures, and
flush failures. Assertions cover exact JSON line bytes, exactly one newline,
observable flush, preserved error source text, both address families, all four
`ConfigSource` spellings, full startup JSON fields, and approved error/domain
mappings.

Exact focused command, cwd, exit status, and complete compiler diagnostics are
preserved in `task-1-restart-b-test.txt`. The command was:

```text
cargo test --locked -p daemar-card --lib listener_zero_reports_bound_port
```

It exited `101` at compilation because the approved Task 1 APIs and variants
are still absent (`Listener`, `LoopbackAddr`, `Port`, `Startup`,
`ConfigSource`, `publish_startup`, `MigrationVersion`,
`SchemaIncompatibility`, `DatabaseMissing`, `SchemaIncompatible`, `Bind`,
`PublishStartup`, `Serve`, `EntryNotFound`, `Unavailable`, and new storage
contexts). This is honest approved-API compile-red under ruling 66; no test
body ran and no behavioral red or green claim is made.

Source hashes for the tested files are in `task-1-restart-b-hashes.txt`.
`git diff --check` passed. Stop here for root validation and the next fresh
implementation executor.

