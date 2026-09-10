# CP01-03 command evidence

All commands ran on 2026-09-10 from `/Users/kendall/code/github/daemar` unless noted.
Each log contains the complete command output; the initial socket failures are
retained rather than rewritten.

| Log | Command | Exit |
| --- | --- | ---: |
| 01-queue.log | `cargo test --locked -p daemar-card --lib queue_` | 0 |
| 02-console.log | `cargo test --locked -p daemar-card --lib console::` (ordinary sandbox) | 101 (socket permission) |
| 03-lib.log | `cargo test --locked -p daemar-card --lib` (ordinary sandbox) | 101 (socket permission) |
| 04-bin.log | `cargo test --locked -p daemar-card --bin card` (ordinary sandbox) | 101 (socket permission) |
| 05-lib-escalated.log | `cargo test --locked -p daemar-card --lib` | 0 |
| 06-bin-escalated.log | `cargo test --locked -p daemar-card --bin card` | 0 |
| 07-fmtcheck.log | `cargo fmt --all -- --check` | 0 |
| 08-clippy.log | strict all-targets Clippy (first attempt) | 101 (test indexing) |
| 09-fmtcheck-final.log | `cargo fmt --all -- --check` | 0 |
| 10-clippy-final.log | strict all-targets Clippy | 0 |
| 11-diffcheck.log | `git diff --check` | 0 |
| 12-just-check.log | `just check` | 0 |
| 13-just-browser.log | `just browser` | 0 |
| 14-browser-tsc.log | `./node_modules/.bin/tsc --noEmit` from `browser/` | 0 |

The first TypeScript logging attempt used the wrong relative log path from
`browser/`; it produced no product output and was rerun successfully with the
absolute evidence path.
