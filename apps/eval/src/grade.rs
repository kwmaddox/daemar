//! Deterministic grading: one strict citation grammar, an answer key of
//! required and forbidden citations, all problems reported at once.
//!
//! Grammar v1 (documented, not repaired): a citation is a BACKTICKED span of
//! the form `path:N` or `path:N-M` — a relative path containing `/` and a
//! dotted filename, then 1-based line numbers. Near-citations that fail the
//! grammar (`file.rs:382+`, absolute paths with valid lines) are recorded as
//! findings, never silently fixed: format drift is evaluation data.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::cases::EvalCase;

#[derive(Debug, Clone, PartialEq)]
pub struct Citation {
    pub path: String,
    pub start: u64,
    pub end: u64,
    pub raw: String,
}

/// A citation after checking it against the pinned checkout.
#[derive(Debug, Clone)]
pub struct VerifiedCitation {
    pub citation: Citation,
    /// What is wrong with it, when something is; mirrors a finding.
    pub problem: Option<String>,
    /// The pinned text of the start line, when the file and line exist.
    pub line_text: Option<String>,
}

/// The closed set of ways a report can fail its key. `key()` is the stable
/// dossier vocabulary — comparisons group on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingKind {
    MalformedCitation,
    CitationNotRelative,
    CitationPathMissing,
    CitationLineOutOfRange,
    RequiredCitationMissing,
    RequiredVerbatimMismatch,
    ForbiddenCitation,
    RequiredTextMissing,
}

impl FindingKind {
    pub fn key(&self) -> &'static str {
        match self {
            FindingKind::MalformedCitation => "malformed_citation",
            FindingKind::CitationNotRelative => "citation_not_relative",
            FindingKind::CitationPathMissing => "citation_path_missing",
            FindingKind::CitationLineOutOfRange => "citation_line_out_of_range",
            FindingKind::RequiredCitationMissing => "required_citation_missing",
            FindingKind::RequiredVerbatimMismatch => "required_verbatim_mismatch",
            FindingKind::ForbiddenCitation => "forbidden_citation",
            FindingKind::RequiredTextMissing => "required_text_missing",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub kind: FindingKind,
    pub detail: String,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.key(), self.detail)
    }
}

#[derive(Debug)]
pub struct Grade {
    pub findings: Vec<Finding>,
    pub citations: Vec<VerifiedCitation>,
}

impl Grade {
    pub fn passed(&self) -> bool {
        self.findings.is_empty()
    }
}

enum Classified {
    Citation(Citation),
    Malformed,
    /// Not citation-shaped at all — code, commands, identifiers.
    Foreign,
}

fn classify(span: &str) -> Classified {
    let s = span.trim();
    if s.is_empty() || s.chars().any(char::is_whitespace) {
        return Classified::Foreign;
    }
    let Some((head, tail)) = s.rsplit_once(':') else {
        return Classified::Foreign;
    };
    // A path: has directory structure and a dotted filename; URLs are not
    // territory paths.
    let path_like = head.contains('/')
        && !head.contains("//")
        && head
            .rsplit('/')
            .next()
            .is_some_and(|name| name.contains('.'));
    if !path_like {
        return Classified::Foreign;
    }
    // From here the span WANTED to be a citation; failing the grammar is
    // malformed, not foreign.
    let range = match tail.split_once('-') {
        None => tail.parse::<u64>().ok().map(|n| (n, n)),
        Some((a, b)) => match (a.parse::<u64>(), b.parse::<u64>()) {
            (Ok(a), Ok(b)) if a <= b => Some((a, b)),
            _ => None,
        },
    };
    match range {
        Some((start, end)) if start >= 1 => Classified::Citation(Citation {
            path: head.to_string(),
            start,
            end,
            raw: s.to_string(),
        }),
        _ => Classified::Malformed,
    }
}

/// Extract every citation and every malformed near-citation from a report.
/// Citations are deduplicated; repetition is style, not evidence.
pub fn extract(text: &str) -> (Vec<Citation>, Vec<String>) {
    let mut citations: Vec<Citation> = Vec::new();
    let mut malformed: Vec<String> = Vec::new();
    for (index, span) in text.split('`').enumerate() {
        if index % 2 == 0 {
            continue; // outside backticks
        }
        match classify(span) {
            Classified::Citation(citation) => {
                if !citations.contains(&citation) {
                    citations.push(citation);
                }
            }
            Classified::Malformed => {
                let raw = span.trim().to_string();
                if !malformed.contains(&raw) {
                    malformed.push(raw);
                }
            }
            Classified::Foreign => {}
        }
    }
    (citations, malformed)
}

/// Pinned files, read once each. Lossy UTF-8 — grading text against text.
struct FileCache {
    root: PathBuf,
    files: HashMap<String, Option<Vec<String>>>,
}

impl FileCache {
    fn new(root: &Path) -> Self {
        FileCache {
            root: root.to_path_buf(),
            files: HashMap::new(),
        }
    }

    fn lines(&mut self, rel: &str) -> Option<&Vec<String>> {
        if !self.files.contains_key(rel) {
            let loaded = std::fs::read(self.root.join(rel)).ok().map(|bytes| {
                String::from_utf8_lossy(&bytes)
                    .lines()
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            });
            self.files.insert(rel.to_string(), loaded);
        }
        self.files.get(rel).and_then(|f| f.as_ref())
    }

    fn line(&mut self, rel: &str, line: u64) -> Option<String> {
        let index = usize::try_from(line.checked_sub(1)?).ok()?;
        self.lines(rel)?.get(index).cloned()
    }
}

fn covers(citation: &Citation, path: &str, line: u64) -> bool {
    citation.path == path && citation.start <= line && line <= citation.end
}

/// Grade one report against one case, checking citations against the pinned
/// checkout. Collects EVERY defect; never stops at the first.
pub fn grade(case: &EvalCase, report: &str, worktree: &Path) -> Grade {
    let mut findings: Vec<Finding> = Vec::new();
    let mut cache = FileCache::new(worktree);

    let (citations, malformed) = extract(report);
    for raw in malformed {
        findings.push(Finding {
            kind: FindingKind::MalformedCitation,
            detail: format!("'{raw}' is citation-shaped but fails the path:N / path:N-M grammar"),
        });
    }

    let mut verified: Vec<VerifiedCitation> = Vec::new();
    for citation in &citations {
        let (problem, line_text) = if citation.path.starts_with('/') {
            (
                Some(Finding {
                    kind: FindingKind::CitationNotRelative,
                    detail: format!("'{}' cites an absolute path", citation.raw),
                }),
                None,
            )
        } else {
            match cache.lines(&citation.path) {
                None => (
                    Some(Finding {
                        kind: FindingKind::CitationPathMissing,
                        detail: format!(
                            "'{}' cites a path absent from the pinned territory",
                            citation.raw
                        ),
                    }),
                    None,
                ),
                Some(lines) => {
                    let count = lines.len() as u64;
                    if citation.end > count {
                        (
                            Some(Finding {
                                kind: FindingKind::CitationLineOutOfRange,
                                detail: format!(
                                    "'{}' exceeds the pinned file's {count} lines",
                                    citation.raw
                                ),
                            }),
                            None,
                        )
                    } else {
                        (None, cache.line(&citation.path, citation.start))
                    }
                }
            }
        };
        if let Some(finding) = &problem {
            findings.push(finding.clone());
        }
        verified.push(VerifiedCitation {
            citation: citation.clone(),
            problem: problem.map(|f| f.detail),
            line_text,
        });
    }

    for required in &case.required {
        let cited = citations
            .iter()
            .any(|c| covers(c, &required.path, required.line));
        if !cited {
            findings.push(Finding {
                kind: FindingKind::RequiredCitationMissing,
                detail: format!(
                    "no citation covers {}:{} ({})",
                    required.path, required.line, required.verbatim
                ),
            });
            continue;
        }
        // The key's own rot guard: the pinned line must still carry the
        // expected fragment, or the fixture (or worktree) has drifted.
        match cache.line(&required.path, required.line) {
            Some(line) if line.contains(&required.verbatim) => {}
            Some(line) => findings.push(Finding {
                kind: FindingKind::RequiredVerbatimMismatch,
                detail: format!(
                    "{}:{} is '{}' — expected it to contain '{}'; the pin or the key has drifted",
                    required.path,
                    required.line,
                    line.trim(),
                    required.verbatim
                ),
            }),
            None => findings.push(Finding {
                kind: FindingKind::RequiredVerbatimMismatch,
                detail: format!(
                    "{}:{} does not exist in the pinned checkout",
                    required.path, required.line
                ),
            }),
        }
    }

    for forbidden in &case.forbidden {
        if citations
            .iter()
            .any(|c| covers(c, &forbidden.path, forbidden.line))
        {
            findings.push(Finding {
                kind: FindingKind::ForbiddenCitation,
                detail: format!(
                    "cited bait {}:{} — {}",
                    forbidden.path, forbidden.line, forbidden.reason
                ),
            });
        }
    }

    for required in &case.required_text {
        if !report.contains(&required.contains) {
            findings.push(Finding {
                kind: FindingKind::RequiredTextMissing,
                detail: format!("report does not contain '{}'", required.contains),
            });
        }
    }

    Grade {
        findings,
        citations: verified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cases::{EvalCase, ForbiddenCitation, RequiredCitation, RequiredText};
    use std::path::PathBuf;

    fn case_with(
        required: Vec<RequiredCitation>,
        forbidden: Vec<ForbiddenCitation>,
        required_text: Vec<RequiredText>,
    ) -> EvalCase {
        EvalCase {
            id: "scout.test".into(),
            class: vec![],
            role: "scout".into(),
            workflow: "scout".into(),
            request: "r".into(),
            territory_repo: ".".into(),
            territory_commit: "0".repeat(40),
            required,
            forbidden,
            required_text,
            human_review: vec![],
            fixture_hash: "hash".into(),
            path: PathBuf::from("test.toml"),
        }
    }

    fn worktree(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("daemar-eval-grade-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("src/lib.rs"),
            "pub fn scout_flight() {}\nlet x = 1;\nlet bait = 2;\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn extracts_single_lines_and_ranges_and_dedupes() {
        let (cites, malformed) = extract(
            "See `crates/x/a.rs:10` and `crates/x/a.rs:10` and `apps/y/b.rs:5-9` for detail.",
        );
        assert_eq!(cites.len(), 2);
        assert_eq!(cites[0].path, "crates/x/a.rs");
        assert_eq!((cites[1].start, cites[1].end), (5, 9));
        assert!(malformed.is_empty());
    }

    #[test]
    fn prose_commands_and_identifiers_are_not_citations() {
        let (cites, malformed) =
            extract("Run `daemar grant slip-1`, call `scout_flight`, read `README.md`.");
        assert!(cites.is_empty());
        assert!(malformed.is_empty());
    }

    #[test]
    fn open_ended_and_reversed_ranges_are_malformed_not_repaired() {
        let (cites, malformed) = extract("At `crates/x/a.rs:382+` and `crates/x/a.rs:9-5`.");
        assert!(cites.is_empty());
        assert_eq!(malformed.len(), 2);
    }

    #[test]
    fn urls_are_foreign_not_citations() {
        let (cites, malformed) = extract("See `https://example.com/a.rs:80` for nothing.");
        assert!(cites.is_empty());
        assert!(malformed.is_empty());
    }

    #[test]
    fn grading_reports_every_defect_at_once() {
        let dir = worktree("all-defects");
        let case = case_with(
            vec![
                RequiredCitation {
                    path: "src/lib.rs".into(),
                    line: 1,
                    verbatim: "pub fn scout_flight(".into(),
                },
                RequiredCitation {
                    path: "src/lib.rs".into(),
                    line: 2,
                    verbatim: "let x".into(),
                },
            ],
            vec![ForbiddenCitation {
                path: "src/lib.rs".into(),
                line: 3,
                verbatim: "bait".into(),
                reason: "test-module bait".into(),
            }],
            vec![RequiredText {
                contains: "must-say-this".into(),
            }],
        );
        // Cites line 1 (ok) and the bait line; misses line 2; one malformed;
        // one missing path; omits the required text.
        let report = "Found `src/lib.rs:1` and `src/lib.rs:3`, also `src/gone.rs:2` \
                      and `src/lib.rs:9+`.";
        let grade = grade(&case, report, &dir);
        let kinds: Vec<&str> = grade.findings.iter().map(|f| f.kind.key()).collect();
        assert!(kinds.contains(&"malformed_citation"), "{kinds:?}");
        assert!(kinds.contains(&"citation_path_missing"), "{kinds:?}");
        assert!(kinds.contains(&"required_citation_missing"), "{kinds:?}");
        assert!(kinds.contains(&"forbidden_citation"), "{kinds:?}");
        assert!(kinds.contains(&"required_text_missing"), "{kinds:?}");
        assert!(!grade.passed());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_range_citation_satisfies_a_required_line_inside_it() {
        let dir = worktree("range");
        let case = case_with(
            vec![RequiredCitation {
                path: "src/lib.rs".into(),
                line: 1,
                verbatim: "scout_flight".into(),
            }],
            vec![],
            vec![],
        );
        let grade = grade(&case, "Declared at `src/lib.rs:1-3`.", &dir);
        assert!(grade.passed(), "{:?}", grade.findings);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_drifted_pin_is_a_verbatim_mismatch_not_a_pass() {
        let dir = worktree("drift");
        let case = case_with(
            vec![RequiredCitation {
                path: "src/lib.rs".into(),
                line: 1,
                verbatim: "something that is not on line one".into(),
            }],
            vec![],
            vec![],
        );
        let grade = grade(&case, "See `src/lib.rs:1`.", &dir);
        assert_eq!(grade.findings.len(), 1);
        assert_eq!(
            grade.findings[0].kind,
            FindingKind::RequiredVerbatimMismatch
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn out_of_range_and_absolute_citations_are_findings() {
        let dir = worktree("bounds");
        let case = case_with(
            vec![],
            vec![],
            vec![RequiredText {
                contains: "lib".into(),
            }],
        );
        let grade = grade(&case, "See `src/lib.rs:99` and `/abs/path/lib.rs:1`.", &dir);
        let kinds: Vec<&str> = grade.findings.iter().map(|f| f.kind.key()).collect();
        assert!(kinds.contains(&"citation_line_out_of_range"), "{kinds:?}");
        assert!(kinds.contains(&"citation_not_relative"), "{kinds:?}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
