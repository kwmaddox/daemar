//! Dossier comparison: objective deltas by case and airframe. It never
//! declares a subjective winner — it shows verdicts, finding categories,
//! and receipt distributions, and points at what still needs human eyes.

use std::collections::BTreeMap;
use std::path::Path;

use crate::dossier::{self, CaseRecord, Dossier, Manifest};
use crate::EvalError;

pub fn compare(left: &Path, right: &Path) -> Result<String, EvalError> {
    let left_loaded = dossier::load(left)?;
    let right_loaded = dossier::load(right)?;
    Ok(compare_loaded(
        (&left_loaded.0, &left_loaded.1),
        (&right_loaded.0, &right_loaded.1),
    ))
}

/// A side's aggregate for one case id.
struct Side<'a> {
    records: Vec<&'a CaseRecord>,
}

impl<'a> Side<'a> {
    fn passes(&self) -> usize {
        self.records.iter().filter(|r| r.verdict == "pass").count()
    }
    fn graded(&self) -> usize {
        self.records
            .iter()
            .filter(|r| r.verdict != "not_graded")
            .count()
    }
    fn finding_counts(&self) -> BTreeMap<&'a str, usize> {
        let mut counts = BTreeMap::new();
        for record in &self.records {
            for finding in &record.findings {
                *counts.entry(finding.kind.as_str()).or_insert(0) += 1;
            }
        }
        counts
    }
    fn stat(&self, pick: impl Fn(&CaseRecord) -> f64) -> String {
        let mut values: Vec<f64> = self.records.iter().map(|r| pick(r)).collect();
        if values.is_empty() {
            return "-".to_string();
        }
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = values[values.len() / 2];
        format!(
            "{:.4}/{:.4}/{:.4}",
            values[0],
            median,
            values[values.len() - 1]
        )
    }
}

fn findings_line(side: &Side) -> String {
    let counts = side.finding_counts();
    if counts.is_empty() {
        "none".to_string()
    } else {
        counts
            .iter()
            .map(|(kind, count)| format!("{kind}×{count}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub fn compare_loaded(left: (&Manifest, &Dossier), right: (&Manifest, &Dossier)) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "left:  {} · {} record(s)\nright: {} · {} record(s)\n",
        left.0.scout_model,
        left.1.records.len(),
        right.0.scout_model,
        right.1.records.len()
    ));

    let mut case_ids: Vec<&str> = left
        .1
        .records
        .iter()
        .chain(&right.1.records)
        .map(|r| r.case_id.as_str())
        .collect();
    case_ids.sort_unstable();
    case_ids.dedup();

    for case_id in &case_ids {
        let l = Side {
            records: left
                .1
                .records
                .iter()
                .filter(|r| r.case_id == *case_id)
                .collect(),
        };
        let r = Side {
            records: right
                .1
                .records
                .iter()
                .filter(|r| r.case_id == *case_id)
                .collect(),
        };
        out.push_str(&format!("\ncase {case_id}\n"));
        out.push_str(&format!(
            "  verdicts     left {}/{} pass ({} graded)   right {}/{} pass ({} graded)\n",
            l.passes(),
            l.records.len(),
            l.graded(),
            r.passes(),
            r.records.len(),
            r.graded()
        ));
        out.push_str(&format!(
            "  findings     left {}   right {}\n",
            findings_line(&l),
            findings_line(&r)
        ));
        out.push_str(&format!(
            "  tokens       left {}   right {}   (min/med/max)\n",
            l.stat(|rec| rec.tokens as f64),
            r.stat(|rec| rec.tokens as f64)
        ));
        out.push_str(&format!(
            "  cost $       left {}   right {}\n",
            l.stat(|rec| rec.cost),
            r.stat(|rec| rec.cost)
        ));
        out.push_str(&format!(
            "  latency ms   left {}   right {}\n",
            l.stat(|rec| rec.flight_latency_ms as f64),
            r.stat(|rec| rec.flight_latency_ms as f64)
        ));
    }

    let totals = |dossier: &Dossier| {
        let passes = dossier
            .records
            .iter()
            .filter(|r| r.verdict == "pass")
            .count();
        let cost: f64 = dossier.records.iter().map(|r| r.cost).sum();
        (passes, dossier.records.len(), cost)
    };
    let (lp, ln, lc) = totals(left.1);
    let (rp, rn, rc) = totals(right.1);
    out.push_str(&format!(
        "\ntotals\n  pass   left {lp}/{ln}   right {rp}/{rn}\n  cost   left ${lc:.4}   right ${rc:.4}\n"
    ));

    let review = left
        .1
        .records
        .iter()
        .chain(&right.1.records)
        .filter(|r| !r.human_review.is_empty())
        .count();
    if review > 0 {
        out.push_str(&format!(
            "\n{review} record(s) carry human-review questions — raw outputs in each dossier.json; \
             the comparison does not score them.\n"
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dossier::{CaseRecord, Dossier, FindingRecord, Manifest, DOSSIER_SCHEMA};

    fn manifest(model: &str) -> Manifest {
        Manifest {
            schema: DOSSIER_SCHEMA.to_string(),
            created: "t".into(),
            invocation: vec![],
            build_commit: "abc".into(),
            scout_model: model.into(),
            scout_model_env: None,
            engineer: "eval:test".into(),
            runs: 2,
            cases: vec![],
        }
    }

    fn record(case: &str, replicate: u32, verdict: &str, cost: f64) -> CaseRecord {
        CaseRecord {
            case_id: case.into(),
            class: vec![],
            replicate,
            model: "m".into(),
            territory_commit: "c".repeat(40),
            slip_id: None,
            ledger_path: None,
            ledger_hash: None,
            outcome: "accepted".into(),
            verdict: verdict.into(),
            findings: if verdict == "fail" {
                vec![FindingRecord {
                    kind: "required_citation_missing".into(),
                    detail: "d".into(),
                }]
            } else {
                vec![]
            },
            citations: vec![],
            raw_output: String::new(),
            tokens: 100,
            model_calls: 3,
            cost,
            flight_latency_ms: 1000,
            human_review: vec!["q".into()],
        }
    }

    #[test]
    fn the_comparison_groups_by_case_and_never_declares_a_winner() {
        let left = Dossier {
            schema: DOSSIER_SCHEMA.to_string(),
            records: vec![
                record("scout.a", 1, "pass", 0.10),
                record("scout.a", 2, "pass", 0.12),
            ],
        };
        let right = Dossier {
            schema: DOSSIER_SCHEMA.to_string(),
            records: vec![
                record("scout.a", 1, "pass", 0.005),
                record("scout.a", 2, "fail", 0.006),
            ],
        };
        let text = compare_loaded(
            (&manifest("gpt-5.6-terra"), &left),
            (&manifest("gpt-5.6-luna"), &right),
        );
        assert!(text.contains("case scout.a"));
        assert!(text.contains("left 2/2 pass"));
        assert!(text.contains("right 1/2 pass"));
        assert!(text.contains("required_citation_missing×1"));
        assert!(text.contains("human-review questions"));
        assert!(!text.to_lowercase().contains("winner"));
    }
}
