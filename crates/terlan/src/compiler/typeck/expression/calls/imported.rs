use super::*;
use crate::terlan_hir::FunctionSignature;

/// Infers a selected imported function call.
///
/// Inputs:
/// - `function_name`: local call name from source, possibly an import alias.
/// - `arg_types`: already inferred argument types.
/// - `type_args`: explicit generic call-site type arguments.
/// - `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - `Some(Type)` when the local name is a selected function import.
/// - `None` when the local name is not imported as a function.
///
/// Transformation:
/// - Resolves the local import target to its provider module interface, parses
///   the public function signature for the call arity, and reuses ordinary
///   function-call inference so argument mismatches are reported before backend
///   emission.
pub(super) fn infer_syntax_imported_function_call(
    function_name: &str,
    arg_types: &[Type],
    type_args: &[SyntaxTypeOutput],
    arg_names: &[Option<String>],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let target = ctx.function_imports.get(function_name)?;
    let resolved_module = ctx
        .module_aliases
        .get(&target.module)
        .map(String::as_str)
        .unwrap_or(target.module.as_str());
    let Some(interface) = ctx.interface_map.get(resolved_module) else {
        errors.push(spanned_expression_error(
            target.span,
            missing_imported_function_interface_message(
                resolved_module,
                &target.function,
                ctx.interface_map,
            ),
        ));
        return Some(Type::Dynamic);
    };

    let candidate_signatures =
        interface_function_signatures(interface, &target.function, arg_types.len());
    let effective_arg_types = if !candidate_signatures.is_empty() {
        let mut named_errors = Vec::new();
        match complete_defaulted_imported_call_args_for_any_signature(
            function_name,
            arg_types,
            arg_names,
            &candidate_signatures,
            interface,
            ctx,
            &mut named_errors,
        ) {
            Some(arg_types) => arg_types,
            None => {
                errors.extend(
                    named_errors
                        .into_iter()
                        .map(|message| spanned_expression_error(target.span, message)),
                );
                return Some(Type::Dynamic);
            }
        }
    } else {
        arg_types.to_vec()
    };

    match infer_interface_function_overload_with_explicit_type_args(
        interface,
        &target.function,
        function_name,
        &effective_arg_types,
        type_args,
        ctx,
        subst,
    ) {
        Ok(Some(ty)) => return Some(ty),
        Ok(None) => {}
        Err(message) => {
            errors.push(spanned_expression_error(target.span, message));
            return Some(Type::Dynamic);
        }
    }

    if !interface
        .functions
        .contains_key(&(target.function.clone(), effective_arg_types.len()))
        && !interface
            .function_overloads
            .contains_key(&(target.function.clone(), effective_arg_types.len()))
    {
        errors.push(spanned_expression_error(
            target.span,
            missing_imported_function_message(
                interface,
                &target.function,
                effective_arg_types.len(),
            ),
        ));
        return Some(Type::Dynamic);
    }

    Some(Type::Dynamic)
}

/// Returns public function signatures that accept one imported callable arity.
///
/// Inputs:
/// - `interface`: imported module interface.
/// - `function_name`: provider-side function name.
/// - `arity`: source call argument count.
///
/// Output:
/// - Public interface signatures matching the requested name and supplied
///   arity after considering trailing defaulted parameters.
///
/// Transformation:
/// - Prefers overload metadata and falls back to the compatibility single
///   signature map so named/default validation sees the same surface as
///   imported call inference.
pub(super) fn interface_function_signatures<'a>(
    interface: &'a ModuleInterface,
    function_name: &str,
    arity: usize,
) -> Vec<&'a FunctionSignature> {
    let overloads = interface
        .function_overloads
        .iter()
        .filter(|((name, _), _)| name == function_name)
        .flat_map(|(_, signatures)| signatures.iter())
        .filter(|signature| imported_signature_accepts_arity(signature, arity))
        .collect::<Vec<_>>();
    if !overloads.is_empty() {
        return overloads;
    }

    interface
        .functions
        .iter()
        .filter(|((name, _), signature)| {
            name == function_name && imported_signature_accepts_arity(signature, arity)
        })
        .map(|(_, signature)| signature)
        .collect()
}

/// Checks whether an imported function signature accepts a supplied arity.
///
/// Inputs:
/// - `signature`: imported function signature.
/// - `arity`: source call argument count.
///
/// Output:
/// - `true` when `arity` is between required and full parameter count.
///
/// Transformation:
/// - Computes required parameters from per-parameter default metadata.
fn imported_signature_accepts_arity(signature: &FunctionSignature, arity: usize) -> bool {
    let required = signature
        .params
        .iter()
        .filter(|param| param.default_text.is_none())
        .count();
    signature.public && arity >= required && arity <= signature.params.len()
}

/// Completes named/defaulted imported call arguments against any signature.
///
/// Inputs:
/// - `display_name`: call-site function name used in diagnostics.
/// - `arg_types`: inferred argument types in source order.
/// - `arg_names`: optional source argument names parallel to call arguments.
/// - `signatures`: callable signatures with parameter names.
/// - `interface`: provider interface that owns the signatures.
/// - `ctx`: active expression inference context.
/// - `errors`: output diagnostics.
///
/// Output:
/// - Argument types in full declaration order when a candidate accepts the
///   supplied names and omitted defaults.
///
/// Transformation:
/// - Validates names and required slots, parses the accepted interface
///   signature, and fills omitted slots with parameter types before normal
///   overload inference.
pub(super) fn complete_defaulted_imported_call_args_for_any_signature(
    display_name: &str,
    arg_types: &[Type],
    arg_names: &[Option<String>],
    signatures: &[&FunctionSignature],
    interface: &ModuleInterface,
    ctx: &ExprInferContext,
    errors: &mut Vec<String>,
) -> Option<Vec<Type>> {
    let mut last_errors = Vec::new();
    for signature in signatures {
        let param_names = signature
            .params
            .iter()
            .map(|param| param.name.as_str())
            .collect::<Vec<_>>();
        let mut candidate_errors = Vec::new();
        if !validate_named_call_args(display_name, arg_names, &param_names, &mut candidate_errors) {
            last_errors = candidate_errors;
            continue;
        }
        if !validate_required_defaulted_imported_call_args(
            display_name,
            arg_names,
            signature,
            &mut candidate_errors,
        ) {
            last_errors = candidate_errors;
            continue;
        }
        let Some(scheme) = parse_interface_signature(signature, interface, ctx.aliases) else {
            last_errors = vec![format!(
                "cannot parse imported function signature {} / {}",
                display_name,
                signature.params.len()
            )];
            continue;
        };
        return Some(complete_defaulted_imported_call_arg_types(
            arg_types,
            arg_names,
            &param_names,
            &scheme,
        ));
    }

    errors.extend(last_errors);
    None
}

/// Validates required imported function parameters for default-aware calls.
///
/// Inputs:
/// - `display_name`: call-site function name used in diagnostics.
/// - `arg_names`: optional source names parallel to supplied arguments.
/// - `signature`: imported function signature.
/// - `errors`: output diagnostics.
///
/// Output:
/// - `true` when every required parameter is supplied.
///
/// Transformation:
/// - Computes supplied declaration slots and rejects any non-defaulted
///   parameter missing from the call site.
fn validate_required_defaulted_imported_call_args(
    display_name: &str,
    arg_names: &[Option<String>],
    signature: &FunctionSignature,
    errors: &mut Vec<String>,
) -> bool {
    let param_names = signature
        .params
        .iter()
        .map(|param| param.name.as_str())
        .collect::<Vec<_>>();
    let supplied = supplied_named_parameter_slots(arg_names, &param_names);
    for (index, parameter) in signature.params.iter().enumerate() {
        if parameter.default_text.is_none() && !supplied.contains(&index) {
            errors.push(format!(
                "missing required argument `{}` for call to `{}`",
                parameter.name, display_name
            ));
        }
    }

    errors.is_empty()
}

/// Completes imported function argument types by inserting defaulted types.
///
/// Inputs:
/// - `arg_types`: inferred source argument types.
/// - `arg_names`: optional source names parallel to `arg_types`.
/// - `param_names`: imported parameter names in declaration order.
/// - `scheme`: parsed imported function scheme.
///
/// Output:
/// - Argument types in full imported function parameter order.
///
/// Transformation:
/// - Places positional and named arguments into signature slots and fills
///   omitted defaulted slots with the parsed parameter type.
fn complete_defaulted_imported_call_arg_types(
    arg_types: &[Type],
    arg_names: &[Option<String>],
    param_names: &[&str],
    scheme: &FunctionScheme,
) -> Vec<Type> {
    complete_defaulted_call_arg_types(arg_types, arg_names, param_names, &scheme.params)
}
