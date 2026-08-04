use std::fs;

use daemar::{ChangeRequestRule, change_request_schema_document, preflight};
use serde_json::{Value, json};

const WHITE_SPACE: &[(char, &str)] = &[
    ('\u{0009}', "tab"),
    ('\u{000a}', "line feed"),
    ('\u{000b}', "vertical tab"),
    ('\u{000c}', "form feed"),
    ('\u{000d}', "carriage return"),
    ('\u{0020}', "space"),
    ('\u{0085}', "next line"),
    ('\u{00a0}', "no-break space"),
    ('\u{1680}', "ogham space mark"),
    ('\u{2000}', "en quad"),
    ('\u{2001}', "em quad"),
    ('\u{2002}', "en space"),
    ('\u{2003}', "em space"),
    ('\u{2004}', "three-per-em space"),
    ('\u{2005}', "four-per-em space"),
    ('\u{2006}', "six-per-em space"),
    ('\u{2007}', "figure space"),
    ('\u{2008}', "punctuation space"),
    ('\u{2009}', "thin space"),
    ('\u{200a}', "hair space"),
    ('\u{2028}', "line separator"),
    ('\u{2029}', "paragraph separator"),
    ('\u{202f}', "narrow no-break space"),
    ('\u{205f}', "medium mathematical space"),
    ('\u{3000}', "ideographic space"),
];

const NON_WHITE_SPACE_NEAR_MISSES: &[(char, &str)] = &[
    ('\u{0008}', "backspace"),
    ('\u{000e}', "shift out"),
    ('\u{200b}', "zero-width space"),
    ('\u{2060}', "word joiner"),
    ('\u{feff}', "zero-width no-break space"),
];

#[test]
fn checked_in_change_request_schema_matches_the_generated_authoring_contract() {
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
fn generated_schema_is_valid_draft_2020_12() {
    let schema: Value = serde_json::from_str(&change_request_schema_document())
        .expect("the generated schema should be valid JSON");
    assert!(
        jsonschema::draft202012::meta::is_valid(&schema),
        "the generated schema should be a valid Draft 2020-12 schema"
    );
    jsonschema::draft202012::new(&schema)
        .expect("the generated Draft 2020-12 schema should compile");
}

#[test]
fn schema_and_preflight_agree_on_document_shape() {
    let validator = schema_validator();
    assert_parity(
        &validator,
        "complete object",
        request("request", "x", "x"),
        true,
        None,
    );

    for (label, document) in [
        ("null root", Value::Null),
        ("boolean root", json!(true)),
        ("number root", json!(1)),
        ("string root", json!("request")),
        ("array root", json!([])),
    ] {
        assert_parity(
            &validator,
            label,
            document,
            false,
            Some((ChangeRequestRule::NotAnObject, "")),
        );
    }

    for field in ["schema", "id", "objective", "acceptance_criteria"] {
        let mut document = request("request", "x", "x");
        document
            .as_object_mut()
            .expect("the fixture should be an object")
            .remove(field);
        assert_parity(
            &validator,
            &format!("missing {field}"),
            document,
            false,
            Some((ChangeRequestRule::MissingField, &format!("/{field}"))),
        );
    }

    let mut document = request("request", "x", "x");
    document
        .as_object_mut()
        .expect("the fixture should be an object")
        .insert("priority".to_owned(), json!("urgent"));
    assert_parity(
        &validator,
        "unknown field",
        document,
        false,
        Some((ChangeRequestRule::UnknownField, "/priority")),
    );
}

#[test]
fn schema_and_preflight_agree_on_field_types_and_version() {
    let validator = schema_validator();
    for (field, value) in [
        ("schema", json!(false)),
        ("id", json!(42)),
        ("objective", json!([])),
        ("acceptance_criteria", json!("done")),
    ] {
        let mut document = request("request", "x", "x");
        document
            .as_object_mut()
            .expect("the fixture should be an object")
            .insert(field.to_owned(), value);
        assert_parity(
            &validator,
            &format!("wrong type for {field}"),
            document,
            false,
            Some((ChangeRequestRule::WrongType, &format!("/{field}"))),
        );
    }

    let mut document = request("request", "x", "x");
    document
        .as_object_mut()
        .expect("the fixture should be an object")
        .insert("schema".to_owned(), json!("change_request.v2"));
    assert_parity(
        &validator,
        "unsupported schema version",
        document,
        false,
        Some((ChangeRequestRule::UnsupportedVersion, "/schema")),
    );
}

#[test]
fn schema_and_preflight_agree_on_slug_and_id_length() {
    let validator = schema_validator();

    for id in ["a", "0", "a-0", "request-123"] {
        assert_parity(
            &validator,
            &format!("valid id {id:?}"),
            request(id, "x", "x"),
            true,
            None,
        );
    }
    for id in ["", "-a", "a-", "a--b", "A", "a_b", "é", "a\n"] {
        assert_parity(
            &validator,
            &format!("invalid id {id:?}"),
            request(id, "x", "x"),
            false,
            Some((
                if id.is_empty() {
                    ChangeRequestRule::FieldTooShort
                } else {
                    ChangeRequestRule::BadSlug
                },
                "/id",
            )),
        );
    }

    let longest_id = "a".repeat(64);
    assert_parity(
        &validator,
        "a 64-character id",
        request(&longest_id, "x", "x"),
        true,
        None,
    );
    let overlong_id = "a".repeat(65);
    assert_parity(
        &validator,
        "a 65-character id",
        request(&overlong_id, "x", "x"),
        false,
        Some((ChangeRequestRule::FieldTooLong, "/id")),
    );
}

#[test]
fn schema_and_preflight_agree_on_objective_bounds() {
    let validator = schema_validator();
    assert_parity(
        &validator,
        "maximum objective length",
        request("request", &"x".repeat(4_096), "x"),
        true,
        None,
    );
    assert_parity(
        &validator,
        "objective over maximum length",
        request("request", &"x".repeat(4_097), "x"),
        false,
        Some((ChangeRequestRule::FieldTooLong, "/objective")),
    );

    for &(character, name) in WHITE_SPACE {
        let blank = character.to_string();
        assert_parity(
            &validator,
            &format!("{name} is a blank objective"),
            request("white-space", &blank, "x"),
            false,
            Some((ChangeRequestRule::BlankField, "/objective")),
        );
    }
    for &(character, name) in NON_WHITE_SPACE_NEAR_MISSES {
        let non_blank = character.to_string();
        assert_parity(
            &validator,
            &format!("{name} is a nonblank objective"),
            request("near-miss", &non_blank, "x"),
            true,
            None,
        );
    }
}

#[test]
fn schema_and_preflight_agree_on_acceptance_criteria_bounds() {
    let validator = schema_validator();

    for (label, criteria, expected_acceptance) in [
        ("minimum criterion count", vec![json!("x")], true),
        ("maximum criterion count", vec![json!("x"); 20], true),
        ("empty criteria", vec![], false),
        ("too many criteria", vec![json!("x"); 21], false),
    ] {
        assert_parity(
            &validator,
            label,
            request_with_criteria(criteria),
            expected_acceptance,
            (!expected_acceptance)
                .then_some((ChangeRequestRule::BadItemCount, "/acceptance_criteria")),
        );
    }

    assert_parity(
        &validator,
        "non-string criterion",
        request_with_criteria(vec![json!(42)]),
        false,
        Some((ChangeRequestRule::WrongType, "/acceptance_criteria/0")),
    );
    assert_parity(
        &validator,
        "maximum criterion length",
        request_with_criteria(vec![json!("x".repeat(1_024))]),
        true,
        None,
    );
    assert_parity(
        &validator,
        "criterion over maximum length",
        request_with_criteria(vec![json!("x".repeat(1_025))]),
        false,
        Some((ChangeRequestRule::FieldTooLong, "/acceptance_criteria/0")),
    );

    for &(character, name) in WHITE_SPACE {
        assert_parity(
            &validator,
            &format!("{name} is a blank criterion"),
            request("white-space", "x", &character.to_string()),
            false,
            Some((ChangeRequestRule::BlankField, "/acceptance_criteria/0")),
        );
    }
    for &(character, name) in NON_WHITE_SPACE_NEAR_MISSES {
        assert_parity(
            &validator,
            &format!("{name} is a nonblank criterion"),
            request("near-miss", "x", &character.to_string()),
            true,
            None,
        );
    }
}

#[test]
fn schema_and_preflight_agree_on_schema_hint() {
    let validator = schema_validator();
    assert_parity(
        &validator,
        "the editor hint may be omitted",
        request("schema-hint", "x", "x"),
        true,
        None,
    );
    for hint in [
        json!("../change-request.schema.json"),
        json!("https://example.com/change-request.schema.json"),
    ] {
        assert_parity(
            &validator,
            &format!("string editor hint {hint}"),
            request_with_schema_hint(hint),
            true,
            None,
        );
    }
    for hint in [json!(""), json!(" \t")] {
        assert_parity(
            &validator,
            &format!("blank editor hint {hint}"),
            request_with_schema_hint(hint),
            false,
            Some((ChangeRequestRule::BlankField, "/$schema")),
        );
    }
    for hint in [json!(null), json!(true), json!(42), json!([]), json!({})] {
        assert_parity(
            &validator,
            &format!("non-string editor hint {hint}"),
            request_with_schema_hint(hint),
            false,
            Some((ChangeRequestRule::WrongType, "/$schema")),
        );
    }
}

fn schema_validator() -> jsonschema::Validator {
    let schema = serde_json::from_str(&change_request_schema_document())
        .expect("the generated schema should be valid JSON");
    jsonschema::draft202012::new(&schema)
        .expect("the generated Draft 2020-12 schema should compile")
}

fn request(id: &str, objective: &str, criterion: &str) -> Value {
    json!({
        "schema": "change_request.v1",
        "id": id,
        "objective": objective,
        "acceptance_criteria": [criterion],
    })
}

fn request_with_schema_hint(hint: Value) -> Value {
    let mut document = request("schema-hint", "x", "x");
    document
        .as_object_mut()
        .expect("the fixture should be an object")
        .insert("$schema".to_owned(), hint);
    document
}

fn request_with_criteria(criteria: Vec<Value>) -> Value {
    json!({
        "schema": "change_request.v1",
        "id": "request",
        "objective": "x",
        "acceptance_criteria": criteria,
    })
}

fn assert_parity(
    validator: &jsonschema::Validator,
    label: &str,
    document: Value,
    expected_acceptance: bool,
    expected_problem: Option<(ChangeRequestRule, &str)>,
) {
    let encoded = serde_json::to_vec(&document).expect("the fixture should serialize");
    let preflight_result = preflight(&encoded);
    let schema_accepts = validator.is_valid(&document);

    assert_eq!(
        preflight_result.is_ok(),
        expected_acceptance,
        "Preflight disagreed with the corpus for {label}: {preflight_result:?}"
    );
    assert_eq!(
        schema_accepts,
        expected_acceptance,
        "the generated schema disagreed with the corpus for {label}: {:?}",
        validator
            .iter_errors(&document)
            .map(|error| error.to_string())
            .collect::<Vec<_>>()
    );

    if let Some((code, pointer)) = expected_problem {
        let problems = preflight_result.expect_err(label);
        assert!(
            problems
                .iter()
                .any(|problem| problem.code == code && problem.pointer == pointer),
            "Preflight did not report {code} at {pointer} for {label}: {problems:?}"
        );
    }
}
