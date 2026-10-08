//! Library-independent aggregate observations carried beside executable images.

use std::collections::BTreeSet;

use super::managed::SemanticTypeId;

/// Exact observed fields, or a fail-closed escape of the complete value.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub(crate) enum AggregateFieldProjection {
    Complete,
    Fields(BTreeSet<usize>),
}

/// Export-specific observations; consumers interpret their own record contract.
/// Suspension is required metadata: absence must never imply a synchronous call.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NativeAggregateProjection {
    pub(crate) module: String,
    pub(crate) function: String,
    pub(crate) arity: usize,
    pub(crate) semantic: SemanticTypeId,
    pub(crate) fields: AggregateFieldProjection,
    pub(crate) scalar_entry: Option<String>,
    pub(crate) scalar_field: Option<usize>,
    pub(crate) suspending: bool,
}

#[cfg(test)]
#[path = "aggregate_projection_test.rs"]
mod tests;
