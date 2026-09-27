//! Stable declaration provenance for compiler-generated callable bodies.

use super::CoreFunction;

/// Original declaration identity, independent of specialized names and arities.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoreFunctionSource {
    pub module: String,
    pub function: String,
    pub arity: usize,
    /// Parser-owned declaration span for bodies materialized from nested declarations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declaration_span: Option<crate::terlan_syntax::span::Span>,
}

impl CoreFunction {
    /// Returns retained provenance, or the identity of an untransformed declaration.
    pub(crate) fn source_declaration(&self, module: &str) -> CoreFunctionSource {
        self.source.clone().unwrap_or_else(|| CoreFunctionSource {
            module: module.to_string(),
            function: self.name.clone(),
            arity: self.arity,
            declaration_span: None,
        })
    }
}
