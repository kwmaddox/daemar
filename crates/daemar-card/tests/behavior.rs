//! Executable behavior spec for PER-82 (S1-B1…S1-B14).
//!
//! The Gherkin features in `tests/features/` are the citable spec; each
//! step here drives the compiled `card` binary — the same boundary agents
//! use — against a scenario-local database selected via `DAEMAR_DB`.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "C4: panics are the assertion mechanism in test code"
)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "cucumber step functions receive captured parameters by value"
)]
#![expect(
    clippy::disallowed_types,
    reason = "C2 boundary: cucumber's World derive names anyhow::Error in its \
              generated impl; none of our error handling uses it"
)]

use std::path::Path;
use std::process::{Command, Output};

use cucumber::{given, then, when, World as _};
use serde_json::Value;
use tempfile::TempDir;

/// Path to the compiled `card` binary under test, injected by Cargo.
const CARD_BIN: &str = env!("CARGO_BIN_EXE_card");

/// One `card` invocation: raw output plus parsed JSON where present.
#[derive(Debug)]
struct Run {
    output: Output,
    stdout_json: Option<Value>,
    stderr_json: Option<Value>,
}

impl Run {
    fn describe(&self) -> String {
        format!(
            "status: {:?}\nstdout: {}\nstderr: {}",
            self.output.status,
            String::from_utf8_lossy(&self.output.stdout),
            String::from_utf8_lossy(&self.output.stderr),
        )
    }

    /// Asserts success and returns the stdout JSON (S1-B12 success half).
    fn success_json(&self) -> &Value {
        assert!(
            self.output.status.success(),
            "expected success\n{}",
            self.describe()
        );
        self.stdout_json
            .as_ref()
            .unwrap_or_else(|| panic!("expected JSON on stdout\n{}", self.describe()))
    }

    /// Asserts failure and returns the structured error object (S1-B12).
    fn error_json(&self) -> &Value {
        assert!(
            !self.output.status.success(),
            "expected failure\n{}",
            self.describe()
        );
        let json = self
            .stderr_json
            .as_ref()
            .unwrap_or_else(|| panic!("expected JSON on stderr\n{}", self.describe()));
        json.get("error")
            .unwrap_or_else(|| panic!("expected an `error` object\n{}", self.describe()))
    }

    fn card_id(&self) -> String {
        self.success_json()["card_id"]
            .as_str()
            .expect("card_id")
            .to_owned()
    }
}

fn run_card_command(configure: impl FnOnce(&mut Command)) -> Run {
    let mut command = Command::new(CARD_BIN);
    configure(&mut command);
    let output = command.output().expect("failed to spawn the card binary");
    let stdout_json = serde_json::from_slice(&output.stdout).ok();
    let stderr_json = serde_json::from_slice(&output.stderr).ok();
    Run {
        output,
        stdout_json,
        stderr_json,
    }
}

fn run_card(db: &str, args: &[&str]) -> Run {
    run_card_command(|command| {
        command.args(args).env("DAEMAR_DB", db);
    })
}

#[derive(Debug, Default, cucumber::World)]
struct CardWorld {
    home: Option<TempDir>,
    cards: Vec<String>,
    last: Option<Run>,
    first_card: Option<String>,
    first_entry: Option<(String, u64)>,
    retry_args: Option<Vec<String>>,
    concurrent: Vec<Run>,
    reads: Vec<Value>,
    acks: Vec<Value>,
    flag_db: Option<(TempDir, String)>,
    dotenv_db: Option<(TempDir, String)>,
    fake_home: Option<TempDir>,
    iso_cwd: Option<TempDir>,
}

impl CardWorld {
    fn db_path(&mut self) -> String {
        let dir = self
            .home
            .get_or_insert_with(|| TempDir::new().expect("temp dir"));
        dir.path()
            .join("daemar.db")
            .to_str()
            .expect("utf-8 path")
            .to_owned()
    }

    fn run(&mut self, args: &[&str]) -> Run {
        let db = self.db_path();
        run_card(&db, args)
    }

    fn last_run(&self) -> &Run {
        self.last.as_ref().expect("a command was run")
    }

    fn primary_card(&self) -> String {
        self.cards.first().expect("an open Card").clone()
    }

    fn create_card(&mut self, extra: &[&str]) -> Run {
        let mut args = vec![
            "create",
            "--producer",
            "test-operator",
            "--producer-kind",
            "operator",
        ];
        args.extend_from_slice(extra);
        self.run(&args)
    }

    fn append_decision(
        &mut self,
        card: &str,
        producer: &str,
        kind: &str,
        summary: &str,
        reason: &str,
        extra: &[&str],
    ) -> Run {
        let payload = serde_json::json!({ "summary": summary, "reason": reason }).to_string();
        let mut args = vec![
            "append",
            card,
            "--entry-type",
            "decision",
            "--payload",
            &payload,
            "--producer",
            producer,
            "--producer-kind",
            kind,
        ];
        args.extend_from_slice(extra);
        self.run(&args)
    }

    fn history(&mut self, card: &str) -> Vec<Value> {
        let run = self.run(&["history", card]);
        run.success_json()["entries"]
            .as_array()
            .expect("entries array")
            .clone()
    }

    fn list_cards(&mut self) -> Vec<Value> {
        let run = self.run(&["list"]);
        run.success_json()["cards"]
            .as_array()
            .expect("cards array")
            .clone()
    }

    fn flag_db_path(&self) -> String {
        self.flag_db.as_ref().expect("a --db override").1.clone()
    }

    fn assert_entry_count(&mut self, count: usize) {
        let card = self.primary_card();
        let entries = self.history(&card);
        assert_eq!(entries.len(), count, "entry count for Card {card}");
    }
}

fn sequences(entries: &[Value]) -> Vec<u64> {
    entries
        .iter()
        .map(|e| e["sequence"].as_u64().expect("sequence"))
        .collect()
}

fn assert_sequence_range(entries: &[Value], from: u64, through: u64) {
    let expected: Vec<u64> = (from..=through).collect();
    assert_eq!(
        sequences(entries),
        expected,
        "sequences out of order or gapped"
    );
}

// --- Creation (S1-B1…S1-B4) -------------------------------------------------

#[when(
    expr = "a producer creates a Card titled {string} with task key {string} and workspace {string}"
)]
fn create_with_references(w: &mut CardWorld, title: String, key: String, workspace: String) {
    let run = w.create_card(&[
        "--title",
        &title,
        "--task-key",
        &key,
        "--workspace",
        &workspace,
    ]);
    w.last = Some(run);
}

#[when(expr = "a producer creates a Card titled {string}")]
fn create_titled(w: &mut CardWorld, title: String) {
    let run = w.create_card(&["--title", &title]);
    w.last = Some(run);
}

#[given(expr = "a Card was created with idempotency key {string} and title {string}")]
fn card_created_with_key(w: &mut CardWorld, key: String, title: String) {
    let run = w.create_card(&["--title", &title, "--idempotency-key", &key]);
    w.first_card = Some(run.card_id());
}

#[when(expr = "a producer creates a Card with idempotency key {string} and title {string}")]
fn create_with_key(w: &mut CardWorld, key: String, title: String) {
    let run = w.create_card(&["--title", &title, "--idempotency-key", &key]);
    w.last = Some(run);
}

#[then("the command succeeds and returns a Card ID")]
fn returns_card_id(w: &mut CardWorld) {
    let id = w.last_run().card_id();
    assert!(!id.is_empty(), "Card ID must be non-empty");
    w.first_card = Some(id);
}

#[then(expr = "the Card ID is not {string}")]
fn card_id_is_not(w: &mut CardWorld, key: String) {
    assert_ne!(
        w.first_card.as_deref(),
        Some(key.as_str()),
        "external key must not be the identity"
    );
}

#[then(expr = "the Card list shows that Card with title {string} and task key {string}")]
fn list_shows_card(w: &mut CardWorld, title: String, key: String) {
    let id = w.first_card.clone().expect("a created Card");
    let cards = w.list_cards();
    let card = cards
        .iter()
        .find(|c| c["card_id"].as_str() == Some(id.as_str()))
        .unwrap_or_else(|| panic!("Card {id} missing from list"));
    assert_eq!(card["title"].as_str(), Some(title.as_str()));
    assert_eq!(card["task_key"].as_str(), Some(key.as_str()));
}

#[then("the listed Card's task key and workspace read as absent")]
fn listed_references_absent(w: &mut CardWorld) {
    let cards = w.list_cards();
    assert_eq!(cards.len(), 1, "expected exactly one listed Card");
    let card = cards.first().unwrap();
    assert!(
        card.get("task_key").is_none_or(Value::is_null),
        "task key must read as absent, got {card}"
    );
    assert!(
        card.get("workspace").is_none_or(Value::is_null),
        "workspace must read as absent, got {card}"
    );
}

#[then("the command succeeds and returns the same Card ID as before")]
fn same_card_id(w: &mut CardWorld) {
    let id = w.last_run().card_id();
    assert_eq!(
        Some(id),
        w.first_card,
        "retry must return the original Card ID"
    );
}

#[then("exactly one Card exists")]
fn exactly_one_card(w: &mut CardWorld) {
    assert_eq!(w.list_cards().len(), 1);
}

#[then(expr = "the command fails with error category {string}")]
fn fails_with_category(w: &mut CardWorld, category: String) {
    let error = w.last_run().error_json();
    assert_eq!(
        error["category"].as_str(),
        Some(category.as_str()),
        "unexpected error: {error}"
    );
}

// --- Append (S1-B5…S1-B10) --------------------------------------------------

#[given("an open Card")]
fn open_card(w: &mut CardWorld) {
    let run = w.create_card(&["--title", "Dogfood card"]);
    w.cards.push(run.card_id());
}

#[given("a second open Card")]
fn second_open_card(w: &mut CardWorld) {
    let run = w.create_card(&["--title", "Second dogfood card"]);
    w.cards.push(run.card_id());
}

#[when(
    expr = "producer {string} of kind {string} appends a decision with summary {string} and reason {string}"
)]
fn append_a_decision(
    w: &mut CardWorld,
    producer: String,
    kind: String,
    summary: String,
    reason: String,
) {
    let card = w.primary_card();
    let run = w.append_decision(&card, &producer, &kind, &summary, &reason, &[]);
    w.last = Some(run);
}

#[then(expr = "the entry is accepted at sequence {int}")]
fn accepted_at_sequence(w: &mut CardWorld, sequence: u64) {
    let json = w.last_run().success_json();
    assert_eq!(
        json["sequence"].as_u64(),
        Some(sequence),
        "unexpected sequence: {json}"
    );
    assert!(
        json["entry_id"].as_str().is_some_and(|id| !id.is_empty()),
        "entry ID missing"
    );
}

#[then(
    expr = "the recorded entry carries type {string}, producer {string} of kind {string}, a schema version, an entry ID, and a server-assigned timestamp"
)]
fn entry_envelope(w: &mut CardWorld, entry_type: String, producer: String, kind: String) {
    let card = w.primary_card();
    let entries = w.history(&card);
    let entry = entries.last().expect("at least one entry");
    assert_eq!(entry["entry_type"].as_str(), Some(entry_type.as_str()));
    assert_eq!(entry["producer"]["id"].as_str(), Some(producer.as_str()));
    assert_eq!(entry["producer"]["kind"].as_str(), Some(kind.as_str()));
    assert!(
        entry["schema_version"].is_u64(),
        "schema version missing: {entry}"
    );
    assert!(
        entry["entry_id"].as_str().is_some_and(|id| !id.is_empty()),
        "entry ID missing"
    );
    assert!(
        entry["recorded_at"].as_str().is_some_and(|t| !t.is_empty()),
        "server-assigned timestamp missing: {entry}"
    );
}

#[then(expr = "the recorded payload has summary {string} and reason {string}")]
fn recorded_payload(w: &mut CardWorld, summary: String, reason: String) {
    let card = w.primary_card();
    let entries = w.history(&card);
    let payload = &entries.last().expect("at least one entry")["payload"];
    assert_eq!(payload["summary"].as_str(), Some(summary.as_str()));
    assert_eq!(payload["reason"].as_str(), Some(reason.as_str()));
}

#[when("decisions are appended alternately to both Cards three times")]
fn append_alternately(w: &mut CardWorld) {
    let first = w.primary_card();
    let second = w.cards.get(1).expect("a second open Card").clone();
    for round in 0..3 {
        for card in [&first, &second] {
            let summary = format!("alternating decision {round}");
            let run = w.append_decision(
                card,
                "claude",
                "agent",
                &summary,
                "sequence independence",
                &[],
            );
            run.success_json();
        }
    }
}

#[then(expr = "the first Card's entries read sequences {int} through {int}")]
fn first_card_sequences(w: &mut CardWorld, from: u64, through: u64) {
    let card = w.primary_card();
    let entries = w.history(&card);
    assert_sequence_range(&entries, from, through);
}

#[then(expr = "the second Card's entries read sequences {int} through {int}")]
fn second_card_sequences(w: &mut CardWorld, from: u64, through: u64) {
    let card = w.cards.get(1).expect("a second open Card").clone();
    let entries = w.history(&card);
    assert_sequence_range(&entries, from, through);
}

#[given(
    expr = "producer {string} of kind {string} appended a decision with idempotency key {string}"
)]
fn appended_with_key(w: &mut CardWorld, producer: String, kind: String, key: String) {
    let card = w.primary_card();
    let payload = serde_json::json!({ "summary": "retriable decision", "reason": "spec fixture" })
        .to_string();
    let args: Vec<String> = [
        "append",
        &card,
        "--entry-type",
        "decision",
        "--payload",
        &payload,
        "--producer",
        &producer,
        "--producer-kind",
        &kind,
        "--idempotency-key",
        &key,
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    let run = w.run(&borrowed);
    let json = run.success_json();
    w.first_entry = Some((
        json["entry_id"].as_str().expect("entry_id").to_owned(),
        json["sequence"].as_u64().expect("sequence"),
    ));
    w.retry_args = Some(args);
}

#[when(expr = "the same append is retried with idempotency key {string}")]
fn retry_append(w: &mut CardWorld, key: String) {
    let args = w.retry_args.clone().expect("a stored append to retry");
    assert!(
        args.iter().any(|a| a == &key),
        "the retried append must reuse idempotency key {key}"
    );
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    let run = w.run(&borrowed);
    w.last = Some(run);
}

#[then("the command succeeds and returns the same entry ID and sequence as before")]
fn same_entry_id_and_sequence(w: &mut CardWorld) {
    let json = w.last_run().success_json();
    let (entry_id, sequence) = w.first_entry.clone().expect("a first append");
    assert_eq!(
        json["entry_id"].as_str(),
        Some(entry_id.as_str()),
        "entry ID changed on retry"
    );
    assert_eq!(
        json["sequence"].as_u64(),
        Some(sequence),
        "sequence changed on retry"
    );
}

#[then(expr = "the Card has exactly {int} entries")]
fn card_has_entries(w: &mut CardWorld, count: u64) {
    w.assert_entry_count(usize::try_from(count).expect("count fits usize"));
}

#[given(expr = "the Card has {int} entries")]
fn card_given_entries(w: &mut CardWorld, count: u64) {
    let card = w.primary_card();
    let target = usize::try_from(count).expect("count fits usize");
    for n in 1..target {
        let summary = format!("fixture decision {n}");
        let run = w.append_decision(&card, "claude", "agent", &summary, "spec fixture", &[]);
        run.success_json();
    }
    w.assert_entry_count(target);
}

#[when("a producer submits an append that is missing the decision reason")]
fn append_missing_reason(w: &mut CardWorld) {
    let card = w.primary_card();
    let payload = serde_json::json!({ "summary": "no reason given" }).to_string();
    let run = w.run(&[
        "append",
        &card,
        "--entry-type",
        "decision",
        "--payload",
        &payload,
        "--producer",
        "claude",
        "--producer-kind",
        "agent",
    ]);
    w.last = Some(run);
}

#[when("a producer submits an append that is missing producer identity")]
fn append_missing_producer(w: &mut CardWorld) {
    let card = w.primary_card();
    let payload = serde_json::json!({ "summary": "s", "reason": "r" }).to_string();
    let run = w.run(&[
        "append",
        &card,
        "--entry-type",
        "decision",
        "--payload",
        &payload,
        "--producer-kind",
        "agent",
    ]);
    w.last = Some(run);
}

#[when("a producer submits an append that is of an unknown entry type")]
fn append_unknown_type(w: &mut CardWorld) {
    let card = w.primary_card();
    let payload = serde_json::json!({ "summary": "s", "reason": "r" }).to_string();
    let run = w.run(&[
        "append",
        &card,
        "--entry-type",
        "gizmo",
        "--payload",
        &payload,
        "--producer",
        "claude",
        "--producer-kind",
        "agent",
    ]);
    w.last = Some(run);
}

#[when("a producer submits an append that is of an unknown schema version")]
fn append_unknown_schema(w: &mut CardWorld) {
    let card = w.primary_card();
    let payload = serde_json::json!({ "summary": "s", "reason": "r" }).to_string();
    let run = w.run(&[
        "append",
        &card,
        "--entry-type",
        "decision",
        "--payload",
        &payload,
        "--producer",
        "claude",
        "--producer-kind",
        "agent",
        "--schema-version",
        "999",
    ]);
    w.last = Some(run);
}

#[when("a producer submits an append that is addressed to a nonexistent Card")]
fn append_nonexistent_card(w: &mut CardWorld) {
    let payload = serde_json::json!({ "summary": "s", "reason": "r" }).to_string();
    let run = w.run(&[
        "append",
        "card-does-not-exist",
        "--entry-type",
        "decision",
        "--payload",
        &payload,
        "--producer",
        "claude",
        "--producer-kind",
        "agent",
    ]);
    w.last = Some(run);
}

#[then(expr = "the Card still has exactly {int} entries")]
fn card_still_has_entries(w: &mut CardWorld, count: u64) {
    w.assert_entry_count(usize::try_from(count).expect("count fits usize"));
}

#[when("two producers append decisions concurrently")]
fn concurrent_appends(w: &mut CardWorld) {
    let card = w.primary_card();
    let db = w.db_path();
    let handles: Vec<_> = [("claude", "ck-1"), ("codex", "ck-2")]
        .into_iter()
        .map(|(producer, key)| {
            let card = card.clone();
            let db = db.clone();
            let producer = producer.to_owned();
            let key = key.to_owned();
            std::thread::spawn(move || {
                let payload = serde_json::json!({
                    "summary": format!("concurrent decision by {producer}"),
                    "reason": "one durable order",
                })
                .to_string();
                run_card(
                    &db,
                    &[
                        "append",
                        &card,
                        "--entry-type",
                        "decision",
                        "--payload",
                        &payload,
                        "--producer",
                        &producer,
                        "--producer-kind",
                        "agent",
                        "--idempotency-key",
                        &key,
                    ],
                )
            })
        })
        .collect();
    w.concurrent = handles
        .into_iter()
        .map(|h| h.join().expect("append thread"))
        .collect();
}

#[then("both appends succeed with distinct consecutive sequences")]
fn distinct_consecutive_sequences(w: &mut CardWorld) {
    let mut seqs: Vec<u64> = w
        .concurrent
        .iter()
        .map(|run| run.success_json()["sequence"].as_u64().expect("sequence"))
        .collect();
    seqs.sort_unstable();
    let (first, second) = (
        *seqs.first().expect("two appends"),
        *seqs.get(1).expect("two appends"),
    );
    assert_eq!(
        second,
        first + 1,
        "sequences must be distinct and consecutive: {seqs:?}"
    );
}

#[then("the recorded history lists both entries in sequence order")]
fn history_in_sequence_order(w: &mut CardWorld) {
    let card = w.primary_card();
    let entries = w.history(&card);
    let seqs = sequences(&entries);
    assert!(
        seqs.windows(2).all(|pair| pair[0] < pair[1]),
        "history out of order: {seqs:?}"
    );
    for run in &w.concurrent {
        let entry_id = run.success_json()["entry_id"].as_str().expect("entry_id");
        assert!(
            entries
                .iter()
                .any(|e| e["entry_id"].as_str() == Some(entry_id)),
            "entry {entry_id} missing from history"
        );
    }
}

#[then("the CLI offers no command that updates or deletes an entry")]
fn no_mutation_surface(w: &mut CardWorld) {
    let run = w.run(&["--help"]);
    assert!(
        run.output.status.success(),
        "--help must succeed\n{}",
        run.describe()
    );
    let help = String::from_utf8_lossy(&run.output.stdout).to_lowercase();
    for verb in ["update", "delete", "edit", "remove"] {
        assert!(
            !help.contains(verb),
            "mutation surface `{verb}` found in help:\n{help}"
        );
    }
}

// --- Reading (S1-B11…S1-B12) ------------------------------------------------

#[given(expr = "{int} appended decisions")]
fn appended_decisions(w: &mut CardWorld, count: u64) {
    let card = w.primary_card();
    for n in 0..count {
        let summary = format!("recorded decision {n}");
        let run = w.append_decision(&card, "claude", "agent", &summary, "spec fixture", &[]);
        run.success_json();
    }
}

#[when("the Card history is read")]
fn read_history(w: &mut CardWorld) {
    let card = w.primary_card();
    let run = w.run(&["history", &card]);
    w.last = Some(run);
}

#[when(expr = "the Card history is read filtered to entry type {string}")]
fn read_history_filtered(w: &mut CardWorld, entry_type: String) {
    let card = w.primary_card();
    let run = w.run(&["history", &card, "--entry-type", &entry_type]);
    w.last = Some(run);
}

#[when("history is read for a nonexistent Card")]
fn read_history_nonexistent(w: &mut CardWorld) {
    let run = w.run(&["history", "card-does-not-exist"]);
    w.last = Some(run);
}

#[then(expr = "{int} entries return in sequence order {int} through {int}")]
fn entries_return_in_order(w: &mut CardWorld, count: u64, from: u64, through: u64) {
    let entries = w.last_run().success_json()["entries"]
        .as_array()
        .expect("entries")
        .clone();
    assert_eq!(
        entries.len(),
        usize::try_from(count).expect("count fits usize"),
        "entry count"
    );
    assert_sequence_range(&entries, from, through);
}

#[then("every entry carries a complete envelope")]
fn complete_envelopes(w: &mut CardWorld) {
    let entries = w.last_run().success_json()["entries"]
        .as_array()
        .expect("entries")
        .clone();
    for entry in &entries {
        for field in ["entry_id", "card_id", "entry_type", "recorded_at"] {
            assert!(
                entry[field].as_str().is_some_and(|v| !v.is_empty()),
                "envelope field `{field}` missing: {entry}"
            );
        }
        assert!(
            entry["sequence"].is_u64(),
            "envelope field `sequence` missing: {entry}"
        );
        assert!(
            entry["schema_version"].is_u64(),
            "envelope field `schema_version` missing: {entry}"
        );
        assert!(
            entry["producer"]["id"]
                .as_str()
                .is_some_and(|v| !v.is_empty()),
            "producer identity missing: {entry}"
        );
        assert!(
            entry["producer"]["kind"]
                .as_str()
                .is_some_and(|v| !v.is_empty()),
            "producer kind missing: {entry}"
        );
        assert!(
            entry.get("payload").is_some_and(Value::is_object),
            "payload missing: {entry}"
        );
    }
}

#[then(expr = "{int} entries return, all of type {string}, in sequence order")]
fn entries_of_type_in_order(w: &mut CardWorld, count: u64, entry_type: String) {
    let entries = w.last_run().success_json()["entries"]
        .as_array()
        .expect("entries")
        .clone();
    assert_eq!(
        entries.len(),
        usize::try_from(count).expect("count fits usize"),
        "entry count"
    );
    for entry in &entries {
        assert_eq!(
            entry["entry_type"].as_str(),
            Some(entry_type.as_str()),
            "wrong type: {entry}"
        );
    }
    let seqs = sequences(&entries);
    assert!(
        seqs.windows(2).all(|pair| pair[0] < pair[1]),
        "filtered history out of order: {seqs:?}"
    );
}

// --- Durability and locality (S1-B13…S1-B14) --------------------------------

#[when("the Card history is read in a fresh process")]
fn read_history_fresh_process(w: &mut CardWorld) {
    // Every CLI invocation is a fresh OS process; this sentence exists to
    // make the restart claim explicit in the spec.
    read_history(w);
}

#[then("the CLI reports the configured database path")]
fn reports_db_path(w: &mut CardWorld) {
    let expected = w.db_path();
    let run = w.run(&["db-path"]);
    assert_eq!(
        run.success_json()["db_path"].as_str(),
        Some(expected.as_str()),
        "db-path must report the configured database"
    );
}

#[then("pointing DAEMAR_DB at a different path uses that database")]
fn alternate_database(w: &mut CardWorld) {
    let alt_dir = TempDir::new().expect("temp dir");
    let alt_db = alt_dir
        .path()
        .join("daemar.db")
        .to_str()
        .expect("utf-8 path")
        .to_owned();
    let run = run_card(
        &alt_db,
        &[
            "create",
            "--title",
            "Alternate store probe",
            "--producer",
            "test-operator",
            "--producer-kind",
            "operator",
        ],
    );
    run.success_json();
    assert!(
        Path::new(&alt_db).exists(),
        "alternate database file was not created"
    );

    let alt_list = run_card(&alt_db, &["list"]);
    let alt_cards = alt_list.success_json()["cards"]
        .as_array()
        .expect("cards")
        .clone();
    assert_eq!(
        alt_cards.len(),
        1,
        "alternate database must hold exactly its own Card"
    );
    assert_eq!(
        alt_cards.first().unwrap()["title"].as_str(),
        Some("Alternate store probe")
    );

    let main_cards = w.list_cards();
    assert!(
        main_cards
            .iter()
            .all(|c| c["title"].as_str() != Some("Alternate store probe")),
        "the alternate Card leaked into the default database"
    );
}

#[tokio::main]
async fn main() {
    CardWorld::cucumber()
        .fail_on_skipped()
        .run_and_exit("tests/features")
        .await;
}

// --- Deep-review additions: reserved types, blanks, syntax contract,
// --- precedence, and the private factory home ---------------------------

#[when("a producer submits an append that is of the reserved card-created type")]
fn append_reserved_card_created(w: &mut CardWorld) {
    let card = w.primary_card();
    let payload = serde_json::json!({ "title": "a second creation fact" }).to_string();
    let run = w.run(&[
        "append",
        &card,
        "--entry-type",
        "card-created",
        "--payload",
        &payload,
        "--producer",
        "claude",
        "--producer-kind",
        "agent",
    ]);
    w.last = Some(run);
}

#[when("a producer submits an append that is carrying a blank producer identity")]
fn append_blank_producer(w: &mut CardWorld) {
    let card = w.primary_card();
    let payload = serde_json::json!({ "summary": "s", "reason": "r" }).to_string();
    let run = w.run(&[
        "append",
        &card,
        "--entry-type",
        "decision",
        "--payload",
        &payload,
        "--producer",
        "   ",
        "--producer-kind",
        "agent",
    ]);
    w.last = Some(run);
}

#[then("no Card exists")]
fn no_card_exists(w: &mut CardWorld) {
    assert!(
        w.list_cards().is_empty(),
        "no Card should have been created"
    );
}

#[when(expr = "the CLI is invoked with arguments {string}")]
fn invoke_with_raw_arguments(w: &mut CardWorld, arguments: String) {
    let split: Vec<&str> = arguments.split_whitespace().collect();
    let run = w.run(&split);
    w.last = Some(run);
}

#[when("the Card history is read twice in separate processes")]
fn read_history_twice(w: &mut CardWorld) {
    let card = w.primary_card();
    let first = w.run(&["history", &card]).success_json().clone();
    let second = w.run(&["history", &card]).success_json().clone();
    w.reads = vec![first, second];
}

#[then(expr = "both reads return {int} identical entries in sequence order {int} through {int}")]
fn reads_are_identical(w: &mut CardWorld, count: u64, from: u64, through: u64) {
    let first = w.reads.first().expect("a first read");
    let second = w.reads.get(1).expect("a second read");
    assert_eq!(
        first, second,
        "a fresh process must reproduce the identical record — IDs, provenance, timestamps, payloads"
    );
    let entries = first["entries"].as_array().expect("entries").clone();
    assert_eq!(
        entries.len(),
        usize::try_from(count).expect("count fits usize"),
        "entry count"
    );
    assert_sequence_range(&entries, from, through);
}

#[when(expr = "a Card titled {string} is created with a --db override")]
fn create_with_db_flag(w: &mut CardWorld, title: String) {
    let dir = TempDir::new().expect("temp dir");
    let flag_path = dir
        .path()
        .join("daemar.db")
        .to_str()
        .expect("utf-8 path")
        .to_owned();
    let env_db = w.db_path();
    let run = run_card_command(|command| {
        command.env("DAEMAR_DB", &env_db).args([
            "create",
            "--db",
            &flag_path,
            "--title",
            &title,
            "--producer",
            "test-operator",
            "--producer-kind",
            "operator",
        ]);
    });
    run.success_json();
    w.flag_db = Some((dir, flag_path));
}

#[then(expr = "the override database holds exactly one Card titled {string}")]
fn override_db_holds_card(w: &mut CardWorld, title: String) {
    let flag_path = w.flag_db_path();
    let env_db = w.db_path();
    let run = run_card_command(|command| {
        command
            .env("DAEMAR_DB", &env_db)
            .args(["list", "--db", &flag_path]);
    });
    let cards = run.success_json()["cards"]
        .as_array()
        .expect("cards")
        .clone();
    assert_eq!(
        cards.len(),
        1,
        "the override database must hold exactly one Card"
    );
    assert_eq!(
        cards.first().expect("one card")["title"].as_str(),
        Some(title.as_str())
    );
}

#[then("the environment database holds no Cards")]
fn environment_db_empty(w: &mut CardWorld) {
    assert!(
        w.list_cards().is_empty(),
        "the flag must have outranked DAEMAR_DB"
    );
}

#[then(expr = "db-path with the override reports source {string}")]
fn db_path_reports_flag(w: &mut CardWorld, source: String) {
    let flag_path = w.flag_db_path();
    let env_db = w.db_path();
    let run = run_card_command(|command| {
        command
            .env("DAEMAR_DB", &env_db)
            .args(["db-path", "--db", &flag_path]);
    });
    let json = run.success_json();
    assert_eq!(json["db_path"].as_str(), Some(flag_path.as_str()));
    assert_eq!(json["source"].as_str(), Some(source.as_str()));
}

#[when("a Card is created in an isolated factory home")]
fn create_in_isolated_home(w: &mut CardWorld) {
    let home = TempDir::new().expect("temp home");
    let cwd = TempDir::new().expect("temp cwd");
    let run = run_card_command(|command| {
        command
            .env("HOME", home.path())
            .env_remove("DAEMAR_DB")
            .current_dir(cwd.path())
            .args([
                "create",
                "--title",
                "Default home probe",
                "--producer",
                "test-operator",
                "--producer-kind",
                "operator",
            ]);
    });
    run.success_json();
    w.fake_home = Some(home);
    w.iso_cwd = Some(cwd);
}

#[then("the database exists at .daemar/daemar.db inside that home")]
fn database_in_factory_home(w: &mut CardWorld) {
    let home = w.fake_home.as_ref().expect("an isolated home");
    let db = home.path().join(".daemar").join("daemar.db");
    assert!(
        db.exists(),
        "expected the default database at {}",
        db.display()
    );
}

#[cfg(unix)]
fn unix_mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .unwrap_or_else(|error| panic!("stat {}: {error}", path.display()))
        .permissions()
        .mode()
        & 0o777
}

#[then("the factory home and database are private to the operator")]
fn factory_home_is_private(w: &mut CardWorld) {
    let home = w.fake_home.as_ref().expect("an isolated home");
    let factory = home.path().join(".daemar");
    #[cfg(unix)]
    {
        assert_eq!(
            unix_mode(&factory),
            0o700,
            "factory home must be owner-only"
        );
        assert_eq!(
            unix_mode(&factory.join("daemar.db")),
            0o600,
            "database must be owner-only"
        );
        for side in ["daemar.db-wal", "daemar.db-shm"] {
            let path = factory.join(side);
            if path.exists() {
                assert_eq!(unix_mode(&path), 0o600, "{side} must be owner-only");
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _unused = factory;
    }
}

#[when(expr = "{int} appends are acknowledged and their writers killed immediately")]
fn appends_acknowledged_then_killed(w: &mut CardWorld, count: u64) {
    use std::io::BufRead;
    let card = w.primary_card();
    let db = w.db_path();
    let mut acks = Vec::new();
    for n in 0..count {
        let payload = serde_json::json!({
            "summary": format!("killed writer {n}"),
            "reason": "termination durability",
        })
        .to_string();
        let mut child = Command::new(CARD_BIN)
            .args([
                "append",
                &card,
                "--entry-type",
                "decision",
                "--payload",
                &payload,
                "--producer",
                "claude",
                "--producer-kind",
                "agent",
            ])
            .env("DAEMAR_DB", &db)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn writer");
        let mut ack_line = String::new();
        std::io::BufReader::new(child.stdout.take().expect("piped stdout"))
            .read_line(&mut ack_line)
            .expect("read acknowledgement");
        // SIGKILL as soon as the acknowledgement is readable: no orderly
        // shutdown, no connection close, no final checkpoint.
        child.kill().ok();
        child.wait().expect("reap writer");
        let ack: Value = serde_json::from_str(ack_line.trim()).expect("acknowledgement JSON");
        acks.push(ack);
    }
    w.acks = acks;
}

#[then(expr = "history holds exactly those acknowledged entries at sequences {int} through {int}")]
fn killed_acknowledgements_survive(w: &mut CardWorld, from: u64, through: u64) {
    let card = w.primary_card();
    let entries = w.history(&card);
    assert_sequence_range(&entries, 1, through);
    let mut acked_sequences: Vec<u64> = Vec::new();
    for ack in &w.acks {
        let entry_id = ack["entry_id"].as_str().expect("acknowledged entry_id");
        let sequence = ack["sequence"].as_u64().expect("acknowledged sequence");
        let stored = entries
            .iter()
            .find(|entry| entry["entry_id"].as_str() == Some(entry_id))
            .unwrap_or_else(|| panic!("acknowledged entry {entry_id} lost after SIGKILL"));
        assert_eq!(
            stored["sequence"].as_u64(),
            Some(sequence),
            "sequence changed after SIGKILL"
        );
        assert_eq!(
            stored["card_id"], ack["card_id"],
            "card changed after SIGKILL"
        );
        acked_sequences.push(sequence);
    }
    acked_sequences.sort_unstable();
    let expected: Vec<u64> = (from..=through).collect();
    assert_eq!(
        acked_sequences, expected,
        "acknowledged sequences must cover the range exactly"
    );
}

#[when("a Card is created via a factory-home dotenv pointing at an external directory")]
fn create_via_dotenv_external(w: &mut CardWorld) {
    let home = TempDir::new().expect("temp home");
    let external = TempDir::new().expect("external dir");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(external.path(), std::fs::Permissions::from_mode(0o755))
            .expect("widen external dir");
    }
    let factory = home.path().join(".daemar");
    std::fs::create_dir_all(&factory).expect("factory home");
    let db = external
        .path()
        .join("daemar.db")
        .to_str()
        .expect("utf-8 path")
        .to_owned();
    std::fs::write(
        factory.join(".env"),
        format!(
            "DAEMAR_DB={db}
"
        ),
    )
    .expect("write dotenv");
    let run = run_card_command(|command| {
        command
            .env("HOME", home.path())
            .env_remove("DAEMAR_DB")
            .args([
                "create",
                "--title",
                "Dotenv probe",
                "--producer",
                "test-operator",
                "--producer-kind",
                "operator",
            ]);
    });
    run.success_json();
    w.fake_home = Some(home);
    w.dotenv_db = Some((external, db));
}

#[then("the external directory keeps its permissions")]
fn external_directory_untouched(w: &mut CardWorld) {
    let (external, _db) = w.dotenv_db.as_ref().expect("a dotenv-selected database");
    #[cfg(unix)]
    assert_eq!(
        unix_mode(external.path()),
        0o755,
        "the factory must not tighten an operator-selected directory"
    );
    #[cfg(not(unix))]
    let _unused = external;
}

#[then("the dotenv database holds exactly one Card")]
fn dotenv_database_holds_card(w: &mut CardWorld) {
    let db = w
        .dotenv_db
        .as_ref()
        .expect("a dotenv-selected database")
        .1
        .clone();
    let cards = run_card(&db, &["list"]).success_json()["cards"]
        .as_array()
        .expect("cards")
        .clone();
    assert_eq!(
        cards.len(),
        1,
        "the record must live in the dotenv-selected database"
    );
}

#[when("a Card creation is attempted with a malformed factory-home dotenv")]
fn create_with_malformed_dotenv(w: &mut CardWorld) {
    let home = TempDir::new().expect("temp home");
    let factory = home.path().join(".daemar");
    std::fs::create_dir_all(&factory).expect("factory home");
    std::fs::write(
        factory.join(".env"),
        "not a valid dotenv line
",
    )
    .expect("write dotenv");
    let run = run_card_command(|command| {
        command
            .env("HOME", home.path())
            .env_remove("DAEMAR_DB")
            .args([
                "create",
                "--title",
                "Should fail loudly",
                "--producer",
                "test-operator",
                "--producer-kind",
                "operator",
            ]);
    });
    w.last = Some(run);
    w.fake_home = Some(home);
}

#[then("no default database was created in that factory home")]
fn no_default_database_created(w: &mut CardWorld) {
    let home = w.fake_home.as_ref().expect("an isolated home");
    let default_db = home.path().join(".daemar").join("daemar.db");
    assert!(
        !default_db.exists(),
        "a broken dotenv must not silently create {}",
        default_db.display()
    );
}
