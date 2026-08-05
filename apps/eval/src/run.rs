//! The run orchestrator: select cases, pin territories, fly the REAL
//! production workflow per case × replicate, grade from the ledger, emit
//! one immutable dossier.
//!
//! Airframe selection is what production uses: DAEMAR_SCOUT_MODEL (falling
//! back to DAEMAR_MODEL) resolved once at `Config::from_env`. The rig sets
//! nothing model-related itself — the invoker chooses the airframe, exactly
//! as they would for a real flight.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use factory::config::Config;
use factory::roster::Role;
use factory::tools::content_hash;
use factory::workflows::{self, FlightError};

use crate::cases::{self, EvalCase};
use crate::dossier::{
    CaseRecord, CitationRecord, Dossier, FindingRecord, Manifest, ManifestCase, PinnedFile,
    DOSSIER_SCHEMA,
};
use crate::{grade, pin, EvalError};

pub struct Roots {
    pub cases: PathBuf,
    pub territories: PathBuf,
    pub dossiers: PathBuf,
    /// The repo the suite lives in: territory_repo resolves against this,
    /// and the manifest records its HEAD as the build commit.
    pub repo: PathBuf,
}

impl Roots {
    pub fn standard(repo: &Path) -> Roots {
        Roots {
            cases: repo.join("eval/cases"),
            territories: repo.join("eval/territories"),
            dossiers: repo.join("eval/dossiers"),
            repo: repo.to_path_buf(),
        }
    }
}

pub struct Selection {
    pub ids: Vec<String>,
    pub classes: Vec<String>,
    pub runs: u32,
}

pub struct RunOutcome {
    pub dossier_dir: PathBuf,
    pub all_passed: bool,
}

fn select(cases: Vec<EvalCase>, selection: &Selection) -> Result<Vec<EvalCase>, EvalError> {
    let known_ids: HashSet<&str> = cases.iter().map(|c| c.id.as_str()).collect();
    let known_classes: HashSet<&str> = cases
        .iter()
        .flat_map(|c| c.class.iter().map(String::as_str))
        .collect();
    for id in &selection.ids {
        if !known_ids.contains(id.as_str()) {
            return Err(EvalError::Selector(format!(
                "unknown case '{id}' — known: {}",
                sorted(&known_ids).join(", ")
            )));
        }
    }
    for class in &selection.classes {
        if !known_classes.contains(class.as_str()) {
            return Err(EvalError::Selector(format!(
                "unknown class '{class}' — known: {}",
                sorted(&known_classes).join(", ")
            )));
        }
    }
    if selection.runs == 0 {
        return Err(EvalError::Selector("--runs must be at least 1".to_string()));
    }
    let all = selection.ids.is_empty() && selection.classes.is_empty();
    Ok(cases
        .into_iter()
        .filter(|c| {
            all || selection.ids.contains(&c.id)
                || selection
                    .classes
                    .iter()
                    .any(|class| c.class.contains(class))
        })
        .collect())
}

fn sorted(set: &HashSet<&str>) -> Vec<String> {
    let mut v: Vec<String> = set.iter().map(|s| s.to_string()).collect();
    v.sort();
    v
}

fn dossier_dir_name() -> String {
    let now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!(
        "{}-p{}",
        ledger::format_ts(now).replace(':', "-"),
        std::process::id()
    )
}

fn build_commit(repo: &Path) -> String {
    std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Fly the selected corpus. `progress` narrates to the invoker as flights
/// land — a paid run must never be silent.
pub fn run(
    roots: &Roots,
    selection: &Selection,
    progress: &mut dyn FnMut(&str),
) -> Result<RunOutcome, EvalError> {
    let (corpus, errors) = cases::load_corpus(&roots.cases);
    if !errors.is_empty() {
        return Err(EvalError::Cases(errors));
    }
    if corpus.is_empty() {
        return Err(EvalError::Selector(format!(
            "no cases found under {}",
            roots.cases.display()
        )));
    }
    let selected = select(corpus, selection)?;
    if selected.is_empty() {
        return Err(EvalError::Selector(
            "selection matched no cases".to_string(),
        ));
    }

    // Pin every territory BEFORE the first paid flight: a broken pin must
    // cost zero dollars.
    let mut worktrees: Vec<PathBuf> = Vec::new();
    for case in &selected {
        let repo = roots.repo.join(&case.territory_repo);
        let worktree = pin::materialize(&repo, &case.territory_commit, &roots.territories)?;
        progress(&format!(
            "pinned {} at {} -> {}",
            case.id,
            &case.territory_commit[..12],
            worktree.display()
        ));
        worktrees.push(worktree);
    }

    // One config for the whole run: same airframe, same provider — the
    // ledgers directory is retargeted per replicate. Eval slips are signed
    // distinctly so a stray ledger can never masquerade as operations.
    let mut config = Config::from_env()?;
    config.engineer = format!("eval:{}", config.engineer);
    let model = config.model_for(Role::Scout).to_string();
    progress(&format!("scout airframe: {model}"));

    let dossier_dir = roots.dossiers.join(dossier_dir_name());
    fs::create_dir_all(&dossier_dir).map_err(|e| EvalError::Io {
        path: dossier_dir.clone(),
        detail: e.to_string(),
    })?;

    let mut records: Vec<CaseRecord> = Vec::new();
    for (case, worktree) in selected.iter().zip(&worktrees) {
        for replicate in 1..=selection.runs {
            let ledgers = dossier_dir
                .join("ledgers")
                .join(&case.id)
                .join(replicate.to_string());
            config.ledgers = ledgers.display().to_string();

            let started = Instant::now();
            let flown =
                workflows::scout_flight(&config, &case.request, &worktree.display().to_string());
            let latency = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

            let record = record_flight(case, worktree, &model, replicate, &ledgers, flown, latency);
            progress(&format!(
                "{} rep {}/{} · {} · {} finding(s) · {} tokens · ${:.4} · {}ms",
                case.id,
                replicate,
                selection.runs,
                record.verdict,
                record.findings.len(),
                record.tokens,
                record.cost,
                record.flight_latency_ms
            ));
            records.push(record);
        }
    }

    let manifest = Manifest {
        schema: DOSSIER_SCHEMA.to_string(),
        created: ledger::format_ts(
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        ),
        invocation: std::env::args().collect(),
        build_commit: build_commit(&roots.repo),
        scout_model: model,
        scout_model_env: factory::config::env("DAEMAR_SCOUT_MODEL"),
        engineer: config.engineer.clone(),
        runs: selection.runs,
        cases: selected
            .iter()
            .zip(&worktrees)
            .map(|(case, worktree)| ManifestCase {
                id: case.id.clone(),
                fixture_hash: case.fixture_hash.clone(),
                territory_commit: case.territory_commit.clone(),
                worktree: worktree.display().to_string(),
                pinned_files: key_file_hashes(case, worktree),
            })
            .collect(),
    };
    let dossier = Dossier {
        schema: DOSSIER_SCHEMA.to_string(),
        records,
    };
    crate::dossier::write(&dossier_dir, &manifest, &dossier)?;

    let all_passed = dossier.records.iter().all(|r| r.verdict == "pass");
    Ok(RunOutcome {
        dossier_dir,
        all_passed,
    })
}

/// Hash every file the answer key touches, as checked out — the manifest's
/// guard against a silently mutated worktree.
fn key_file_hashes(case: &EvalCase, worktree: &Path) -> Vec<PinnedFile> {
    let mut paths: Vec<&str> = case
        .required
        .iter()
        .map(|r| r.path.as_str())
        .chain(case.forbidden.iter().map(|b| b.path.as_str()))
        .collect();
    paths.sort_unstable();
    paths.dedup();
    paths
        .into_iter()
        .filter_map(|rel| {
            fs::read(worktree.join(rel)).ok().map(|bytes| PinnedFile {
                path: rel.to_string(),
                hash: content_hash(&bytes),
            })
        })
        .collect()
}

/// Fold one flight result into a dossier record. Failures are recorded
/// truthfully — refused, failed-open, ledger errors — never skipped and
/// never converted into success.
fn record_flight(
    case: &EvalCase,
    worktree: &Path,
    model: &str,
    replicate: u32,
    ledgers: &Path,
    flown: Result<workflows::FlightReport, FlightError>,
    latency: u64,
) -> CaseRecord {
    let mut record = CaseRecord {
        case_id: case.id.clone(),
        class: case.class.clone(),
        replicate,
        model: model.to_string(),
        territory_commit: case.territory_commit.clone(),
        slip_id: None,
        ledger_path: None,
        ledger_hash: None,
        outcome: String::new(),
        verdict: "not_graded".to_string(),
        findings: Vec::new(),
        citations: Vec::new(),
        raw_output: String::new(),
        tokens: 0,
        model_calls: 0,
        cost: 0.0,
        flight_latency_ms: latency,
        human_review: case.human_review.clone(),
    };

    let slip_id = match flown {
        Ok(report) => {
            record.outcome = "accepted".to_string();
            report.slip_id
        }
        Err(FlightError::Failed { slip_id }) => {
            record.outcome = "failed_open".to_string();
            slip_id
        }
        Err(FlightError::Refused(message)) => {
            record.outcome = "refused".to_string();
            record.raw_output = message;
            return record;
        }
        Err(FlightError::Ledger(error)) => {
            record.outcome = "ledger_error".to_string();
            record.raw_output = error.to_string();
            return record;
        }
    };
    record.slip_id = Some(slip_id.clone());

    // The ledger is the source of truth for output, tokens, and frozen
    // cost — not the in-memory report.
    let ledger_path = ledgers.join(format!("{slip_id}.jsonl"));
    record.ledger_path = Some(ledger_path.display().to_string());
    match fs::read(&ledger_path) {
        Ok(bytes) => record.ledger_hash = Some(content_hash(&bytes)),
        Err(error) => {
            record.outcome = format!("ledger_error: {error}");
            return record;
        }
    }
    let loaded = match ledger::load_ledger(&ledger_path) {
        Ok(loaded) => loaded,
        Err(error) => {
            record.outcome = format!("ledger_error: {error}");
            return record;
        }
    };
    let Some(slip) = ledger::fold(&loaded.events) else {
        record.outcome = "ledger_error: ledger never opened a slip".to_string();
        return record;
    };
    record.tokens = slip.tokens;
    record.cost = slip.cost;
    record.model_calls = slip.model_calls;

    if record.outcome != "accepted" {
        return record;
    }
    let Some(section) = slip.sections.iter().rev().find(|s| s.section == "scout.v1") else {
        record.findings.push(FindingRecord {
            kind: "section_missing".to_string(),
            detail: "accepted flight wrote no scout.v1 section".to_string(),
        });
        return record;
    };
    record.raw_output = section.body.clone();

    let graded = grade::grade(case, &section.body, worktree);
    record.verdict = if graded.passed() { "pass" } else { "fail" }.to_string();
    record.findings = graded
        .findings
        .iter()
        .map(|f| FindingRecord {
            kind: f.kind.key().to_string(),
            detail: f.detail.clone(),
        })
        .collect();
    record.citations = graded
        .citations
        .into_iter()
        .map(|v| CitationRecord {
            raw: v.citation.raw,
            path: v.citation.path,
            start: v.citation.start,
            end: v.citation.end,
            problem: v.problem,
            line_text: v.line_text,
        })
        .collect();
    record
}
