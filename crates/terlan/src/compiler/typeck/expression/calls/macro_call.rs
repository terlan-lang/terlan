use super::*;

/// Infers a resolved macro call.
///
/// Inputs:
/// - `macro_name`: source macro identifier.
/// - `arg_types`: inferred argument types.
/// - `ctx`, `subst`, and `errors`: active inference state.
///
/// Output:
/// - Macro return type when a visible macro signature matches.
///
/// Transformation:
/// - Looks up macro-call signatures and checks arguments through ordinary
///   function inference, unwrapping macro-specific return wrappers.
pub(crate) fn infer_syntax_macro_call(
    name: &str,
    arg_types: &[Type],
    ctx: &ExprInferContext,
    subst: &mut HashMap<TypeVarId, Type>,
    errors: &mut Vec<String>,
) -> Option<Type> {
    let candidates: Vec<_> = ctx
        .signatures
        .iter()
        .filter_map(|((candidate_name, arity), schemes)| {
            if candidate_name == name {
                Some((*arity, schemes))
            } else {
                None
            }
        })
        .collect();

    if candidates.is_empty() {
        return None;
    }

    for (arity, scheme) in candidates.iter() {
        if *arity == arg_types.len() {
            match infer_function_scheme_overload(scheme, name, arg_types, ctx, subst) {
                Ok(ty) => return Some(unwrap_macro_return_type(ty)),
                Err(message) => {
                    errors.push(message);
                    return Some(Type::Dynamic);
                }
            }
        }
    }

    let arities = candidates
        .iter()
        .map(|(arity, _)| arity.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    errors.push(format!(
        "wrong arity for macro `{}`: expected one of [{}] args, found {}",
        name,
        arities,
        arg_types.len()
    ));
    Some(Type::Dynamic)
}

/// Unwraps macro-specific return wrappers.
///
/// Inputs:
/// - `ty`: inferred macro implementation return type.
///
/// Output:
/// - User-visible macro expansion result type.
///
/// Transformation:
/// - Removes one known macro wrapper layer and leaves all other types
///   unchanged.
fn unwrap_macro_return_type(ty: Type) -> Type {
    match ty {
        Type::Named {
            module,
            name: tag,
            args,
        } if module.is_none() && tag == "Ast" && args.len() == 1 => {
            args.into_iter().next().unwrap_or(Type::Dynamic)
        }
        other => other,
    }
}
