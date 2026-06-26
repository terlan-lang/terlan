use std::collections::HashSet;

use crate::Type;

/// Returns whether a call-site argument vector contains named arguments.
///
/// Inputs:
/// - `arg_names`: optional source argument names parallel to call arguments.
///
/// Output:
/// - `true` when at least one argument has a source name.
///
/// Transformation:
/// - Scans metadata only; expression and type values are not inspected.
pub(super) fn call_has_named_args(arg_names: &[Option<String>]) -> bool {
    arg_names.iter().any(Option::is_some)
}

/// Validates one named call-site argument list against parameter names.
///
/// Inputs:
/// - `display_name`: callable name used in diagnostics.
/// - `arg_names`: optional source argument names parallel to call arguments.
/// - `param_names`: declaration parameter names in positional order.
/// - `errors`: output diagnostics.
///
/// Output:
/// - `true` when names are absent or all names refer to unsupplied parameters.
///
/// Transformation:
/// - Treats leading positional arguments as already supplied, rejects unknown
///   names, rejects duplicate named arguments, and rejects names that target a
///   parameter already provided positionally.
pub(super) fn validate_named_call_args(
    display_name: &str,
    arg_names: &[Option<String>],
    param_names: &[&str],
    errors: &mut Vec<String>,
) -> bool {
    if !call_has_named_args(arg_names) {
        return true;
    }

    let positional_count = arg_names.iter().take_while(|name| name.is_none()).count();
    let mut supplied = HashSet::new();
    for name in param_names.iter().take(positional_count) {
        supplied.insert((*name).to_string());
    }

    for name in arg_names.iter().filter_map(Option::as_ref) {
        let Some(param_index) = param_names.iter().position(|param| param == name) else {
            errors.push(format!(
                "unknown named argument `{}` for call to `{}`",
                name, display_name
            ));
            continue;
        };

        if param_index < positional_count {
            errors.push(format!(
                "argument `{}` for call to `{}` is already supplied positionally",
                name, display_name
            ));
            continue;
        }

        if !supplied.insert(name.clone()) {
            errors.push(format!(
                "duplicate named argument `{}` for call to `{}`",
                name, display_name
            ));
        }
    }

    errors.is_empty()
}

/// Reorders call argument types into declaration parameter order.
///
/// Inputs:
/// - `arg_types`: inferred call argument types in source order.
/// - `arg_names`: optional source argument names parallel to `arg_types`.
/// - `param_names`: declaration parameter names in positional order.
///
/// Output:
/// - Argument types rearranged so each named argument appears at its declared
///   parameter index.
///
/// Transformation:
/// - Keeps leading positional arguments at their original indexes and places
///   named arguments into matching parameter slots. Validation runs before this
///   helper, so missing names are represented defensively as `Dynamic`.
pub(super) fn reorder_named_call_arg_types(
    arg_types: &[Type],
    arg_names: &[Option<String>],
    param_names: &[&str],
) -> Vec<Type> {
    if !call_has_named_args(arg_names) {
        return arg_types.to_vec();
    }

    let mut reordered = vec![None; arg_types.len()];
    for (index, arg_type) in arg_types.iter().enumerate() {
        match arg_names.get(index).and_then(Option::as_ref) {
            Some(name) => {
                if let Some(param_index) = param_names.iter().position(|param| param == name) {
                    if param_index < reordered.len() {
                        reordered[param_index] = Some(arg_type.clone());
                    }
                }
            }
            None => {
                if index < reordered.len() {
                    reordered[index] = Some(arg_type.clone());
                }
            }
        }
    }

    reordered
        .into_iter()
        .map(|arg_type| arg_type.unwrap_or(Type::Dynamic))
        .collect()
}

/// Completes call argument types by inserting defaulted parameter types.
///
/// Inputs:
/// - `arg_types`: inferred source argument types.
/// - `arg_names`: optional source names parallel to `arg_types`.
/// - `param_names`: declaration parameter names in positional order.
/// - `param_types`: declaration parameter types in positional order.
///
/// Output:
/// - Argument types in declaration order, including expected types for omitted
///   defaulted parameters.
///
/// Transformation:
/// - Keeps pure positional calls in source order and appends omitted trailing
///   parameter types. For named calls, places each supplied type into its
///   declaration slot and fills missing slots from `param_types`.
pub(super) fn complete_defaulted_call_arg_types(
    arg_types: &[Type],
    arg_names: &[Option<String>],
    param_names: &[&str],
    param_types: &[Type],
) -> Vec<Type> {
    if !call_has_named_args(arg_names) {
        return arg_types
            .iter()
            .cloned()
            .chain(param_types.iter().skip(arg_types.len()).cloned())
            .collect();
    }

    let mut ordered = vec![None; param_types.len()];
    for (index, arg_type) in arg_types.iter().enumerate() {
        match arg_names.get(index).and_then(Option::as_ref) {
            Some(name) => {
                if let Some(param_index) = param_names.iter().position(|param| param == name) {
                    if param_index < ordered.len() {
                        ordered[param_index] = Some(arg_type.clone());
                    }
                }
            }
            None => {
                if index < ordered.len() {
                    ordered[index] = Some(arg_type.clone());
                }
            }
        }
    }

    ordered
        .into_iter()
        .enumerate()
        .map(|(index, ty)| ty.unwrap_or_else(|| param_types[index].clone()))
        .collect()
}

/// Computes supplied declaration parameter slots for a call.
///
/// Inputs:
/// - `arg_names`: optional source names parallel to supplied arguments.
/// - `param_names`: declaration parameter names in positional order.
///
/// Output:
/// - Set of parameter indexes supplied by positional or named arguments.
///
/// Transformation:
/// - Treats unnamed arguments as positional slots and named arguments as the
///   matching declaration slot.
pub(super) fn supplied_named_parameter_slots(
    arg_names: &[Option<String>],
    param_names: &[&str],
) -> HashSet<usize> {
    let mut supplied = HashSet::new();
    for (index, name) in arg_names.iter().enumerate() {
        match name {
            Some(name) => {
                if let Some(param_index) = param_names.iter().position(|param| param == name) {
                    supplied.insert(param_index);
                }
            }
            None => {
                supplied.insert(index);
            }
        }
    }
    supplied
}
