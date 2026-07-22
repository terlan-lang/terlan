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
    let targets = ctx.function_imports.get(function_name)?;
    if targets.len() > 1 {
        let mut matches = Vec::new();
        let mut first_error = None;
        for target in targets {
            if !selected_import_target_matches_arg_types(target, arg_types) {
                continue;
            }
            let mut trial_subst = subst.clone();
            match infer_one_selected_imported_function_call(
                function_name,
                target,
                arg_types,
                type_args,
                arg_names,
                ctx,
                &mut trial_subst,
            ) {
                Ok(ty) => matches.push((ty, trial_subst, target)),
                Err(message) => {
                    first_error.get_or_insert((target.span, message));
                }
            }
        }

        return Some(match matches.len() {
            1 => {
                let (ty, selected_subst, _) = matches.remove(0);
                *subst = selected_subst;
                ty
            }
            0 => {
                if let Some((span, message)) = first_error {
                    errors.push(spanned_expression_error(span, message));
                }
                Type::Dynamic
            }
            _ => {
                let modules = matches
                    .into_iter()
                    .map(|(_, _, target)| format!("{}.{}", target.module, target.function))
                    .collect::<Vec<_>>()
                    .join(", ");
                errors.push(spanned_expression_error(
                    targets[0].span,
                    format!(
                        "selected imported function `{function_name}/{}` is ambiguous across {modules}",
                        arg_types.len()
                    ),
                ));
                Type::Dynamic
            }
        });
    }

    let target = &targets[0];
    match infer_one_selected_imported_function_call(
        function_name,
        target,
        arg_types,
        type_args,
        arg_names,
        ctx,
        subst,
    ) {
        Ok(ty) => Some(ty),
        Err(message) => {
            errors.push(spanned_expression_error(target.span, message));
            Some(Type::Dynamic)
        }
    }
}

fn infer_one_selected_imported_function_call(
    function_name: &str,
    target: &ImportedFunctionTarget,
    arg_types: &[Type],
    type_args: &[SyntaxTypeOutput],
    arg_names: &[Option<String>],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<Type, String> {
    let resolved_module = ctx
        .module_aliases
        .get(&target.module)
        .map(String::as_str)
        .unwrap_or(target.module.as_str());
    require_explicit_process_value_type(resolved_module, &target.function, type_args)?;
    let Some(interface) = ctx.interface_map.get(resolved_module) else {
        return Err(missing_imported_function_interface_message(
            resolved_module,
            &target.function,
            ctx.interface_map,
        ));
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
                return Err(named_errors.into_iter().next().unwrap_or_else(|| {
                    format!("imported function `{function_name}` arguments did not match")
                }));
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
        Ok(Some(ty)) => return Ok(ty),
        Ok(None) => {}
        Err(message) => return Err(message),
    }

    if !interface
        .functions
        .contains_key(&(target.function.clone(), effective_arg_types.len()))
        && !interface
            .function_overloads
            .contains_key(&(target.function.clone(), effective_arg_types.len()))
    {
        return Err(missing_imported_function_message(
            interface,
            &target.function,
            effective_arg_types.len(),
        ));
    }

    Ok(Type::Dynamic)
}

fn selected_import_target_matches_arg_types(
    target: &ImportedFunctionTarget,
    arg_types: &[Type],
) -> bool {
    let Some(first) = arg_types.first() else {
        return true;
    };
    match (target.module.as_str(), target.function.as_str(), first) {
        ("std.core.Bool", "equal" | "compare" | "to_string", Type::Bool) => true,
        (
            "std.core.Int",
            "equal" | "compare" | "min" | "max" | "abs" | "to_string" | "to_string_base",
            Type::Int | Type::LiteralInt(_),
        ) => true,
        (
            "std.core.Float",
            "equal" | "compare" | "min" | "max" | "abs" | "to_string",
            Type::Float,
        ) => true,
        ("std.core.String", "equal" | "compare" | "to_string", Type::Binary) => true,
        ("std.core.Unit", "equal" | "compare" | "to_string", Type::Named { name, .. })
            if name == "Unit" =>
        {
            true
        }
        (
            "std.core.Ordering",
            "equal" | "compare" | "to_string",
            Type::Named { module, name, .. },
        ) if module.as_deref() == Some("std.core.Ordering") && name == "Comparison" => true,
        (
            "std.core.Bool" | "std.core.Int" | "std.core.Float" | "std.core.String"
            | "std.core.Unit" | "std.core.Ordering",
            "from_string",
            Type::Binary,
        ) => true,
        ("std.core.Int", "from_string_base", Type::Binary) => true,
        ("std.core.Atom", "equal" | "to_string", Type::Atom | Type::LiteralAtom(_)) => true,
        (
            "std.core.Bool" | "std.core.Int" | "std.core.Float" | "std.core.String"
            | "std.core.Unit" | "std.core.Ordering" | "std.core.Atom",
            _,
            _,
        ) => false,
        _ => true,
    }
}

/// Infers an imported module-member function call.
///
/// Inputs:
/// - `module_alias`: source module alias from `Module.function(...)`.
/// - `member`: provider-side function name.
/// - `display_name`: source text used in diagnostics.
/// - `arg_types`, `type_args`, and `arg_names`: call-site argument metadata.
/// - `ctx`, `subst`, and `errors`: active expression inference state.
///
/// Output:
/// - `Some(Type)` when the module alias resolves to a loaded interface.
/// - `None` when the receiver is not an imported module alias.
///
/// Transformation:
/// - Resolves `Module.function(args...)` through the same interface overload
///   machinery used by selected imports, preserving default arguments,
///   explicit type arguments, and provider diagnostics.
pub(super) fn infer_syntax_imported_module_member_function_call(
    module_alias: &str,
    member: &str,
    display_name: &str,
    arg_types: &[Type],
    type_args: &[SyntaxTypeOutput],
    arg_names: &[Option<String>],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let resolved_module = ctx.module_aliases.get(module_alias)?;
    let Some(interface) = ctx.interface_map.get(resolved_module) else {
        errors.push(missing_imported_function_interface_message(
            resolved_module,
            member,
            ctx.interface_map,
        ));
        return Some(Type::Dynamic);
    };

    let candidate_signatures = interface_function_signatures(interface, member, arg_types.len());
    let effective_arg_types = if !candidate_signatures.is_empty() {
        let mut named_errors = Vec::new();
        match complete_defaulted_imported_call_args_for_any_signature(
            display_name,
            arg_types,
            arg_names,
            &candidate_signatures,
            interface,
            ctx,
            &mut named_errors,
        ) {
            Some(arg_types) => arg_types,
            None => {
                errors.extend(named_errors);
                return Some(Type::Dynamic);
            }
        }
    } else {
        arg_types.to_vec()
    };

    match infer_interface_function_overload_with_explicit_type_args(
        interface,
        member,
        display_name,
        &effective_arg_types,
        type_args,
        ctx,
        subst,
    ) {
        Ok(Some(ty)) => return Some(ty),
        Ok(None) => {}
        Err(message) => {
            errors.push(message);
            return Some(Type::Dynamic);
        }
    }

    if !interface
        .functions
        .contains_key(&(member.to_string(), effective_arg_types.len()))
        && !interface
            .function_overloads
            .contains_key(&(member.to_string(), effective_arg_types.len()))
    {
        errors.push(missing_imported_function_message(
            interface,
            member,
            effective_arg_types.len(),
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
