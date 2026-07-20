use std::collections::HashSet;
use std::fmt;

use serde::de::{IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

const MAX_DOCUMENT_BYTES: usize = 16 * 1024;
const SCHEMA_VERSION: &str = "change_request.v1";
const MAX_ID_CHARACTERS: usize = 64;
const MAX_OBJECTIVE_CHARACTERS: usize = 4_096;
const MAX_ACCEPTANCE_CRITERIA: usize = 20;
const MAX_CRITERION_CHARACTERS: usize = 1_024;
const REQUIRED_FIELDS: [&str; 4] = ["schema", "id", "objective", "acceptance_criteria"];

#[derive(Debug, PartialEq, Eq)]
pub struct ChangeRequest {
    schema: String,
    id: String,
    objective: String,
    acceptance_criteria: Vec<String>,
}

impl ChangeRequest {
    pub fn schema(&self) -> &str {
        &self.schema
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn objective(&self) -> &str {
        &self.objective
    }

    pub fn acceptance_criteria(&self) -> &[String] {
        &self.acceptance_criteria
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ChangeRequestProblem {
    pub code: ChangeRequestRule,
    pub pointer: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeRequestRule {
    IoError,
    DocumentTooLarge,
    InvalidEncoding,
    InvalidJson,
    NotAnObject,
    DuplicateField,
    UnknownField,
    MissingField,
    WrongType,
    UnsupportedVersion,
    FieldTooLong,
    BadItemCount,
    BadSlug,
    BlankField,
}

impl ChangeRequestRule {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::IoError => "io_error",
            Self::DocumentTooLarge => "document_too_large",
            Self::InvalidEncoding => "invalid_encoding",
            Self::InvalidJson => "invalid_json",
            Self::NotAnObject => "not_an_object",
            Self::DuplicateField => "duplicate_field",
            Self::UnknownField => "unknown_field",
            Self::MissingField => "missing_field",
            Self::WrongType => "wrong_type",
            Self::UnsupportedVersion => "unsupported_version",
            Self::FieldTooLong => "field_too_long",
            Self::BadItemCount => "bad_item_count",
            Self::BadSlug => "bad_slug",
            Self::BlankField => "blank_field",
        }
    }
}

impl fmt::Display for ChangeRequestRule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

pub fn preflight(bytes: &[u8]) -> Result<ChangeRequest, Vec<ChangeRequestProblem>> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(one_problem(
            ChangeRequestRule::DocumentTooLarge,
            "document is larger than 16 KiB",
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| {
        one_problem(
            ChangeRequestRule::InvalidEncoding,
            "document is not valid UTF-8",
        )
    })?;
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

    validate_object(object)
}

pub fn execute_run<T>(
    change_request_bytes: &[u8],
    initialize_workflow_run: impl FnOnce(ChangeRequest) -> T,
) -> Result<T, Vec<ChangeRequestProblem>> {
    preflight(change_request_bytes).map(initialize_workflow_run)
}

fn validate_object(object: RawObject) -> Result<ChangeRequest, Vec<ChangeRequestProblem>> {
    let mut problems = Vec::new();

    for field in &object.fields {
        if field.repeated {
            problems.push(problem(
                ChangeRequestRule::DuplicateField,
                pointer_for_key(&field.name),
                format!("duplicate field `{}`", field.name),
            ));
        } else if !is_known_or_ignored(&field.name) {
            problems.push(problem(
                ChangeRequestRule::UnknownField,
                pointer_for_key(&field.name),
                format!(
                    "unknown field `{}`; {SCHEMA_VERSION} accepts exactly: {}",
                    field.name,
                    REQUIRED_FIELDS.join(", ")
                ),
            ));
        }
    }

    let schema = string_field(&object, "schema", &mut problems);
    if let Some(schema) = schema.as_deref()
        && schema != SCHEMA_VERSION
    {
        problems.push(problem(
            ChangeRequestRule::UnsupportedVersion,
            "/schema",
            format!("`schema` is {schema:?}; this Daemar accepts exactly {SCHEMA_VERSION:?}"),
        ));
    }

    let id = string_field(&object, "id", &mut problems);
    if let Some(id) = id.as_deref() {
        let characters = id.chars().count();
        if !(1..=MAX_ID_CHARACTERS).contains(&characters) {
            problems.push(problem(
                ChangeRequestRule::FieldTooLong,
                "/id",
                format!("`id` is {characters} characters, allowed 1-{MAX_ID_CHARACTERS}"),
            ));
        }
        if !is_lowercase_kebab_case(id) {
            problems.push(problem(
                ChangeRequestRule::BadSlug,
                "/id",
                "`id` must be lowercase kebab-case (a-z, 0-9, single dashes)",
            ));
        }
    }

    let objective = string_field(&object, "objective", &mut problems);
    if let Some(objective) = objective.as_deref() {
        if objective.trim().is_empty() {
            problems.push(problem(
                ChangeRequestRule::BlankField,
                "/objective",
                "`objective` must not be blank",
            ));
        }
        let characters = objective.chars().count();
        if characters > MAX_OBJECTIVE_CHARACTERS {
            problems.push(problem(
                ChangeRequestRule::FieldTooLong,
                "/objective",
                format!(
                    "`objective` is {characters} characters, maximum is {MAX_OBJECTIVE_CHARACTERS}"
                ),
            ));
        }
    }

    let acceptance_criteria = criteria_field(&object, &mut problems);

    if problems.is_empty() {
        Ok(ChangeRequest {
            schema: schema.expect("validated required field"),
            id: id.expect("validated required field"),
            objective: objective.expect("validated required field"),
            acceptance_criteria: acceptance_criteria.expect("validated required field"),
        })
    } else {
        Err(problems)
    }
}

fn string_field(
    object: &RawObject,
    name: &str,
    problems: &mut Vec<ChangeRequestProblem>,
) -> Option<String> {
    let Some(value) = object.value(name) else {
        problems.push(problem(
            ChangeRequestRule::MissingField,
            format!("/{name}"),
            format!("missing required field `{name}`"),
        ));
        return None;
    };
    match value.as_str() {
        Some(value) => Some(value.to_owned()),
        None => {
            problems.push(problem(
                ChangeRequestRule::WrongType,
                format!("/{name}"),
                format!("`{name}` must be a string"),
            ));
            None
        }
    }
}

fn criteria_field(
    object: &RawObject,
    problems: &mut Vec<ChangeRequestProblem>,
) -> Option<Vec<String>> {
    let Some(value) = object.value("acceptance_criteria") else {
        problems.push(problem(
            ChangeRequestRule::MissingField,
            "/acceptance_criteria",
            "missing required field `acceptance_criteria`",
        ));
        return None;
    };
    let Some(items) = value.as_array() else {
        problems.push(problem(
            ChangeRequestRule::WrongType,
            "/acceptance_criteria",
            "`acceptance_criteria` must be an array of strings",
        ));
        return None;
    };

    if !(1..=MAX_ACCEPTANCE_CRITERIA).contains(&items.len()) {
        problems.push(problem(
            ChangeRequestRule::BadItemCount,
            "/acceptance_criteria",
            format!(
                "`acceptance_criteria` has {} items, allowed 1-{MAX_ACCEPTANCE_CRITERIA}",
                items.len()
            ),
        ));
    }

    let mut criteria = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let pointer = format!("/acceptance_criteria/{index}");
        match item.as_str() {
            Some(item) => {
                if item.trim().is_empty() {
                    problems.push(problem(
                        ChangeRequestRule::BlankField,
                        pointer.clone(),
                        format!("criterion #{} must not be blank", index + 1),
                    ));
                }
                let characters = item.chars().count();
                if characters > MAX_CRITERION_CHARACTERS {
                    problems.push(problem(
                        ChangeRequestRule::FieldTooLong,
                        pointer,
                        format!(
                            "criterion #{} is {characters} characters, maximum is {MAX_CRITERION_CHARACTERS}",
                            index + 1
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

    Some(criteria)
}

fn is_lowercase_kebab_case(value: &str) -> bool {
    !value.is_empty()
        && value.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

fn is_known_or_ignored(field: &str) -> bool {
    REQUIRED_FIELDS.contains(&field) || field == "$schema"
}

fn pointer_for_key(key: &str) -> String {
    format!("/{}", key.replace('~', "~0").replace('/', "~1"))
}

fn one_problem(code: ChangeRequestRule, message: impl Into<String>) -> Vec<ChangeRequestProblem> {
    vec![problem(code, "/", message)]
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
    fn value(&self, name: &str) -> Option<&Value> {
        self.fields
            .iter()
            .rev()
            .find(|field| field.name == name)
            .map(|field| &field.value)
    }
}

struct RawField {
    name: String,
    value: Value,
    repeated: bool,
}
