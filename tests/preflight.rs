use daemar::preflight;

#[test]
fn preflight_rejects_unreadable_document_shapes_at_the_root() {
    let oversized = vec![b' '; 16 * 1024 + 1];
    let cases: [(&str, &[u8], &str); 4] = [
        ("oversized", &oversized, "document_too_large"),
        ("non-UTF-8", &[0xff], "invalid_encoding"),
        ("invalid JSON", br#"{"schema": }"#, "invalid_json"),
        ("non-object", br#"[]"#, "not_an_object"),
    ];

    for (label, source, expected_code) in cases {
        let problems = preflight(source).expect_err(label);
        assert_eq!(problems.len(), 1, "{label}");
        assert_eq!(problems[0].code.as_str(), expected_code, "{label}");
        assert_eq!(problems[0].pointer, "/", "{label}");
    }
}

#[test]
fn preflight_reports_key_problems_in_document_order_then_fields_in_contract_order() {
    let source = br#"{
        "z/field": 0,
        "schema": "change_request.v2",
        "id": "first-id",
        "id": "Second_Id",
        "a~field": 0,
        "objective": " ",
        "acceptance_criteria": [],
        "$schema": {"ignored": true}
    }"#;

    let problems = preflight(source).expect_err("the request should fail Preflight");
    let diagnostics: Vec<_> = problems
        .iter()
        .map(|problem| (problem.code.as_str(), problem.pointer.as_str()))
        .collect();

    assert_eq!(
        diagnostics,
        [
            ("unknown_field", "/z~1field"),
            ("duplicate_field", "/id"),
            ("unknown_field", "/a~0field"),
            ("unsupported_version", "/schema"),
            ("bad_slug", "/id"),
            ("blank_field", "/objective"),
            ("bad_item_count", "/acceptance_criteria"),
        ]
    );
}

#[test]
fn preflight_reports_wrong_types_in_contract_and_item_order() {
    let source = br#"{
        "schema": 1,
        "id": false,
        "objective": [],
        "acceptance_criteria": [7, "usable", null]
    }"#;

    let problems = preflight(source).expect_err("wrong types should fail Preflight");
    let diagnostics: Vec<_> = problems
        .iter()
        .map(|problem| (problem.code.as_str(), problem.pointer.as_str()))
        .collect();

    assert_eq!(
        diagnostics,
        [
            ("wrong_type", "/schema"),
            ("wrong_type", "/id"),
            ("wrong_type", "/objective"),
            ("wrong_type", "/acceptance_criteria/0"),
            ("wrong_type", "/acceptance_criteria/2"),
        ]
    );
}

#[test]
fn preflight_reports_every_applicable_bound_problem_without_suppression() {
    let id = format!("{}-", "A".repeat(64));
    let objective = " ".repeat(4_097);
    let long_blank_criterion = "\u{2003}".repeat(1_025);
    let mut criteria = vec![long_blank_criterion];
    criteria.extend((1..21).map(|index| format!("criterion {index}")));
    let criteria = serde_json::to_string(&criteria).expect("fixture should serialize");
    let source = format!(
        r#"{{
            "schema": "change_request.v1",
            "id": "{id}",
            "objective": "{objective}",
            "acceptance_criteria": {criteria}
        }}"#
    );

    let problems =
        preflight(source.as_bytes()).expect_err("every applicable bound should be reported");
    let diagnostics: Vec<_> = problems
        .iter()
        .map(|problem| (problem.code.as_str(), problem.pointer.as_str()))
        .collect();

    assert_eq!(
        diagnostics,
        [
            ("field_too_long", "/id"),
            ("bad_slug", "/id"),
            ("blank_field", "/objective"),
            ("field_too_long", "/objective"),
            ("bad_item_count", "/acceptance_criteria"),
            ("blank_field", "/acceptance_criteria/0"),
            ("field_too_long", "/acceptance_criteria/0"),
        ]
    );
}

#[test]
fn preflight_accepts_the_complete_shape_at_every_inclusive_maximum() {
    let id = "a".repeat(64);
    let objective = "é".repeat(4_096);
    let criterion = "é".repeat(1_024);
    let mut criteria = vec![criterion];
    criteria.extend((1..20).map(|index| format!("criterion {index}")));
    let source = serde_json::to_vec(&serde_json::json!({
        "$schema": {"any value is ignored": true},
        "schema": "change_request.v1",
        "id": id,
        "objective": objective,
        "acceptance_criteria": criteria,
    }))
    .expect("fixture should serialize");

    let request = preflight(&source).expect("inclusive maxima should pass Preflight");

    assert_eq!(request.schema, "change_request.v1");
    assert_eq!(request.id.chars().count(), 64);
    assert_eq!(request.objective.chars().count(), 4_096);
    assert_eq!(request.acceptance_criteria.len(), 20);
    assert_eq!(request.acceptance_criteria[0].chars().count(), 1_024);
}

#[test]
fn preflight_reports_every_repeated_field_occurrence() {
    let source = br#"{
        "schema": "change_request.v1",
        "id": "first-id",
        "id": "second-id",
        "id": "third-id",
        "objective": "Inspect Workflow Runs.",
        "acceptance_criteria": ["Inspection is read-only."]
    }"#;

    let problems = preflight(source).expect_err("duplicates should fail Preflight");
    let duplicates: Vec<_> = problems
        .iter()
        .filter(|problem| problem.code.as_str() == "duplicate_field")
        .map(|problem| problem.pointer.as_str())
        .collect();

    assert_eq!(duplicates, ["/id", "/id"]);
}
