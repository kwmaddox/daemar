use std::collections::HashSet;
use std::fmt;

use serde::de::{IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};

struct PreflightPolicy {
    schema_version: ChangeRequestSchemaVersion,
    max_document_bytes: usize,
    min_id_characters: usize,
    max_id_characters: usize,
    max_objective_characters: usize,
    min_acceptance_criteria: usize,
    max_acceptance_criteria: usize,
    max_criterion_characters: usize,
    json_schema_id_pattern: &'static str,
    json_schema_non_blank_pattern: &'static str,
    accepted_fields: &'static [&'static str],
}

const PREFLIGHT_POLICY: PreflightPolicy = PreflightPolicy {
    schema_version: ChangeRequestSchemaVersion::V1,
    max_document_bytes: 16 * 1024,
    min_id_characters: 1,
    max_id_characters: 64,
    max_objective_characters: 4_096,
    min_acceptance_criteria: 1,
    max_acceptance_criteria: 20,
    max_criterion_characters: 1_024,
    json_schema_id_pattern: r"^[a-z0-9]+(-[a-z0-9]+)*$",
    json_schema_non_blank_pattern: r"[^\u0009-\u000d\u0020\u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]",
    accepted_fields: &[
        "schema",
        "id",
        "objective",
        "acceptance_criteria",
        "$schema",
    ],
};

/// The typed result of a Change Request that passed Preflight.
///
/// The authoring document is a UTF-8 JSON object no larger than 16 KiB. Duplicate keys and
/// document size are enforced by Preflight because JSON Schema operates on parsed values.
#[derive(Debug, PartialEq, Eq)]
pub struct ChangeRequest {
    /// Contract version. Exact match required; no negotiation.
    schema: ChangeRequestSchemaVersion,
    /// Human-authored slug identifying this Change Request.
    id: ChangeRequestSlug,
    /// What the Workflow Run should achieve.
    objective: String,
    /// How a human reviewer judges success. Daemar does not machine-check these criteria.
    acceptance_criteria: Vec<String>,
}

impl ChangeRequest {
    pub fn schema(&self) -> &str {
        self.schema.as_str()
    }

    pub fn id(&self) -> &str {
        &self.id.0
    }

    pub fn objective(&self) -> &str {
        &self.objective
    }

    pub fn acceptance_criteria(&self) -> &[String] {
        &self.acceptance_criteria
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChangeRequestSchemaVersion {
    V1,
}

impl ChangeRequestSchemaVersion {
    const fn as_str(self) -> &'static str {
        match self {
            Self::V1 => "change_request.v1",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        if value == Self::V1.as_str() {
            Some(Self::V1)
        } else {
            None
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ChangeRequestSlug(String);

#[derive(Debug, PartialEq, Eq)]
pub struct ChangeRequestProblem {
    pub code: ChangeRequestRule,
    pub pointer: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeRequestRule {
    DocumentTooLarge,
    InvalidEncoding,
    InvalidJson,
    NotAnObject,
    DuplicateField,
    UnknownField,
    MissingField,
    WrongType,
    UnsupportedVersion,
    FieldTooShort,
    FieldTooLong,
    BadItemCount,
    BadSlug,
    BlankField,
}

impl fmt::Display for ChangeRequestRule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::DocumentTooLarge => "document_too_large",
            Self::InvalidEncoding => "invalid_encoding",
            Self::InvalidJson => "invalid_json",
            Self::NotAnObject => "not_an_object",
            Self::DuplicateField => "duplicate_field",
            Self::UnknownField => "unknown_field",
            Self::MissingField => "missing_field",
            Self::WrongType => "wrong_type",
            Self::UnsupportedVersion => "unsupported_version",
            Self::FieldTooShort => "field_too_short",
            Self::FieldTooLong => "field_too_long",
            Self::BadItemCount => "bad_item_count",
            Self::BadSlug => "bad_slug",
            Self::BlankField => "blank_field",
        })
    }
}

impl fmt::Display for ChangeRequestProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "[{}] {} (at ", self.code, self.message)?;
        if self.pointer.is_empty() {
            formatter.write_str("\"\"")?;
        } else {
            formatter.write_str(&self.pointer)?;
        }
        formatter.write_str(")")
    }
}

pub fn preflight(
    change_request_document: &[u8],
) -> Result<ChangeRequest, Vec<ChangeRequestProblem>> {
    let policy = &PREFLIGHT_POLICY;
    if change_request_document.len() > policy.max_document_bytes {
        return Err(one_problem(
            ChangeRequestRule::DocumentTooLarge,
            "document is larger than 16 KiB",
        ));
    }
    let text = std::str::from_utf8(change_request_document).map_err(|_| {
        one_problem(
            ChangeRequestRule::InvalidEncoding,
            "document is not valid UTF-8",
        )
    })?;
    if !text.trim_start().starts_with('{') {
        serde_json::from_str::<IgnoredAny>(text).map_err(|error| {
            one_problem(
                ChangeRequestRule::InvalidJson,
                format!("not valid JSON: {error}"),
            )
        })?;
        return Err(one_problem(
            ChangeRequestRule::NotAnObject,
            "top level must be a JSON object",
        ));
    }
    let document: RawDocument = serde_json::from_str(text).map_err(|error| {
        one_problem(
            ChangeRequestRule::InvalidJson,
            format!("not valid JSON: {error}"),
        )
    })?;
    let Some(object) = document.object else {
        return Err(one_problem(
            ChangeRequestRule::NotAnObject,
            "top level must be a JSON object",
        ));
    };

    validate_object(object, policy)
}

pub fn change_request_document_byte_limit() -> usize {
    PREFLIGHT_POLICY.max_document_bytes
}

/// Generates the editor-facing JSON Schema for the Change Request accepted by Preflight.
pub fn change_request_schema_document() -> String {
    let policy = &PREFLIGHT_POLICY;
    let schema = json!({
        "$id": "change-request.schema.json",
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "additionalProperties": false,
        "description": "The authoring contract for a Change Request accepted by Preflight. Document size and duplicate keys are enforced by Preflight because JSON Schema operates on parsed values.",
        "properties": {
            "$schema": {
                "description": "Optional non-blank path or URL that associates the document with this schema. Daemar validates it, then discards it.",
                "pattern": policy.json_schema_non_blank_pattern,
                "type": "string",
            },
            "acceptance_criteria": {
                "description": "How a human reviewer judges success. Daemar does not machine-check these criteria.",
                "items": {
                    "maxLength": policy.max_criterion_characters,
                    "pattern": policy.json_schema_non_blank_pattern,
                    "type": "string",
                },
                "maxItems": policy.max_acceptance_criteria,
                "minItems": policy.min_acceptance_criteria,
                "type": "array",
            },
            "id": {
                "description": "Human-authored slug identifying this Change Request.",
                "maxLength": policy.max_id_characters,
                "minLength": policy.min_id_characters,
                "pattern": policy.json_schema_id_pattern,
                "type": "string",
            },
            "objective": {
                "description": "What the Workflow Run should achieve.",
                "maxLength": policy.max_objective_characters,
                "pattern": policy.json_schema_non_blank_pattern,
                "type": "string",
            },
            "schema": {
                "const": policy.schema_version.as_str(),
                "description": "Contract version. Exact match required; no negotiation.",
                "type": "string",
            },
        },
        "required": ["schema", "id", "objective", "acceptance_criteria"],
        "title": "Change Request",
        "type": "object",
    });
    let schema_fields: HashSet<_> = schema
        .pointer("/properties")
        .and_then(Value::as_object)
        .expect("Change Request schema should contain properties")
        .keys()
        .map(String::as_str)
        .collect();
    let accepted_fields: HashSet<_> = policy.accepted_fields.iter().copied().collect();
    assert_eq!(
        schema_fields, accepted_fields,
        "schema fields must match the fields accepted by Preflight"
    );

    let mut document =
        serde_json::to_string_pretty(&schema).expect("JSON Schema should serialize as JSON");
    document.push('\n');
    document
}

fn validate_object(
    object: RawObject,
    policy: &PreflightPolicy,
) -> Result<ChangeRequest, Vec<ChangeRequestProblem>> {
    let mut problems = Vec::new();

    for field in &object.fields {
        if field.repeated {
            problems.push(problem(
                ChangeRequestRule::DuplicateField,
                pointer_for_key(&field.name),
                format!("duplicate field `{}`", field.name),
            ));
        } else if !policy.accepted_fields.contains(&field.name.as_str()) {
            problems.push(problem(
                ChangeRequestRule::UnknownField,
                pointer_for_key(&field.name),
                format!(
                    "unknown field `{}`; {} accepts: {} (`$schema` is optional metadata)",
                    field.name,
                    policy.schema_version.as_str(),
                    policy.accepted_fields.join(", ")
                ),
            ));
        }
    }

    let schema = string_field(&object, "schema", &mut problems, |schema, problems| {
        if schema != policy.schema_version.as_str() {
            problems.push(problem(
                ChangeRequestRule::UnsupportedVersion,
                "/schema",
                format!(
                    "`schema` is {schema:?}; this Daemar accepts exactly {:?}",
                    policy.schema_version.as_str()
                ),
            ));
        }
    });

    let id = slug_field(&object, &mut problems, policy);

    let objective = string_field(
        &object,
        "objective",
        &mut problems,
        |objective, problems| {
            if !has_non_whitespace(objective) {
                problems.push(problem(
                    ChangeRequestRule::BlankField,
                    "/objective",
                    "`objective` must not be blank",
                ));
            }
            let characters = objective.chars().count();
            if characters > policy.max_objective_characters {
                problems.push(problem(
                    ChangeRequestRule::FieldTooLong,
                    "/objective",
                    format!(
                        "`objective` is {characters} characters, maximum is {}",
                        policy.max_objective_characters
                    ),
                ));
            }
        },
    );

    let acceptance_criteria = criteria_field(&object, &mut problems, policy);
    for schema_hint in object.values("$schema") {
        match schema_hint.as_str() {
            Some(schema_hint) if !has_non_whitespace(schema_hint) => {
                problems.push(problem(
                    ChangeRequestRule::BlankField,
                    "/$schema",
                    "`$schema` must not be blank",
                ));
            }
            Some(_) => {}
            None => {
                problems.push(problem(
                    ChangeRequestRule::WrongType,
                    "/$schema",
                    "`$schema` must be a string",
                ));
            }
        }
    }

    if problems.is_empty() {
        let schema = only_occurrence(schema, "schema");
        Ok(ChangeRequest {
            schema: ChangeRequestSchemaVersion::parse(&schema)
                .expect("Preflight accepted a supported schema version"),
            id: only_occurrence(id, "id"),
            objective: only_occurrence(objective, "objective"),
            acceptance_criteria: only_occurrence(acceptance_criteria, "acceptance_criteria"),
        })
    } else {
        Err(problems)
    }
}

fn string_field(
    object: &RawObject,
    name: &str,
    problems: &mut Vec<ChangeRequestProblem>,
    mut validate: impl FnMut(&str, &mut Vec<ChangeRequestProblem>),
) -> Option<Vec<String>> {
    let values: Vec<_> = object.values(name).collect();
    if values.is_empty() {
        problems.push(problem(
            ChangeRequestRule::MissingField,
            format!("/{name}"),
            format!("missing required field `{name}`"),
        ));
        return None;
    }

    let mut strings = Vec::with_capacity(values.len());
    for value in values {
        match value.as_str() {
            Some(value) => {
                validate(value, problems);
                strings.push(value.to_owned());
            }
            None => problems.push(problem(
                ChangeRequestRule::WrongType,
                format!("/{name}"),
                format!("`{name}` must be a string"),
            )),
        }
    }
    Some(strings)
}

fn slug_field(
    object: &RawObject,
    problems: &mut Vec<ChangeRequestProblem>,
    policy: &PreflightPolicy,
) -> Option<Vec<ChangeRequestSlug>> {
    let values: Vec<_> = object.values("id").collect();
    if values.is_empty() {
        problems.push(problem(
            ChangeRequestRule::MissingField,
            "/id",
            "missing required field `id`",
        ));
        return None;
    }

    let mut slugs = Vec::with_capacity(values.len());
    for value in values {
        let Some(id) = value.as_str() else {
            problems.push(problem(
                ChangeRequestRule::WrongType,
                "/id",
                "`id` must be a string",
            ));
            continue;
        };

        let mut valid = true;
        let characters = id.chars().count();
        if characters < policy.min_id_characters {
            valid = false;
            problems.push(problem(
                ChangeRequestRule::FieldTooShort,
                "/id",
                format!(
                    "`id` is {characters} characters, minimum is {}",
                    policy.min_id_characters
                ),
            ));
        } else if characters > policy.max_id_characters {
            valid = false;
            problems.push(problem(
                ChangeRequestRule::FieldTooLong,
                "/id",
                format!(
                    "`id` is {characters} characters, maximum is {}",
                    policy.max_id_characters
                ),
            ));
        }
        if !is_change_request_slug(id) {
            valid = false;
            problems.push(problem(
                ChangeRequestRule::BadSlug,
                "/id",
                "`id` must be lowercase kebab-case (a-z, 0-9, single dashes)",
            ));
        }
        if valid {
            slugs.push(ChangeRequestSlug(id.to_owned()));
        }
    }
    Some(slugs)
}

fn is_change_request_slug(value: &str) -> bool {
    let mut needs_alphanumeric = true;
    for byte in value.bytes() {
        if byte.is_ascii_lowercase() || byte.is_ascii_digit() {
            needs_alphanumeric = false;
        } else if byte == b'-' && !needs_alphanumeric {
            needs_alphanumeric = true;
        } else {
            return false;
        }
    }
    !needs_alphanumeric
}

fn has_non_whitespace(value: &str) -> bool {
    value.chars().any(|character| !character.is_whitespace())
}

fn criteria_field(
    object: &RawObject,
    problems: &mut Vec<ChangeRequestProblem>,
    policy: &PreflightPolicy,
) -> Option<Vec<Vec<String>>> {
    let values: Vec<_> = object.values("acceptance_criteria").collect();
    if values.is_empty() {
        problems.push(problem(
            ChangeRequestRule::MissingField,
            "/acceptance_criteria",
            "missing required field `acceptance_criteria`",
        ));
        return None;
    }

    let mut criteria_occurrences = Vec::with_capacity(values.len());
    for value in values {
        let Some(items) = value.as_array() else {
            problems.push(problem(
                ChangeRequestRule::WrongType,
                "/acceptance_criteria",
                "`acceptance_criteria` must be an array of strings",
            ));
            continue;
        };

        if !(policy.min_acceptance_criteria..=policy.max_acceptance_criteria).contains(&items.len())
        {
            problems.push(problem(
                ChangeRequestRule::BadItemCount,
                "/acceptance_criteria",
                format!(
                    "`acceptance_criteria` has {} items, allowed {}-{}",
                    items.len(),
                    policy.min_acceptance_criteria,
                    policy.max_acceptance_criteria
                ),
            ));
        }

        let mut criteria = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            let pointer = format!("/acceptance_criteria/{index}");
            match item.as_str() {
                Some(item) => {
                    if !has_non_whitespace(item) {
                        problems.push(problem(
                            ChangeRequestRule::BlankField,
                            pointer.clone(),
                            format!("criterion #{} must not be blank", index + 1),
                        ));
                    }
                    let characters = item.chars().count();
                    if characters > policy.max_criterion_characters {
                        problems.push(problem(
                            ChangeRequestRule::FieldTooLong,
                            pointer,
                            format!(
                                "criterion #{} is {characters} characters, maximum is {}",
                                index + 1,
                                policy.max_criterion_characters
                            ),
                        ));
                    }
                    criteria.push(item.to_owned());
                }
                None => problems.push(problem(
                    ChangeRequestRule::WrongType,
                    pointer,
                    format!("criterion #{} must be a string", index + 1),
                )),
            }
        }
        criteria_occurrences.push(criteria);
    }

    Some(criteria_occurrences)
}

fn only_occurrence<T>(values: Option<Vec<T>>, field: &str) -> T {
    let mut values = values.expect("validated required field");
    assert_eq!(values.len(), 1, "validated `{field}` field occurrence");
    values.pop().expect("validated field value")
}

fn pointer_for_key(key: &str) -> String {
    format!("/{}", key.replace('~', "~0").replace('/', "~1"))
}

fn one_problem(code: ChangeRequestRule, message: impl Into<String>) -> Vec<ChangeRequestProblem> {
    vec![problem(code, "", message)]
}

fn problem(
    code: ChangeRequestRule,
    pointer: impl Into<String>,
    message: impl Into<String>,
) -> ChangeRequestProblem {
    ChangeRequestProblem {
        code,
        pointer: pointer.into(),
        message: message.into(),
    }
}

struct RawDocument {
    object: Option<RawObject>,
}

impl<'de> Deserialize<'de> for RawDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(RawDocumentVisitor)
    }
}

struct RawDocumentVisitor;

impl<'de> Visitor<'de> for RawDocumentVisitor {
    type Value = RawDocument;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("any JSON value")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut seen = HashSet::new();
        let mut fields = Vec::new();
        while let Some((name, value)) = map.next_entry::<String, Value>()? {
            let repeated = !seen.insert(name.clone());
            fields.push(RawField {
                name,
                value,
                repeated,
            });
        }
        Ok(RawDocument {
            object: Some(RawObject { fields }),
        })
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence.next_element::<IgnoredAny>()?.is_some() {}
        Ok(non_object())
    }

    fn visit_bool<E>(self, _: bool) -> Result<Self::Value, E> {
        Ok(non_object())
    }

    fn visit_i64<E>(self, _: i64) -> Result<Self::Value, E> {
        Ok(non_object())
    }

    fn visit_u64<E>(self, _: u64) -> Result<Self::Value, E> {
        Ok(non_object())
    }

    fn visit_f64<E>(self, _: f64) -> Result<Self::Value, E> {
        Ok(non_object())
    }

    fn visit_str<E>(self, _: &str) -> Result<Self::Value, E> {
        Ok(non_object())
    }

    fn visit_string<E>(self, _: String) -> Result<Self::Value, E> {
        Ok(non_object())
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(non_object())
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(non_object())
    }
}

fn non_object() -> RawDocument {
    RawDocument { object: None }
}

struct RawObject {
    fields: Vec<RawField>,
}

impl RawObject {
    fn values<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Value> {
        self.fields
            .iter()
            .filter(move |field| field.name == name)
            .map(|field| &field.value)
    }
}

struct RawField {
    name: String,
    value: Value,
    repeated: bool,
}
