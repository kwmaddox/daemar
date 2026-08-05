//! Eval cases: the authored fixture corpus. One TOML file per scenario —
//! stable id, explicit expectations, room for comments explaining intent
//! (the moghedien lesson: fixture prose stays honest and adversarial).
//!
//! Parsing is the serde edge. Every problem in a file is reported at once:
//! a fixture author fixes one round-trip, not one field per run.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

pub const CASE_SCHEMA: &str = "daemar.eval-case.v1";

/// A required citation: the report must cite this path at (or spanning) this
/// line, and the PINNED line must still contain `verbatim` — the answer
/// key's own rot guard.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredCitation {
    pub path: String,
    pub line: u64,
    pub verbatim: String,
}

/// Bait: a tempting occurrence the report must NOT cite. The reason is part
/// of the fixture — future readers must know why it tempts.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForbiddenCitation {
    pub path: String,
    pub line: u64,
    pub verbatim: String,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredText {
    pub contains: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanReview {
    #[serde(default)]
    pub questions: Vec<String>,
}

/// The wire form: strings cross here, are validated once, and become an
/// `EvalCase` or a list of problems.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireCase {
    schema: String,
    id: String,
    #[serde(default)]
    class: Vec<String>,
    role: String,
    workflow: String,
    request: String,
    /// The repository the pinned territory is cut from, resolved against
    /// the eval suite's own repo root ("." = this repo).
    territory_repo: String,
    /// Full 40-hex commit SHA. Short specs are refused: the pin must be
    /// exact or the answer keys rot invisibly.
    territory_commit: String,
    #[serde(default)]
    required: Vec<RequiredCitation>,
    #[serde(default)]
    forbidden: Vec<ForbiddenCitation>,
    #[serde(default)]
    required_text: Vec<RequiredText>,
    #[serde(default)]
    human_review: HumanReview,
}

#[derive(Debug, Clone)]
pub struct EvalCase {
    pub id: String,
    pub class: Vec<String>,
    pub role: String,
    pub workflow: String,
    pub request: String,
    pub territory_repo: String,
    pub territory_commit: String,
    pub required: Vec<RequiredCitation>,
    pub forbidden: Vec<ForbiddenCitation>,
    pub required_text: Vec<RequiredText>,
    pub human_review: Vec<String>,
    /// Content hash of the fixture file bytes: the dossier's identity for
    /// "which version of this case flew".
    pub fixture_hash: String,
    pub path: PathBuf,
}

#[derive(Debug)]
pub enum CaseError {
    Io {
        path: PathBuf,
        detail: String,
    },
    Parse {
        path: PathBuf,
        detail: String,
    },
    /// Every validation problem in one file, together.
    Invalid {
        path: PathBuf,
        problems: Vec<String>,
    },
    DuplicateId {
        id: String,
        first: PathBuf,
        second: PathBuf,
    },
}

impl fmt::Display for CaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaseError::Io { path, detail } => write!(f, "{}: {detail}", path.display()),
            CaseError::Parse { path, detail } => write!(f, "{}: {detail}", path.display()),
            CaseError::Invalid { path, problems } => {
                write!(f, "{}: {}", path.display(), problems.join("; "))
            }
            CaseError::DuplicateId { id, first, second } => write!(
                f,
                "duplicate case id '{id}' in {} and {}",
                first.display(),
                second.display()
            ),
        }
    }
}

/// Any `..` component escapes whatever root the path is joined to.
pub(crate) fn traverses_up(path: &str) -> bool {
    Path::new(path)
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
}

fn is_full_sha(s: &str) -> bool {
    s.len() == 40
        && s.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn validate(wire: &WireCase) -> Vec<String> {
    let mut problems = Vec::new();
    if wire.schema != CASE_SCHEMA {
        problems.push(format!(
            "schema is '{}', expected '{CASE_SCHEMA}'",
            wire.schema
        ));
    }
    if wire.id.trim().is_empty() {
        problems.push("id is empty".to_string());
    }
    if wire.role != "scout" {
        problems.push(format!(
            "role '{}' is not executable in v1 (only 'scout')",
            wire.role
        ));
    }
    if wire.workflow != "scout" {
        problems.push(format!(
            "workflow '{}' is not executable in v1 (only 'scout')",
            wire.workflow
        ));
    }
    if wire.request.trim().is_empty() {
        problems.push("request is empty".to_string());
    }
    if wire.territory_repo.trim().is_empty() {
        problems.push("territory_repo is empty".to_string());
    } else if Path::new(&wire.territory_repo).is_absolute() || traverses_up(&wire.territory_repo) {
        problems.push(format!(
            "territory_repo '{}' must stay inside this repo (relative, no '..')",
            wire.territory_repo
        ));
    }
    if !is_full_sha(&wire.territory_commit) {
        problems.push(format!(
            "territory_commit '{}' is not a full 40-hex lowercase SHA",
            wire.territory_commit
        ));
    }
    if wire.required.is_empty() && wire.forbidden.is_empty() && wire.required_text.is_empty() {
        problems.push("case has no expectations (required/forbidden/required_text)".to_string());
    }
    for (index, r) in wire.required.iter().enumerate() {
        expect_citation(
            &mut problems,
            "required",
            index,
            &r.path,
            r.line,
            &r.verbatim,
        );
    }
    for (index, b) in wire.forbidden.iter().enumerate() {
        expect_citation(
            &mut problems,
            "forbidden",
            index,
            &b.path,
            b.line,
            &b.verbatim,
        );
        if b.reason.trim().is_empty() {
            problems.push(format!("forbidden[{index}] has no reason"));
        }
    }
    for (index, t) in wire.required_text.iter().enumerate() {
        if t.contains.trim().is_empty() {
            problems.push(format!("required_text[{index}].contains is empty"));
        }
    }
    problems
}

fn expect_citation(
    problems: &mut Vec<String>,
    kind: &str,
    index: usize,
    path: &str,
    line: u64,
    verbatim: &str,
) {
    if path.trim().is_empty() {
        problems.push(format!("{kind}[{index}].path is empty"));
    } else if path.starts_with('/') {
        problems.push(format!("{kind}[{index}].path '{path}' must be relative"));
    } else if traverses_up(path) {
        problems.push(format!(
            "{kind}[{index}].path '{path}' must not contain '..' — keys stay inside the pin"
        ));
    }
    if line == 0 {
        problems.push(format!("{kind}[{index}].line must be 1-based"));
    }
    if verbatim.trim().is_empty() {
        problems.push(format!("{kind}[{index}].verbatim is empty"));
    }
}

pub fn load_case(path: &Path) -> Result<EvalCase, CaseError> {
    let bytes = fs::read(path).map_err(|e| CaseError::Io {
        path: path.to_path_buf(),
        detail: e.to_string(),
    })?;
    let text = String::from_utf8_lossy(&bytes);
    let wire: WireCase = toml::from_str(&text).map_err(|e| CaseError::Parse {
        path: path.to_path_buf(),
        detail: e.to_string(),
    })?;
    let problems = validate(&wire);
    if !problems.is_empty() {
        return Err(CaseError::Invalid {
            path: path.to_path_buf(),
            problems,
        });
    }
    Ok(EvalCase {
        id: wire.id,
        class: wire.class,
        role: wire.role,
        workflow: wire.workflow,
        request: wire.request,
        territory_repo: wire.territory_repo,
        territory_commit: wire.territory_commit,
        required: wire.required,
        forbidden: wire.forbidden,
        required_text: wire.required_text,
        human_review: wire.human_review.questions,
        fixture_hash: factory::tools::content_hash(&bytes),
        path: path.to_path_buf(),
    })
}

/// Load every `*.toml` case under `dir`, recursively, in sorted order.
/// Returns everything that parsed AND every problem found — the caller
/// decides that any problem stops the run (it does).
pub fn load_corpus(dir: &Path) -> (Vec<EvalCase>, Vec<CaseError>) {
    let mut files = Vec::new();
    collect_toml(dir, &mut files);
    files.sort();

    let mut loaded: Vec<EvalCase> = Vec::new();
    let mut errors = Vec::new();
    for file in files {
        match load_case(&file) {
            Ok(case) => {
                if let Some(first) = loaded.iter().find(|c| c.id == case.id) {
                    errors.push(CaseError::DuplicateId {
                        id: case.id.clone(),
                        first: first.path.clone(),
                        second: case.path.clone(),
                    });
                } else {
                    loaded.push(case);
                }
            }
            Err(error) => errors.push(error),
        }
    }
    (loaded, errors)
}

fn collect_toml(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_toml(&path, files);
        } else if path.extension().and_then(|e| e.to_str()) == Some("toml") {
            files.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("daemar-eval-cases-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const VALID: &str = r#"
schema = "daemar.eval-case.v1"
id = "scout.example"
class = ["scout", "example"]
role = "scout"
workflow = "scout"
request = "Where is the thing? Cite path:line."
territory_repo = "."
territory_commit = "6ee526ef2400f6748917e8c652c91fddb5493ac9"

[[required]]
path = "src/lib.rs"
line = 1
verbatim = "pub fn"

[human_review]
questions = ["Did it explain the connection?"]
"#;

    #[test]
    fn a_valid_case_parses_with_a_fixture_hash() {
        let dir = scratch("valid");
        let file = dir.join("example.toml");
        std::fs::write(&file, VALID).unwrap();
        let case = load_case(&file).expect("valid case");
        assert_eq!(case.id, "scout.example");
        assert_eq!(case.required.len(), 1);
        assert_eq!(case.human_review.len(), 1);
        assert_eq!(case.fixture_hash.len(), 16);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn every_validation_problem_is_reported_at_once() {
        let dir = scratch("problems");
        let file = dir.join("bad.toml");
        std::fs::write(
            &file,
            r#"
schema = "daemar.eval-case.v2"
id = ""
role = "builder"
workflow = "scout"
request = "  "
territory_repo = "."
territory_commit = "6ee526e"
"#,
        )
        .unwrap();
        match load_case(&file) {
            Err(CaseError::Invalid { problems, .. }) => {
                assert!(
                    problems.len() >= 5,
                    "expected all problems, got {problems:?}"
                );
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unknown_fields_are_refused_not_ignored() {
        let dir = scratch("unknown");
        let file = dir.join("typo.toml");
        std::fs::write(&file, VALID.replace("[human_review]", "[human_reviw]")).unwrap();
        assert!(matches!(load_case(&file), Err(CaseError::Parse { .. })));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn territory_repo_may_not_escape_the_suite_repo() {
        let dir = scratch("territory-escape");
        let file = dir.join("escape.toml");
        std::fs::write(
            &file,
            VALID.replace("territory_repo = \".\"", "territory_repo = \"../..\""),
        )
        .unwrap();
        match load_case(&file) {
            Err(CaseError::Invalid { problems, .. }) => {
                assert!(
                    problems.iter().any(|p| p.contains("must stay inside")),
                    "{problems:?}"
                );
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn expectation_paths_may_not_traverse_upward() {
        let dir = scratch("key-escape");
        let file = dir.join("escape.toml");
        std::fs::write(
            &file,
            VALID.replace("path = \"src/lib.rs\"", "path = \"src/../../lib.rs\""),
        )
        .unwrap();
        match load_case(&file) {
            Err(CaseError::Invalid { problems, .. }) => {
                assert!(
                    problems.iter().any(|p| p.contains("must not contain '..'")),
                    "{problems:?}"
                );
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn duplicate_ids_across_files_are_refused() {
        let dir = scratch("dup");
        std::fs::write(dir.join("a.toml"), VALID).unwrap();
        std::fs::write(dir.join("b.toml"), VALID).unwrap();
        let (cases, errors) = load_corpus(&dir);
        assert_eq!(cases.len(), 1);
        assert!(matches!(&errors[..], [CaseError::DuplicateId { .. }]));
        std::fs::remove_dir_all(&dir).ok();
    }
}
