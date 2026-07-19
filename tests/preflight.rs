use daemar::{PreflightError, preflight};

const MAX_DOCUMENT_BYTES: usize = 16 * 1024;

#[test]
fn preflight_reports_unknown_and_missing_fields_in_stable_order() {
    let diagnostics = preflight(br#"{"priority":"high"}"#).expect_err("request should be invalid");

    let observed = codes_and_pointers(&diagnostics);

    assert_eq!(
        observed,
        vec![
            "unknown_field /priority",
            "missing_field /schema",
            "missing_field /id",
            "missing_field /objective",
            "missing_field /acceptance_criteria",
        ]
    );
}

#[test]
fn preflight_rejects_invalid_document_envelopes_before_field_checks() {
    let oversized = vec![b' '; MAX_DOCUMENT_BYTES + 1];
    let cases: [(&str, &[u8], &str); 4] = [
        ("invalid encoding", &[0xff], "invalid_encoding"),
        ("oversized document", &oversized, "document_too_large"),
        ("invalid JSON", br#"{"schema":}"#, "invalid_json"),
        ("non-object JSON", br#"[]"#, "not_an_object"),
    ];

    for (name, raw, expected_code) in cases {
        let diagnostics = preflight(raw).expect_err(name);

        assert_eq!(
            codes_and_pointers(&diagnostics),
            [format!("{expected_code} /")],
            "{name}"
        );
        assert_eq!(diagnostics.len(), 1, "{name}");
    }
}

#[test]
fn preflight_rejects_duplicate_fields_but_ignores_the_optional_schema_hint() {
    let duplicate = r#"{
        "$schema": {"editor": "hint only"},
        "schema": "change_request.v1",
        "id": "first-id",
        "id": "second-id",
        "objective": "Do the thing.",
        "acceptance_criteria": ["It is done."]
    }"#;

    let diagnostics =
        preflight(duplicate.as_bytes()).expect_err("duplicate field should be rejected");
    assert_eq!(codes_and_pointers(&diagnostics), ["duplicate_field /id"]);

    let unique = duplicate.replacen("\n        \"id\": \"first-id\",", "", 1);
    assert!(preflight(unique.as_bytes()).is_ok());
}

#[test]
fn preflight_reports_each_canonical_field_type_problem_in_contract_order() {
    let raw = br#"{
        "schema": false,
        "id": 22,
        "objective": [],
        "acceptance_criteria": {}
    }"#;

    let diagnostics = preflight(raw).expect_err("field types should be enforced");
    assert_eq!(
        codes_and_pointers(&diagnostics),
        vec![
            "wrong_type /schema",
            "wrong_type /id",
            "wrong_type /objective",
            "wrong_type /acceptance_criteria",
        ]
    );
}

#[test]
fn preflight_enforces_the_change_request_version_and_slug_grammar() {
    let raw = br#"{
        "schema": "change_request.v2",
        "id": "Not--Kebab-Case",
        "objective": "Do the thing.",
        "acceptance_criteria": ["It is done."]
    }"#;

    let diagnostics = preflight(raw).expect_err("version and slug should be enforced");
    assert_eq!(
        codes_and_pointers(&diagnostics),
        ["unsupported_version /schema", "bad_slug /id"]
    );
}

#[test]
fn preflight_reports_blank_length_and_item_count_problems_in_field_order() {
    let mut criteria = vec![
        serde_json::json!("   "),
        serde_json::json!("x".repeat(1_025)),
        serde_json::json!(22),
    ];
    criteria.extend((3..21).map(|index| serde_json::json!(format!("criterion-{index}"))));
    let raw = serde_json::to_vec(&serde_json::json!({
        "schema": "change_request.v1",
        "id": "a".repeat(65),
        "objective": "x".repeat(4_097),
        "acceptance_criteria": criteria,
    }))
    .expect("fixture should serialize");

    let diagnostics = preflight(&raw).expect_err("bounds should be enforced");
    assert_eq!(
        codes_and_pointers(&diagnostics),
        vec![
            "field_too_long /id",
            "field_too_long /objective",
            "bad_item_count /acceptance_criteria",
            "blank_field /acceptance_criteria/0",
            "field_too_long /acceptance_criteria/1",
            "wrong_type /acceptance_criteria/2",
        ]
    );
}

fn codes_and_pointers(diagnostics: &[PreflightError]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| format!("{} {}", diagnostic.code(), diagnostic.pointer()))
        .collect()
}

#[test]
fn preflight_returns_the_complete_typed_change_request() {
    let raw = br#"{
        "$schema": "../docs/change-request.schema.json",
        "schema": "change_request.v1",
        "id": "add-run-inspection",
        "objective": "List recent Workflow Runs.",
        "acceptance_criteria": ["Runs appear newest first.", "Inspection performs no writes."]
    }"#;

    let request = preflight(raw).expect("request should be valid");

    assert_eq!(request.schema(), "change_request.v1");
    assert_eq!(request.id(), "add-run-inspection");
    assert_eq!(request.objective(), "List recent Workflow Runs.");
    assert_eq!(
        request.acceptance_criteria(),
        [
            "Runs appear newest first.",
            "Inspection performs no writes."
        ]
    );
}
