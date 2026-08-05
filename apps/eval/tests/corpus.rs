//! Corpus validation: every tracked case parses clean, and every answer key
//! holds AT ITS PINNED COMMIT — required and forbidden lines are read from
//! the Git object store (`git show`), so no worktree is materialized and no
//! generated state is touched.
//!
//! Needs full history (CI checks out with fetch-depth: 0). If a pinned
//! commit is missing locally, this fails loud — a shallow clone cannot
//! vouch for the corpus.

use std::path::PathBuf;

use daemar_eval::cases;
use daemar_eval::pin;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root resolves")
}

#[test]
fn the_tracked_corpus_parses_clean() {
    let (cases, errors) = cases::load_corpus(&repo_root().join("eval/cases"));
    assert!(errors.is_empty(), "corpus problems: {errors:?}");
    assert!(!cases.is_empty(), "the corpus must not be empty");
}

#[test]
fn every_answer_key_holds_at_its_pinned_commit() {
    let root = repo_root();
    let (cases, errors) = cases::load_corpus(&root.join("eval/cases"));
    assert!(errors.is_empty(), "corpus problems: {errors:?}");

    let mut failures: Vec<String> = Vec::new();
    for case in &cases {
        let repo = root.join(&case.territory_repo);
        let mut check = |path: &str, line: u64, verbatim: &str, kind: &str| match pin::pinned_line(
            &repo,
            &case.territory_commit,
            path,
            line,
        ) {
            Ok(Some(text)) if text.contains(verbatim) => {}
            Ok(Some(text)) => failures.push(format!(
                "{}: {kind} {path}:{line} is '{}' — expected it to contain '{verbatim}'",
                case.id,
                text.trim()
            )),
            Ok(None) => failures.push(format!(
                "{}: {kind} {path}:{line} is out of range at the pinned commit",
                case.id
            )),
            Err(error) => failures.push(format!(
                "{}: {kind} {path}:{line} unreadable at pin: {error}",
                case.id
            )),
        };
        for required in &case.required {
            check(
                &required.path,
                required.line,
                &required.verbatim,
                "required",
            );
        }
        for forbidden in &case.forbidden {
            check(
                &forbidden.path,
                forbidden.line,
                &forbidden.verbatim,
                "forbidden",
            );
        }
    }
    assert!(
        failures.is_empty(),
        "rotted answer keys:\n{}",
        failures.join("\n")
    );
}
