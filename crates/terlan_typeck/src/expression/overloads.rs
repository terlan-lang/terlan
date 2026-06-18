use super::*;

/// Infers a call against local function signature candidates.
///
/// Inputs:
/// - `schemes`: same-name same-arity local function candidates.
/// - `function_name`: source name used in diagnostics and trait-bound checks.
/// - `arg_types`: already inferred argument types.
/// - `ctx` and `subst`: active expression inference context and substitution
///   state.
///
/// Output:
/// - `Ok(Type)` when exactly one candidate accepts the argument types.
/// - `Err(message)` when no candidate or multiple candidates match.
///
/// Transformation:
/// - Evaluates each candidate with cloned substitutions, commits only the
///   selected substitution set, and uses the same no-match/ambiguous contract
///   as imported overload selection.
pub(crate) fn infer_function_scheme_overload(
    schemes: &[FunctionScheme],
    function_name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<Type, String> {
    if schemes.len() == 1 {
        return infer_function_with_bounds(&schemes[0], Some(function_name), arg_types, ctx, subst);
    }

    let mut matches = Vec::new();
    for scheme in schemes {
        let mut trial_subst = subst.clone();
        if let Ok(ty) = infer_function_with_bounds(
            scheme,
            Some(function_name),
            arg_types,
            ctx,
            &mut trial_subst,
        ) {
            matches.push((ty, trial_subst));
        }
    }

    match matches.len() {
        1 => {
            let (ty, selected_subst) = matches.remove(0);
            *subst = selected_subst;
            Ok(ty)
        }
        0 => Err(format!(
            "no overload of {} / {} accepts [{}]",
            function_name,
            arg_types.len(),
            arg_types
                .iter()
                .map(pretty_type)
                .collect::<Vec<_>>()
                .join(", ")
        )),
        _ => Err(format!(
            "ambiguous overload of {} / {} for [{}]",
            function_name,
            arg_types.len(),
            arg_types
                .iter()
                .map(pretty_type)
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// Checks whether an imported function overload accepts a candidate call.
///
/// Inputs:
/// - `interface`: imported module interface.
/// - `imported_name`: function name in that interface.
/// - `display_name`: local source name used for diagnostic context.
/// - `arg_types`: candidate argument types.
/// - `ctx` and `subst`: active inference context and substitutions.
///
/// Output:
/// - `true` when exactly one interface overload can accept the call.
/// - `false` when no candidate or multiple candidates match.
///
/// Transformation:
/// - Runs overload inference with cloned substitutions so pipe ambiguity probing
///   never mutates real inference state.
pub(super) fn infer_imported_function_candidate_matches(
    interface: &ModuleInterface,
    imported_name: &str,
    display_name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &HashMap<TypeVarId, Type>,
) -> bool {
    let mut trial_subst = subst.clone();
    matches!(
        infer_interface_function_overload(
            interface,
            imported_name,
            display_name,
            arg_types,
            ctx,
            &mut trial_subst,
        ),
        Ok(Some(_))
    )
}

/// Infers a call against all public interface overload candidates.
///
/// Inputs:
/// - `interface`: imported module interface that owns the candidate signatures.
/// - `function_name`: source function name inside the imported interface.
/// - `display_name`: diagnostic name used at the call site.
/// - `arg_types`: already inferred argument types.
/// - `ctx` and `subst`: active expression inference context and substitution
///   state.
///
/// Output:
/// - `Ok(Some(Type))` when exactly one overload accepts the argument types.
/// - `Ok(None)` when the interface has no candidate for the name and arity.
/// - `Err(message)` when candidate signatures cannot be parsed, no overload
///   accepts the arguments, or more than one overload accepts them.
///
/// Transformation:
/// - Reads the overload table first, falls back to the compatibility single
///   signature map, tries each candidate with cloned substitutions, and commits
///   substitutions only for the unique selected candidate.
pub(super) fn infer_interface_function_overload(
    interface: &ModuleInterface,
    function_name: &str,
    display_name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<Option<Type>, String> {
    let key = (function_name.to_string(), arg_types.len());
    let candidates = interface
        .function_overloads
        .get(&key)
        .map(|overloads| overloads.iter().collect::<Vec<_>>())
        .or_else(|| {
            interface
                .functions
                .get(&key)
                .map(|signature| vec![signature])
        })
        .unwrap_or_default();

    if candidates.is_empty() {
        return Ok(None);
    }

    if candidates.len() == 1 {
        let signature = candidates[0];
        let Some(scheme) = parse_interface_signature(signature, interface, ctx.aliases) else {
            return Err(format!(
                "cannot parse imported function signature {} / {}",
                display_name,
                arg_types.len()
            ));
        };
        return infer_function_with_bounds(&scheme, Some(display_name), arg_types, ctx, subst)
            .map(Some);
    }

    let mut matches = Vec::new();
    let mut parse_failures = 0usize;
    for signature in candidates {
        let Some(scheme) = parse_interface_signature(signature, interface, ctx.aliases) else {
            parse_failures += 1;
            continue;
        };
        let mut trial_subst = subst.clone();
        if let Ok(ty) = infer_function_with_bounds(
            &scheme,
            Some(display_name),
            arg_types,
            ctx,
            &mut trial_subst,
        ) {
            matches.push((ty, trial_subst));
        }
    }

    match matches.len() {
        1 => {
            let (ty, selected_subst) = matches.remove(0);
            *subst = selected_subst;
            Ok(Some(ty))
        }
        0 if parse_failures > 0 => Err(format!(
            "cannot parse imported overload signature {} / {}",
            display_name,
            arg_types.len()
        )),
        0 => Err(format!(
            "no overload of {} / {} accepts [{}]",
            display_name,
            arg_types.len(),
            arg_types
                .iter()
                .map(pretty_type)
                .collect::<Vec<_>>()
                .join(", ")
        )),
        _ => Err(format!(
            "ambiguous overload of {} / {} for [{}]",
            display_name,
            arg_types.len(),
            arg_types
                .iter()
                .map(pretty_type)
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}
