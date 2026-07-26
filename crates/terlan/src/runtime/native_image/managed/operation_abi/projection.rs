//! Compiler-facing analysis of exact managed aggregate projections.

use super::{
    decode_operation, encode_string_prepend_literal_operation, ManagedOperation, SemanticTypeId,
};

/// Decodes only an exact aggregate-reference projection for compiler analysis.
///
/// Other managed operations and malformed payloads deliberately return `None`;
/// callers must then retain their conservative full-value behavior.
pub(crate) fn decode_aggregate_field_projection(encoded: &[u8]) -> Option<(SemanticTypeId, usize)> {
    match decode_operation(encoded).ok()? {
        ManagedOperation::Project { semantic, field }
        | ManagedOperation::StringPrependProjectedLiteral {
            semantic, field, ..
        } => Some((semantic, field)),
        _ => None,
    }
}

/// Converts a projected-string operation into its scalar equivalent.
pub(crate) fn scalar_string_projection_rewrite(
    encoded: &[u8],
) -> Option<(SemanticTypeId, usize, Option<Vec<u8>>)> {
    match decode_operation(encoded).ok()? {
        ManagedOperation::Project { semantic, field } => Some((semantic, field, None)),
        ManagedOperation::StringPrependProjectedLiteral {
            semantic,
            field,
            literal,
        } => Some((
            semantic,
            field,
            Some(encode_string_prepend_literal_operation(literal).ok()?),
        )),
        _ => None,
    }
}
