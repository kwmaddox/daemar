# Task 4 final-gate repair: console ownership transfer

Fresh Luna executor. Root transfers ownership of crates/daemar-card/src/console.rs, private src/console/ modules, templates/ and static/console.css for final-gate-driven repairs. Main.rs/private binary tests are also available if the same execution repair requires them. No changes to storage/domain/error APIs, protected external tests, migrations or policy. Narrow manifest/lock changes only if already permitted by the approved plan; otherwise report need.

Read AGENTS.md, CONTEXT.md, conventions.md, active-dispatch.md, contract.md, current approved plan Tasks3/4 and final barrier. Predecessor run2-task4-handoff.md reports ordinary failing acceptance commands and required ownership transfer. Evidence run2-task4-just-check.txt (104/119 pass,15fail) and run2-task4-browser.txt (9/11 pass,2fail); TypeScript passes. Diagnose and repair these command failures yourself within scope. Root supplies no semantic findings or implementation suggestions. All settled product scope and acceptance assertions stay intact.

Continue the implementation/test/static/conventions check-fix loop until unfiltered just check, just browser and browser local TypeScript are all green. Preserve existing tests; add meaningful focused regression tests before corresponding fixes, capture actual results, keep initially passing cases and historical evidence honest. No artificial red or retroactive TDD claims. Read TDD skill/references as directed by active dispatch. All normal repairs within this ownership transfer proceed without another approval.

Save actual complete command outputs/cwd/exits and candidate/evidence hashes as run2-final-repair-*; poll running commands to real exit; use necessary sandbox permissions and documented browser prerequisites. No independent review, other agents, commits, accepted-test changes, policy relaxation or public API drift. Genuine missing-authority conflicts use Card append-and-stop protocol.

At final all-green and completed executor checklist, perform the plan-required executor-authored execution-completion Card append using the authoritative DB/Card in active-dispatch.md. Record returned sequence/entry ID and return run2-final-repair-handoff.md. Root verifies command results and routes the review-ready candidate; it does not perform model review.

