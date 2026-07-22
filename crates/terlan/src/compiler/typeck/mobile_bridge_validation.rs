//! Typechecker-facing validation for mobile bridge declarations.
#![allow(dead_code)]
//!
//! Inputs:
//! - Typed mobile bridge declarations collected by future source extraction.
//!
//! Outputs:
//! - Ordinary typechecker diagnostics and validated mobile bridge metadata.
//!
//! Transformation:
//! - Adapts compiler-owned bridge validation into the typechecker diagnostic
//!   surface without duplicating bridge semantic rules.

use crate::mobile::mobile_bridge::{
    generate_mobile_bridge_metadata, MobileBridgeDeclaration, MobileBridgeMetadata,
};
use crate::terlan_syntax::span::Span;

use super::{DiagSeverity, Diagnostic};

/// Validates mobile bridge declarations through the typechecker diagnostic path.
///
/// Inputs:
/// - `declarations`: typed mobile bridge declarations collected for one module
///   or project.
///
/// Output:
/// - Empty vector when declarations are semantically coherent.
/// - Typechecker diagnostics with stable bridge diagnostic codes otherwise.
///
/// Transformation:
/// - Reuses `mobile::mobile_bridge` validation and maps each bridge
///   diagnostic into a typechecker error so source integration can add spans
///   later without changing bridge validation semantics.
pub(crate) fn check_mobile_bridge_declarations(
    declarations: &[MobileBridgeDeclaration],
) -> Vec<Diagnostic> {
    match generate_mobile_bridge_metadata(declarations) {
        Ok(_) => Vec::new(),
        Err(diagnostics) => diagnostics
            .into_iter()
            .map(|diagnostic| {
                mobile_bridge_typecheck_diagnostic(diagnostic.code, diagnostic.message)
            })
            .collect(),
    }
}

/// Generates validated mobile bridge metadata through typechecking.
///
/// Inputs:
/// - `declarations`: typed bridge declarations collected before metadata
///   emission.
///
/// Output:
/// - `Ok(MobileBridgeMetadata)` when declarations are valid.
/// - Typechecker diagnostics when declaration validation fails.
///
/// Transformation:
/// - Keeps metadata emission gated by typechecker diagnostics so future mobile
///   build flows cannot bypass source-level bridge validation.
pub(crate) fn typecheck_mobile_bridge_metadata(
    declarations: &[MobileBridgeDeclaration],
) -> Result<MobileBridgeMetadata, Vec<Diagnostic>> {
    generate_mobile_bridge_metadata(declarations).map_err(|diagnostics| {
        diagnostics
            .into_iter()
            .map(|diagnostic| {
                mobile_bridge_typecheck_diagnostic(diagnostic.code, diagnostic.message)
            })
            .collect()
    })
}

/// Builds one typechecker diagnostic from a mobile bridge validation failure.
fn mobile_bridge_typecheck_diagnostic(code: &'static str, message: String) -> Diagnostic {
    Diagnostic {
        span: Span::new(0, 0),
        message: format!("{}: {}", code, message),
        severity: DiagSeverity::Error,
    }
}

#[cfg(test)]
#[path = "mobile_bridge_validation_test.rs"]
mod mobile_bridge_validation_test;
