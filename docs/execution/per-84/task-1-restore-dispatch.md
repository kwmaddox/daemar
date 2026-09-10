# PER-84 Task 1 restoration dispatch

Operator approved archiving rejected Task1 and restarting from saved baseline, retaining approved BSD-3-Clause allowance. This is RESTORATION ONLY, no new implementation. Read AGENTS.md and this packet. Current working directory /Users/kendall/code/github/daemar. Use apply_patch for all source edits. Fresh Luna agent owns exactly these targets: crates/daemar-card/src/domain.rs, crates/daemar-card/src/error.rs, crates/daemar-card/src/lib.rs, crates/daemar-card/src/console.rs, crates/daemar-card/Cargo.toml, Cargo.lock.

Approved baseline archive /private/tmp/per84-execution-GGJtBp/admission.tar.gz, hash bbfd967bad23f986aec41ccbeb8370d3d035ce892b4158680c4454ab2c2a7ae6. Read individual original files via tar -xOf, restoring only domain.rs/error.rs/lib.rs/crate Cargo.toml/Cargo.lock to their exact archive contents with apply_patch. Archive includes pre-existing user manifest/lock edits: DO NOT use git HEAD for those files. console.rs was absent at admission and is solely rejected executor output: delete exactly crates/daemar-card/src/console.rs via apply_patch. Its contents are recoverable from /private/tmp/per84-execution-GGJtBp/failed-task-1.tar.gz. Do not restore any other file from the archive. Especially preserve deny.toml (license allowance), AGENTS.md, all tests/browser/, agent configurations, justfile, .gitignore, plan and evidence docs. No git reset/checkout, no broad deletion, no commits.

Before touching any target verify current hashes match these recorded failed-attempt hashes; if any differs STOP and report overlap, without editing:
f6fef980e321f1ab3dbe4e5cc74973db6faedf94e543e8ae250d28f863e59180 crates/daemar-card/src/domain.rs
458f0c93ffc853f8c00b1a1f6aeb2e87cac97b54c13daf712b679df3b7f13530 crates/daemar-card/src/error.rs
45407b9bbfa777a4f662c3bce4b03b30b43c598ee1cef37c791054a54f49e180 crates/daemar-card/src/lib.rs
867633462215314f3fc4e84345e0520403fd32bfdfc9e523c9bfa27a310f2018 crates/daemar-card/src/console.rs
40cf1bcf5e5d1f995b5611a7dd90e4a71f7bc20c03ba3b6e64fb0307daecf141 crates/daemar-card/Cargo.toml
11e58bb5ee865d2d8c7022e6e52ff580fa430dc6fd459640cf780dd1429b30cd Cargo.lock

After restoration compare each restored file byte-for-byte to archive content, confirm console.rs absent, deny.toml hash still 1dc7f379b94a00de399bc7d4dd8df21cad2290be2cdba2e981a6d41562e62d9d, git diff --check, and cargo test --locked --offline -p daemar-card --lib (baseline31). Report results in docs/execution/per-84/task-1-restore-handoff.md via apply_patch. No other edits, Card writes, reviews or agents. Return done/blocked and exact verification. Root is recording recovery authority and preparing the fresh executor packet concurrently. Stop after restoration.

