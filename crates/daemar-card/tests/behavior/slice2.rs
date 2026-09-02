//! S2 step definitions for PER-83 (S2-B1…S2-B4): stage events through
//! the `card` CLI. The read path is programmatic — `card history` in
//! sequence order with entry-type filtering — and any visual view is
//! deferred entirely (operator decision, 2026-08-31, recorded on the
//! story Card).
//!
//! Surface choices encoded here are provisional until the typed skeleton
//! is approved — the entry-type name (`stage-event`) and the CLI flag
//! shape. Each lives in one helper so a skeleton-time change touches a
//! single site.

use cucumber::gherkin::Step;
use cucumber::{given, then, when};
use serde_json::Value;

use crate::{CardWorld, Run};

// --- Stage events through the CLI --------------------------------------------

/// One stage event as submitted: the three event-content fields riding
/// the slice-1 envelope (S2-B1). Kept whole so retry steps can replay or
/// perturb the identical submission.
#[derive(Debug, Clone)]
pub(crate) struct StoredStageEvent {
    card: String,
    stage: String,
    summary: String,
    payload: Option<String>,
    producer: String,
    // ast-grep-ignore: no-stringly-typed-field -- fixture replays raw CLI wire strings verbatim
    kind: String,
    key: Option<String>,
}

/// Provisional CLI shape: stage events ride the existing `append` verb
/// with `--entry-type stage-event`; stage and summary are dedicated flags
/// and `--payload` carries the raw JSON object verbatim.
fn stage_event_args(event: &StoredStageEvent, schema_version: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = [
        "append",
        &event.card,
        "--entry-type",
        "stage-event",
        "--stage",
        &event.stage,
        "--summary",
        &event.summary,
        "--producer",
        &event.producer,
        "--producer-kind",
        &event.kind,
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    if let Some(payload) = &event.payload {
        args.push("--payload".to_owned());
        args.push(payload.clone());
    }
    if let Some(key) = &event.key {
        args.push("--idempotency-key".to_owned());
        args.push(key.clone());
    }
    if let Some(version) = schema_version {
        args.push("--schema-version".to_owned());
        args.push(version.to_owned());
    }
    args
}

impl CardWorld {
    fn submit_stage_event(&mut self, event: &StoredStageEvent) -> Run {
        let args = stage_event_args(event, None);
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run(&borrowed)
    }

    /// Records a stage event that must succeed, retaining its
    /// acknowledgement and submission for later steps.
    fn recorded_stage_event(&mut self, event: StoredStageEvent) {
        let run = self.submit_stage_event(&event);
        let json = run.success_json();
        let entry_id = json["entry_id"].as_str().expect("entry_id").to_owned();
        let sequence = json["sequence"].as_u64().expect("sequence");
        self.first_entry = Some((entry_id, sequence));
        self.stage_retry = Some(event);
    }

    fn last_history_entry(&mut self) -> Value {
        let card = self.primary_card();
        let entries = self.history(&card);
        entries.last().expect("at least one entry").clone()
    }
}

#[when(
    expr = "producer {string} of kind {string} records a stage event with stage {string} and summary {string} and payload:"
)]
fn record_event_with_payload(
    w: &mut CardWorld,
    #[step] step: &Step,
    producer: String,
    kind: String,
    stage: String,
    summary: String,
) {
    let payload = step
        .docstring
        .as_ref()
        .expect("a payload docstring")
        .trim()
        .to_owned();
    let event = StoredStageEvent {
        card: w.primary_card(),
        stage,
        summary,
        payload: Some(payload),
        producer,
        kind,
        key: None,
    };
    let run = w.submit_stage_event(&event);
    w.stage_retry = Some(event);
    w.last = Some(run);
}

#[when(
    expr = "producer {string} of kind {string} records a stage event with stage {string} and summary {string} and no payload"
)]
fn record_event_without_payload(
    w: &mut CardWorld,
    producer: String,
    kind: String,
    stage: String,
    summary: String,
) {
    let event = StoredStageEvent {
        card: w.primary_card(),
        stage,
        summary,
        payload: None,
        producer,
        kind,
        key: None,
    };
    let run = w.submit_stage_event(&event);
    w.stage_retry = Some(event);
    w.last = Some(run);
}

#[then(expr = "the recorded stage is {string} and the recorded summary is {string}")]
fn recorded_stage_and_summary(w: &mut CardWorld, stage: String, summary: String) {
    let entry = w.last_history_entry();
    assert_eq!(
        entry["stage"].as_str(),
        Some(stage.as_str()),
        "recorded stage: {entry}"
    );
    assert_eq!(
        entry["summary"].as_str(),
        Some(summary.as_str()),
        "recorded summary: {entry}"
    );
}

#[then("the recorded payload is exactly the submitted JSON")]
fn recorded_payload_exact(w: &mut CardWorld) {
    // Parse before `last_history_entry` needs `&mut w`, so the borrow of
    // the stored submission ends without a clone.
    let expected: Value = {
        let submitted = w
            .stage_retry
            .as_ref()
            .and_then(|event| event.payload.as_deref())
            .expect("a submitted payload");
        serde_json::from_str(submitted).expect("submitted payload parses")
    };
    let entry = w.last_history_entry();
    assert_eq!(entry["payload"], expected, "payload drifted: {entry}");
}

#[then("the recorded event carries no payload")]
fn recorded_event_carries_no_payload(w: &mut CardWorld) {
    let entry = w.last_history_entry();
    // Strict absence (Copilot review, Card seq 48): the gate rejects an
    // explicit `payload: null`, and the history seam omits the key for a
    // payload-less event — a null here would mean one of them regressed.
    assert!(
        entry.get("payload").is_none(),
        "expected the payload key to be absent: {entry}"
    );
}

#[when(regex = r"^a producer submits a stage event that is (.+)$")]
fn submit_malformed_stage_event(w: &mut CardWorld, defect: String) {
    // (stage, summary, payload, schema version) — well-formed defaults,
    // perturbed by exactly the one defect under test.
    let well_formed: (&str, &str, Option<&str>, Option<&str>) =
        ("gherkin", "well formed", None, None);
    let (stage, summary, payload, schema_version) = match defect.as_str() {
        // ast-grep-ignore: no-string-literal-dispatch -- Gherkin example text is a parse boundary, converts once into a fixture
        "carrying a blank stage" => ("", well_formed.1, None, None),
        // ast-grep-ignore: no-string-literal-dispatch -- Gherkin example text is a parse boundary, converts once into a fixture
        "carrying a whitespace-only stage" => ("   ", well_formed.1, None, None),
        // ast-grep-ignore: no-string-literal-dispatch -- Gherkin example text is a parse boundary, converts once into a fixture
        "carrying a blank summary" => (well_formed.0, "", None, None),
        // ast-grep-ignore: no-string-literal-dispatch -- Gherkin example text is a parse boundary, converts once into a fixture
        "carrying a whitespace-only summary" => (well_formed.0, "   ", None, None),
        // ast-grep-ignore: no-string-literal-dispatch -- Gherkin example text is a parse boundary, converts once into a fixture
        "carrying a summary containing a bare carriage return" => {
            (well_formed.0, "split\rsummary", None, None)
        }
        // ast-grep-ignore: no-string-literal-dispatch -- Gherkin example text is a parse boundary, converts once into a fixture
        "carrying a summary containing a bare line feed" => {
            (well_formed.0, "split\nsummary", None, None)
        }
        // ast-grep-ignore: no-string-literal-dispatch -- Gherkin example text is a parse boundary, converts once into a fixture
        "carrying a payload that is a JSON array, not an object" => {
            (well_formed.0, well_formed.1, Some("[1, 2, 3]"), None)
        }
        // ast-grep-ignore: no-string-literal-dispatch -- Gherkin example text is a parse boundary, converts once into a fixture
        "carrying a payload that is a JSON string, not an object" => (
            well_formed.0,
            well_formed.1,
            Some("\"just a string\""),
            None,
        ),
        // ast-grep-ignore: no-string-literal-dispatch -- Gherkin example text is a parse boundary, converts once into a fixture
        "carrying a payload that is a JSON number, not an object" => {
            (well_formed.0, well_formed.1, Some("7"), None)
        }
        // ast-grep-ignore: no-string-literal-dispatch -- Gherkin example text is a parse boundary, converts once into a fixture
        "carrying a payload that is a JSON boolean, not an object" => {
            (well_formed.0, well_formed.1, Some("true"), None)
        }
        // ast-grep-ignore: no-string-literal-dispatch -- Gherkin example text is a parse boundary, converts once into a fixture
        "carrying a payload that is JSON null, not an object" => {
            (well_formed.0, well_formed.1, Some("null"), None)
        }
        // ast-grep-ignore: no-string-literal-dispatch -- Gherkin example text is a parse boundary, converts once into a fixture
        "carrying a payload with duplicate member names" => (
            well_formed.0,
            well_formed.1,
            Some(r#"{"count": 1, "count": 2}"#),
            None,
        ),
        // ast-grep-ignore: no-string-literal-dispatch -- Gherkin example text is a parse boundary, converts once into a fixture
        "carrying a payload with duplicate member names nested deep" => (
            well_formed.0,
            well_formed.1,
            Some(r#"{"outer": {"mid": {"count": 1, "count": 2}}}"#),
            None,
        ),
        // ast-grep-ignore: no-string-literal-dispatch -- Gherkin example text is a parse boundary, converts once into a fixture
        "carrying a payload with duplicate member names followed by a later member" => (
            well_formed.0,
            well_formed.1,
            // Positional coverage (operator-review finding): a duplicate
            // that is NOT the final member must still reject — a probe
            // that stops reading at the first duplicate corrupts the
            // parse and lets last-member-wins acceptance through.
            Some(r#"{"count": 1, "count": 2, "later": 3}"#),
            None,
        ),
        // ast-grep-ignore: no-string-literal-dispatch -- Gherkin example text is a parse boundary, converts once into a fixture
        "carrying a nested duplicate with a later sibling after the nested object" => (
            well_formed.0,
            well_formed.1,
            Some(r#"{"outer": {"count": 1, "count": 2}, "later": 3}"#),
            None,
        ),
        // ast-grep-ignore: no-string-literal-dispatch -- Gherkin example text is a parse boundary, converts once into a fixture
        "of an unknown schema version" => {
            (well_formed.0, well_formed.1, well_formed.2, Some("999"))
        }
        other => panic!("unmapped defect: {other}"),
    };
    let event = StoredStageEvent {
        card: w.primary_card(),
        stage: stage.to_owned(),
        summary: summary.to_owned(),
        payload: payload.map(str::to_owned),
        producer: "claude".to_owned(),
        kind: "agent".to_owned(),
        key: None,
    };
    let args = stage_event_args(&event, schema_version);
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    let run = w.run(&borrowed);
    w.last = Some(run);
}

#[given(
    expr = "producer {string} of kind {string} recorded a stage event with idempotency key {string}"
)]
fn recorded_stage_event_with_key(w: &mut CardWorld, producer: String, kind: String, key: String) {
    let event = StoredStageEvent {
        card: w.primary_card(),
        stage: "fixture-stage".to_owned(),
        summary: "retriable stage event".to_owned(),
        payload: Some(r#"{"outer": {"inner": "original"}}"#.to_owned()),
        producer,
        kind,
        key: Some(key),
    };
    w.recorded_stage_event(event);
}

/// The 2^64 fixture pair: mathematically distinct neighbours that a
/// 64-bit float cannot tell apart (operator-review finding — value
/// fidelity and changed-content conflict must both survive them).
const BIG_INT_PAYLOAD: &str = r#"{"n": 18446744073709551616}"#;
const BIG_INT_PAYLOAD_INCREMENTED: &str = r#"{"n": 18446744073709551617}"#;

#[given(
    expr = "producer {string} of kind {string} recorded a stage event with idempotency key {string} and a large-integer payload"
)]
fn recorded_stage_event_with_key_big_int(
    w: &mut CardWorld,
    producer: String,
    kind: String,
    key: String,
) {
    let event = StoredStageEvent {
        card: w.primary_card(),
        stage: "measure".to_owned(),
        summary: "large counter recorded".to_owned(),
        payload: Some(BIG_INT_PAYLOAD.to_owned()),
        producer,
        kind,
        key: Some(key),
    };
    w.recorded_stage_event(event);
}

#[when(
    expr = "the same stage event is retried with idempotency key {string} and the large integer incremented"
)]
fn retry_stage_event_big_int_incremented(w: &mut CardWorld, key: String) {
    let mut event = w.stage_retry.take().expect("a stored stage event");
    assert_eq!(
        event.key.as_deref(),
        Some(key.as_str()),
        "the retry must reuse idempotency key {key}"
    );
    event.payload = Some(BIG_INT_PAYLOAD_INCREMENTED.to_owned());
    let run = w.submit_stage_event(&event);
    w.stage_retry = Some(event);
    w.last = Some(run);
}

#[then(expr = "the Card history text contains {string}")]
fn history_text_contains(w: &mut CardWorld, needle: String) {
    // Raw-text oracle on purpose: a parsed comparison would run the
    // reader through the same JSON number pipeline under test, hiding
    // exactly the precision collapse this asserts against.
    let card = w.primary_card();
    let run = w.run(&["history", &card]);
    assert!(
        run.output.status.success(),
        "history failed: {}",
        run.describe()
    );
    let text = String::from_utf8_lossy(&run.output.stdout).into_owned();
    assert!(
        text.contains(&needle),
        "history text lacks `{needle}`:\n{text}"
    );
}

#[when(expr = "the same stage event is retried with idempotency key {string}")]
fn retry_stage_event(w: &mut CardWorld, key: String) {
    let event = w.stage_retry.take().expect("a stored stage event");
    assert_eq!(
        event.key.as_deref(),
        Some(key.as_str()),
        "the retry must reuse idempotency key {key}"
    );
    let run = w.submit_stage_event(&event);
    w.stage_retry = Some(event);
    w.last = Some(run);
}

#[when(
    expr = "the same stage event is retried with idempotency key {string} and a changed nested payload value"
)]
fn retry_stage_event_changed(w: &mut CardWorld, key: String) {
    let mut event = w.stage_retry.take().expect("a stored stage event");
    assert_eq!(
        event.key.as_deref(),
        Some(key.as_str()),
        "the retry must reuse idempotency key {key}"
    );
    event.payload = Some(r#"{"outer": {"inner": "changed"}}"#.to_owned());
    let run = w.submit_stage_event(&event);
    w.stage_retry = Some(event);
    w.last = Some(run);
}

#[then(expr = "the entry types read {string}, {string}, {string}, {string} in that order")]
fn entry_types_in_order(w: &mut CardWorld, t1: String, t2: String, t3: String, t4: String) {
    let entries = w.last_run().success_json()["entries"]
        .as_array()
        .expect("entries");
    let types: Vec<&str> = entries
        .iter()
        .map(|entry| entry["entry_type"].as_str().expect("entry_type"))
        .collect();
    assert_eq!(types, vec![&t1, &t2, &t3, &t4], "entry types out of order");
}

#[given("an appended stage event with a nested payload")]
fn appended_stage_event_nested(w: &mut CardWorld) {
    let event = StoredStageEvent {
        card: w.primary_card(),
        stage: "code".to_owned(),
        summary: "durable stage event".to_owned(),
        payload: Some(r#"{"outer": {"inner": ["deep", 1]}}"#.to_owned()),
        producer: "claude".to_owned(),
        kind: "agent".to_owned(),
        key: None,
    };
    w.recorded_stage_event(event);
}
