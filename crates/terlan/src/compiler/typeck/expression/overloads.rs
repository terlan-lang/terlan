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
    infer_function_scheme_overload_with_explicit_type_args(
        schemes,
        function_name,
        arg_types,
        &[],
        ctx,
        subst,
    )
}

/// Infers a call against local function candidates with explicit type args.
///
/// Inputs:
/// - `schemes`: same-name same-arity callable candidates.
/// - `function_name`: source name used in diagnostics and trait-bound checks.
/// - `arg_types`: already inferred value argument types.
/// - `type_args`: explicit call-site type arguments.
/// - `ctx` and `subst`: active inference context and substitution state.
///
/// Output:
/// - `Ok(Type)` when exactly one candidate accepts the value and type args.
/// - `Err(message)` when no candidate or multiple candidates match.
///
/// Transformation:
/// - Evaluates each candidate with cloned substitutions, first binding
///   explicit type arguments and then value arguments, committing only the
///   selected candidate's substitutions.
pub(crate) fn infer_function_scheme_overload_with_explicit_type_args(
    schemes: &[FunctionScheme],
    function_name: &str,
    arg_types: &[Type],
    type_args: &[SyntaxTypeOutput],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<Type, String> {
    infer_function_scheme_overload_with_selection(
        schemes,
        function_name,
        arg_types,
        type_args,
        ctx,
        subst,
    )
    .map(|(ty, _)| ty)
}

pub(super) fn infer_function_scheme_overload_with_selection<'a>(
    schemes: &'a [FunctionScheme],
    function_name: &str,
    arg_types: &[Type],
    type_args: &[SyntaxTypeOutput],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<(Type, &'a FunctionScheme), String> {
    if schemes.len() == 1 {
        return infer_function_with_explicit_type_args(
            &schemes[0],
            Some(function_name),
            arg_types,
            type_args,
            ctx,
            subst,
        )
        .map(|ty| (ty, &schemes[0]));
    }

    let mut matches = Vec::new();
    for scheme in schemes {
        let mut trial_subst = subst.clone();
        if let Ok(ty) = infer_function_with_explicit_type_args(
            scheme,
            Some(function_name),
            arg_types,
            type_args,
            ctx,
            &mut trial_subst,
        ) {
            matches.push((ty, trial_subst, scheme));
        }
    }

    match matches.len() {
        1 => {
            let (ty, selected_subst, selected_scheme) = matches.remove(0);
            *subst = selected_subst;
            Ok((ty, selected_scheme))
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

pub(super) fn infer_interface_function_overload_with_selection<'a>(
    interface: &'a ModuleInterface,
    function_name: &str,
    display_name: &str,
    arg_types: &[Type],
    type_args: &[SyntaxTypeOutput],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<Option<(Type, &'a crate::terlan_hir::FunctionSignature)>, String> {
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
        return infer_function_with_explicit_type_args(
            &scheme,
            Some(display_name),
            arg_types,
            type_args,
            ctx,
            subst,
        )
        .map(|ty| Some((ty, signature)));
    }

    let mut matches = Vec::new();
    let mut parse_failures = 0usize;
    for signature in candidates {
        let Some(scheme) = parse_interface_signature(signature, interface, ctx.aliases) else {
            parse_failures += 1;
            continue;
        };
        let mut trial_subst = subst.clone();
        if let Ok(ty) = infer_function_with_explicit_type_args(
            &scheme,
            Some(display_name),
            arg_types,
            type_args,
            ctx,
            &mut trial_subst,
        ) {
            matches.push((ty, trial_subst, signature));
        }
    }

    match matches.len() {
        1 => {
            let (ty, selected_subst, signature) = matches.remove(0);
            *subst = selected_subst;
            Ok(Some((ty, signature)))
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
