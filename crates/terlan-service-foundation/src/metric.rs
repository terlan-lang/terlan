use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::value::{is_secret_bearing_name, validate_name};

/// Portable metric instrument semantics supported by every host adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentKind {
    Counter,
    Gauge,
    Histogram,
}

/// Bounded declaration for one metric instrument and its label shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricDeclaration {
    pub name: String,
    pub kind: InstrumentKind,
    pub label_keys: Vec<String>,
    /// Maximum distinct label tuples retained by a host.
    pub cardinality_limit: usize,
}

/// Deterministic registry of admitted metric declarations.
#[derive(Debug, Clone, Default)]
pub struct MetricRegistry {
    declarations: BTreeMap<String, MetricDeclaration>,
}

impl MetricRegistry {
    /// Validates and registers one metric declaration.
    pub fn declare(&mut self, declaration: MetricDeclaration) -> Result<(), MetricError> {
        validate_name(&declaration.name).map_err(|_| MetricError::InvalidName)?;
        if declaration.name.contains("url") || declaration.name.contains("path") {
            return Err(MetricError::UnboundedIdentity);
        }
        if declaration.cardinality_limit == 0 || declaration.cardinality_limit > 1_024 {
            return Err(MetricError::InvalidCardinality);
        }
        let mut keys = BTreeSet::new();
        for key in &declaration.label_keys {
            validate_name(key).map_err(|_| MetricError::InvalidLabel)?;
            if is_secret_bearing_name(key) || key.contains("url") || key.contains("path") {
                return Err(MetricError::UnboundedIdentity);
            }
            if !keys.insert(key) {
                return Err(MetricError::DuplicateLabel);
            }
        }
        if self.declarations.contains_key(&declaration.name) {
            return Err(MetricError::DuplicateInstrument);
        }
        self.declarations
            .insert(declaration.name.clone(), declaration);
        Ok(())
    }

    /// Verifies that a sample names a declared instrument with exact labels.
    pub fn validate_sample<'a>(
        &self,
        name: &str,
        labels: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Result<(), MetricError> {
        let declaration = self
            .declarations
            .get(name)
            .ok_or(MetricError::UndeclaredInstrument)?;
        let supplied: BTreeSet<_> = labels.into_iter().map(|(key, _)| key).collect();
        let declared: BTreeSet<_> = declaration.label_keys.iter().map(String::as_str).collect();
        if supplied != declared {
            return Err(MetricError::LabelShape);
        }
        Ok(())
    }

    /// Iterates declarations in stable name order.
    pub fn declarations(&self) -> impl Iterator<Item = &MetricDeclaration> {
        self.declarations.values()
    }
}

/// Stable reasons a metric declaration or sample can be rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricError {
    InvalidName,
    InvalidLabel,
    InvalidCardinality,
    UnboundedIdentity,
    DuplicateLabel,
    DuplicateInstrument,
    UndeclaredInstrument,
    LabelShape,
}

impl fmt::Display for MetricError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid metric contract: {self:?}")
    }
}

impl std::error::Error for MetricError {}

#[cfg(test)]
#[path = "metric_test.rs"]
mod tests;
