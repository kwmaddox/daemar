use daemar::{ChangeRequestProblem, ChangeRequestRule, preflight};

#[test]
fn preflight_rejects_unreadable_document_shapes_at_the_root() {
    let oversized = vec![b' '; 16 * 1024 + 1];
    let cases: [(&str, &[u8], ChangeRequestRule); 4] = [
        ("oversized", &oversized, ChangeRequestRule::DocumentTooLarge),
        ("non-UTF-8", &[0xff], ChangeRequestRule::InvalidEncoding),
        (
            "invalid JSON",
            br#"{"schema": }"#,
            ChangeRequestRule::InvalidJson,
        ),
        ("non-object", br#"[]"#, ChangeRequestRule::NotAnObject),
    ];

    for (label, source, expected_code) in cases {
        let problems = preflight(source).expect_err(label);
        assert_eq!(problems.len(), 1, "{label}");
        assert_eq!(problems[0].code, expected_code, "{label}");
        assert_eq!(problems[0].pointer, "", "{label}");
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
    assert_eq!(
        diagnostics(&problems),
        [
            (ChangeRequestRule::UnknownField, "/z~1field"),
            (ChangeRequestRule::DuplicateField, "/id"),
            (ChangeRequestRule::UnknownField, "/a~0field"),
            (ChangeRequestRule::UnsupportedVersion, "/schema"),
            (ChangeRequestRule::BadSlug, "/id"),
            (ChangeRequestRule::BlankField, "/objective"),
            (ChangeRequestRule::BadItemCount, "/acceptance_criteria"),
        ]
    );
}

#[test]
fn unknown_field_guidance_includes_the_optional_schema_metadata_field() {
    let source = br#"{
        "schema": "change_request.v1",
        "id": "inspect-runs",
        "objective": "Inspect Workflow Runs.",
        "acceptance_criteria": ["Inspection is read-only."],
        "unexpected": true
    }"#;

    let problems = preflight(source).expect_err("the unknown field should fail Preflight");
    let problem = problems
        .iter()
        .find(|problem| problem.code == ChangeRequestRule::UnknownField)
        .expect("an unknown-field diagnostic should be present");

    assert!(problem.message.contains("$schema"), "{}", problem.message);
    assert!(
        problem.message.contains("optional metadata"),
        "{}",
        problem.message
    );
}

#[test]
fn preflight_ignores_arbitrary_precision_numbers_in_schema_metadata() {
    let source = br#"{
        "$schema": 1e400,
        "schema": "change_request.v1",
        "id": "inspect-runs",
        "objective": "Inspect Workflow Runs.",
        "acceptance_criteria": ["Inspection is read-only."]
    }"#;

    let request = preflight(source).expect("arbitrary-precision `$schema` metadata is ignored");

    assert_eq!(request.id(), "inspect-runs");
}

#[test]
fn preflight_classifies_arbitrary_precision_numbers_by_field_type() {
    let source = br#"{
        "schema": "change_request.v1",
        "id": 1e400,
        "objective": "Inspect Workflow Runs.",
        "acceptance_criteria": ["Inspection is read-only."]
    }"#;

    let problems = preflight(source).expect_err("a numeric `id` should fail its field type rule");

    assert_eq!(
        diagnostics(&problems),
        [(ChangeRequestRule::WrongType, "/id")]
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
    assert_eq!(
        diagnostics(&problems),
        [
            (ChangeRequestRule::WrongType, "/schema"),
            (ChangeRequestRule::WrongType, "/id"),
            (ChangeRequestRule::WrongType, "/objective"),
            (ChangeRequestRule::WrongType, "/acceptance_criteria/0"),
            (ChangeRequestRule::WrongType, "/acceptance_criteria/2"),
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
    assert_eq!(
        diagnostics(&problems),
        [
            (ChangeRequestRule::FieldTooLong, "/id"),
            (ChangeRequestRule::BadSlug, "/id"),
            (ChangeRequestRule::BlankField, "/objective"),
            (ChangeRequestRule::FieldTooLong, "/objective"),
            (ChangeRequestRule::BadItemCount, "/acceptance_criteria"),
            (ChangeRequestRule::BlankField, "/acceptance_criteria/0"),
            (ChangeRequestRule::FieldTooLong, "/acceptance_criteria/0"),
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

    assert_eq!(request.schema(), "change_request.v1");
    assert_eq!(request.id().chars().count(), 64);
    assert_eq!(request.objective().chars().count(), 4_096);
    assert_eq!(request.acceptance_criteria().len(), 20);
    assert_eq!(request.acceptance_criteria()[0].chars().count(), 1_024);
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
        .filter(|problem| problem.code == ChangeRequestRule::DuplicateField)
        .map(|problem| problem.pointer.as_str())
        .collect();

    assert_eq!(duplicates, ["/id", "/id"]);
}

#[test]
fn preflight_validates_every_duplicate_field_occurrence_in_document_order() {
    let source = br#"{
        "schema": 7,
        "schema": "change_request.v2",
        "id": 8,
        "id": "Bad_Id",
        "objective": [],
        "objective": " ",
        "acceptance_criteria": false,
        "acceptance_criteria": [9, ""]
    }"#;

    let problems = preflight(source).expect_err("every duplicate occurrence should be validated");

    assert_eq!(
        diagnostics(&problems),
        [
            (ChangeRequestRule::DuplicateField, "/schema"),
            (ChangeRequestRule::DuplicateField, "/id"),
            (ChangeRequestRule::DuplicateField, "/objective"),
            (ChangeRequestRule::DuplicateField, "/acceptance_criteria",),
            (ChangeRequestRule::WrongType, "/schema"),
            (ChangeRequestRule::UnsupportedVersion, "/schema"),
            (ChangeRequestRule::WrongType, "/id"),
            (ChangeRequestRule::BadSlug, "/id"),
            (ChangeRequestRule::WrongType, "/objective"),
            (ChangeRequestRule::BlankField, "/objective"),
            (ChangeRequestRule::WrongType, "/acceptance_criteria"),
            (ChangeRequestRule::WrongType, "/acceptance_criteria/0"),
            (ChangeRequestRule::BlankField, "/acceptance_criteria/1"),
        ]
    );
}

#[test]
fn preflight_interleaves_missing_and_present_field_problems_in_contract_order() {
    let problems = preflight(br#"{"schema":"change_request.v2"}"#)
        .expect_err("missing and invalid fields should fail Preflight");
    assert_eq!(
        diagnostics(&problems),
        [
            (ChangeRequestRule::UnsupportedVersion, "/schema"),
            (ChangeRequestRule::MissingField, "/id"),
            (ChangeRequestRule::MissingField, "/objective"),
            (ChangeRequestRule::MissingField, "/acceptance_criteria"),
        ]
    );
}

#[test]
fn problem_display_renders_the_empty_json_pointer_as_the_document_root() {
    let problem = preflight(br#"[]"#).expect_err("an array should fail Preflight");

    assert_eq!(
        problem[0].to_string(),
        "[not_an_object] top level must be a JSON object (at \"\")"
    );
}

#[test]
fn preflight_accepts_contract_minima_at_the_exact_document_size_limit() {
    let mut source =
        br#"{"schema":"change_request.v1","id":"a","objective":"x","acceptance_criteria":["x"]}"#
            .to_vec();
    source.resize(16 * 1024, b' ');

    let request = preflight(&source).expect("the inclusive document limit should pass");

    assert_eq!(request.id(), "a");
    assert_eq!(request.objective(), "x");
    assert_eq!(request.acceptance_criteria(), ["x"]);
}

#[test]
fn preflight_reports_empty_lower_bounds() {
    let source = br#"{
        "schema": "change_request.v1",
        "id": "",
        "objective": "",
        "acceptance_criteria": []
    }"#;

    let problems = preflight(source).expect_err("empty bounded values should fail Preflight");
    assert_eq!(
        diagnostics(&problems),
        [
            (ChangeRequestRule::FieldTooLong, "/id"),
            (ChangeRequestRule::BadSlug, "/id"),
            (ChangeRequestRule::BlankField, "/objective"),
            (ChangeRequestRule::BadItemCount, "/acceptance_criteria"),
        ]
    );
}

#[test]
fn preflight_enforces_every_slug_grammar_edge() {
    for valid in ["a", "0", "a-0", "request-123"] {
        let source = request_with_id(valid);
        preflight(source.as_bytes()).unwrap_or_else(|problems| {
            panic!("valid slug {valid:?} failed Preflight: {problems:?}")
        });
    }

    for invalid in ["-a", "a-", "a--b", "A", "a_b", "é"] {
        let source = request_with_id(invalid);
        let Err(problems) = preflight(source.as_bytes()) else {
            panic!("invalid slug {invalid:?} passed Preflight");
        };
        assert!(
            problems
                .iter()
                .any(|problem| problem.code == ChangeRequestRule::BadSlug),
            "invalid slug {invalid:?}: {problems:?}"
        );
    }
}

fn diagnostics(problems: &[ChangeRequestProblem]) -> Vec<(ChangeRequestRule, &str)> {
    problems
        .iter()
        .map(|problem| (problem.code, problem.pointer.as_str()))
        .collect()
}

fn request_with_id(id: &str) -> String {
    serde_json::to_string(&serde_json::json!({
        "schema": "change_request.v1",
        "id": id,
        "objective": "Inspect Workflow Runs.",
        "acceptance_criteria": ["Inspection is read-only."],
    }))
    .expect("fixture should serialize")
}
