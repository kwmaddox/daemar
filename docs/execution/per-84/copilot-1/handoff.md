# CP01-03 executor handoff

Implemented only the three surviving Copilot dispositions. No Card, GitHub,
commit, manifest, migration, acceptance-feature, browser-assertion, or public
API/error-variant changes were made.

CP01: `Reader::queue()` now uses a left join, decodes the nullable joined
`recorded_at` as `Option<OffsetDateTime>`, preserves decode/storage failures,
and maps a missing latest entry to `Error::Corrupt { context: Queue }`.
`queue_rejects_card_without_entries` proves a retained Card identity is not
silently dropped. The console regression removes entries only and proves both
root and Card requests return storage-failed 500 pages without queue/Card/
stream/inspector/payload content.

CP02: corrected the public `ErrorCategory` taxonomy docs to five categories and
documented `unavailable` in the CLI module description. Runtime mappings are
unchanged.

CP03: added a private deterministic router fixture port and derived valid and
foreign Host authorities in `console::web::tests`; the bound-listener tests and
their exact display spellings remain untouched.

Verification: `just check`, `just browser`, browser-local TypeScript, strict
Clippy, formatting, diff check, full library tests, full card binary tests, and
the named queue/console regressions are green. Initial ordinary-sandbox
loopback failures are preserved in evidence; elevated reruns are green.

Owned source hashes and every command/cwd/exit are in
`evidence/01/source-hashes.txt` and `evidence/01/command-status.md`.

## C1-C16 conformance checklist

C1-C8: unchanged public error boundaries and typed error payloads; no new
foreign errors, helper crates, string-only error payloads, panics in production,
suppression, string dispatch, primitive public domain values, or blanket
conversions.

C9-C16: no new public items or API surface; existing names and visibility are
preserved; the SQL remains one ordered query and the web fixture helper is
private; no production fixture or schema refactor was introduced. Formatting,
strict Clippy, and repository gate provide the applicable automated evidence;
the remaining convention sites are unchanged existing code.
