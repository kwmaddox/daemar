//! The dossier: one immutable directory per eval run — manifest, per-flight
//! records with verdicts and receipts, raw outputs preserved for the human
//! review the grader refuses to fake, and the flights' actual ledgers.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::EvalError;

pub const DOSSIER_SCHEMA: &str = "daemar.eval-dossier.v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedFile {
    pub path: String,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestCase {
    pub id: String,
    pub fixture_hash: String,
    pub territory_commit: String,
    pub worktree: String,
    /// Hashes of the files the answer key touches, as checked out — the
    /// audit guard against a mutated worktree.
    pub pinned_files: Vec<PinnedFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: String,
    pub created: String,
    pub invocation: Vec<String>,
    /// The eval suite's own commit at run time; "unknown" outside a repo.
    pub build_commit: String,
    /// The airframe resolved for the scout seat — the thing under test.
    pub scout_model: String,
    /// Whether DAEMAR_SCOUT_MODEL was set explicitly or fell back.
    pub scout_model_env: Option<String>,
    pub engineer: String,
    pub runs: u32,
    pub cases: Vec<ManifestCase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingRecord {
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationRecord {
    pub raw: String,
    pub path: String,
    pub start: u64,
    pub end: u64,
    pub problem: Option<String>,
    pub line_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseRecord {
    pub case_id: String,
    pub class: Vec<String>,
    pub replicate: u32,
    pub model: String,
    pub territory_commit: String,
    pub slip_id: Option<String>,
    pub ledger_path: Option<String>,
    pub ledger_hash: Option<String>,
    /// How the flight itself ended: accepted | failed_open | refused |
    /// ledger_error. Only accepted flights are graded.
    pub outcome: String,
    /// pass | fail | not_graded. A flight that never produced a gradable
    /// section is not_graded — visible, never converted into success.
    pub verdict: String,
    pub findings: Vec<FindingRecord>,
    pub citations: Vec<CitationRecord>,
    pub raw_output: String,
    pub tokens: u64,
    pub model_calls: u32,
    /// Frozen USD from the ledger receipts, never recomputed.
    pub cost: f64,
    /// Evaluator-observed whole-flight wall time. NOT a provider timing.
    pub flight_latency_ms: u64,
    pub human_review: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Dossier {
    pub schema: String,
    pub records: Vec<CaseRecord>,
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), EvalError> {
    let text = serde_json::to_string_pretty(value).map_err(|e| EvalError::Dossier {
        detail: e.to_string(),
    })?;
    fs::write(path, text).map_err(|e| EvalError::Io {
        path: path.to_path_buf(),
        detail: e.to_string(),
    })
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, EvalError> {
    let text = fs::read_to_string(path).map_err(|e| EvalError::Io {
        path: path.to_path_buf(),
        detail: e.to_string(),
    })?;
    serde_json::from_str(&text).map_err(|e| EvalError::Dossier {
        detail: format!("{}: {e}", path.display()),
    })
}

pub fn write(dir: &Path, manifest: &Manifest, dossier: &Dossier) -> Result<(), EvalError> {
    write_json(&dir.join("manifest.json"), manifest)?;
    write_json(&dir.join("dossier.json"), dossier)?;
    let summary = summary_md(manifest, dossier);
    let path = dir.join("summary.md");
    fs::write(&path, summary).map_err(|e| EvalError::Io {
        path,
        detail: e.to_string(),
    })
}

pub fn load(dir: &Path) -> Result<(Manifest, Dossier), EvalError> {
    let manifest: Manifest = read_json(&dir.join("manifest.json"))?;
    let dossier: Dossier = read_json(&dir.join("dossier.json"))?;
    Ok((manifest, dossier))
}

pub fn summary_md(manifest: &Manifest, dossier: &Dossier) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Eval dossier — {} — {}\n\n",
        manifest.scout_model, manifest.created
    ));
    out.push_str(&format!(
        "engineer {} · build {} · {} case(s) × {} run(s)\n\n",
        manifest.engineer,
        manifest.build_commit,
        manifest.cases.len(),
        manifest.runs
    ));
    out.push_str("| case | rep | verdict | findings | tokens | calls | cost | latency |\n");
    out.push_str("|---|---:|---|---:|---:|---:|---:|---:|\n");
    for r in &dossier.records {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | ${:.4} | {}ms |\n",
            r.case_id,
            r.replicate,
            r.verdict,
            r.findings.len(),
            r.tokens,
            r.model_calls,
            r.cost,
            r.flight_latency_ms
        ));
    }
    let passed = dossier
        .records
        .iter()
        .filter(|r| r.verdict == "pass")
        .count();
    let cost: f64 = dossier.records.iter().map(|r| r.cost).sum();
    out.push_str(&format!(
        "\n**{passed}/{} pass** · total ${cost:.4}\n",
        dossier.records.len()
    ));
    let mut detailed = false;
    for r in dossier.records.iter().filter(|r| !r.findings.is_empty()) {
        if !detailed {
            out.push_str("\n## Findings\n\n");
            detailed = true;
        }
        out.push_str(&format!("### {} rep {}\n\n", r.case_id, r.replicate));
        for finding in &r.findings {
            out.push_str(&format!("- {}: {}\n", finding.kind, finding.detail));
        }
        out.push('\n');
    }
    let review: Vec<&CaseRecord> = dossier
        .records
        .iter()
        .filter(|r| !r.human_review.is_empty())
        .collect();
    if !review.is_empty() {
        out.push_str("\n## Awaiting human review\n\n");
        out.push_str("Raw outputs live in dossier.json; the grader does not score these.\n\n");
        for r in review {
            for question in &r.human_review {
                out.push_str(&format!(
                    "- {} rep {}: {}\n",
                    r.case_id, r.replicate, question
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (Manifest, Dossier) {
        let manifest = Manifest {
            schema: DOSSIER_SCHEMA.to_string(),
            created: "2026-08-05T00:00:00Z".into(),
            invocation: vec!["run".into()],
            build_commit: "abc".into(),
            scout_model: "gpt-5.6-luna".into(),
            scout_model_env: Some("gpt-5.6-luna".into()),
            engineer: "eval:kendall".into(),
            runs: 1,
            cases: vec![ManifestCase {
                id: "scout.example".into(),
                fixture_hash: "f".repeat(16),
                territory_commit: "c".repeat(40),
                worktree: "/tmp/wt".into(),
                pinned_files: vec![PinnedFile {
                    path: "src/lib.rs".into(),
                    hash: "a".repeat(16),
                }],
            }],
        };
        let dossier = Dossier {
            schema: DOSSIER_SCHEMA.to_string(),
            records: vec![CaseRecord {
                case_id: "scout.example".into(),
                class: vec!["scout".into()],
                replicate: 1,
                model: "gpt-5.6-luna".into(),
                territory_commit: "c".repeat(40),
                slip_id: Some("slip-1".into()),
                ledger_path: Some("ledgers/scout.example/1/slip-1.jsonl".into()),
                ledger_hash: Some("b".repeat(16)),
                outcome: "accepted".into(),
                verdict: "fail".into(),
                findings: vec![FindingRecord {
                    kind: "required_citation_missing".into(),
                    detail: "no citation covers src/lib.rs:1".into(),
                }],
                citations: vec![],
                raw_output: "report text".into(),
                tokens: 1000,
                model_calls: 3,
                cost: 0.0057,
                flight_latency_ms: 8200,
                human_review: vec!["Did it explain?".into()],
            }],
        };
        (manifest, dossier)
    }

    #[test]
    fn a_dossier_round_trips_through_its_directory() {
        let dir = std::env::temp_dir().join(format!("daemar-eval-dossier-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let (manifest, dossier) = sample();
        write(&dir, &manifest, &dossier).unwrap();
        let (loaded_manifest, loaded) = load(&dir).unwrap();
        assert_eq!(loaded_manifest.scout_model, "gpt-5.6-luna");
        assert_eq!(loaded.records.len(), 1);
        assert_eq!(
            loaded.records[0].findings[0].kind,
            "required_citation_missing"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_summary_shows_verdicts_findings_and_review_questions() {
        let (manifest, dossier) = sample();
        let summary = summary_md(&manifest, &dossier);
        assert!(summary.contains("0/1 pass"));
        assert!(summary.contains("required_citation_missing"));
        assert!(summary.contains("Awaiting human review"));
    }
}
