use super::*;

mod explicit_type_args;
mod trait_bounds;

use explicit_type_args::bind_explicit_call_type_args;
pub(crate) use trait_bounds::{
    alias_name_for_type, canonicalize_trait_lookup_types, check_function_bounds,
    collect_trait_bound_impl_type_args, infer_trait_method_call_from_current_bounds,
    refine_by_syntax_guard, trait_has_bound_implementation, TraitLookupCache,
};
pub(super) use trait_bounds::{TraitMethodLookupKey, TraitMethodLookupResult};

/// Infers a function call while checking generic trait bounds.
///
/// Inputs:
/// - `scheme`: function type scheme.
/// - `function_name`: optional diagnostic context.
/// - `args`: inferred argument types.
/// - `ctx` and `subst`: active inference context and substitutions.
///
/// Output:
/// - Instantiated return type or diagnostic string.
///
/// Transformation:
/// - Instantiates generic variables, unifies parameters with arguments,
///   validates generic bounds, and returns the substituted return type.
pub(crate) fn infer_function_with_bounds(
    scheme: &FunctionScheme,
    function_name: Option<&str>,
    args: &[Type],
    ctx: &ExprInferContext<'_>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<Type, String> {
    infer_function_with_explicit_type_args(scheme, function_name, args, &[], ctx, subst)
}

/// Infers a function call with optional explicit generic arguments.
///
/// Inputs:
/// - `scheme`: function type scheme.
/// - `function_name`: optional diagnostic context.
/// - `args`: inferred argument types.
/// - `type_args`: explicit source type arguments from `name[Type](...)`.
/// - `ctx` and `subst`: active inference context and substitutions.
///
/// Output:
/// - Instantiated return type or diagnostic string.
///
/// Transformation:
/// - Instantiates generic variables, binds explicit call type arguments to the
///   scheme's deterministic type-variable order, unifies parameters with
///   value arguments, validates bounds, and returns the substituted result.
pub(crate) fn infer_function_with_explicit_type_args(
    scheme: &FunctionScheme,
    function_name: Option<&str>,
    args: &[Type],
    type_args: &[SyntaxTypeOutput],
    ctx: &ExprInferContext<'_>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<Type, String> {
    let instantiated =
        instantiate_function_scheme_from(scheme, next_function_type_var(args, subst));
    bind_explicit_call_type_args(&instantiated, function_name, type_args, ctx, subst)?;
    infer_instantiated_function_with_bounds(&instantiated, function_name, args, ctx, subst)
}

/// Infers a function call from an already-instantiated function scheme.
///
/// Inputs:
/// - `scheme`: function scheme whose generic variables have already been
///   freshened for this call site.
/// - `function_name`: optional diagnostic context.
/// - `args`: inferred argument types.
/// - `ctx` and `subst`: active inference context and substitutions.
///
/// Output:
/// - Instantiated return type or diagnostic string.
///
/// Transformation:
/// - Checks arity, unifies instantiated parameters with value arguments,
///   validates trait bounds, and applies the final substitution to the return
///   type. This is used when a caller must freshen a larger synthetic callable,
///   such as receiver-method dispatch where the receiver type and method
///   parameters must share one type-variable mapping.
pub(crate) fn infer_instantiated_function_with_bounds(
    scheme: &FunctionScheme,
    function_name: Option<&str>,
    args: &[Type],
    ctx: &ExprInferContext<'_>,
    subst: &mut HashMap<TypeVarId, Type>,
) -> Result<Type, String> {
    if scheme.params.len() != args.len() {
        return Err(format!(
            "wrong arity for function call: expected {} args, found {}",
            scheme.params.len(),
            args.len()
        ));
    }

    for (expected, actual) in scheme.params.iter().zip(args.iter()) {
        let expected_substituted = apply_subst(expected, subst);
        let actual_substituted = apply_subst(actual, subst);
        if is_subtype_with_aliases(&actual_substituted, &expected_substituted, ctx.aliases) {
            continue;
        }
        if let Err(original_message) = unify(expected, actual, subst) {
            let expected_expanded = expand_type_aliases(&expected_substituted, ctx.aliases);
            let actual_expanded = expand_type_aliases(&actual_substituted, ctx.aliases);
            if is_subtype_with_aliases(&actual_expanded, &expected_expanded, ctx.aliases) {
                continue;
            }
            if unify(&expected_expanded, &actual_expanded, subst).is_err() {
                return Err(original_message);
            }
        }
    }

    if let Err(message) = check_function_bounds(scheme, function_name, ctx, subst) {
        return Err(message);
    }

    Ok(instantiate_type(&scheme.ret, subst))
}
