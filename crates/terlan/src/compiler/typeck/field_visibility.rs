use super::*;

/// Splits source-level private field spelling from the field name.
///
/// Inputs:
/// - `field`: field text from syntax output, optionally written as `#name`.
///
/// Output:
/// - Clean field name and whether the source requested private access.
///
/// Transformation:
/// - Removes one leading private marker so lookup code can use canonical field
///   names while diagnostics still know source intent.
pub(crate) fn split_private_field_spelling(field: &str) -> (&str, bool) {
    field
        .strip_prefix('#')
        .map(|name| (name, true))
        .unwrap_or((field, false))
}

/// Computes a struct field visibility diagnostic.
///
/// Inputs:
/// - `struct_name`: receiver or pattern struct name visible in the current
///   module.
/// - `field_name`: canonical field name without a private marker.
/// - `requested_private`: whether source used `#field`.
/// - `struct_field_visibility`: local and imported visibility metadata keyed
///   by local struct name.
/// - `imported_type_names`: imported type metadata keyed by local type name.
///
/// Output:
/// - `Some(message)` when source violates field visibility, otherwise `None`.
///
/// Transformation:
/// - Applies the Terlan rule that private fields require `#` in their defining
///   module and cannot be accessed from imported struct types.
pub(crate) fn struct_field_visibility_error(
    struct_name: &str,
    field_name: &str,
    requested_private: bool,
    struct_field_visibility: &HashMap<String, HashMap<String, StructFieldVisibility>>,
    imported_type_names: &HashMap<String, QualifiedTypeName>,
) -> Option<String> {
    let is_private = struct_field_visibility
        .get(struct_name)
        .and_then(|fields| fields.get(field_name))
        .map(|visibility| visibility.is_private)
        .unwrap_or(false);

    if is_private && !requested_private {
        return Some(format!(
            "private field {} on struct {} must be accessed as #{}",
            field_name, struct_name, field_name
        ));
    }

    if requested_private && !is_private {
        return Some(format!(
            "field {} on struct {} is not declared private",
            field_name, struct_name
        ));
    }

    if requested_private {
        if let Some(imported) = imported_type_names.get(struct_name) {
            return Some(format!(
                "private field {} on imported struct {}.{} cannot be accessed outside defining module",
                field_name, imported.module, imported.name
            ));
        }
    }

    None
}
