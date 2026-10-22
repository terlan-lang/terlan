//! Shared operation identities for source native declarations and metadata.

use crate::terlan_syntax::syntax_output::SyntaxAnnotationValueOutput;
use crate::terlan_syntax::{SyntaxDeclarationOutput, SyntaxDeclarationPayload};

/// Derives the default native key from the module path and callable name.
pub(crate) fn inferred_operation(module: &str, function: &str) -> String {
    let namespace = module
        .split('.')
        .map(crate::terlan_hir::identifier_to_snake)
        .collect::<Vec<_>>()
        .join(".");
    format!("{namespace}.{function}")
}

/// Emits an explicit override only when the provider key differs from the default.
pub(crate) fn annotation(module: &str, function: &str, operation: &str) -> String {
    if inferred_operation(module, function) == operation {
        "@compiler.native".into()
    } else {
        format!("@compiler.native {{{operation}}}")
    }
}

/// Resolves a native marker, preserving an explicit operation alias when given.
pub(crate) fn declaration_operation(
    module: &str,
    declaration: &SyntaxDeclarationOutput,
) -> Option<String> {
    let annotation = declaration
        .annotations
        .iter()
        .find(|annotation| annotation.path == ["compiler", "native"])?;
    let name = match &declaration.payload {
        SyntaxDeclarationPayload::Function { name, .. }
        | SyntaxDeclarationPayload::Method { name, .. } => name,
        _ => return None,
    };
    match annotation.values.first() {
        Some(SyntaxAnnotationValueOutput::Name { segments }) => Some(segments.join(".")),
        None => Some(inferred_operation(module, name)),
        _ => None,
    }
}

#[cfg(test)]
#[path = "native_bindings_test.rs"]
mod tests;
