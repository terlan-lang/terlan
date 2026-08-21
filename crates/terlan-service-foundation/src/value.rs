use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{MAX_FIELDS, MAX_FIELD_VALUE_BYTES, MAX_NAME_BYTES};

/// Portable, bounded values allowed to cross the service host ABI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Scalar {
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    String(String),
}

impl Scalar {
    fn validate(&self) -> Result<(), FieldError> {
        match self {
            Self::F64(value) if !value.is_finite() => Err(FieldError::NonFiniteNumber),
            Self::String(value) if value.len() > MAX_FIELD_VALUE_BYTES => {
                Err(FieldError::ValueTooLong)
            }
            _ => Ok(()),
        }
    }
}

/// One validated structured field. Secret handles cannot be converted to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub value: Scalar,
}

/// A deterministic set of portable structured fields.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FieldSet(Vec<Field>);

impl FieldSet {
    /// Validates names, values, secrecy, uniqueness, and the field-count bound.
    pub fn try_new(fields: impl IntoIterator<Item = Field>) -> Result<Self, FieldError> {
        let fields: Vec<_> = fields.into_iter().collect();
        if fields.len() > MAX_FIELDS {
            return Err(FieldError::TooManyFields);
        }
        let mut names = BTreeSet::new();
        for field in &fields {
            validate_name(&field.name)?;
            field.value.validate()?;
            if is_secret_bearing_name(&field.name) {
                return Err(FieldError::SecretBearingName(field.name.clone()));
            }
            if !names.insert(field.name.as_str()) {
                return Err(FieldError::DuplicateName(field.name.clone()));
            }
        }
        Ok(Self(fields))
    }

    /// Borrows validated fields in their deterministic insertion order.
    pub fn as_slice(&self) -> &[Field] {
        &self.0
    }
}

pub(crate) fn validate_name(name: &str) -> Result<(), FieldError> {
    if name.is_empty() || name.len() > MAX_NAME_BYTES {
        return Err(FieldError::InvalidName(name.to_owned()));
    }
    if !name
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(FieldError::InvalidName(name.to_owned()));
    }
    Ok(())
}

pub(crate) fn is_secret_bearing_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "secret",
        "password",
        "token",
        "authorization",
        "cookie",
        "api_key",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

/// Stable reasons a structured service field can be rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldError {
    InvalidName(String),
    DuplicateName(String),
    SecretBearingName(String),
    TooManyFields,
    ValueTooLong,
    NonFiniteNumber,
}

impl fmt::Display for FieldError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid service field: {self:?}")
    }
}

impl std::error::Error for FieldError {}

#[cfg(test)]
#[path = "value_test.rs"]
mod tests;
