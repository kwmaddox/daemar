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
    pub schema: String,
    pub id: String,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PreflightProblem {
    pub code: PreflightRule,
    pub pointer: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreflightRule {
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

impl PreflightRule {
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

impl fmt::Display for PreflightRule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

pub fn preflight(bytes: &[u8]) -> Result<ChangeRequest, Vec<PreflightProblem>> {
    if bytes.len() > MAX_DOCUMENT_BYTES {
        return Err(one_problem(
            PreflightRule::DocumentTooLarge,
            "document is larger than 16 KiB",
        ));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| {
        one_problem(
            PreflightRule::InvalidEncoding,
            "document is not valid UTF-8",
        )
    })?;
    let document: RawDocument = serde_json::from_str(text).map_err(|error| {
        one_problem(
            PreflightRule::InvalidJson,
            format!("not valid JSON: {error}"),
        )
    })?;
    let Some(object) = document.object else {
        return Err(one_problem(
            PreflightRule::NotAnObject,
            "top level must be a JSON object",
        ));
    };

    validate_object(object)
}

fn validate_object(object: RawObject) -> Result<ChangeRequest, Vec<PreflightProblem>> {
    let mut problems = Vec::new();

    for field in &object.fields {
        if field.repeated {
            problems.push(problem(
                PreflightRule::DuplicateField,
                pointer_for_key(&field.name),
                format!("duplicate field `{}`", field.name),
            ));
        } else if !is_known_or_ignored(&field.name) {
            problems.push(problem(
                PreflightRule::UnknownField,
                pointer_for_key(&field.name),
                format!(
                    "unknown field `{}`; {SCHEMA_VERSION} accepts exactly: {}",
                    field.name,
                    REQUIRED_FIELDS.join(", ")
                ),
            ));
        }
    }

    for field in REQUIRED_FIELDS {
        if object.value(field).is_none() {
            problems.push(problem(
                PreflightRule::MissingField,
                format!("/{field}"),
                format!("missing required field `{field}`"),
            ));
        }
    }

    let schema = string_field(&object, "schema", &mut problems);
    if let Some(schema) = schema.as_deref()
        && schema != SCHEMA_VERSION
    {
        problems.push(problem(
            PreflightRule::UnsupportedVersion,
            "/schema",
            format!("`schema` is {schema:?}; this Daemar accepts exactly {SCHEMA_VERSION:?}"),
        ));
    }

    let id = string_field(&object, "id", &mut problems);
    if let Some(id) = id.as_deref() {
        let characters = id.chars().count();
        if !(1..=MAX_ID_CHARACTERS).contains(&characters) {
            problems.push(problem(
                PreflightRule::FieldTooLong,
                "/id",
                format!("`id` is {characters} characters, allowed 1-{MAX_ID_CHARACTERS}"),
            ));
        }
        if !is_lowercase_kebab_case(id) {
            problems.push(problem(
                PreflightRule::BadSlug,
                "/id",
                "`id` must be lowercase kebab-case (a-z, 0-9, single dashes)",
            ));
        }
    }

    let objective = string_field(&object, "objective", &mut problems);
    if let Some(objective) = objective.as_deref() {
        if objective.trim().is_empty() {
            problems.push(problem(
                PreflightRule::BlankField,
                "/objective",
                "`objective` must not be blank",
            ));
        }
        let characters = objective.chars().count();
        if characters > MAX_OBJECTIVE_CHARACTERS {
            problems.push(problem(
                PreflightRule::FieldTooLong,
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
    problems: &mut Vec<PreflightProblem>,
) -> Option<String> {
    let value = object.value(name)?;
    match value.as_str() {
        Some(value) => Some(value.to_owned()),
        None => {
            problems.push(problem(
                PreflightRule::WrongType,
                format!("/{name}"),
                format!("`{name}` must be a string"),
            ));
            None
        }
    }
}

fn criteria_field(object: &RawObject, problems: &mut Vec<PreflightProblem>) -> Option<Vec<String>> {
    let value = object.value("acceptance_criteria")?;
    let Some(items) = value.as_array() else {
        problems.push(problem(
            PreflightRule::WrongType,
            "/acceptance_criteria",
            "`acceptance_criteria` must be an array of strings",
        ));
        return None;
    };

    if !(1..=MAX_ACCEPTANCE_CRITERIA).contains(&items.len()) {
        problems.push(problem(
            PreflightRule::BadItemCount,
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
                        PreflightRule::BlankField,
                        pointer.clone(),
                        format!("criterion #{} must not be blank", index + 1),
                    ));
                }
                let characters = item.chars().count();
                if characters > MAX_CRITERION_CHARACTERS {
                    problems.push(problem(
                        PreflightRule::FieldTooLong,
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
                PreflightRule::WrongType,
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

fn one_problem(code: PreflightRule, message: impl Into<String>) -> Vec<PreflightProblem> {
    vec![problem(code, "/", message)]
}

fn problem(
    code: PreflightRule,
    pointer: impl Into<String>,
    message: impl Into<String>,
) -> PreflightProblem {
    PreflightProblem {
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
