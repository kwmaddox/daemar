use std::borrow::Cow;
use std::fmt;

use serde::de::{IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

const CHANGE_REQUEST_SCHEMA: &str = "change_request.v1";
const REQUIRED_FIELDS: [&str; 4] = ["schema", "id", "objective", "acceptance_criteria"];
const MAX_DOCUMENT_BYTES: usize = 16 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub struct ChangeRequest {
    id: ChangeRequestId,
    objective: Objective,
    acceptance_criteria: Vec<AcceptanceCriterion>,
}

#[derive(Debug, PartialEq, Eq)]
struct ChangeRequestId(String);

#[derive(Debug, PartialEq, Eq)]
struct Objective(String);

#[derive(Debug, PartialEq, Eq)]
struct AcceptanceCriterion(String);

impl ChangeRequest {
    pub fn schema(&self) -> &'static str {
        CHANGE_REQUEST_SCHEMA
    }

    pub fn id(&self) -> &str {
        &self.id.0
    }

    pub fn objective(&self) -> &str {
        &self.objective.0
    }

    pub fn acceptance_criteria(&self) -> Vec<&str> {
        self.acceptance_criteria
            .iter()
            .map(|criterion| criterion.0.as_str())
            .collect()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PreflightError {
    IoError {
        detail: String,
    },
    DocumentTooLarge {
        actual_bytes: usize,
    },
    InvalidEncoding,
    InvalidJson {
        detail: String,
    },
    NotAnObject,
    DuplicateField {
        field: String,
    },
    UnknownField {
        field: String,
    },
    MissingSchema,
    MissingId,
    MissingObjective,
    MissingAcceptanceCriteria,
    SchemaWrongType,
    UnsupportedVersion {
        actual: String,
    },
    IdWrongType,
    IdLength {
        actual_characters: usize,
    },
    BadSlug,
    ObjectiveWrongType,
    BlankObjective,
    ObjectiveTooLong {
        actual_characters: usize,
    },
    AcceptanceCriteriaWrongType,
    BadItemCount {
        actual_items: usize,
    },
    CriterionWrongType {
        index: usize,
    },
    BlankCriterion {
        index: usize,
    },
    CriterionTooLong {
        index: usize,
        actual_characters: usize,
    },
}

impl PreflightError {
    pub fn io_error(error: &std::io::Error) -> Self {
        Self::IoError {
            detail: error.to_string(),
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::IoError { .. } => "io_error",
            Self::DocumentTooLarge { .. } => "document_too_large",
            Self::InvalidEncoding => "invalid_encoding",
            Self::InvalidJson { .. } => "invalid_json",
            Self::NotAnObject => "not_an_object",
            Self::DuplicateField { .. } => "duplicate_field",
            Self::UnknownField { .. } => "unknown_field",
            Self::MissingSchema
            | Self::MissingId
            | Self::MissingObjective
            | Self::MissingAcceptanceCriteria => "missing_field",
            Self::SchemaWrongType
            | Self::IdWrongType
            | Self::ObjectiveWrongType
            | Self::AcceptanceCriteriaWrongType
            | Self::CriterionWrongType { .. } => "wrong_type",
            Self::UnsupportedVersion { .. } => "unsupported_version",
            Self::IdLength { .. }
            | Self::ObjectiveTooLong { .. }
            | Self::CriterionTooLong { .. } => "field_too_long",
            Self::BadSlug => "bad_slug",
            Self::BlankObjective | Self::BlankCriterion { .. } => "blank_field",
            Self::BadItemCount { .. } => "bad_item_count",
        }
    }

    pub fn pointer(&self) -> Cow<'_, str> {
        match self {
            Self::IoError { .. }
            | Self::DocumentTooLarge { .. }
            | Self::InvalidEncoding
            | Self::InvalidJson { .. }
            | Self::NotAnObject => Cow::Borrowed("/"),
            Self::DuplicateField { field } | Self::UnknownField { field } => {
                Cow::Owned(json_pointer(field))
            }
            Self::MissingSchema | Self::SchemaWrongType | Self::UnsupportedVersion { .. } => {
                Cow::Borrowed("/schema")
            }
            Self::MissingId | Self::IdWrongType | Self::IdLength { .. } | Self::BadSlug => {
                Cow::Borrowed("/id")
            }
            Self::MissingObjective
            | Self::ObjectiveWrongType
            | Self::BlankObjective
            | Self::ObjectiveTooLong { .. } => Cow::Borrowed("/objective"),
            Self::MissingAcceptanceCriteria
            | Self::AcceptanceCriteriaWrongType
            | Self::BadItemCount { .. } => Cow::Borrowed("/acceptance_criteria"),
            Self::CriterionWrongType { index }
            | Self::BlankCriterion { index }
            | Self::CriterionTooLong { index, .. } => {
                Cow::Owned(format!("/acceptance_criteria/{index}"))
            }
        }
    }

    fn message(&self) -> Cow<'_, str> {
        match self {
            Self::IoError { detail } => Cow::Owned(format!("cannot read file: {detail}")),
            Self::DocumentTooLarge { actual_bytes } => Cow::Owned(format!(
                "document is {actual_bytes} bytes, maximum is {MAX_DOCUMENT_BYTES}"
            )),
            Self::InvalidEncoding => Cow::Borrowed("document is not valid UTF-8"),
            Self::InvalidJson { detail } => Cow::Owned(format!("not valid JSON: {detail}")),
            Self::NotAnObject => Cow::Borrowed("top level must be a JSON object"),
            Self::DuplicateField { field } => Cow::Owned(format!("duplicate field `{field}`")),
            Self::UnknownField { field } => Cow::Owned(format!(
                "unknown field `{field}`; {CHANGE_REQUEST_SCHEMA} accepts exactly: {}",
                REQUIRED_FIELDS.join(", ")
            )),
            Self::MissingSchema => Cow::Borrowed("missing required field `schema`"),
            Self::MissingId => Cow::Borrowed("missing required field `id`"),
            Self::MissingObjective => Cow::Borrowed("missing required field `objective`"),
            Self::MissingAcceptanceCriteria => {
                Cow::Borrowed("missing required field `acceptance_criteria`")
            }
            Self::SchemaWrongType => Cow::Borrowed("`schema` must be a string"),
            Self::UnsupportedVersion { actual } => Cow::Owned(format!(
                "`schema` is \"{actual}\"; this Daemar accepts exactly \"{CHANGE_REQUEST_SCHEMA}\""
            )),
            Self::IdWrongType => Cow::Borrowed("`id` must be a string"),
            Self::IdLength { actual_characters } => Cow::Owned(format!(
                "`id` is {actual_characters} characters, allowed 1-64"
            )),
            Self::BadSlug => {
                Cow::Borrowed("`id` must be lowercase kebab-case (a-z, 0-9, single dashes)")
            }
            Self::ObjectiveWrongType => Cow::Borrowed("`objective` must be a string"),
            Self::BlankObjective => Cow::Borrowed("`objective` must not be blank"),
            Self::ObjectiveTooLong { actual_characters } => Cow::Owned(format!(
                "`objective` is {actual_characters} characters, maximum is 4096"
            )),
            Self::AcceptanceCriteriaWrongType => {
                Cow::Borrowed("`acceptance_criteria` must be an array of strings")
            }
            Self::BadItemCount { actual_items } => Cow::Owned(format!(
                "`acceptance_criteria` has {actual_items} items, allowed 1-20"
            )),
            Self::CriterionWrongType { index } => {
                Cow::Owned(format!("criterion #{} must be a string", index + 1))
            }
            Self::BlankCriterion { index } => {
                Cow::Owned(format!("criterion #{} must not be blank", index + 1))
            }
            Self::CriterionTooLong {
                index,
                actual_characters,
            } => Cow::Owned(format!(
                "criterion #{} is {actual_characters} characters, maximum is 1024",
                index + 1
            )),
        }
    }
}

impl fmt::Display for PreflightError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "  [{}] {} (at {})",
            self.code(),
            self.message(),
            self.pointer()
        )
    }
}

pub fn preflight(raw: &[u8]) -> Result<ChangeRequest, Vec<PreflightError>> {
    if raw.len() > MAX_DOCUMENT_BYTES {
        return Err(vec![PreflightError::DocumentTooLarge {
            actual_bytes: raw.len(),
        }]);
    }

    let text = std::str::from_utf8(raw).map_err(|_| vec![PreflightError::InvalidEncoding])?;
    let mut deserializer = serde_json::Deserializer::from_str(text);
    let document = TopLevelDocument::deserialize(&mut deserializer)
        .and_then(|document| {
            deserializer.end()?;
            Ok(document)
        })
        .map_err(|error| {
            vec![PreflightError::InvalidJson {
                detail: error.to_string(),
            }]
        })?;
    let TopLevelDocument::Object(entries) = document else {
        return Err(vec![PreflightError::NotAnObject]);
    };

    let mut diagnostics = Vec::new();
    let mut object = Vec::<(String, Value)>::new();

    for (field, value) in entries {
        if let Some((_, previous_value)) = object
            .iter_mut()
            .find(|(previous_field, _)| previous_field == &field)
        {
            diagnostics.push(PreflightError::DuplicateField {
                field: field.clone(),
            });
            *previous_value = value;
        } else {
            object.push((field, value));
        }
    }

    for (field, _) in &object {
        if field != "$schema" && !REQUIRED_FIELDS.contains(&field.as_str()) {
            diagnostics.push(PreflightError::UnknownField {
                field: field.clone(),
            });
        }
    }

    if field_value(&object, "schema").is_none() {
        diagnostics.push(PreflightError::MissingSchema);
    }
    if field_value(&object, "id").is_none() {
        diagnostics.push(PreflightError::MissingId);
    }
    if field_value(&object, "objective").is_none() {
        diagnostics.push(PreflightError::MissingObjective);
    }
    if field_value(&object, "acceptance_criteria").is_none() {
        diagnostics.push(PreflightError::MissingAcceptanceCriteria);
    }

    validate_schema(field_value(&object, "schema"), &mut diagnostics);
    validate_id(field_value(&object, "id"), &mut diagnostics);
    validate_objective(field_value(&object, "objective"), &mut diagnostics);
    validate_criteria(
        field_value(&object, "acceptance_criteria"),
        &mut diagnostics,
    );

    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    Ok(ChangeRequest {
        id: ChangeRequestId(required_string(&object, "id").to_owned()),
        objective: Objective(required_string(&object, "objective").to_owned()),
        acceptance_criteria: field_value(&object, "acceptance_criteria")
            .and_then(Value::as_array)
            .expect("validated acceptance criteria")
            .iter()
            .map(|criterion| {
                AcceptanceCriterion(
                    criterion
                        .as_str()
                        .expect("validated acceptance criterion")
                        .to_owned(),
                )
            })
            .collect(),
    })
}

fn required_string<'a>(object: &'a [(String, Value)], field: &str) -> &'a str {
    field_value(object, field)
        .and_then(Value::as_str)
        .expect("validated required string")
}

fn validate_schema(value: Option<&Value>, diagnostics: &mut Vec<PreflightError>) {
    let Some(value) = value else { return };
    let Some(schema) = value.as_str() else {
        diagnostics.push(PreflightError::SchemaWrongType);
        return;
    };
    if schema != CHANGE_REQUEST_SCHEMA {
        diagnostics.push(PreflightError::UnsupportedVersion {
            actual: schema.to_owned(),
        });
    }
}

fn validate_id(value: Option<&Value>, diagnostics: &mut Vec<PreflightError>) {
    let Some(value) = value else { return };
    let Some(id) = value.as_str() else {
        diagnostics.push(PreflightError::IdWrongType);
        return;
    };
    let character_count = id.chars().count();
    if !(1..=64).contains(&character_count) {
        diagnostics.push(PreflightError::IdLength {
            actual_characters: character_count,
        });
    } else if !is_slug(id) {
        diagnostics.push(PreflightError::BadSlug);
    }
}

fn validate_objective(value: Option<&Value>, diagnostics: &mut Vec<PreflightError>) {
    let Some(value) = value else { return };
    let Some(objective) = value.as_str() else {
        diagnostics.push(PreflightError::ObjectiveWrongType);
        return;
    };
    if objective.trim().is_empty() {
        diagnostics.push(PreflightError::BlankObjective);
    } else {
        let character_count = objective.chars().count();
        if character_count > 4_096 {
            diagnostics.push(PreflightError::ObjectiveTooLong {
                actual_characters: character_count,
            });
        }
    }
}

fn validate_criteria(value: Option<&Value>, diagnostics: &mut Vec<PreflightError>) {
    let Some(value) = value else { return };
    let Some(criteria) = value.as_array() else {
        diagnostics.push(PreflightError::AcceptanceCriteriaWrongType);
        return;
    };
    if !(1..=20).contains(&criteria.len()) {
        diagnostics.push(PreflightError::BadItemCount {
            actual_items: criteria.len(),
        });
    }
    for (index, criterion) in criteria.iter().enumerate() {
        let Some(criterion) = criterion.as_str() else {
            diagnostics.push(PreflightError::CriterionWrongType { index });
            continue;
        };
        if criterion.trim().is_empty() {
            diagnostics.push(PreflightError::BlankCriterion { index });
        } else {
            let character_count = criterion.chars().count();
            if character_count > 1_024 {
                diagnostics.push(PreflightError::CriterionTooLong {
                    index,
                    actual_characters: character_count,
                });
            }
        }
    }
}

fn is_slug(value: &str) -> bool {
    !value.is_empty()
        && value.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

fn field_value<'a>(object: &'a [(String, Value)], field: &str) -> Option<&'a Value> {
    object
        .iter()
        .find_map(|(present, value)| (present == field).then_some(value))
}

enum TopLevelDocument {
    Object(Vec<(String, Value)>),
    Other,
}

impl<'de> Deserialize<'de> for TopLevelDocument {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(TopLevelVisitor)
    }
}

struct TopLevelVisitor;

impl<'de> Visitor<'de> for TopLevelVisitor {
    type Value = TopLevelDocument;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut entries = Vec::new();
        while let Some(entry) = map.next_entry()? {
            entries.push(entry);
        }
        Ok(TopLevelDocument::Object(entries))
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence.next_element::<IgnoredAny>()?.is_some() {}
        Ok(TopLevelDocument::Other)
    }

    fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E> {
        Ok(TopLevelDocument::Other)
    }

    fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(TopLevelDocument::Other)
    }

    fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(TopLevelDocument::Other)
    }

    fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E> {
        Ok(TopLevelDocument::Other)
    }

    fn visit_str<E>(self, _value: &str) -> Result<Self::Value, E> {
        Ok(TopLevelDocument::Other)
    }

    fn visit_string<E>(self, _value: String) -> Result<Self::Value, E> {
        Ok(TopLevelDocument::Other)
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(TopLevelDocument::Other)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(TopLevelDocument::Other)
    }
}

fn json_pointer(field: &str) -> String {
    format!("/{}", field.replace('~', "~0").replace('/', "~1"))
}
