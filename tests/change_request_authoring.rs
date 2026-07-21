use std::fs;

use daemar::{change_request_schema_document, preflight};

#[test]
fn checked_in_change_request_schema_matches_the_canonical_rust_contract() {
    let checked_in = fs::read_to_string("docs/change-request.schema.json")
        .expect("the checked-in Change Request schema should be readable");

    assert_eq!(checked_in, change_request_schema_document());
}

#[test]
fn complete_authoring_example_is_accepted_by_preflight() {
    let document = fs::read("docs/examples/change-request.json")
        .expect("the complete Change Request example should be readable");

    let request = preflight(&document).expect("the complete example should pass Preflight");

    assert_eq!(request.schema(), "change_request.v1");
    assert_eq!(request.id(), "add-run-inspection");
    assert!(!request.objective().is_empty());
    assert_eq!(request.acceptance_criteria().len(), 3);
}

#[test]
fn generated_schema_captures_preflights_unicode_blank_rule() {
    let source = serde_json::to_vec(&serde_json::json!({
        "schema": "change_request.v1",
        "id": "unicode-whitespace",
        "objective": "\u{0085}",
        "acceptance_criteria": ["\u{0085}"],
    }))
    .expect("the Unicode whitespace fixture should serialize");
    let problems = preflight(&source).expect_err("Unicode whitespace should be blank");
    assert_eq!(problems.len(), 2);

    let schema: serde_json::Value = serde_json::from_str(&change_request_schema_document())
        .expect("the generated schema should be valid JSON");
    for pointer in [
        "/properties/objective/pattern",
        "/properties/acceptance_criteria/items/pattern",
    ] {
        let pattern = schema
            .pointer(pointer)
            .and_then(serde_json::Value::as_str)
            .expect("non-blank string fields should have a pattern");
        assert!(
            pattern.contains(r"\u{0085}"),
            "{pointer} must cover the Unicode whitespace rejected by Preflight: {pattern}"
        );
    }
}
