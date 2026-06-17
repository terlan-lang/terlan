use super::*;

/// Tests whether a receiver type has a specific fully qualified type head.
///
/// Inputs:
/// - `receiver_type`: normalized source type text, optionally generic.
/// - `qualified_head`: expected fully qualified head before generic arguments.
///
/// Output:
/// - `true` when the receiver type's head exactly matches `qualified_head`.
///
/// Transformation:
/// - Compacts generic spacing and strips type arguments, preserving module
///   qualifiers so receiver dispatch can distinguish same-named public types
///   such as `std.core.Task.Task` and `std.beam.Task.Task`.
pub(super) fn receiver_type_has_head(receiver_type: &str, qualified_head: &str) -> bool {
    let compact = compact_type_application(&compact_spaces(receiver_type));
    let head = compact
        .split_once('[')
        .map_or(compact.as_str(), |(head, _)| head);
    head == qualified_head
}

/// Extracts the nominal type head from receiver type text.
///
/// Inputs:
/// - `receiver_type`: normalized source type text, optionally generic or
///   qualified.
///
/// Output:
/// - Final nominal type segment without generic arguments.
///
/// Transformation:
/// - Compacts type-application spacing, strips generic arguments after `[`,
///   and keeps the segment after the final module qualifier.
pub(super) fn receiver_type_head(receiver_type: &str) -> String {
    let compact = compact_type_application(&compact_spaces(receiver_type));
    let head = compact
        .split_once('[')
        .map_or(compact.as_str(), |(head, _)| head);
    head.rsplit('.').next().unwrap_or(head).to_string()
}
