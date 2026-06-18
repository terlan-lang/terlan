use serde::{Deserialize, Serialize};

use crate::ebnf::EbnfSourceSpan;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Serializable type annotation emitted by syntax output.
///
/// Inputs:
/// - Parsed type annotation text and span.
///
/// Outputs:
/// - Stable textual type payload plus source location.
///
/// Transformation:
/// - Keeps syntax output independent from typechecker internals while
///   preserving enough data for diagnostics and later parsing.
pub struct SyntaxTypeOutput {
    pub text: String,
    pub span: EbnfSourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
/// Serializable callable parameter emitted by syntax output.
///
/// Inputs:
/// - Parsed parameter name, type annotation, mutability flag, and span.
///
/// Outputs:
/// - Stable parameter payload for functions, methods, lambdas, and
///   constructors.
///
/// Transformation:
/// - Preserves receiver/parameter mutability as metadata before typechecking.
pub struct SyntaxParamOutput {
    pub name: String,
    pub annotation: SyntaxTypeOutput,
    #[serde(default, rename = "mutable", skip_serializing_if = "is_false")]
    pub is_mutable: bool,
    pub span: EbnfSourceSpan,
}

/// Returns whether a boolean value is false for compact syntax JSON output.
///
/// Inputs:
/// - `value`: boolean metadata value being considered for serialization.
///
/// Output:
/// - `true` when the value is false and can be omitted from serialized output.
///
/// Transformation:
/// - Performs a direct boolean negation with no side effects.
fn is_false(value: &bool) -> bool {
    !*value
}
