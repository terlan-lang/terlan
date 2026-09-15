//! Stable declaration provenance for compiler-generated callable bodies.

use super::CoreFunction;

/// Original declaration identity, independent of specialized names and arities.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoreFunctionSource {
    pub module: String,
    pub function: String,
    pub arity: usize,
}

impl CoreFunction {
    /// Returns retained provenance, or the identity of an untransformed declaration.
    pub(crate) fn source_declaration(&self, module: &str) -> CoreFunctionSource {
        self.source.clone().unwrap_or_else(|| CoreFunctionSource {
            module: module.to_string(),
            function: self.name.clone(),
            arity: self.arity,
        })
    }
}
