use super::*;

/// Infers a local receiver-method call.
///
/// Inputs:
/// - `expr`: syntax-output call expression whose callee may be field access.
/// - `arg_types`: inferred non-receiver argument types.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference context.
///
/// Output:
/// - `Some(Type)` for a resolved local receiver method or a method-shaped call
///   that has candidates but no matching receiver.
/// - `None` when the expression is not a receiver-method call known to the
///   current module.
///
/// Transformation:
/// - Reads `receiver.method(args...)` from the field-access callee, infers the
///   receiver type, selects a receiver-method signature by method/arity and
///   receiver unification, then checks the non-receiver arguments with the
///   existing function-scheme inference path.
pub(super) fn infer_syntax_receiver_method_call(
    expr: &SyntaxExprOutput,
    arg_types: &[Type],
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let callee = expr.children.first()?;
    if !matches!(callee.kind, SyntaxExprKind::FieldAccess) {
        return None;
    }
    let method = callee.text.as_deref()?;
    let candidates = receiver_method_candidates_accepting_call(ctx, method, arg_types.len())?;
    let receiver = callee.children.first()?;
    let receiver_type = infer_syntax_expr(receiver, locals, ctx, subst, errors);

    for candidate in &candidates {
        let mut trial_subst = subst.clone();
        let param_names = candidate
            .param_names
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        if !validate_named_call_args(method, &expr.arg_names, &param_names, errors) {
            return Some(Type::Dynamic);
        }
        if !validate_required_defaulted_receiver_call_args(
            method,
            &expr.arg_names,
            candidate,
            errors,
        ) {
            return Some(Type::Dynamic);
        }
        let effective_arg_types = complete_defaulted_receiver_call_arg_types(
            arg_types,
            &expr.arg_names,
            &param_names,
            candidate,
        );
        match infer_receiver_method_candidate(
            candidate,
            Some(method),
            &receiver_type,
            &effective_arg_types,
            ctx,
            &mut trial_subst,
        ) {
            Ok(ty) => {
                *subst = trial_subst;
                return Some(ty);
            }
            Err(_) => {
                continue;
            }
        }
    }

    let candidate_types = candidates
        .iter()
        .map(|candidate| pretty_type(&candidate.receiver_type))
        .collect::<Vec<_>>()
        .join(", ");
    errors.push(format!(
        "no receiver method `{}` / {} for {}; candidates: {}",
        method,
        arg_types.len(),
        pretty_type(&receiver_type),
        candidate_types
    ));
    Some(Type::Dynamic)
}

/// Returns receiver-method candidates that accept a supplied argument count.
///
/// Inputs:
/// - `ctx`: expression inference context with receiver dispatch metadata.
/// - `method`: source method name.
/// - `supplied_arity`: number of non-receiver arguments at the call site.
///
/// Output:
/// - Candidate receiver-method signatures whose required/full arity range
///   accepts the supplied arity.
/// - `None` when no receiver method with that source name can accept the arity.
///
/// Transformation:
/// - Scans receiver dispatch buckets by method name rather than exact arity so
///   trailing defaulted parameters are callable without duplicating dispatch
///   entries for every accepted arity.
pub(super) fn receiver_method_candidates_accepting_call<'a>(
    ctx: &'a ExprInferContext<'_>,
    method: &str,
    supplied_arity: usize,
) -> Option<Vec<&'a ReceiverMethodDispatchSignature>> {
    let candidates = ctx
        .receiver_methods
        .iter()
        .filter(|((candidate_method, _), _)| candidate_method == method)
        .flat_map(|(_, candidates)| candidates.iter())
        .filter(|candidate| receiver_method_candidate_accepts_arity(candidate, supplied_arity))
        .collect::<Vec<_>>();
    (!candidates.is_empty()).then_some(candidates)
}

/// Checks whether one receiver-method candidate accepts a supplied arity.
///
/// Inputs:
/// - `candidate`: receiver-method dispatch signature.
/// - `supplied_arity`: number of non-receiver call arguments.
///
/// Output:
/// - `true` when supplied arguments cover required parameters and do not exceed
///   the method's full non-receiver arity.
///
/// Transformation:
/// - Uses declaration default metadata to compute the required arity.
fn receiver_method_candidate_accepts_arity(
    candidate: &ReceiverMethodDispatchSignature,
    supplied_arity: usize,
) -> bool {
    let required = candidate
        .param_defaults
        .iter()
        .filter(|default| default.is_none())
        .count();
    supplied_arity >= required && supplied_arity <= candidate.scheme.params.len()
}

/// Validates required receiver-method arguments for default-aware calls.
///
/// Inputs:
/// - `method`: source method name for diagnostics.
/// - `arg_names`: optional source names parallel to supplied arguments.
/// - `candidate`: selected receiver-method dispatch signature.
/// - `errors`: output diagnostics.
///
/// Output:
/// - `true` when every required non-receiver parameter is supplied.
///
/// Transformation:
/// - Computes supplied parameter slots from positional/named arguments and
///   rejects missing slots without defaults.
pub(super) fn validate_required_defaulted_receiver_call_args(
    method: &str,
    arg_names: &[Option<String>],
    candidate: &ReceiverMethodDispatchSignature,
    errors: &mut Vec<String>,
) -> bool {
    let param_names = candidate
        .param_names
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let supplied = supplied_named_parameter_slots(arg_names, &param_names);
    for (index, name) in candidate.param_names.iter().enumerate() {
        if candidate.param_defaults[index].is_none() && !supplied.contains(&index) {
            errors.push(format!(
                "missing required argument `{}` for call to `{}`",
                name, method
            ));
        }
    }

    errors.is_empty()
}

/// Completes receiver-method argument types by inserting defaulted types.
///
/// Inputs:
/// - `arg_types`: inferred source argument types.
/// - `arg_names`: optional source names parallel to `arg_types`.
/// - `param_names`: declaration parameter names.
/// - `candidate`: selected receiver-method dispatch signature.
///
/// Output:
/// - Argument types in full method parameter order, including expected types
///   for omitted defaulted parameters.
///
/// Transformation:
/// - Places positional and named arguments into method declaration slots and
///   fills omitted slots with the selected method scheme's parameter type.
pub(super) fn complete_defaulted_receiver_call_arg_types(
    arg_types: &[Type],
    arg_names: &[Option<String>],
    param_names: &[&str],
    candidate: &ReceiverMethodDispatchSignature,
) -> Vec<Type> {
    complete_defaulted_call_arg_types(arg_types, arg_names, param_names, &candidate.scheme.params)
}

/// Infers compiler-known primitive receiver method calls.
///
/// Inputs:
/// - `expr`: syntax-output call expression whose callee may be field access.
/// - `arg_types`: inferred non-receiver argument types.
/// - `locals`, `ctx`, `subst`, and `errors`: active inference context.
///
/// Output:
/// - `Some(Type)` for supported primitive receiver calls.
/// - `None` when the expression is not a supported primitive receiver call.
///
/// Transformation:
/// - Reads the receiver type from the field-access callee, prepends that type to
///   the argument check, validates the primitive method's arity and parameter
///   types, and returns the method result type.
pub(super) fn infer_syntax_primitive_receiver_method_call(
    expr: &SyntaxExprOutput,
    arg_types: &[Type],
    locals: &HashMap<String, Type>,
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let callee = expr.children.first()?;
    if !matches!(callee.kind, SyntaxExprKind::FieldAccess) {
        return None;
    }
    let method = callee.text.as_deref()?;
    let receiver = callee.children.first()?;
    let receiver_type = infer_syntax_expr(receiver, locals, ctx, subst, errors);
    let scheme = primitive_receiver_method_scheme(&receiver_type, method, arg_types.len())?;
    let param_names =
        primitive_receiver_method_param_names(&receiver_type, method, arg_types.len())?;
    if !validate_named_call_args(method, &expr.arg_names, &param_names, errors) {
        return Some(Type::Dynamic);
    }
    let effective_arg_types =
        reorder_named_call_arg_types(arg_types, &expr.arg_names, &param_names);

    infer_function_with_bounds(&scheme, Some(method), &effective_arg_types, ctx, subst)
        .map(Some)
        .unwrap_or_else(|message| {
            errors.push(message);
            Some(Type::Dynamic)
        })
}
